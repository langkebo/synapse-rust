#![cfg(feature = "openapi-docs")]

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

/// `GET /_synapse/admin/v1/health` — Read admin health probe output.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/health",
    tag = "Admin",
    responses(
        (status = 200, description = "Admin health probe",
            body = serde_json::Value,
            example = json!({
                "status": "ok",
                "database": "ok"
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_health_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/versions` — Compatibility alias for supported Matrix protocol versions.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/versions",
    tag = "Health",
    responses(
        (status = 200, description = "Supported Matrix protocol versions",
            body = serde_json::Value,
            example = json!({
                "versions": ["v1.1", "v1.2", "v1.3", "v1.4", "v1.5", "v1.6", "v1.7", "v1.8", "v1.9", "v1.10", "v1.11", "v1.12", "v1.13"],
                "unstable_features": {}
            })
        )
    )
)]
pub fn get_client_versions_v3_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/rooms/{room_id}/widgets/{widget_id}/capabilities` — Get widget capabilities.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/rooms/{room_id}/widgets/{widget_id}/capabilities",
    tag = "Private Extension - Widget",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
        ("widget_id" = String, Path, description = "The ID of the widget"),
    ),
    responses(
        (status = 200, description = "Widget capabilities", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_room_widget_capabilities_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v3/rooms/{room_id}/widgets/{widget_id}/capabilities` — Set widget capabilities.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v3/rooms/{room_id}/widgets/{widget_id}/capabilities",
    tag = "Private Extension - Widget",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
        ("widget_id" = String, Path, description = "The ID of the widget"),
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Capabilities set", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn set_room_widget_capabilities_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/admin/v1/external_services/{as_id}/health` — Get external service health.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/admin/v1/external_services/{as_id}/health",
    tag = "Private Extension - External Services",
    params(
        ("as_id" = String, Path, description = "The ID of the service"),
    ),
    responses(
        (status = 200, description = "Health status", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_external_service_health_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/external_services/{as_id}/health` — Check external service health (client endpoint).
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/external_services/{as_id}/health",
    tag = "Private Extension - External Services",
    params(
        ("as_id" = String, Path, description = "The ID of the service"),
    ),
    responses(
        (status = 200, description = "Health check", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn check_service_health_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}
