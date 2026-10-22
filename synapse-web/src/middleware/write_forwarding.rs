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
//! * **No `X-Forwarded-For` propagation.** The writer sees this instance's address, so
//!   per-IP rate limiting aggregates per reader. Fixing that means changing how the
//!   limiter reads the peer address *and* deciding when to trust the header; it is
//!   tracked separately rather than half-done here.

use std::time::Duration;

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::header::HeaderName;
use axum::http::{HeaderMap, Method};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use synapse_common::config::worker::WorkerConfig;
use synapse_common::error::ApiError;
use synapse_common::http_client::client_with_timeout;
use synapse_services::worker::write_streams::{
    instance_base_url, stream_for_write, write_route, WriteRoute, WriteStream, WRITE_FORWARD_SECRET_HEADER,
    WRITE_FORWARD_STREAM_HEADER,
};

/// Upper bound on a forwarded body.
///
/// Only the six light streams are forwarded, and none of them carries media; 16 MiB is
/// far above a `sendToDevice` batch while still keeping this path from becoming an
/// unbounded proxy. Larger bodies get the same `413 M_TOO_LARGE` the global body-limit
/// layer produces, so a client cannot tell the two apart.
pub const MAX_FORWARD_BODY: usize = 16 * 1024 * 1024;

/// Timeout for the forwarded request.
///
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
    /// Only the streams whose writes are not executed here.
    routes: Vec<(WriteStream, WriteRoute)>,
}

impl WriteForwarding {
    /// Derive the policy from the worker configuration.
    ///
    /// Every route that needs a peer is resolved here — including reading
    /// `replication.http.secret_path` **once** instead of on the request path (a
    /// per-request file read would block a tokio worker thread). A secret that cannot
    /// be loaded turns every forward into `Unavailable`, which fails closed: writing
    /// locally instead would quietly restore the multi-writer state this feature exists
    /// to remove.
    pub fn from_config(config: &WorkerConfig) -> Self {
        let enabled = config.write_forwarding && config.enabled;
        if !enabled {
            return Self { enabled: false, secret: None, routes: Vec::new() };
        }

        let secret = resolve_secret(config);
        let mut routes: Vec<(WriteStream, WriteRoute)> = WriteStream::all()
            .iter()
            .copied()
            .filter(|stream| stream.is_forwardable())
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

        Self { enabled, secret, routes }
    }

    /// The precomputed route for `stream`, if this instance does not write it.
    fn target(&self, stream: WriteStream) -> Option<&WriteRoute> {
        self.routes.iter().find(|(candidate, _)| *candidate == stream).map(|(_, route)| route)
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
        return ApiError::service_unavailable(format!(
            "Writes for stream '{}' are owned by instance '{instance}', but no replication secret is available",
            stream.stream_name()
        ))
        .into_response();
    };

    let (parts, body) = request.into_parts();
    let body = match axum::body::to_bytes(body, MAX_FORWARD_BODY).await {
        Ok(bytes) => bytes,
        Err(_) => {
            return ApiError::too_large(format!(
                "Forwarded write for stream '{}' exceeds {MAX_FORWARD_BODY} bytes",
                stream.stream_name()
            ))
            .into_response();
        }
    };

    let mut url = format!("{base_url}{}", parts.uri.path());
    if let Some(query) = parts.uri.query() {
        url.push('?');
        url.push_str(query);
    }

    let started = std::time::Instant::now();
    let mut builder = client_with_timeout(FORWARD_TIMEOUT).request(method.clone(), &url);
    for (name, value) in &parts.headers {
        // `content-length` is dropped with the rest of the framing headers: the body we
        // just buffered may differ from the one the client declared, and reqwest sets
        // the real length itself.
        if is_hop_by_hop(name) || name.as_str() == "content-length" {
            continue;
        }
        builder = builder.header(name, value);
    }
    builder = builder.header(WRITE_FORWARD_STREAM_HEADER, stream.stream_name());
    builder = builder.header(WRITE_FORWARD_SECRET_HEADER, secret);

    let response = match builder.body(body).send().await {
        Ok(response) => response,
        Err(error) => {
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
        Ok(bytes) if bytes.len() <= MAX_FORWARD_BODY => {
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
}

/// Build the policy used by the middleware from a worker configuration.
///
/// Exists so the assembly (`synapse_rust::server::router`) does not have to know the
/// internals, and so tests can build the same object the server would.
pub fn policy_for(config: &WorkerConfig) -> WriteForwarding {
    WriteForwarding::from_config(config)
}

/// Base URL of `instance`, for callers that need to log it (the topology endpoint).
pub fn base_url_of(config: &WorkerConfig, instance: &str) -> Option<String> {
    instance_base_url(config, instance)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use axum::http::StatusCode;
    use axum::routing::{any, get};
    use axum::Router;
    use synapse_common::config::worker::{InstanceLocationConfig, WorkerConfig};
    use tower::ServiceExt;

    fn config(instance: &str) -> WorkerConfig {
        let mut config = WorkerConfig {
            enabled: true,
            instance_name: instance.to_string(),
            write_forwarding: true,
            ..Default::default()
        };
        config.replication.http.enabled = true;
        config.replication.http.secret = Some("k".repeat(32));
        config
    }

    fn with_peer(mut config: WorkerConfig, host: &str, port: u16) -> WorkerConfig {
        config
            .instance_map
            .insert("event_persister".to_string(), InstanceLocationConfig { host: host.to_string(), port, tls: false });
        config
    }

    /// A stand-in for the writer instance: echoes what it received.
    async fn spawn_echo_upstream() -> (String, u16) {
        async fn echo(request: axum::extract::Request) -> (StatusCode, String) {
            let method = request.method().clone();
            let path = request.uri().path().to_string();
            let stream = request
                .headers()
                .get(WRITE_FORWARD_STREAM_HEADER)
                .and_then(|value| value.to_str().ok())
                .unwrap_or("<none>")
                .to_string();
            let body = axum::body::to_bytes(request.into_body(), 1024 * 1024).await.unwrap_or_default();
            (StatusCode::CREATED, format!("{method} {path} body={} stream={stream}", String::from_utf8_lossy(&body)))
        }
        let app = Router::new().route("/{*path}", any(echo));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("addr").port();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        ("127.0.0.1".to_string(), port)
    }

    fn app(policy: WriteForwarding, handler_calls: Arc<std::sync::atomic::AtomicUsize>) -> Router {
        Router::new()
            .route(
                "/{*path}",
                any(move || {
                    let counter = handler_calls.clone();
                    async move {
                        counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        (StatusCode::OK, "local")
                    }
                }),
            )
            .route("/health", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn_with_state(policy, write_forwarding_middleware))
    }

    async fn send(app: &Router, method: Method, path: &str, headers: &[(&str, &str)], body: &str) -> (u16, String) {
        let mut builder = Request::builder().method(method).uri(path);
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        let request = builder.body(Body::from(body.to_string())).expect("request");
        let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.expect("response");
        let status = response.status().as_u16();
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.expect("body");
        (status, String::from_utf8_lossy(&bytes).to_string())
    }

    #[tokio::test]
    async fn a_write_for_another_instances_stream_is_forwarded_verbatim() {
        let (host, port) = spawn_echo_upstream().await;
        let mut config = with_peer(config("client_reader"), &host, port);
        config.stream_writers.to_device = vec!["event_persister".to_string()];
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let app = app(WriteForwarding::from_config(&config), calls.clone());

        let (status, body) = send(
            &app,
            Method::PUT,
            "/_matrix/client/v3/sendToDevice/m.room_key/txn1",
            &[("authorization", "Bearer client-token")],
            r#"{"messages":{}}"#,
        )
        .await;

        assert_eq!(status, 201, "the writer's status must reach the client: {body}");
        assert!(body.contains("PUT /_matrix/client/v3/sendToDevice/m.room_key/txn1"), "{body}");
        assert!(body.contains("stream=to_device"), "the loop-break header must name the stream: {body}");
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 0, "the local handler must not run");
    }

    #[tokio::test]
    async fn a_write_the_configured_writer_cannot_serve_is_refused_not_degraded() {
        // No peer reachable ⇒ 503. Executing locally would silently restore the
        // multi-writer state this feature exists to remove.
        let mut config = config("client_reader");
        config.stream_writers.to_device = vec!["event_persister".to_string()];
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let app = app(WriteForwarding::from_config(&config), calls.clone());

        let (status, body) = send(&app, Method::PUT, "/_matrix/client/v3/sendToDevice/m.x/t1", &[], "{}").await;
        assert_eq!(status, 503, "{body}");
        assert!(body.contains("M_UNKNOWN") || body.contains("to_device"), "{body}");
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn the_local_writer_does_not_make_a_hop() {
        let mut config = config("event_persister");
        config.stream_writers.to_device = vec!["event_persister".to_string()];
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let app = app(WriteForwarding::from_config(&config), calls.clone());

        let (status, body) = send(&app, Method::PUT, "/_matrix/client/v3/sendToDevice/m.x/t1", &[], "{}").await;
        assert_eq!((status, body.as_str()), (200, "local"));
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn a_forwarded_request_executes_locally_instead_of_looping() {
        let (host, port) = spawn_echo_upstream().await;
        let mut config = with_peer(config("client_reader"), &host, port);
        config.stream_writers.to_device = vec!["event_persister".to_string()];
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let app = app(WriteForwarding::from_config(&config), calls.clone());
        let secret = config.replication.http.secret.clone().expect("secret");

        let (status, body) = send(
            &app,
            Method::PUT,
            "/_matrix/client/v3/sendToDevice/m.room_key/txn1",
            &[(WRITE_FORWARD_STREAM_HEADER, "to_device"), (WRITE_FORWARD_SECRET_HEADER, secret.as_str())],
            "{}",
        )
        .await;

        assert_eq!((status, body.as_str()), (200, "local"), "must not forward again");
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn a_forged_loop_break_header_without_the_secret_is_ignored() {
        // Otherwise any client could pin its write to whichever instance it reached.
        let (host, port) = spawn_echo_upstream().await;
        let mut config = with_peer(config("client_reader"), &host, port);
        config.stream_writers.to_device = vec!["event_persister".to_string()];
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let app = app(WriteForwarding::from_config(&config), calls.clone());

        let (status, body) = send(
            &app,
            Method::PUT,
            "/_matrix/client/v3/sendToDevice/m.room_key/txn1",
            &[(WRITE_FORWARD_STREAM_HEADER, "to_device"), (WRITE_FORWARD_SECRET_HEADER, "wrong")],
            "{}",
        )
        .await;

        assert_eq!(status, 201, "it must be forwarded, not executed here: {body}");
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn reads_and_unmapped_writes_stay_local() {
        let (host, port) = spawn_echo_upstream().await;
        let mut config = with_peer(config("client_reader"), &host, port);
        for stream in ["typing", "to_device", "account_data", "receipts", "presence", "push_rules"] {
            set_writer(&mut config, stream, "event_persister");
        }
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let app = app(WriteForwarding::from_config(&config), calls.clone());

        // A read on the same prefix, and a write that is not part of any stream.
        for (method, path) in [
            (Method::GET, "/_matrix/client/v3/sendToDevice/m.x/t1"),
            (Method::POST, "/_matrix/client/v3/createRoom"),
            (Method::PUT, "/_matrix/client/v3/keys/upload"),
        ] {
            let (status, _) = send(&app, method.clone(), path, &[], "{}").await;
            assert_eq!(status, 200, "{method} {path} must be answered locally");
        }
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn the_six_forwardable_streams_each_reach_the_writer() {
        let (host, port) = spawn_echo_upstream().await;
        let mut config = with_peer(config("client_reader"), &host, port);
        for stream in ["typing", "to_device", "account_data", "receipts", "presence", "push_rules"] {
            set_writer(&mut config, stream, "event_persister");
        }
        let policy = WriteForwarding::from_config(&config);
        assert!(policy.is_enabled());
        for stream in WriteStream::all().iter().copied() {
            let routed = policy.target(stream).is_some();
            assert_eq!(routed, stream.is_forwardable(), "{} routing mismatch", stream.stream_name());
        }
    }

    fn set_writer(config: &mut WorkerConfig, stream: &str, instance: &str) {
        let owners = vec![instance.to_string()];
        match stream {
            "typing" => config.stream_writers.typing = owners,
            "to_device" => config.stream_writers.to_device = owners,
            "account_data" => config.stream_writers.account_data = owners,
            "receipts" => config.stream_writers.receipts = owners,
            "presence" => config.stream_writers.presence = owners,
            "push_rules" => config.stream_writers.push_rules = owners,
            other => panic!("unknown stream {other}"),
        }
    }
}
