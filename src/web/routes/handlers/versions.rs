//! 版本相关处理器

use crate::web::AppState;
use axum::{extract::State, routing::get, Json, Router};
use serde_json::json;

/// 获取客户端 API 版本
pub async fn get_client_versions() -> impl axum::response::IntoResponse {
    Json(json!({
        "versions": ["r0.5.0", "r0.6.0", "v1.1", "v1.2", "v1.3", "v1.4", "v1.5", "v1.6"],
        "unstable_features": {
            "m.lazy_load_members": true,
            "m.require_identity_server": false,
            "m.supports_login_via_phone_number": true,
            "org.matrix.msc3882": true,
            "org.matrix.msc3983": true,
            "org.matrix.msc3245": true,
            "org.matrix.msc3266": true
        }
    }))
}

/// 获取服务端版本
pub async fn get_server_version(
    State(state): State<AppState>,
) -> impl axum::response::IntoResponse {
    Json(json!({
        "server_version": env!("CARGO_PKG_VERSION"),
        "python_version": "Rust",
        "server_name": state.services.config.server.name
    }))
}

/// .well-known: Matrix 服务器发现
pub async fn get_well_known_server(State(state): State<AppState>) -> Json<serde_json::Value> {
    let server_name = &state.services.config.server.name;
    let port = state.services.config.server.port;
    Json(json!({
        "m.server": format!("{}:{}", server_name, port)
    }))
}

/// .well-known: Matrix 客户端发现
pub async fn get_well_known_client(State(state): State<AppState>) -> Json<serde_json::Value> {
    let base_url = state.services.config.server.get_public_baseurl();
    Json(json!({
        "m.homeserver": {
            "base_url": base_url
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

/// 获取服务端能力
pub async fn get_capabilities() -> impl axum::response::IntoResponse {
    Json(json!({
        "capabilities": {
            "m.change_password": { "enabled": true },
            "m.room_versions": {
                "default": "6",
                "available": {
                    "1": "stable", "2": "stable", "3": "stable",
                    "4": "stable", "5": "stable", "6": "stable",
                    "7": "stable", "8": "stable", "9": "stable",
                    "10": "stable", "11": "stable"
                }
            },
            "m.set_displayname": { "enabled": true },
            "m.set_avatar_url": { "enabled": true },
            "m.3pid_changes": { "enabled": true },
            "m.room.summary": { "enabled": true },
            "m.room.suggested": { "enabled": true }
        }
    }))
}

/// 创建版本路由
pub fn create_versions_router() -> Router<AppState> {
    Router::new()
        .route("/_matrix/client/versions", get(get_client_versions))
        .route("/_matrix/client/v3/versions", get(get_client_versions))
        .route("/_matrix/client/r0/version", get(get_server_version))
        .route("/_matrix/server_version", get(get_server_version))
        .route("/.well-known/matrix/server", get(get_well_known_server))
        .route("/.well-known/matrix/client", get(get_well_known_client))
        .route("/.well-known/matrix/support", get(get_well_known_support))
        .route("/_matrix/client/v3/capabilities", get(get_capabilities))
        .route("/_matrix/client/r0/capabilities", get(get_capabilities))
}
