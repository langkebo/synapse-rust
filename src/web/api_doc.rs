//! OpenAPI / Swagger documentation for the Synapse-Rust Matrix homeserver.
//!
//! Enabled via the `openapi-docs` feature flag. When enabled, the Swagger UI
//! is served at `/_swagger` and the OpenAPI JSON schema at `/_api-doc/openapi.json`.
//!
//! Route annotation is progressive — only the health, versions, and well-known
//! endpoints are annotated as canonical examples. Additional routes should be
//! annotated incrementally through follow-up patches.

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
            get_well_known_server,
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