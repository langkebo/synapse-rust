//! OpenAPI / Swagger documentation for the Synapse-Rust Matrix homeserver.
//!
//! Enabled via the `openapi-docs` feature flag. When enabled, the Swagger UI
//! is served at `/_swagger` and the OpenAPI JSON schema at `/_api-doc/openapi.json`.
//!
//! Route annotation is progressive — health, versions, capabilities, and
//! well-known endpoints are annotated as canonical examples. Additional routes
//! should be annotated incrementally through follow-up patches.

use crate::web::routes::AppState;

/// Build the Swagger UI router for the given OpenAPI schema.
///
/// The UI is mounted at `/_swagger` with a redirect from `/_swagger/` for
/// convenience. The raw OpenAPI JSON is served at `/_api-doc/openapi.json`.
#[cfg(feature = "openapi-docs")]
pub fn swagger_ui_router(_state: AppState) -> axum::Router<AppState> {
    use utoipa::OpenApi;
    use utoipa_swagger_ui::SwaggerUi;

    #[derive(OpenApi)]
    #[openapi(
        info(
            title = "Synapse-Rust Matrix Homeserver API",
            version = env!("CARGO_PKG_VERSION"),
            description = "Matrix Client-Server API implementation in Rust. \
                Compliant with Matrix Spec v1.13."
        ),
        servers(
            (url = "/", description = "Local Synapse-Rust instance"),
        ),
        tags(
            (name = "Health", description = "Server health and version endpoints"),
            (name = "Authentication", description = "Login, registration, and token management"),
            (name = "Client-Server", description = "Matrix Client-Server API (v3)"),
            (name = "Admin", description = "Server administration endpoints"),
            (name = "Federation", description = "Server-to-server federation API"),
        ),
        paths(
            health_check,
            detailed_health_check,
            get_client_versions,
            get_server_version,
            get_capabilities,
            get_well_known_server,
            get_well_known_client,
            get_well_known_support,
            list_account_data,
            get_account_data,
            get_room_account_data,
            get_filter,
            get_pushers,
            get_push_rules,
            get_push_rules_scope,
            get_push_rules_kind,
            get_push_rule,
            get_devices,
            get_device,
            get_global_tags,
            get_room_tags,
        ),
        components(
            schemas(
                crate::common::health::HealthStatus,
                crate::common::health::CheckResult,
            ),
        ),
    )]
    struct ApiDoc;

    let openapi = ApiDoc::openapi();

    axum::Router::new()
        .merge(SwaggerUi::new("/_swagger").url("/_api-doc/openapi.json", openapi))
        .with_state(_state)
}

/// Stub for when `openapi-docs` is not enabled.
#[cfg(not(feature = "openapi-docs"))]
pub fn swagger_ui_router(_state: AppState) -> axum::Router<AppState> {
    axum::Router::new()
}

// ==========================
// Path annotations (canonical examples)
// ==========================

/// `GET /_matrix/client/versions` — Return supported Matrix protocol versions.
#[cfg(feature = "openapi-docs")]
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
#[cfg(feature = "openapi-docs")]
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
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_health",
    tag = "Health",
    responses(
        (status = 200, description = "Detailed health status",
            body = crate::common::health::HealthStatus,
        ),
    ),
)]
pub fn detailed_health_check() -> axum::Json<crate::common::health::HealthStatus> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /.well-known/matrix/server` — Server discovery.
#[cfg(feature = "openapi-docs")]
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
#[cfg(feature = "openapi-docs")]
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
#[cfg(feature = "openapi-docs")]
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
                    "m.room_versions": {
                        "default": "11",
                        "available": {
                            "10": "stable",
                            "11": "stable"
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
#[cfg(feature = "openapi-docs")]
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
#[cfg(feature = "openapi-docs")]
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

/// `GET /_matrix/client/v3/user/{user_id}/account_data/` — List account data for the authenticated user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/user/{user_id}/account_data/",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Authenticated Matrix user ID")
    ),
    responses(
        (status = 200, description = "Account data map",
            body = serde_json::Value,
            example = json!({
                "account_data": {
                    "m.push_rules": {
                        "global": {
                            "override": []
                        }
                    }
                }
            })
        ),
    ),
)]
pub fn list_account_data() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/user/{user_id}/account_data/{type}` — Read one account data event.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/user/{user_id}/account_data/{type}",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Authenticated Matrix user ID"),
        ("type" = String, Path, description = "Account data event type")
    ),
    responses(
        (status = 200, description = "Account data content", body = serde_json::Value),
        (status = 404, description = "Account data not found")
    ),
)]
pub fn get_account_data() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/user/{user_id}/rooms/{room_id}/account_data/{type}` — Read one room-scoped account data event.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/user/{user_id}/rooms/{room_id}/account_data/{type}",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Authenticated Matrix user ID"),
        ("room_id" = String, Path, description = "Matrix room ID"),
        ("type" = String, Path, description = "Room account data event type")
    ),
    responses(
        (status = 200, description = "Room account data content", body = serde_json::Value),
        (status = 404, description = "Room account data not found")
    ),
)]
pub fn get_room_account_data() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/user/{user_id}/filter/{filter_id}` — Read one saved sync filter.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/user/{user_id}/filter/{filter_id}",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Authenticated Matrix user ID"),
        ("filter_id" = String, Path, description = "Filter ID")
    ),
    responses(
        (status = 200, description = "Saved filter document", body = serde_json::Value),
        (status = 404, description = "Filter not found")
    ),
)]
pub fn get_filter() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/pushers` — List pushers for the authenticated device.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/pushers",
    tag = "Client-Server",
    responses(
        (status = 200, description = "Pushers",
            body = serde_json::Value,
            example = json!({
                "pushers": [{
                    "pushkey": "push-key",
                    "kind": "http",
                    "app_id": "com.example.app"
                }]
            })
        ),
    ),
)]
pub fn get_pushers() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/pushrules` — Read all push rules.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/pushrules",
    tag = "Client-Server",
    responses(
        (status = 200, description = "All push rules", body = serde_json::Value)
    ),
)]
pub fn get_push_rules() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/pushrules/{scope}` — Read one push rule scope.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/pushrules/{scope}",
    tag = "Client-Server",
    params(
        ("scope" = String, Path, description = "Push rule scope, for example global")
    ),
    responses(
        (status = 200, description = "Push rule scope", body = serde_json::Value)
    ),
)]
pub fn get_push_rules_scope() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/pushrules/{scope}/{kind}` — Read rules of one kind within a scope.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/pushrules/{scope}/{kind}",
    tag = "Client-Server",
    params(
        ("scope" = String, Path, description = "Push rule scope"),
        ("kind" = String, Path, description = "Push rule kind")
    ),
    responses(
        (status = 200, description = "Push rule kind listing", body = serde_json::Value)
    ),
)]
pub fn get_push_rules_kind() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/pushrules/{scope}/{kind}/{rule_id}` — Read one push rule.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/pushrules/{scope}/{kind}/{rule_id}",
    tag = "Client-Server",
    params(
        ("scope" = String, Path, description = "Push rule scope"),
        ("kind" = String, Path, description = "Push rule kind"),
        ("rule_id" = String, Path, description = "Push rule ID")
    ),
    responses(
        (status = 200, description = "Push rule", body = serde_json::Value),
        (status = 404, description = "Push rule not found")
    ),
)]
pub fn get_push_rule() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/devices` — List devices for the authenticated user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/devices",
    tag = "Client-Server",
    responses(
        (status = 200, description = "User devices",
            body = serde_json::Value,
            example = json!({
                "devices": [{
                    "device_id": "DEVICEID",
                    "display_name": "Primary phone",
                    "last_seen_ts": 1718000000000_i64,
                    "last_seen_ip": "203.0.113.10"
                }]
            })
        ),
    ),
)]
pub fn get_devices() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/devices/{device_id}` — Read one device.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/devices/{device_id}",
    tag = "Client-Server",
    params(
        ("device_id" = String, Path, description = "Matrix device ID")
    ),
    responses(
        (status = 200, description = "Device details",
            body = serde_json::Value,
            example = json!({
                "device": {
                    "device_id": "DEVICEID",
                    "display_name": "Primary phone",
                    "last_seen_ts": 1718000000000_i64
                },
                "device_id": "DEVICEID",
                "display_name": "Primary phone",
                "last_seen_ts": 1718000000000_i64
            })
        ),
        (status = 404, description = "Device not found")
    ),
)]
pub fn get_device() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/user/{user_id}/tags` — List all tags grouped by room for the authenticated user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/user/{user_id}/tags",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Authenticated Matrix user ID")
    ),
    responses(
        (status = 200, description = "All room tags for the user",
            body = serde_json::Value,
            example = json!({
                "tags": {
                    "!room:example.com": {
                        "m.favourite": {
                            "order": 0.5
                        }
                    }
                }
            })
        ),
        (status = 403, description = "Access denied")
    ),
)]
pub fn get_global_tags() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/user/{user_id}/rooms/{room_id}/tags` — List tags for one room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/user/{user_id}/rooms/{room_id}/tags",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Authenticated Matrix user ID"),
        ("room_id" = String, Path, description = "Matrix room ID")
    ),
    responses(
        (status = 200, description = "Room tags",
            body = serde_json::Value,
            example = json!({
                "tags": {
                    "m.favourite": {
                        "order": 0.5
                    },
                    "u.work": {
                        "order": 1.0
                    }
                }
            })
        ),
        (status = 403, description = "Access denied")
    ),
)]
pub fn get_room_tags() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}
