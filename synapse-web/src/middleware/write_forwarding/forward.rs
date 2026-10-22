//! The forwarding hop itself: request framing, the outbound call, and response relay.
//!
//! Split out of the parent module so [`super::write_forwarding_middleware`] reads as a
//! decision (local / refuse / forwarded) while the mechanics of one hop live here. The
//! split is also what keeps the parent under the 600-line ceiling.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use axum::body::Body;
use axum::extract::Request;
use axum::http::header::HeaderName;
use axum::http::{HeaderMap, Method};
use axum::response::{IntoResponse, Response};
use futures::StreamExt;
use synapse_common::error::ApiError;
use synapse_common::http_client::no_proxy_client_with_timeout;
use synapse_services::worker::write_streams::{WriteStream, WRITE_FORWARD_SECRET_HEADER, WRITE_FORWARD_STREAM_HEADER};

use super::{peer_ip, record_forward, LastOutcome, WriteForwarding, FORWARD_TIMEOUT, WRITE_FORWARD_CLIENT_IP_HEADER};

/// The `content-length` the client declared, if any.
///
/// Used only as a *pre-check*: the value is never forwarded (a client can declare a length
/// it does not deliver) and the authoritative enforcement stays the outer
/// `RequestBodyLimitLayer`.
fn declared_content_length(headers: &HeaderMap) -> Option<u64> {
    headers.get(axum::http::header::CONTENT_LENGTH)?.to_str().ok()?.parse().ok()
}

/// Pipe the inbound body to the writer instead of buffering it.
///
/// Two reasons, and the first one is the important one:
///
/// * **No pre-authentication allocation.** This middleware sits before every handler, i.e.
///   before authentication, while handlers authenticate *before* reading the body
///   (`send_message` takes `AuthenticatedUser` and only then `MatrixJson`). Buffering here
///   would hand any unauthenticated client `server.max_upload_size` (default 50 MB) of
///   allocation per connection — a capability the pre-forwarding path never had. Measured
///   on this repo's two-instance harness; the assertion is structural, not incidental.
/// * **Backpressure instead of a copy.** Bytes travel from the client socket to the writer
///   socket; the reader's footprint is one chunk, not one body.
///
/// `forwarded` counts what actually went out, which is how [`forward`] separates "the size
/// guard cut us off" from "the writer is gone" after the request has failed — `reqwest`
/// does not expose the body stream's error in a downcastable form.
fn streaming_body(body: Body, forwarded: Arc<AtomicU64>) -> reqwest::Body {
    let stream = Box::pin(body.into_data_stream());
    reqwest::Body::wrap_stream(futures::stream::unfold((stream, forwarded), |(mut stream, forwarded)| async move {
        match stream.next().await {
            Some(Ok(chunk)) => {
                forwarded.fetch_add(chunk.len() as u64, Ordering::Relaxed);
                Some((Ok::<bytes::Bytes, axum::Error>(chunk), (stream, forwarded)))
            }
            Some(Err(error)) => Some((Err(error), (stream, forwarded))),
            None => None,
        }
    }))
}

/// Perform the forward.
pub(super) async fn forward(
    policy: &WriteForwarding,
    stream: WriteStream,
    instance: &str,
    base_url: &str,
    method: Method,
    request: Request<Body>,
) -> Response {
    let Some(secret) = policy.secret.clone() else {
        // `from_config` collapses this case into `Unavailable`, so reaching it means the
        // policy was built by hand. Fail closed rather than write locally.
        record_forward(stream, LastOutcome::Refused);
        return ApiError::service_unavailable(format!(
            "Writes for stream '{}' are owned by instance '{instance}', but no replication secret is available",
            stream.stream_name()
        ))
        .into_response();
    };

    let limit = policy.body_limit;
    let client_ip = peer_ip(&request).map(|address| address.to_string());
    let (parts, body) = request.into_parts();

    // Reject on the **declared** length before the body is touched.
    //
    // The middleware runs *before* authentication (it is a routing decision, not an auth
    // one) while every handler authenticates before it reads the body — `send_message`
    // takes `AuthenticatedUser` and only then `MatrixJson`. Buffering here would therefore
    // hand any unauthenticated client `max_upload_size` bytes of allocation per connection,
    // which is a capability the pre-forwarding path never had. So the body is **streamed**
    // (below), and the only size decision taken up front is on the header the client
    // declared.
    if let Some(declared) = declared_content_length(&parts.headers) {
        if declared > limit as u64 {
            record_forward(stream, LastOutcome::Refused);
            return ApiError::too_large(format!(
                "Forwarded write for stream '{}' exceeds the {limit}-byte forward limit",
                stream.stream_name()
            ))
            .into_response();
        }
    }

    // Byte counter for the streamed body: it tells an oversized transfer apart from a
    // transport failure after the fact. `limit` is where the outer `RequestBodyLimitLayer`
    // cuts the inbound stream, so `>= limit` means "the size guard fired", not "the
    // network broke".
    let forwarded_bytes = Arc::new(AtomicU64::new(0));

    let mut url = format!("{base_url}{}", parts.uri.path());
    if let Some(query) = parts.uri.query() {
        url.push('?');
        url.push_str(query);
    }

    let started = std::time::Instant::now();
    // **No proxy on this hop.** `instance_map` names another process of *this* deployment;
    // routing the request through `HTTP_PROXY` would hand it to a third party — including
    // the `x-synapse-worker-secret` it carries — and would turn "the writer is down" into
    // the proxy's 502. See `no_proxy_client_with_timeout`.
    let mut builder = no_proxy_client_with_timeout(FORWARD_TIMEOUT).request(method.clone(), &url);
    for (name, value) in &parts.headers {
        // `content-length` is dropped with the rest of the framing headers: a client can
        // declare a length it does not deliver (buffering used to hide that, streaming
        // must not), so the upstream request is sent chunked and `reqwest` frames it.
        // The declared value has already been checked against `limit` above.
        if is_hop_by_hop(name) || name.as_str() == "content-length" {
            continue;
        }
        builder = builder.header(name, value);
    }
    builder = builder.header(WRITE_FORWARD_STREAM_HEADER, stream.stream_name());
    builder = builder.header(WRITE_FORWARD_SECRET_HEADER, secret);
    if let Some(client_ip) = client_ip.as_deref() {
        // Paired with the secret above, which is what makes the writer believe it
        // (`forwarded_client_ip`). Without a peer address there is nothing to add and
        // the writer keeps treating this instance as the client — the pre-existing
        // behaviour.
        builder = builder.header(WRITE_FORWARD_CLIENT_IP_HEADER, client_ip);
    }

    let response = match builder.body(streaming_body(body, forwarded_bytes.clone())).send().await {
        Ok(response) => response,
        Err(error) => {
            // Two different failures look identical here (`reqwest::Error` does not carry
            // the body stream's cause in a downcastable form): the writer being down, and
            // the inbound size guard cutting the stream we were piping. The counter is
            // what separates them — the guard fires exactly at `limit`.
            if forwarded_bytes.load(Ordering::Relaxed) >= limit as u64 {
                record_forward(stream, LastOutcome::Refused);
                ::tracing::warn!(
                    stream = stream.stream_name(),
                    instance = %instance,
                    limit,
                    "Forwarded write exceeded the forward limit mid-stream; the inbound body guard \
                     cut the connection, so the client gets 413 rather than a transport error"
                );
                return ApiError::too_large(format!(
                    "Forwarded write for stream '{}' exceeds the {limit}-byte forward limit",
                    stream.stream_name()
                ))
                .into_response();
            }
            record_forward(stream, LastOutcome::Unreachable);
            ::tracing::error!(
                stream = stream.stream_name(),
                instance = %instance,
                elapsed_ms = started.elapsed().as_millis() as u64,
                error = %error,
                "Forwarding a write failed at the transport layer; not retrying \
                 (the client's retry plus the existing txn dedup is the safe path)"
            );
            return ApiError::service_unavailable(format!(
                "Writes for stream '{}' are owned by instance '{instance}', which could not be reached",
                stream.stream_name()
            ))
            .into_response();
        }
    };

    let status = response.status();
    let headers = response.headers().clone();
    record_forward(stream, if status.is_success() { LastOutcome::Ok } else { LastOutcome::UpstreamError });
    ::tracing::debug!(
        stream = stream.stream_name(),
        instance = %instance,
        %method,
        %status,
        elapsed_ms = started.elapsed().as_millis() as u64,
        "Forwarded a write to its stream writer"
    );

    // A non-2xx is the writer's verdict about this write and must reach the client
    // unchanged: rewriting it (e.g. to 502) would hide a 403 or a 429 behind an
    // infrastructure error and make the client retry something it must not.
    //
    // `bytes()` rather than a streaming copy: a write response is a small JSON body, and
    // the alternative (re-streaming) would have to reimplement framing. The size is
    // checked afterwards so an unexpectedly large answer cannot be relayed blindly.
    match response.bytes().await {
        Ok(bytes) if bytes.len() <= limit => {
            let mut out = Response::new(Body::from(bytes));
            *out.status_mut() = status;
            copy_response_headers(&headers, out.headers_mut());
            out
        }
        Ok(bytes) => {
            ::tracing::error!(
                stream = stream.stream_name(),
                instance = %instance,
                bytes = bytes.len(),
                "Writer's response exceeded the forward limit; refusing to relay it"
            );
            ApiError::service_unavailable("The stream writer's response was too large to relay".to_string())
                .into_response()
        }
        Err(error) => {
            ::tracing::error!(
                stream = stream.stream_name(),
                instance = %instance,
                error = %error,
                "Writer's response could not be read back"
            );
            ApiError::service_unavailable("The stream writer's response could not be read".to_string()).into_response()
        }
    }
}

/// Headers that must not be relayed between hops (RFC 9110 §7.6.1).
fn is_hop_by_hop(name: &HeaderName) -> bool {
    matches!(
        name.as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
            | "host"
    )
}

/// Copy the writer's response headers, minus the framing ones.
///
/// `content-length` is skipped on purpose: we read the body to the end, so its length
/// is whatever we got, and letting axum derive it avoids a mismatch when the upstream
/// answered with chunked encoding.
fn copy_response_headers(source: &HeaderMap, target: &mut HeaderMap) {
    for (name, value) in source {
        if is_hop_by_hop(name) || name.as_str() == "content-length" {
            continue;
        }
        target.insert(name, value.clone());
    }
}
