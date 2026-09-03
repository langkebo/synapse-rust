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

// ─────────────────────────────────────────────────────────────────────────────
// P2-Media: /v3/media/config
// ─────────────────────────────────────────────────────────────────────────────

/// Media configuration returned by `GET /_matrix/client/v3/media/config`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiMediaConfig {
    /// `m.upload.size` — maximum upload size in bytes.
    #[serde(rename = "m.upload.size")]
    pub m_upload_size: i64,
}

// ─────────────────────────────────────────────────────────────────────────────
// P2-Room core: list/joined/create/join/leave/forget/invite/send
// ─────────────────────────────────────────────────────────────────────────────

/// Response body of `GET /_matrix/client/v3/joined_rooms`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiJoinedRoomsResponse {
    pub joined_rooms: Vec<String>,
}

/// Public room directory entry returned by `GET /v3/publicRooms`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiPublicRoomsChunkEntry {
    pub room_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canonical_alias: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_joined_members: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub world_readable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guest_can_join: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
}

/// Response body of `GET /v3/publicRooms`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiPublicRoomsResponse {
    pub chunk: Vec<ApiPublicRoomsChunkEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_batch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prev_batch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_room_count_estimate: Option<i32>,
}

/// Request body for `POST /v3/createRoom`.
#[derive(utoipa::ToSchema, serde::Deserialize)]
#[allow(dead_code)]
pub struct ApiCreateRoomRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub room_alias_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invite: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub power_level_content_override: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initial_state: Option<Vec<serde_json::Value>>,
}

/// Response body of `POST /v3/createRoom`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiCreateRoomResponse {
    pub room_id: String,
}

/// Response body of `PUT /v3/rooms/{room_id}/send/{event_type}/{txn_id}`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiSendEventResponse {
    pub event_id: String,
}

/// Response body of `POST /v3/rooms/{room_id}/join` and `/leave` and `/forget`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiRoomIdResponse {
    pub room_id: String,
}

/// Response body of `POST /v3/rooms/{room_id}/forget`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiForgetRoomResponse {
    pub room_id: String,
    pub is_forgotten: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_ts: Option<i64>,
}

/// Response body of `POST /v3/rooms/{room_id}/invite`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiInviteResponse {
    pub room_id: String,
    pub invited_user_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invited_ts: Option<i64>,
}

/// Response body of `GET /v3/rooms/{room_id}/joined_members`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiJoinedMembersResponse {
    pub joined: serde_json::Value,
}

// ─────────────────────────────────────────────────────────────────────────────
// P2-User directory
// ─────────────────────────────────────────────────────────────────────────────

/// Response body of `GET /v3/user_directory/profiles/{user_id}`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiUserDirectoryProfileResponse {
    pub user_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub displayname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
}

/// Response body of `POST /v3/user_directory/search`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiUserDirectorySearchResponse {
    pub limited: bool,
    pub results: Vec<ApiUserDirectoryProfileResponse>,
}

/// Response body of `GET/PUT /v3/directory/list/room/{room_id}`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiRoomVisibilityResponse {
    pub visibility: String,
}

/// Response body of `PUT /v3/directory/list/room/{room_id}`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiRoomVisibilityUpdateResponse {
    pub room_id: String,
    pub visibility: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// P2-Room alias + sync/events/my_rooms + relations + search
// ─────────────────────────────────────────────────────────────────────────────

/// Response body of `PUT /v3/directory/room/{room_alias}`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiRoomAliasCreatedResponse {
    pub room_id: String,
    pub alias: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_ts: Option<i64>,
}

/// Response body of `DELETE /v3/directory/room/{room_alias}`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiRoomAliasRemovedResponse {
    pub removed: bool,
    pub alias: String,
}

/// Response body of `GET /r0/directory/room/{room_id}/alias`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiRoomAliasesResponse {
    pub aliases: Vec<String>,
}

/// Response body of `POST /v3/publicRooms` (alias of GET).
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiQueryPublicRoomsResponse {
    pub chunk: Vec<ApiPublicRoomsChunkEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_batch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_room_count_estimate: Option<i32>,
}

/// Per-room summary returned by `GET /v3/my_rooms`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiMyRoomEntry {
    pub room_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub join_state: Option<String>,
}

/// Response body of `GET /v3/my_rooms`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiMyRoomsResponse {
    pub rooms: Vec<ApiMyRoomEntry>,
    pub total: i32,
}

/// Request body for `GET /v3/search` (POST with body).
#[derive(utoipa::ToSchema, serde::Deserialize)]
#[allow(dead_code)]
pub struct ApiSearchRequest {
    pub search_categories: serde_json::Value,
}

/// Response body of `POST /v3/search`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiSearchResponse {
    pub search_categories: serde_json::Value,
}

/// Request body for `POST /v3/rooms/{room_id}/report`.
#[derive(utoipa::ToSchema, serde::Deserialize)]
#[allow(dead_code)]
pub struct ApiRoomReportRequest {
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<i32>,
}

/// Response body of `POST /v3/rooms/{room_id}/report`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiReportAcceptedResponse {
    pub accepted: bool,
}

/// Request body for `POST /v3/rooms/{room_id}/send/{event_type}/{txn_id}`-style state event.
#[derive(utoipa::ToSchema, serde::Deserialize)]
#[allow(dead_code)]
pub struct ApiStateEventRequest {
    #[serde(flatten)]
    pub content: serde_json::Value,
}

/// Response body of `PUT /v3/rooms/{room_id}/state/{event_type}`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiStateEventPutResponse {
    pub event_id: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// P2-Admin: User / room admin endpoints
// ─────────────────────────────────────────────────────────────────────────────

/// Per-user admin listing entry (`GET /_synapse/admin/v1/users`).
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminUserEntry {
    pub user_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub displayname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_admin: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deactivated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_type: Option<String>,
}

/// Response body of `GET /_synapse/admin/v1/users`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminUserListResponse {
    pub users: Vec<ApiAdminUserEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<i64>,
}

/// Per-room admin listing entry (`GET /_synapse/admin/v1/rooms`).
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminRoomEntry {
    pub room_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canonical_alias: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub joined_members: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub joined_local_members: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encryption: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_public: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
}

/// Response body of `GET /_synapse/admin/v1/rooms`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminRoomListResponse {
    pub rooms: Vec<ApiAdminRoomEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_batch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prev_batch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_rooms: Option<i64>,
}

/// Generic admin success response (`{"success": true}`).
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminSuccess {
    pub success: bool,
}

/// Response body of `DELETE /_synapse/admin/v1/users/{user_id}`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminUserDeleteResponse {
    pub user_id: String,
    pub deleted: bool,
}

/// Response body of `POST /_synapse/admin/v1/users/{user_id}/evict`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminUserEvictResponse {
    pub user_id: String,
    pub rooms_evicted: i64,
    pub rooms: Vec<String>,
    pub failures: Vec<serde_json::Value>,
}

/// Request body for `PUT /_synapse/admin/v1/users/{user_id}/admin`.
#[derive(utoipa::ToSchema, serde::Deserialize)]
#[allow(dead_code)]
pub struct ApiAdminSetAdminRequest {
    pub admin: bool,
}

/// Request body for `POST /_synapse/admin/v1/users/{user_id}/password`.
#[derive(utoipa::ToSchema, serde::Deserialize)]
#[allow(dead_code)]
pub struct ApiAdminResetPasswordRequest {
    pub new_password: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logout_devices: Option<bool>,
}

/// Response body of `POST /_synapse/admin/v1/users/{user_id}/deactivate`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminDeactivateResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id_server_unbind_result: Option<String>,
}

/// Per-device entry in admin device listings.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminDeviceEntry {
    pub device_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_seen_ts: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_seen_ip: Option<String>,
}

/// Response body of `GET /_synapse/admin/v1/users/{user_id}/devices`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminUserDevicesResponse {
    pub devices: Vec<ApiAdminDeviceEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<i64>,
}

/// Response body of `GET /_synapse/admin/v1/users/{user_id}/rooms`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminUserRoomsResponse {
    pub joined_rooms: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_batch: Option<String>,
}

/// Response body of `POST /_synapse/admin/v1/users/{user_id}/login`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminLoginAsUserResponse {
    pub access_token: String,
    pub device_id: String,
    pub user_id: String,
}

/// Response body of `POST /_synapse/admin/v1/users/{user_id}/logout`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminLogoutUserDevicesResponse {
    pub devices_deleted: i64,
}

/// Response body of `GET /_synapse/admin/v1/user_stats`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminUserStats {
    pub total_users: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_users: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub admin_users: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deactivated_users: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guest_users: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub average_rooms_per_user: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_registration_enabled: Option<bool>,
}

/// Response body of `GET /_synapse/admin/v1/users/{user_id}/stats`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminSingleUserStats {
    pub user_id: String,
    pub rooms_joined: i64,
    pub messages_sent: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_seen_ts: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creation_ts: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_admin: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dashboard: Option<serde_json::Value>,
}

/// Response body of `POST /_synapse/admin/v1/users/batch` and `/batch_deactivate`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminBatchUserResponse {
    pub created: Option<i64>,
    pub deactivated: Option<i64>,
    pub failed: i64,
    pub total: i64,
}

/// Response body of `GET /_synapse/admin/v2/users/{user_id}`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminUserDetail {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub user_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_guest: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub admin: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deactivated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub displayname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_ts: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub devices: Option<Vec<ApiAdminDeviceEntry>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threepids: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_ids: Option<Vec<serde_json::Value>>,
}

/// Request body for `PUT /_synapse/admin/v2/users/{user_id}`.
#[derive(utoipa::ToSchema, serde::Deserialize)]
#[allow(dead_code)]
pub struct ApiAdminUpsertUserRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub displayname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub admin: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deactivated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_type: Option<String>,
}
