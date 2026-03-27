//! 版本相关处理器

use axum::{routing::get, Json, Router};
use serde_json::json;

/// 获取客户端 API 版本
pub async fn get_client_versions() -> impl axum::response::IntoResponse {
    Json(json!({
        "versions": ["r0", "v1", "v2", "v3"],
        "unstable_features": {
            "org.matrix.msc3861": true,
            "org.matrix.msc3912": true
        }
    }))
}

/// 获取服务端版本
pub async fn get_server_version() -> impl axum::response::IntoResponse {
    Json(json!({
        "name": "synapse-rust",
        "version": "0.1.0",
        "server_time": 1700000000
    }))
}

/// .well-known: Matrix 服务器发现
pub async fn get_well_known_server() -> impl axum::response::IntoResponse {
    Json(json!({
        "m.server": "localhost:8008"
    }))
}

/// .well-known: Matrix 客户端发现
pub async fn get_well_known_client() -> impl axum::response::IntoResponse {
    Json(json!({
        "m.homeserver": {
            "base_url": "http://localhost:8008"
        },
        "m.identity_server": {
            "base_url": "http://localhost:8090"
        }
    }))
}

/// .well-known: Matrix 支持
pub async fn get_well_known_support() -> impl axum::response::IntoResponse {
    Json(json!({
        "url": "https://matrix.org"
    }))
}

/// 创建版本路由
pub fn create_versions_router() -> Router {
    Router::new()
        .route("/_matrix/client/versions", get(get_client_versions))
        .route("/_matrix/client/v3/versions", get(get_client_versions))
        .route("/_matrix/client/r0/version", get(get_server_version))
        .route("/_matrix/server_version", get(get_server_version))
        .route("/.well-known/matrix/server", get(get_well_known_server))
        .route("/.well-known/matrix/client", get(get_well_known_client))
        .route("/.well-known/matrix/support", get(get_well_known_support))
}