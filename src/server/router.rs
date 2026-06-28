use axum::Router;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;

use crate::common::config::Config;
use crate::web::middleware::{request_debug_middleware, request_timeout_middleware};
use crate::web::routes::create_router;
use crate::web::AppState;

pub fn build_router(app_state: AppState, config: &Config) -> Router {
    create_router(app_state)
        .layer(RequestBodyLimitLayer::new(config.server.max_upload_size as usize))
        .layer(axum::middleware::from_fn(request_debug_middleware))
        .layer(axum::middleware::from_fn(request_timeout_middleware))
        .layer(TraceLayer::new_for_http())
}
