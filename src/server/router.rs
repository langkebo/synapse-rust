use axum::Router;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;

use crate::common::config::Config;
use synapse_services::worker::topology_validator::current_instance_worker_type;
use synapse_services::worker::types::WorkerType;
use synapse_web::middleware::{
    http_metrics_middleware, payload_too_large_json_middleware, request_debug_middleware, request_timeout_middleware,
    worker_surface_middleware, write_forwarding_middleware, RequestTimeouts, WorkerSurfaceGate, WriteForwarding,
};
use synapse_web::routes::create_router;
use synapse_web::AppState;

/// See [`build_router`].
pub fn build_router(app_state: AppState, config: &Config) -> Router {
    // Captured before `create_router` consumes `app_state`. This is the handle the
    // global HTTP metrics middleware records through — without it
    // `http_requests_total` / `http_request_errors_total` / `http_active_requests`
    // / `http_request_duration_ms` never move and every HTTP alert stays dead.
    let server_metrics = app_state.services.core.server_metrics.clone();

    // `request_timeout_middleware` enforces these budgets; they are configuration
    // (`server.request_timeout_secs` / `server.long_poll_request_timeout_secs`) with
    // the documented defaults applied here, in the one place that assembles the
    // server. They used to be undocumented environment variables read per request.
    let request_timeouts = RequestTimeouts {
        default_secs: config
            .server
            .request_timeout_secs
            .unwrap_or(synapse_common::config::server::DEFAULT_REQUEST_TIMEOUT_SECS),
        long_poll_secs: config
            .server
            .long_poll_request_timeout_secs
            .unwrap_or(synapse_common::config::server::DEFAULT_LONG_POLL_REQUEST_TIMEOUT_SECS),
    };

    // W-F: make the worker type's declared route prefixes executable.
    //
    // `surface_gate` off (the default) resolves to `Master`, whose predicate
    // short-circuits to "owns everything" — so the single-process surface is
    // unchanged and pays one comparison per request. The layer is added
    // unconditionally (rather than conditionally) so there is exactly one
    // assembly path to reason about; the predicate, not the wiring, decides.
    let effective_role = if config.worker.surface_gate {
        let role = current_instance_worker_type(&config.worker);
        if role != WorkerType::Master {
            ::tracing::info!(
                worker_type = role.as_str(),
                "Enforcing the worker HTTP surface gate (worker.surface_gate = true): \
                 this instance answers 404 for paths its role does not own"
            );
        }
        role
    } else {
        WorkerType::Master
    };
    let surface_gate = WorkerSurfaceGate::new(effective_role);

    // W-I: make `worker.stream_writers` executable.
    //
    // Built once (the policy resolves `instance_map` targets and reads the replication
    // secret file), and installed **inside** the surface gate on purpose: the gate
    // narrows what this instance answers, so a request outside its role is still a 404
    // and is never forwarded. Forwarding only moves a request between two instances that
    // are both allowed to serve it.
    //
    // Off by default (`worker.write_forwarding: false`), in which case the policy reports
    // itself disabled and the layer is a single boolean check per request.
    //
    // The body limit comes from the same field `RequestBodyLimitLayer` uses: a forward
    // limit below it would silently become a *new* write limit that only appears once
    // traffic is forwarded.
    let write_forwarding = WriteForwarding::from_config(&config.worker, config.server.max_upload_size);

    // The rate limiter runs *outside* every layer here, so it cannot see the policy
    // object; it reads the resolved secret instead, which is what lets a forwarded write
    // keep its original client address in the limiter's key
    // (`write_forwarding::forwarded_client_ip`). Process-wide on purpose: one router per
    // process.
    synapse_web::middleware::write_forwarding::install_process_secret(write_forwarding.secret().map(str::to_string));

    // The surface gate must also admit the writes this instance owns as a configured
    // stream writer, otherwise W-F and W-I cancel each other out: forwarding a room send
    // to `event_persister` would reach a process whose role owns only
    // `/_synapse/worker/v1/replication/*`, and the gate would answer 404 at the person it
    // was forwarded to. The grant is empty unless forwarding is on, so the default
    // deployment's surface is unchanged.
    let surface_gate = surface_gate.with_stream_writes(write_forwarding.locally_written_streams());

    create_router(app_state)
        // Innermost layer: a write for a stream owned by another instance is handed
        // there before the local handler can execute it.
        .layer(axum::middleware::from_fn_with_state(write_forwarding, write_forwarding_middleware))
        // Innermost layer: a refused path still passes through the HTTP metrics,
        // timeout and 413-rewriting layers outside it, so refusals are counted
        // and never masquerade as infrastructure errors.
        .layer(axum::middleware::from_fn_with_state(surface_gate, worker_surface_middleware))
        // G-1: 全局 body 上限读取权威字段 config.server.max_upload_size，
        // media 路由的 DefaultBodyLimit 也从同一字段派生（见 web/routes/media/mod.rs）
        .layer(RequestBodyLimitLayer::new(config.server.max_upload_size as usize))
        // HTTP RED 指标：计数 / 时长直方图 / 错误率 / 在途请求数。
        // 放在 debug/timeout 层之外，使被超时或 413 改写后的响应也能被计入。
        .layer(axum::middleware::from_fn_with_state(server_metrics, http_metrics_middleware))
        .layer(axum::middleware::from_fn(request_debug_middleware))
        .layer(axum::middleware::from_fn_with_state(request_timeouts, request_timeout_middleware))
        .layer(TraceLayer::new_for_http())
        // ISSUE-07: 最外层兜底——body limit 层产生的裸 413（text/plain）
        // 统一改写为 M_TOO_LARGE JSON，客户端可识别 errcode
        .layer(axum::middleware::from_fn(payload_too_large_json_middleware))
}
