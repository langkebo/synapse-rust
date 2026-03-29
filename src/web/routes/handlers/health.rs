//! 健康检查和根路由处理器

use axum::{response::IntoResponse, routing::get, Json, Router};
use serde_json::json;

/// 根路由处理器
pub async fn root_handler() -> impl IntoResponse {
    Json(json!({
        "msg": "Synapse Rust Matrix Server",
        "version": "0.1.0"
    }))
}

/// 健康检查处理器
pub async fn health_check() -> impl IntoResponse {
    Json(json!({
        "status": "ok",
        "timestamp": chrono::Utc::now().to_rfc3339()
    }))
}

/// 创建基础健康检查路由
pub fn create_health_router() -> Router {
    Router::new()
        .route("/", get(root_handler))
        .route("/health", get(health_check))
}
