#![cfg(feature = "openapi-docs")]

//! Health, version, capabilities, and well-known endpoint annotations.

use super::schemas::*;

/// `GET /_matrix/client/versions` — Return supported Matrix protocol versions.
#[utoipa::path(
    get,
    path = "/_matrix/client/versions",
    tag = "Health",
    responses(
        (status = 200, description = "Supported Matrix protocol versions",
            body = serde_json::Value,
            example = json!({
                "versions": ["r0.0.1", "r0.1.0", "r0.2.0", "r0.3.0", "r0.4.0",
                    "r0.5.0", "r0.6.0", "r0.6.1", "v1.1", "v1.2", "v1.3", "v1.4",
                    "v1.5", "v1.6", "v1.7", "v1.8", "v1.9", "v1.10", "v1.11", "v1.12", "v1.13"
                ],
                "unstable_features": {}
            })
        ),
    ),
)]
pub fn get_client_versions() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /health` — Basic liveness check.
#[utoipa::path(
    get,
    path = "/health",
    tag = "Health",
    responses(
        (status = 200, description = "Service is alive",
            body = serde_json::Value,
            example = json!({"status": "ok"})
        ),
    ),
)]
pub fn health_check() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_health` — Detailed health check with component statuses.
#[utoipa::path(
    get,
    path = "/_health",
    tag = "Health",
    responses(
        (status = 200, description = "Detailed health status",
            body = ApiHealthStatus,
        ),
    ),
)]
pub fn detailed_health_check() -> axum::Json<ApiHealthStatus> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /.well-known/matrix/server` — Server discovery.
#[utoipa::path(
    get,
    path = "/.well-known/matrix/server",
    tag = "Health",
    responses(
        (status = 200, description = "Server well-known information",
            body = serde_json::Value,
            example = json!({"m.server": "matrix.example.com:443"})
        ),
    ),
)]
pub fn get_well_known_server() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/server_version` — Return homeserver version metadata.
#[utoipa::path(
    get,
    path = "/_matrix/server_version",
    tag = "Health",
    responses(
        (status = 200, description = "Homeserver version metadata",
            body = serde_json::Value,
            example = json!({
                "server_version": "6.0.4",
                "python_version": "Rust",
                "server_name": "example.com"
            })
        ),
    ),
)]
pub fn get_server_version() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/capabilities` — Return client capability surface.
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/capabilities",
    tag = "Client-Server",
    responses(
        (status = 200, description = "Client capabilities",
            body = serde_json::Value,
            example = json!({
                "capabilities": {
                    "m.change_password": { "enabled": true },
                    "m.set_displayname": { "enabled": true },
                    "m.set_avatar_url": { "enabled": true },
                    "m.3pid_changes": { "enabled": true },
                    "m.room_versions": {
                        "default": "10",
                        "available": {
                            "1": "stable",
                            "2": "stable",
                            "3": "stable",
                            "4": "stable",
                            "5": "stable",
                            "6": "stable",
                            "7": "stable",
                            "8": "stable",
                            "9": "stable",
                            "10": "stable",
                            "11": "stable",
                            "12": "stable",
                            "13": "stable"
                        }
                    }
                },
                "unstable_features": {
                    "org.matrix.msc3886.sliding_sync": true
                }
            })
        ),
    ),
)]
pub fn get_capabilities() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /.well-known/matrix/client` — Client discovery.
#[utoipa::path(
    get,
    path = "/.well-known/matrix/client",
    tag = "Health",
    responses(
        (status = 200, description = "Client well-known information",
            body = serde_json::Value,
            example = json!({
                "m.homeserver": {
                    "base_url": "https://matrix.example.com"
                }
            })
        ),
    ),
)]
pub fn get_well_known_client() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /.well-known/matrix/support` — Support discovery.
#[utoipa::path(
    get,
    path = "/.well-known/matrix/support",
    tag = "Health",
    responses(
        (status = 200, description = "Support metadata",
            body = serde_json::Value,
            example = json!({
                "url": "https://matrix.org"
            })
        ),
    ),
)]
pub fn get_well_known_support() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}
