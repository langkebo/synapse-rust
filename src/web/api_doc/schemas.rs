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

// ─────────────────────────────────────────────────────────────────────────────
// P1-1: Device endpoints  (GET/DELETE /v3/devices, PUT /v3/devices/{device_id},
//                           POST /v3/delete_devices)
// ─────────────────────────────────────────────────────────────────────────────

/// Individual device descriptor returned by `GET /_matrix/client/v3/devices`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiDevice {
    /// Identifier for this device.
    pub device_id: String,
    /// Human-readable display name for the device.
    pub display_name: Option<String>,
    /// Unix timestamp (ms) when this device was last seen.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_seen_ts: Option<i64>,
    /// The IP address the device was last seen from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_seen_ip: Option<String>,
}

/// Response body of `GET /_matrix/client/v3/devices`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiDeviceListResponse {
    /// The authenticated user's active devices.
    pub devices: Vec<ApiDevice>,
}

/// Full device detail returned by `GET /_matrix/client/v3/devices/{device_id}`.
///
/// Matrix spec duplicates fields at the top level; we follow the spec shape.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiDeviceDetailResponse {
    /// Nested device object (present in the spec response).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<ApiDevice>,
    /// Device identifier.
    pub device_id: String,
    /// Human-readable display name for the device.
    pub display_name: Option<String>,
    /// Unix timestamp (ms) when this device was last seen.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_seen_ts: Option<i64>,
}

/// Request body for `PUT /_matrix/client/v3/devices/{device_id}`.
#[derive(utoipa::ToSchema, serde::Deserialize)]
#[allow(dead_code)]
pub struct ApiUpdateDeviceRequest {
    /// New human-readable display name for the device.
    pub display_name: Option<String>,
}

/// Response body of `PUT /_matrix/client/v3/devices/{device_id}`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiUpdateDeviceResponse {
    pub device_id: String,
    pub display_name: Option<String>,
    /// Unix timestamp (ms) of the update.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_ts: Option<i64>,
}

/// Empty JSON object returned on successful DELETE.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiEmptyResponse {}

/// Request body for `POST /_matrix/client/v3/delete_devices`.
#[derive(utoipa::ToSchema, serde::Deserialize)]
#[allow(dead_code)]
pub struct ApiDeleteDevicesRequest {
    /// List of device IDs to delete.
    pub device_ids: Vec<String>,
}
