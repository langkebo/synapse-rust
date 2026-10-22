//! Two-**instance** end-to-end coverage for W-I (`worker.stream_writers` forwarding).
//!
//! The existing middleware tests use a stub upstream (`spawn_echo_upstream`): it echoes
//! whatever arrives and knows nothing about the surface gate. That is enough to prove the
//! forwarding path works, but it cannot exercise the two things that only break when two
//! *real* instances meet:
//!
//! 1. **the surface gate on the receiving side** (§5.4). `event_persister` owns only
//!    `/_synapse/worker/v1/replication/*`, so a room send forwarded to it is answered 404
//!    by the gate *before* the handler — unless the receiver is also granted the writes of
//!    the streams it owns. The two features are individually correct and jointly broken,
//!    and a stub upstream cannot show it.
//! 2. **the loop break**. The writer runs the same middleware and is configured with its
//!    own `stream_writers`; without the envelope check it would forward the request again.
//!    That path only exists when the receiver is a genuine instance.
//!
//! So each test here starts **two real HTTP servers** — their own `WorkerConfig`, their own
//! instance name, the same shared replication secret, both middlewares in the production
//! order (gate outside, forwarding inside) — and sends the request through the reader.
//!
//! What this deliberately does **not** cover: two OS processes. Boot order, config-file
//! parsing, env overrides and migrations are process-level concerns covered by the W-H
//! stack script; this file is the protocol-level guarantee, and it runs in seconds.
//!
//! Each test uses a **different stream** on purpose: the observation registry that backs
//! `build_report` is process-wide, so sharing a stream would make the assertions
//! order-dependent under `cargo test` (nextest hides that by using one process per test).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::from_fn_with_state;
use axum::routing::any;
use axum::Router;

use synapse_common::config::worker::{InstanceLocationConfig, WorkerConfig};
use synapse_services::worker::types::WorkerType;
use synapse_services::worker::write_streams::{WriteStream, WRITE_FORWARD_SECRET_HEADER, WRITE_FORWARD_STREAM_HEADER};
use synapse_web::middleware::write_forwarding::{
    build_report, LastOutcome, WriteForwarding, WRITE_FORWARD_CLIENT_IP_HEADER,
};
use synapse_web::middleware::{worker_surface_middleware, write_forwarding_middleware, WorkerSurfaceGate};

const SECRET: &str = "0123456789abcdef0123456789abcdef";
const BODY_LIMIT: u64 = 50_000_000;

/// What the receiving handler saw. Recorded so the envelope can be inspected at the point
/// a real handler would run.
#[derive(Debug, Clone, Default)]
struct SeenRequest {
    path: String,
    has_stream_header: bool,
    has_secret_header: bool,
    client_ip: Option<String>,
}

#[derive(Clone)]
struct Instance {
    port: u16,
    handler_calls: Arc<AtomicUsize>,
    seen: Arc<Mutex<Vec<SeenRequest>>>,
}

impl Instance {
    fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    fn calls(&self) -> usize {
        self.handler_calls.load(Ordering::SeqCst)
    }

    fn last_seen(&self) -> Option<SeenRequest> {
        self.seen.lock().ok().and_then(|seen| seen.last().cloned())
    }
}

fn reader_config(peer_port: u16) -> WorkerConfig {
    let mut config = WorkerConfig {
        enabled: true,
        instance_name: "client_reader".to_string(),
        write_forwarding: true,
        ..Default::default()
    };
    config.replication.enabled = true;
    config.replication.http.enabled = true;
    config.replication.http.secret = Some(SECRET.to_string());
    config.instance_map.insert(
        "master".to_string(),
        InstanceLocationConfig { host: "127.0.0.1".to_string(), port: peer_port, tls: false },
    );
    config
}

/// The writer: it owns `master`'s streams, so it executes what it receives.
fn writer_config() -> WorkerConfig {
    let mut config = WorkerConfig {
        enabled: true,
        instance_name: "master".to_string(),
        write_forwarding: true,
        ..Default::default()
    };
    config.replication.enabled = true;
    config.replication.http.enabled = true;
    config.replication.http.secret = Some(SECRET.to_string());
    config
}

/// Start one instance: routing table → forwarding (inner) → surface gate (outer), which is
/// exactly the order `synapse_rust::server::router` installs them in.
async fn spawn_instance(config: WorkerConfig, role: WorkerType, grant_stream_writes: bool) -> Instance {
    let handler_calls = Arc::new(AtomicUsize::new(0));
    let seen = Arc::new(Mutex::new(Vec::new()));

    let policy = WriteForwarding::from_config(&config, BODY_LIMIT);
    let mut gate = WorkerSurfaceGate::new(role);
    if grant_stream_writes {
        // Production grants the receiver the writes of the streams it owns — that is what
        // keeps W-F and W-I from cancelling each other out (§5.4).
        gate = gate.with_stream_writes(policy.locally_written_streams());
    }

    let calls = handler_calls.clone();
    let recorded = seen.clone();
    let handler = move |request: Request<Body>| {
        let calls = calls.clone();
        let recorded = recorded.clone();
        async move {
            calls.fetch_add(1, Ordering::SeqCst);
            if let Ok(mut seen) = recorded.lock() {
                seen.push(SeenRequest {
                    path: request.uri().path().to_string(),
                    has_stream_header: request.headers().contains_key(WRITE_FORWARD_STREAM_HEADER),
                    has_secret_header: request.headers().contains_key(WRITE_FORWARD_SECRET_HEADER),
                    client_ip: request
                        .headers()
                        .get(WRITE_FORWARD_CLIENT_IP_HEADER)
                        .and_then(|value| value.to_str().ok())
                        .map(str::to_string),
                });
            }
            (StatusCode::OK, "executed-by-writer")
        }
    };

    let app = Router::new()
        .route("/{*path}", any(handler))
        .layer(from_fn_with_state(policy, write_forwarding_middleware))
        .layer(from_fn_with_state(gate, worker_surface_middleware));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("local addr").port();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    Instance { port, handler_calls, seen }
}

/// A client that ignores the ambient `HTTP_PROXY`.
///
/// The suite must measure the instances, not the egress proxy: with `HTTP_PROXY` set
/// (common in CI images and on developer machines) a proxied request to a closed port
/// answers the *proxy's* 502, which would make the "writer is down ⇒ 503" assertion pass or
/// fail for the wrong reason.
fn test_client() -> reqwest::Client {
    reqwest::Client::builder().no_proxy().build().expect("test client")
}

/// Send a write through the reader, over a real socket.
async fn send(reader: &Instance, method: &str, path: &str, body: &str) -> (u16, String) {
    // A full round trip through the forwarding middleware's own client, so a failure here
    // is a real HTTP failure rather than a `tower::oneshot` in-process call.
    let request = test_client()
        .request(method.parse().expect("method"), format!("{}{path}", reader.base_url()))
        .header("authorization", "Bearer test-token")
        .body(body.to_string());
    let response = request.send().await.expect("reader must answer");
    let status = response.status().as_u16();
    let text = response.text().await.expect("body");
    (status, text)
}

/// Reserve a port and drop the listener: nothing is listening there.
async fn closed_port() -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    drop(listener);
    port
}

// ── 1. the happy path, with the envelope checked where a handler would see it ────────────

#[tokio::test]
async fn a_room_send_is_executed_on_the_writer_and_the_envelope_never_reaches_its_handler() {
    let writer = spawn_instance(writer_config(), WorkerType::EventPersister, true).await;
    let reader = spawn_instance(reader_config(writer.port), WorkerType::Frontend, true).await;

    let (status, body) = send(
        &reader,
        "PUT",
        "/_matrix/client/v3/rooms/!r:example.org/send/m.room.message/t1",
        r#"{"msgtype":"m.text","body":"hello"}"#,
    )
    .await;

    assert_eq!(status, 200, "the writer's answer must be relayed verbatim: {body}");
    assert_eq!(body, "executed-by-writer");
    assert_eq!(reader.calls(), 0, "the reader must not execute a write it does not own");
    assert_eq!(writer.calls(), 1);

    let seen = writer.last_seen().expect("the writer's handler must have run");
    assert_eq!(seen.path, "/_matrix/client/v3/rooms/!r:example.org/send/m.room.message/t1");
    assert!(!seen.has_stream_header, "the loop-break marker must be stripped before the handler");
    assert!(!seen.has_secret_header, "the shared secret must never reach a handler");
    assert_eq!(seen.client_ip.as_deref(), None, "the test client has no peer address to forward");
}

// ── 2. the surface gate on the receiving side (§5.4) ─────────────────────────────────────

#[tokio::test]
async fn without_the_stream_write_grant_the_writer_answers_404_before_its_handler() {
    // `event_persister` owns only `/_synapse/worker/v1/replication/*`; the forwarded receipt
    // is outside that. With the grant (production) it passes; without it the gate answers
    // 404 and the handler never runs. This is the failure mode the stub upstream cannot see.
    let writer = spawn_instance(writer_config(), WorkerType::EventPersister, false).await;
    let reader = spawn_instance(reader_config(writer.port), WorkerType::Frontend, true).await;

    let (status, body) = send(&reader, "POST", "/_matrix/client/v3/rooms/!r:example.org/receipt/m.read/$e", "{}").await;

    assert_eq!(status, 404, "the gate must refuse a path the role does not own: {body}");
    assert_eq!(writer.calls(), 0, "the handler must never run behind a refused path");
    assert_eq!(reader.calls(), 0);
}

// ── 3. a dead writer: 503, and the report says which stream broke ────────────────────────

#[tokio::test]
async fn a_dead_writer_turns_the_write_into_503_and_shows_up_in_the_report() {
    let mut config = reader_config(closed_port().await);
    // Only `typing` is of interest here; keeping the other streams on a dead peer would just
    // repeat the same assertion. Use a distinct stream so the process-wide observation
    // registry cannot make another test order-dependent.
    config.stream_writers.typing = vec!["master".to_string()];
    let reader = spawn_instance(config.clone(), WorkerType::Frontend, true).await;

    let (status, body) =
        send(&reader, "PUT", "/_matrix/client/v3/rooms/!r:example.org/typing/@u:example.org", "{}").await;

    assert_eq!(status, 503, "an unreachable writer must fail closed, not write locally: {body}");
    assert_eq!(reader.calls(), 0, "the reader must not fall back to executing it itself");

    let report = build_report(&config, true, None);
    let typing = report
        .streams
        .iter()
        .find(|stream| stream.stream == WriteStream::Typing.stream_name())
        .expect("typing must be reported");
    assert_eq!(typing.route, "forward");
    assert_eq!(
        typing.last_outcome,
        Some(LastOutcome::Unreachable),
        "the report must name the failure instead of leaving the operator with a 503 and no cause"
    );
    assert_eq!(
        report.status(),
        "degraded",
        "and `/_health` must degrade rather than report healthy: {}",
        report.summary()
    );
}

// ── 4. the loop break: a forwarded request is executed, not forwarded again ─────────────

#[tokio::test]
async fn the_writer_executes_a_forwarded_request_locally_even_when_it_would_forward_it() {
    // The writer is configured to hand `typing` to an instance that is not running, so:
    //   * a request carrying the envelope must be executed here (loop break), and
    //   * the same request *without* the envelope must try to forward and fail.
    // If the envelope check regressed, two instances that disagree would ping-pong — or,
    // here, the forwarded write would 503 instead of being written.
    let mut config = writer_config();
    config.stream_writers.typing = vec!["elsewhere".to_string()];
    config.instance_map.insert(
        "elsewhere".to_string(),
        InstanceLocationConfig { host: "127.0.0.1".to_string(), port: closed_port().await, tls: false },
    );
    let writer = spawn_instance(config, WorkerType::Master, false).await;

    let forwarded = test_client()
        .put(format!("{}/_matrix/client/v3/rooms/!r:example.org/typing/@u:example.org", writer.base_url()))
        .header(WRITE_FORWARD_STREAM_HEADER, "typing")
        .header(WRITE_FORWARD_SECRET_HEADER, SECRET)
        .body("{}")
        .send()
        .await
        .expect("writer must answer");
    assert_eq!(forwarded.status().as_u16(), 200, "a properly signed forward must be executed locally");
    assert_eq!(writer.calls(), 1);

    let plain = test_client()
        .put(format!("{}/_matrix/client/v3/rooms/!r:example.org/typing/@u:example.org", writer.base_url()))
        .body("{}")
        .send()
        .await
        .expect("writer must answer");
    assert_eq!(
        plain.status().as_u16(),
        503,
        "without the envelope this request is a client write for a stream owned elsewhere ⇒ failed forward"
    );
    assert_eq!(writer.calls(), 1, "and it must not have been executed here");

    // The same envelope with a wrong secret must not be believed: it is an unauthenticated
    // header, and honouring it would let any client suppress forwarding for its own write.
    let forged = test_client()
        .put(format!("{}/_matrix/client/v3/rooms/!r:example.org/typing/@u:example.org", writer.base_url()))
        .header(WRITE_FORWARD_STREAM_HEADER, "typing")
        .header(WRITE_FORWARD_SECRET_HEADER, "wrong-secret-wrong-secret-wrong!")
        .body("{}")
        .send()
        .await
        .expect("writer must answer");
    assert_eq!(forged.status().as_u16(), 503, "a forged envelope must not be honoured");
    assert_eq!(writer.calls(), 1);
}

/// Kept as a compile-time witness that the two header constants are the ones the middleware
/// reads: if either is renamed, this file breaks instead of silently testing nothing.
#[test]
fn the_envelope_headers_are_the_ones_the_middleware_reads() {
    assert_eq!(WRITE_FORWARD_STREAM_HEADER, "x-synapse-write-stream");
    assert_eq!(WRITE_FORWARD_SECRET_HEADER, "x-synapse-worker-secret");
    assert_eq!(WRITE_FORWARD_CLIENT_IP_HEADER, "x-synapse-client-ip");
}
