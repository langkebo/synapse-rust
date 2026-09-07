use axum::Router;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;

use crate::common::config::Config;
use crate::web::middleware::{payload_too_large_json_middleware, request_debug_middleware, request_timeout_middleware};
use crate::web::routes::create_router;
use crate::web::AppState;

/// See [`build_router`].
pub fn build_router(app_state: AppState, config: &Config) -> Router {
    create_router(app_state)
        // G-1: 全局 body 上限读取权威字段 config.server.max_upload_size，
        // media 路由的 DefaultBodyLimit 也从同一字段派生（见 web/routes/media/mod.rs）
        .layer(RequestBodyLimitLayer::new(config.server.max_upload_size as usize))
        .layer(axum::middleware::from_fn(request_debug_middleware))
        .layer(axum::middleware::from_fn(request_timeout_middleware))
        .layer(TraceLayer::new_for_http())
        // ISSUE-07: 最外层兜底——body limit 层产生的裸 413（text/plain）
        // 统一改写为 M_TOO_LARGE JSON，客户端可识别 errcode
        .layer(axum::middleware::from_fn(payload_too_large_json_middleware))
}
