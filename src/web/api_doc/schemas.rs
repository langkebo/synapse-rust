#![cfg(feature = "openapi-docs")]

//! Shared OpenAPI schema definitions.
//!
//! P3-8: prefer `utoipa::ToSchema` derive over inline `body = serde_json::Value`.
//! Add a typed struct here whenever a new response shape needs OpenAPI coverage;
//! the corresponding `#[utoipa::path]` in `client_server.rs` / `admin.rs` / etc.
//! can then reference the named type instead of an untyped `Value`.

use std::collections::HashMap;

/// Result of a single health-check component.
#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
pub struct ApiHealthCheckResult {
    status: String,
    message: String,
    duration_ms: u64,
}

/// Composite health status returned by the detailed health endpoint.
#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
pub struct ApiHealthStatus {
    status: String,
    version: String,
    timestamp: i64,
    checks: HashMap<String, ApiHealthCheckResult>,
}

/// Pusher descriptor as returned by `GET /_matrix/client/v3/pushers`.
///
/// The wire shape is intentionally open (`data` is an arbitrary JSON object
/// because Matrix lets clients store arbitrary backend-specific configuration),
/// so we keep `data` as `serde_json::Value` and document only the fixed fields.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiPusher {
    /// Required when `kind` is not null. The pushkey for this pusher.
    pub pushkey: String,
    /// Required when `kind` is not null. The application ID for this pusher.
    pub app_id: String,
    /// The kind of pusher. `null` means delete the pusher for the (app_id, pushkey) pair.
    /// `http` is the only kind currently defined by the spec; servers should ignore
    /// unknown kinds.
    pub kind: Option<String>,
    /// A human-readable display name for the pusher.
    pub app_display_name: Option<String>,
    /// A human-readable display name for the device.
    pub device_display_name: Option<String>,
    /// The profile tag of the session that created the pusher.
    pub profile_tag: Option<String>,
    /// The preferred language for receiving push notifications.
    pub lang: String,
    /// Arbitrary backend-specific data. For `http` kind, includes the URL to POST to.
    pub data: serde_json::Value,
}

/// Response body of `GET /_matrix/client/v3/pushers`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiPushersResponse {
    /// The list of pushers for the authenticated user/device.
    pub pushers: Vec<ApiPusher>,
}
