use axum::Router;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;

use crate::common::config::Config;
use synapse_web::middleware::{
    http_metrics_middleware, payload_too_large_json_middleware, request_debug_middleware, request_timeout_middleware,
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

    create_router(app_state)
        // G-1: 全局 body 上限读取权威字段 config.server.max_upload_size，
        // media 路由的 DefaultBodyLimit 也从同一字段派生（见 web/routes/media/mod.rs）
        .layer(RequestBodyLimitLayer::new(config.server.max_upload_size as usize))
        // HTTP RED 指标：计数 / 时长直方图 / 错误率 / 在途请求数。
        // 放在 debug/timeout 层之外，使被超时或 413 改写后的响应也能被计入。
        .layer(axum::middleware::from_fn_with_state(server_metrics, http_metrics_middleware))
        .layer(axum::middleware::from_fn(request_debug_middleware))
        .layer(axum::middleware::from_fn(request_timeout_middleware))
        .layer(TraceLayer::new_for_http())
        // ISSUE-07: 最外层兜底——body limit 层产生的裸 413（text/plain）
        // 统一改写为 M_TOO_LARGE JSON，客户端可识别 errcode
        .layer(axum::middleware::from_fn(payload_too_large_json_middleware))
}
