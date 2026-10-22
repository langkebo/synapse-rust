//! `worker.stream_writers` enforcement (W-I): forward a write to its configured writer.
//!
//! The decision itself lives in `synapse_services::worker::write_streams`; this module
//! only *obeys* it. Splitting it that way is what lets the routing table be tested with
//! nothing running, while the HTTP layer stays small enough to reason about.
//!
//! Placed **inside** `worker_surface_middleware`: the surface gate narrows what an
//! instance answers, so a request that does not belong to this role is still a 404 and
//! is never forwarded. Forwarding can only move a request between instances that are
//! both allowed to serve it.
//!
//! # The wire contract
//!
//! The request keeps its method, path, query, body and `Authorization` header; the
//! target re-validates the client token itself. Two headers are added:
//!
//! * [`WRITE_FORWARD_STREAM_HEADER`] — names the stream, so the receiver knows this is
//!   a forwarded write;
//! * [`WRITE_FORWARD_SECRET_HEADER`] — the `worker.replication.http` shared secret,
//!   which is what makes the first header trustworthy.
//!
//! Both together mean "already routed here, execute it locally". Without the secret the
//! stream header is ignored **and stripped**, so an external request cannot use it to
//! influence routing. Forging it would only suppress a forward — which is exactly what
//! happens when the feature is off — so it grants nothing.
//!
//! # What this deliberately does not do
//!
//! * **No retry.** Idempotency for writes comes from `room_event_txn_dedup` and the
//!   per-stream semantics; retrying here would turn "writer committed, response lost"
//!   into a duplicate write.
//! * **No `X-Forwarded-For` trust on its own.** The header is appended so the writer can
//!   recover the real client address for per-IP rate limiting, but it is only *believed*
//!   when the same request carries a valid `x-synapse-worker-secret` — see
//!   [`WRITE_FORWARD_CLIENT_IP_HEADER`] and [`forwarded_client_ip`].

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::header::HeaderName;
use axum::http::{HeaderMap, Method};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use futures::StreamExt;
use synapse_common::config::worker::WorkerConfig;
use synapse_common::error::ApiError;
use synapse_common::http_client::no_proxy_client_with_timeout;
use synapse_common::server_metrics::global_server_metrics;
use synapse_services::worker::write_streams::{
    instance_base_url, stream_for_write, write_route, WriteRoute, WriteStream, WRITE_FORWARD_SECRET_HEADER,
    WRITE_FORWARD_STREAM_HEADER,
};

/// Upper bound on a forwarded body, derived from `server.max_upload_size`.
///
/// **It must not be smaller than the outer `RequestBodyLimitLayer`**, otherwise the
/// forward layer becomes a new, invisible write limit: a federation transaction that
/// the instance would have accepted locally would 413 as soon as it is forwarded. The
/// caller passes the configured `server.max_upload_size` in for exactly that reason
/// ([`WriteForwarding::from_config`] takes it as an argument).
///
/// The body is **not buffered** (see [`forward`]); this bound is used for two things: to
/// reject a request whose *declared* `content-length` already exceeds it, before the body
/// is touched, and to tell an oversized transfer apart from a transport failure when the
/// streaming request dies mid-flight.
pub fn max_forward_body(max_upload_size: u64) -> usize {
    usize::try_from(max_upload_size).unwrap_or(usize::MAX)
}

/// Header carrying the client address across the forward hop.
///
/// Only honoured together with a valid [`WRITE_FORWARD_SECRET_HEADER`]: without the
/// shared secret any client could claim to be anyone and escape its own rate limit.
pub const WRITE_FORWARD_CLIENT_IP_HEADER: &str = "x-synapse-client-ip";

/// Recover the original client address of a forwarded write.
///
/// Returns `None` when the request was not forwarded by a trusted peer (no header, or a
/// header whose accompanying secret does not match), in which case the caller must keep
/// using its own peer address. This is the *only* place that decision is made, so the
/// rate limiter cannot accidentally trust an unauthenticated header.
pub fn forwarded_client_ip(headers: &HeaderMap, secret: Option<&str>) -> Option<String> {
    let secret = secret?;
    if !headers.contains_key(WRITE_FORWARD_STREAM_HEADER) {
        return None;
    }
    let presented = headers.get(WRITE_FORWARD_SECRET_HEADER).and_then(|value| value.to_str().ok()).unwrap_or("");
    if !synapse_common::crypto::secure_compare(presented, secret) {
        return None;
    }
    let value = headers.get(WRITE_FORWARD_CLIENT_IP_HEADER).and_then(|value| value.to_str().ok())?;
    // The value is a bare socket address. Anything else (a list, a hostname, an empty
    // string) is a bug in the sender; treat it as absent rather than propagating junk
    // into a rate-limit key.
    value.parse::<std::net::IpAddr>().ok().map(|address| address.to_string())
}

/// The replication secret this process resolved at assembly time.
///
/// Installed once by `synapse_rust::server::router::build_router` and read by anything
/// that must decide whether a forwarded marker can be believed — currently the rate
/// limiter, which uses [`forwarded_client_ip`] so that per-IP limits follow the client
/// across the hop instead of aggregating per reader.
///
/// A process-wide `OnceLock` rather than a threaded argument because there is exactly
/// one router per process: the value is a property of the deployment, not of a request.
/// It is set from the **assembly**, not from [`WriteForwarding::from_config`], so that
/// building a policy in a test does not mutate global state.
static PROCESS_SECRET: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();

/// Record the secret this process uses for forwarded writes. Later calls are ignored.
pub fn install_process_secret(secret: Option<String>) {
    let _ = PROCESS_SECRET.set(secret);
}

/// The secret recorded by [`install_process_secret`], if any.
pub fn process_secret() -> Option<&'static str> {
    PROCESS_SECRET.get().and_then(|secret| secret.as_deref())
}

/// Timeout for the forwarded request.
/// Longer than the server's own request budget on purpose: this layer must not be the
/// one that fails first, otherwise a write that used to take 45 s locally would start
/// failing at whatever the default client timeout is. The outer
/// `request_timeout_middleware` still bounds the latency the client sees.
const FORWARD_TIMEOUT: Duration = Duration::from_secs(120);

/// Precomputed forwarding policy, built once at assembly time.
#[derive(Debug, Clone)]
pub struct WriteForwarding {
    enabled: bool,
    secret: Option<String>,
    body_limit: usize,
    /// Only the streams whose writes are not executed here.
    routes: Vec<(WriteStream, WriteRoute)>,
}

impl WriteForwarding {
    /// Derive the policy from the worker configuration.
    ///
    /// `max_upload_size` is `server.max_upload_size`, the same field
    /// `RequestBodyLimitLayer` is built from — see [`max_forward_body`] for why the two
    /// must not diverge.
    ///
    /// Every route that needs a peer is resolved here — including reading
    /// `replication.http.secret_path` **once** instead of on the request path (a
    /// per-request file read would block a tokio worker thread). A secret that cannot
    /// be loaded turns every forward into `Unavailable`, which fails closed: writing
    /// locally instead would quietly restore the multi-writer state this feature exists
    /// to remove.
    pub fn from_config(config: &WorkerConfig, max_upload_size: u64) -> Self {
        let body_limit = max_forward_body(max_upload_size);
        let enabled = config.write_forwarding && config.enabled;
        if !enabled {
            return Self { enabled: false, secret: None, body_limit, routes: Vec::new() };
        }

        let secret = resolve_secret(config);
        let mut routes: Vec<(WriteStream, WriteRoute)> = WriteStream::all()
            .iter()
            .copied()
            .map(|stream| (stream, write_route(config, stream)))
            .filter(|(_, route)| !matches!(route, WriteRoute::Local))
            .collect();

        if secret.is_none() {
            routes = routes
                .into_iter()
                .map(|(stream, route)| {
                    let instance = match route {
                        WriteRoute::Forward { instance, .. } | WriteRoute::Unavailable { instance, .. } => instance,
                        WriteRoute::Local => String::new(),
                    };
                    (
                        stream,
                        WriteRoute::Unavailable {
                            instance,
                            reason: "worker.replication.http.secret / secret_path 读不到可用密钥".to_string(),
                        },
                    )
                })
                .collect();
        }

        if !routes.is_empty() {
            ::tracing::info!(
                streams = ?routes.iter().map(|(stream, _)| stream.stream_name()).collect::<Vec<_>>(),
                "Write forwarding enabled (worker.write_forwarding = true): writes for these streams \
                 are handed to their configured writer instead of being executed here"
            );
        }
        for (stream, route) in &routes {
            if let WriteRoute::Unavailable { instance, reason } = route {
                ::tracing::error!(
                    stream = stream.stream_name(),
                    instance = %instance,
                    reason = %reason,
                    "Write forwarding cannot reach the configured writer: writes on this stream will fail \
                     rather than be executed locally (see worker.topology.strict for a startup failure)"
                );
            }
        }

        Self { enabled, secret, body_limit, routes }
    }

    /// Whether this request was forwarded here by another instance.
    ///
    /// Requires **both** headers and a secret match; the stream header alone is noise.
    fn is_forwarded(&self, headers: &HeaderMap) -> bool {
        let Some(secret) = self.secret.as_deref() else {
            return false;
        };
        if !headers.contains_key(WRITE_FORWARD_STREAM_HEADER) {
            return false;
        }
        let presented = headers.get(WRITE_FORWARD_SECRET_HEADER).and_then(|value| value.to_str().ok()).unwrap_or("");
        synapse_common::crypto::secure_compare(presented, secret)
    }

    /// Whether the middleware should do anything at all for this request.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// The shared secret, for callers that need to validate forwarded markers
    /// (the surface gate and the rate limiter).
    pub fn secret(&self) -> Option<&str> {
        self.secret.as_deref()
    }

    /// Whether this instance executes `stream`'s writes itself.
    ///
    /// `None` (stream not routed anywhere) reads as `true`: this instance is the only
    /// candidate, which is the pre-forwarding behaviour and must not be narrowed by a
    /// caller that only wanted to ask.
    pub fn writes_locally(&self, stream: WriteStream) -> bool {
        match self.target(stream) {
            None | Some(WriteRoute::Local) => true,
            Some(WriteRoute::Forward { .. } | WriteRoute::Unavailable { .. }) => false,
        }
    }

    /// The precomputed route for `stream`, if this instance does not write it.
    fn target(&self, stream: WriteStream) -> Option<&WriteRoute> {
        self.routes.iter().find(|(candidate, _)| *candidate == stream).map(|(_, route)| route)
    }

    /// Streams whose writes this instance executes itself **because it is a configured
    /// writer** — the grant the surface gate needs
    /// (`synapse_web::middleware::worker_surface::WorkerSurfaceGate::stream_writes`).
    ///
    /// Empty when forwarding is off, which is what keeps the default deployment's
    /// surface unchanged: `write_route` returns `Local` for everything then, and
    /// handing that to the gate would let every instance answer every write.
    pub fn locally_written_streams(&self) -> Vec<WriteStream> {
        if !self.enabled {
            return Vec::new();
        }
        WriteStream::all().iter().copied().filter(|stream| self.writes_locally(*stream)).collect()
    }

    /// Publish the resolved routes so the topology endpoint and `/_health` report what
    /// this process actually does.
    ///
    /// Assembly-only, and deliberately **not** part of [`Self::from_config`]: a test that
    /// builds a policy must not mutate process-wide state (the same rule
    /// [`install_process_secret`] follows). Reporting from the config instead would
    /// disagree with this process exactly when it matters — a replication secret that
    /// failed to load turns every forward into `Unavailable` here while `write_route`
    /// still says `Forward`.
    pub fn publish_routes(&self) {
        report::install_routes(self.routes.clone());
    }
}

/// The configured replication secret, read once.
fn resolve_secret(config: &WorkerConfig) -> Option<String> {
    if let Some(secret) = config.replication.http.secret.as_deref() {
        if !secret.is_empty() {
            return Some(secret.to_string());
        }
    }
    let path = config.replication.http.secret_path.as_deref()?;
    match std::fs::read_to_string(path) {
        Ok(contents) if !contents.trim().is_empty() => Some(contents.trim().to_string()),
        Ok(_) => {
            ::tracing::error!(path = %path, "Write forwarding: replication secret file is empty");
            None
        }
        Err(error) => {
            ::tracing::error!(path = %path, error = %error, "Write forwarding: cannot read replication secret file");
            None
        }
    }
}

/// Outcome labels of `worker_write_forward_total`, and the runtime view built from the
/// same four values.
///
/// The report also answers "where does this stream's write go, and is that target
/// usable" for the topology endpoint and `/_health` — see [`report`].
mod report;

pub use report::{build_report, LastOutcome, StreamRoute, WriteForwardingReport};

/// Record one forwarding outcome, in the metric and in the process-local report.
///
/// Both halves are telemetry: a failure here must never fail the write, which is why the
/// report's `record` swallows a poisoned lock instead of propagating it.
fn record_forward(stream: WriteStream, outcome: LastOutcome) {
    report::record(stream, outcome);
    if let Some(metrics) = global_server_metrics() {
        metrics.worker_write_forward_total.observe(&[stream.stream_name(), outcome.label()]);
    }
}

/// Hand writes for `worker.stream_writers` streams to their configured writer.
pub async fn write_forwarding_middleware(
    State(policy): State<WriteForwarding>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    if !policy.enabled {
        return next.run(request).await;
    }

    // Loop break: this request is one we sent. Execute it here, whatever the config
    // says about ownership — otherwise two instances that disagree would ping-pong.
    if policy.is_forwarded(request.headers()) {
        strip_forward_headers(request.headers_mut());
        return next.run(request).await;
    }

    let method = request.method().clone();
    let Some(stream) = stream_for_write(method.as_str(), request.uri().path()) else {
        return next.run(request).await;
    };

    match policy.target(stream) {
        None | Some(WriteRoute::Local) => next.run(request).await,
        Some(WriteRoute::Unavailable { instance, reason }) => {
            record_forward(stream, LastOutcome::Refused);
            ::tracing::error!(
                stream = stream.stream_name(),
                instance = %instance,
                reason = %reason,
                "Refusing a write: its configured writer is unusable"
            );
            ApiError::service_unavailable(format!(
                "Writes for stream '{}' are owned by instance '{}', which is not usable: {reason}",
                stream.stream_name(),
                instance
            ))
            .into_response()
        }
        Some(WriteRoute::Forward { instance, base_url }) => {
            forward(&policy, stream, instance, base_url, method, request).await
        }
    }
}

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
async fn forward(
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

/// Remove the loop-break headers so they cannot reach a handler, a log line, or an
/// outbound request.
fn strip_forward_headers(headers: &mut HeaderMap) {
    headers.remove(WRITE_FORWARD_STREAM_HEADER);
    headers.remove(WRITE_FORWARD_SECRET_HEADER);
    // The client address is part of the same trusted envelope: once it has been read
    // (or deliberately ignored) it must not travel further, or a handler could mistake
    // a forwarded value for a peer address.
    headers.remove(WRITE_FORWARD_CLIENT_IP_HEADER);
}

/// The address the request arrived from, if the server was built with connect info.
///
/// `None` in unit tests and in any embedding that serves the router without
/// `into_make_service_with_connect_info` — in which case the forward simply omits the
/// client-address header and the writer falls back to this instance's address.
fn peer_ip(request: &Request<Body>) -> Option<std::net::SocketAddr> {
    request.extensions().get::<axum::extract::ConnectInfo<std::net::SocketAddr>>().map(|info| info.0)
}

/// Build the policy used by the middleware from a worker configuration.
///
/// Exists so the assembly (`synapse_rust::server::router`) does not have to know the
/// internals, and so tests can build the same object the server would.
pub fn policy_for(config: &WorkerConfig, max_upload_size: u64) -> WriteForwarding {
    WriteForwarding::from_config(config, max_upload_size)
}

/// Base URL of `instance`, for callers that need to log it (the topology endpoint).
pub fn base_url_of(config: &WorkerConfig, instance: &str) -> Option<String> {
    instance_base_url(config, instance)
}

#[cfg(test)]
mod tests;
