//! OpenAPI / Swagger documentation for the Synapse-Rust Matrix homeserver.
//!
//! Enabled via the `openapi-docs` feature flag. When enabled, the Swagger UI
//! is served at `/_swagger` and the OpenAPI JSON schema at `/_api-doc/openapi.json`.
//!
//! Route annotation is progressive — health, versions, capabilities, and
//! well-known endpoints are annotated as canonical examples. Additional routes
//! should be annotated incrementally through follow-up patches.

pub mod admin;
pub mod auth;
pub mod client_server;
pub mod federation;
pub mod health;
pub mod schemas;

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
        paths(),
    )]
    struct ApiDoc;

    let openapi = ApiDoc::openapi();

    axum::Router::new().merge(SwaggerUi::new("/_swagger").url("/_api-doc/openapi.json", openapi)).with_state(_state)
}

/// Stub for when `openapi-docs` is not enabled.
#[cfg(not(feature = "openapi-docs"))]
pub fn swagger_ui_router(_state: AppState) -> axum::Router<AppState> {
    axum::Router::new()
}
