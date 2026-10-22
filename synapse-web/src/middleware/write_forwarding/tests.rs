use super::*;
use std::sync::Arc;

use axum::http::Method;
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

/// Body limit used by the tests: the `server.max_upload_size` default.
const TEST_BODY_LIMIT: u64 = 50_000_000;

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
    let app = app(WriteForwarding::from_config(&config, TEST_BODY_LIMIT), calls.clone());

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
    let app = app(WriteForwarding::from_config(&config, TEST_BODY_LIMIT), calls.clone());

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
    let app = app(WriteForwarding::from_config(&config, TEST_BODY_LIMIT), calls.clone());

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
    let app = app(WriteForwarding::from_config(&config, TEST_BODY_LIMIT), calls.clone());
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
    let app = app(WriteForwarding::from_config(&config, TEST_BODY_LIMIT), calls.clone());

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
    for stream in
        ["events", "typing", "to_device", "account_data", "receipts", "presence", "push_rules", "device_lists"]
    {
        set_writer(&mut config, stream, "event_persister");
    }
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let app = app(WriteForwarding::from_config(&config, TEST_BODY_LIMIT), calls.clone());

    // A read on a write-mapped path, and writes that belong to no stream at all
    // (logout keeps the device row, `forget` only drops the local membership, the
    // thread/space families are this server's own tables).
    for (method, path) in [
        (Method::GET, "/_matrix/client/v3/sendToDevice/m.x/t1"),
        (Method::POST, "/_matrix/client/v3/logout"),
        (Method::POST, "/_matrix/client/v3/rooms/!r:localhost/forget"),
        (Method::POST, "/_matrix/vendor/v1/rooms/!r:localhost/threads"),
    ] {
        let (status, _) = send(&app, method.clone(), path, &[], "{}").await;
        assert_eq!(status, 200, "{method} {path} must be answered locally");
    }
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 4);
}

#[tokio::test]
async fn every_stream_configured_to_a_reachable_peer_gets_a_route() {
    // Eight streams, eight routes: `events` and `device_lists` used to be excluded
    // because their mapping was incomplete, and a stream that silently keeps writing
    // locally while the config says otherwise is the failure this feature removes.
    let (host, port) = spawn_echo_upstream().await;
    let mut config = with_peer(config("client_reader"), &host, port);
    for stream in
        ["events", "typing", "to_device", "account_data", "receipts", "presence", "push_rules", "device_lists"]
    {
        set_writer(&mut config, stream, "event_persister");
    }
    let policy = WriteForwarding::from_config(&config, TEST_BODY_LIMIT);
    assert!(policy.is_enabled());
    for stream in WriteStream::all().iter().copied() {
        assert!(policy.target(stream).is_some(), "{} has no route", stream.stream_name());
        assert!(!policy.writes_locally(stream), "{} must not be written here", stream.stream_name());
    }
    assert!(policy.locally_written_streams().is_empty(), "the reader writes none of them");
}

#[tokio::test]
async fn a_configured_writer_keeps_its_own_streams_local() {
    // The other side of the same coin: on the writer itself every routed stream is
    // executed locally, and that set is what the surface gate has to admit.
    let (host, port) = spawn_echo_upstream().await;
    let mut config = with_peer(config("event_persister"), &host, port);
    set_writer(&mut config, "events", "event_persister");
    set_writer(&mut config, "device_lists", "event_persister");
    let policy = WriteForwarding::from_config(&config, TEST_BODY_LIMIT);

    assert!(policy.writes_locally(WriteStream::Events));
    let granted = policy.locally_written_streams();
    assert_eq!(granted, vec![WriteStream::Events, WriteStream::DeviceLists]);
}

#[tokio::test]
async fn no_stream_is_granted_to_the_surface_gate_while_forwarding_is_off() {
    // Handing the gate "everything is local" in the default configuration would let
    // every instance answer every write — the opposite of what the gate is for.
    let mut config = config("client_reader");
    config.write_forwarding = false;
    let policy = WriteForwarding::from_config(&config, TEST_BODY_LIMIT);
    assert!(policy.locally_written_streams().is_empty());
}

fn set_writer(config: &mut WorkerConfig, stream: &str, instance: &str) {
    let owners = vec![instance.to_string()];
    match stream {
        "events" => config.stream_writers.events = owners,
        "typing" => config.stream_writers.typing = owners,
        "to_device" => config.stream_writers.to_device = owners,
        "account_data" => config.stream_writers.account_data = owners,
        "receipts" => config.stream_writers.receipts = owners,
        "presence" => config.stream_writers.presence = owners,
        "push_rules" => config.stream_writers.push_rules = owners,
        "device_lists" => config.stream_writers.device_lists = owners,
        other => panic!("unknown stream {other}"),
    }
}

#[test]
fn the_forward_body_limit_follows_the_configured_upload_limit() {
    // A forward limit below `RequestBodyLimitLayer`'s would be a new, invisible write
    // limit that only appears once traffic is forwarded — a federation transaction
    // the instance would have accepted locally would 413 on the hop.
    assert_eq!(max_forward_body(50_000_000), 50_000_000);
    let policy = WriteForwarding::from_config(&config("client_reader"), 1234);
    assert_eq!(policy.body_limit, 1234);
}

#[test]
fn a_client_address_is_believed_only_with_the_shared_secret() {
    let secret = "k".repeat(32);
    let mut headers = HeaderMap::new();
    headers.insert(WRITE_FORWARD_CLIENT_IP_HEADER, "203.0.113.9".parse().expect("header value"));
    assert_eq!(forwarded_client_ip(&headers, Some(&secret)), None, "no stream header, no trust");

    headers.insert(WRITE_FORWARD_STREAM_HEADER, "events".parse().expect("header value"));
    assert_eq!(forwarded_client_ip(&headers, Some(&secret)), None, "no secret presented");

    headers.insert(WRITE_FORWARD_SECRET_HEADER, secret.parse().expect("header value"));
    assert_eq!(forwarded_client_ip(&headers, Some(&secret)).as_deref(), Some("203.0.113.9"));
    assert_eq!(forwarded_client_ip(&headers, Some(&"x".repeat(32))), None, "a wrong secret is not trusted");
    assert_eq!(forwarded_client_ip(&headers, None), None, "no local secret, no trust");
}

#[test]
fn a_malformed_client_address_is_dropped_rather_than_forwarded() {
    // The value ends up in a rate-limit key, so anything that is not a bare address
    // (a list, a hostname, junk) must read as absent instead of becoming a key.
    let secret = "k".repeat(32);
    for value in ["1.2.3.4, 5.6.7.8", "example.com", "", "not-an-ip"] {
        let mut headers = HeaderMap::new();
        headers.insert(WRITE_FORWARD_STREAM_HEADER, "events".parse().expect("header value"));
        headers.insert(WRITE_FORWARD_SECRET_HEADER, secret.parse().expect("header value"));
        headers.insert(WRITE_FORWARD_CLIENT_IP_HEADER, value.parse().expect("header value"));
        assert_eq!(forwarded_client_ip(&headers, Some(&secret)), None, "{value:?} must be dropped");
    }
}
