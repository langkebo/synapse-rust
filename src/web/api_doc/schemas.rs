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

// ─────────────────────────────────────────────────────────────────────────────
// P1-2: Profile endpoints  (GET/PUT /v3/profile/{user_id}{,/displayname,/avatar_url})
// ─────────────────────────────────────────────────────────────────────────────

/// Public user profile as returned by `GET /_matrix/client/v3/profile/{user_id}`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiProfileResponse {
    /// The user's display name, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub displayname: Option<String>,
    /// The user's avatar MXC URI, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
}

/// Response body of `GET /_matrix/client/v3/profile/{user_id}/displayname`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiDisplaynameResponse {
    /// The user's display name.
    pub displayname: String,
}

/// Request body for `PUT /_matrix/client/v3/profile/{user_id}/displayname`.
#[derive(utoipa::ToSchema, serde::Deserialize)]
#[allow(dead_code)]
pub struct ApiSetDisplaynameRequest {
    pub displayname: String,
}

/// Response body of `GET /_matrix/client/v3/profile/{user_id}/avatar_url`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAvatarUrlResponse {
    /// The user's avatar MXC URI.
    pub avatar_url: String,
}

/// Request body for `PUT /_matrix/client/v3/profile/{user_id}/avatar_url`.
#[derive(utoipa::ToSchema, serde::Deserialize)]
#[allow(dead_code)]
pub struct ApiSetAvatarUrlRequest {
    /// The new avatar MXC URI.
    pub avatar_url: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// P1-3: Presence endpoints  (GET /v3/presence/{user_id}/status, /v3/presence/list)
// ─────────────────────────────────────────────────────────────────────────────

/// User presence state.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiPresenceState {
    /// One of: offline, online, unavailable, do_not_disturb.
    pub presence: String,
    /// User-defined status message, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_msg: Option<String>,
    /// Milliseconds since the user was last active.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_active_ago: Option<i64>,
    /// Whether the user is currently active.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currently_active: Option<bool>,
}

/// Response body of `GET /_matrix/client/v3/presence/{user_id}/status`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiPresenceStatusResponse {
    pub presence: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_msg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_active_ago: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currently_active: Option<bool>,
}

/// Individual entry in a presence list.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiPresenceListEntry {
    pub user_id: String,
    pub presence: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_msg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_active_ago: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currently_active: Option<bool>,
}

/// Response body of `GET /_matrix/client/v3/presence/list` and `/list/{user_id}`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiPresenceListResponse {
    pub presences: Vec<ApiPresenceListEntry>,
}

/// Dehydrated device status returned by MSC3886.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiDehydratedDeviceStatus {
    /// The dehydrated device ID.
    pub device_id: String,
    /// Device-specific data (serialized JSON object).
    pub device_data: serde_json::Value,
}

/// MatrixRTC ice-server transport entry.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiRtcIceServer {
    /// ICE server URLs (STUN/TURN).
    pub urls: Vec<String>,
    /// Optional TURN username.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    /// Optional TURN credential.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential: Option<String>,
}

/// MatrixRTC transport entry (MSC4403).
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiRtcTransport {
    /// Transport type identifier.
    #[serde(rename = "type")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transport_type: Option<String>,
    /// List of ICE servers for this transport.
    pub ice_servers: Vec<ApiRtcIceServer>,
}

/// Response body of `GET /_matrix/client/unstable/org.matrix.msc4143/rtc/transports`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiRtcTransportsResponse {
    pub transports: Vec<ApiRtcTransport>,
}
