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
    /// The `last_seen_ts` field.
    pub last_seen_ts: Option<i64>,
    /// The IP address the device was last seen from.
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `last_seen_ip` field.
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
    /// The `device` field.
    pub device: Option<ApiDevice>,
    /// Device identifier.
    pub device_id: String,
    /// Human-readable display name for the device.
    pub display_name: Option<String>,
    /// Unix timestamp (ms) when this device was last seen.
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `last_seen_ts` field.
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
    /// The `device_id` field.
    pub device_id: String,
    /// The `display_name` field.
    pub display_name: Option<String>,
    /// Unix timestamp (ms) of the update.
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `updated_ts` field.
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
    /// The `displayname` field.
    pub displayname: Option<String>,
    /// The user's avatar MXC URI, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `avatar_url` field.
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
    /// The `displayname` field.
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
    /// The `status_msg` field.
    pub status_msg: Option<String>,
    /// Milliseconds since the user was last active.
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `last_active_ago` field.
    pub last_active_ago: Option<i64>,
    /// Whether the user is currently active.
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `currently_active` field.
    pub currently_active: Option<bool>,
}

/// Response body of `GET /_matrix/client/v3/presence/{user_id}/status`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiPresenceStatusResponse {
    /// The `presence` field.
    pub presence: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `status_msg` field.
    pub status_msg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `last_active_ago` field.
    pub last_active_ago: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `currently_active` field.
    pub currently_active: Option<bool>,
}

/// Individual entry in a presence list.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiPresenceListEntry {
    /// The `user_id` field.
    pub user_id: String,
    /// The `presence` field.
    pub presence: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `status_msg` field.
    pub status_msg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `last_active_ago` field.
    pub last_active_ago: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `currently_active` field.
    pub currently_active: Option<bool>,
}

/// Response body of `GET /_matrix/client/v3/presence/list` and `/list/{user_id}`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiPresenceListResponse {
    /// The `presences` field.
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
    /// The `username` field.
    pub username: Option<String>,
    /// Optional TURN credential.
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `credential` field.
    pub credential: Option<String>,
}

/// MatrixRTC transport entry (MSC4403).
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiRtcTransport {
    /// Transport type identifier.
    #[serde(rename = "type")]
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `transport_type` field.
    pub transport_type: Option<String>,
    /// List of ICE servers for this transport.
    pub ice_servers: Vec<ApiRtcIceServer>,
}

/// Response body of `GET /_matrix/client/unstable/org.matrix.msc4143/rtc/transports`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiRtcTransportsResponse {
    /// The `transports` field.
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
    /// The `m_upload_size` field.
    pub m_upload_size: i64,
}

// ─────────────────────────────────────────────────────────────────────────────
// P2-Room core: list/joined/create/join/leave/forget/invite/send
// ─────────────────────────────────────────────────────────────────────────────

/// Response body of `GET /_matrix/client/v3/joined_rooms`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiJoinedRoomsResponse {
    /// The `joined_rooms` field.
    pub joined_rooms: Vec<String>,
}

/// Public room directory entry returned by `GET /v3/publicRooms`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiPublicRoomsChunkEntry {
    /// The `room_id` field.
    pub room_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `name` field.
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `topic` field.
    pub topic: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `canonical_alias` field.
    pub canonical_alias: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `num_joined_members` field.
    pub num_joined_members: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `world_readable` field.
    pub world_readable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `guest_can_join` field.
    pub guest_can_join: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `avatar_url` field.
    pub avatar_url: Option<String>,
}

/// Response body of `GET /v3/publicRooms`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiPublicRoomsResponse {
    /// The `chunk` field.
    pub chunk: Vec<ApiPublicRoomsChunkEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `next_batch` field.
    pub next_batch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `prev_batch` field.
    pub prev_batch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `total_room_count_estimate` field.
    pub total_room_count_estimate: Option<i32>,
}

/// Request body for `POST /v3/createRoom`.
#[derive(utoipa::ToSchema, serde::Deserialize)]
#[allow(dead_code)]
pub struct ApiCreateRoomRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `preset` field.
    pub preset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `name` field.
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `topic` field.
    pub topic: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `visibility` field.
    pub visibility: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `room_alias_name` field.
    pub room_alias_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `invite` field.
    pub invite: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `power_level_content_override` field.
    pub power_level_content_override: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `initial_state` field.
    pub initial_state: Option<Vec<serde_json::Value>>,
}

/// Response body of `POST /v3/createRoom`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiCreateRoomResponse {
    /// The `room_id` field.
    pub room_id: String,
}

/// Response body of `PUT /v3/rooms/{room_id}/send/{event_type}/{txn_id}`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiSendEventResponse {
    /// The `event_id` field.
    pub event_id: String,
}

/// Response body of `POST /v3/rooms/{room_id}/join` and `/leave` and `/forget`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiRoomIdResponse {
    /// The `room_id` field.
    pub room_id: String,
}

/// Response body of `POST /v3/rooms/{room_id}/forget`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiForgetRoomResponse {
    /// The `room_id` field.
    pub room_id: String,
    /// The `is_forgotten` field.
    pub is_forgotten: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `updated_ts` field.
    pub updated_ts: Option<i64>,
}

/// Response body of `POST /v3/rooms/{room_id}/invite`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiInviteResponse {
    /// The `room_id` field.
    pub room_id: String,
    /// The `invited_user_id` field.
    pub invited_user_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `invited_ts` field.
    pub invited_ts: Option<i64>,
}

/// Response body of `GET /v3/rooms/{room_id}/joined_members`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiJoinedMembersResponse {
    /// The `joined` field.
    pub joined: serde_json::Value,
}

// ─────────────────────────────────────────────────────────────────────────────
// P2-User directory
// ─────────────────────────────────────────────────────────────────────────────

/// Response body of `GET /v3/user_directory/profiles/{user_id}`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiUserDirectoryProfileResponse {
    /// The `user_id` field.
    pub user_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `displayname` field.
    pub displayname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `avatar_url` field.
    pub avatar_url: Option<String>,
}

/// Response body of `POST /v3/user_directory/search`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiUserDirectorySearchResponse {
    /// The `limited` field.
    pub limited: bool,
    /// The `results` field.
    pub results: Vec<ApiUserDirectoryProfileResponse>,
}

/// Response body of `GET/PUT /v3/directory/list/room/{room_id}`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiRoomVisibilityResponse {
    /// The `visibility` field.
    pub visibility: String,
}

/// Response body of `PUT /v3/directory/list/room/{room_id}`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiRoomVisibilityUpdateResponse {
    /// The `room_id` field.
    pub room_id: String,
    /// The `visibility` field.
    pub visibility: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// P2-Room alias + sync/events/my_rooms + relations + search
// ─────────────────────────────────────────────────────────────────────────────

/// Response body of `PUT /v3/directory/room/{room_alias}`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiRoomAliasCreatedResponse {
    /// The `room_id` field.
    pub room_id: String,
    /// The `alias` field.
    pub alias: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `created_ts` field.
    pub created_ts: Option<i64>,
}

/// Response body of `DELETE /v3/directory/room/{room_alias}`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiRoomAliasRemovedResponse {
    /// The `removed` field.
    pub removed: bool,
    /// The `alias` field.
    pub alias: String,
}

/// Response body of `GET /r0/directory/room/{room_id}/alias`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiRoomAliasesResponse {
    /// The `aliases` field.
    pub aliases: Vec<String>,
}

/// Response body of `POST /v3/publicRooms` (alias of GET).
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiQueryPublicRoomsResponse {
    /// The `chunk` field.
    pub chunk: Vec<ApiPublicRoomsChunkEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `next_batch` field.
    pub next_batch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `total_room_count_estimate` field.
    pub total_room_count_estimate: Option<i32>,
}

/// Per-room summary returned by `GET /v3/my_rooms`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiMyRoomEntry {
    /// The `room_id` field.
    pub room_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `name` field.
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `topic` field.
    pub topic: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `avatar_url` field.
    pub avatar_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `join_state` field.
    pub join_state: Option<String>,
}

/// Response body of `GET /v3/my_rooms`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiMyRoomsResponse {
    /// The `rooms` field.
    pub rooms: Vec<ApiMyRoomEntry>,
    /// The `total` field.
    pub total: i32,
}

/// Request body for `GET /v3/search` (POST with body).
#[derive(utoipa::ToSchema, serde::Deserialize)]
#[allow(dead_code)]
pub struct ApiSearchRequest {
    /// The `search_categories` field.
    pub search_categories: serde_json::Value,
}

/// Response body of `POST /v3/search`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiSearchResponse {
    /// The `search_categories` field.
    pub search_categories: serde_json::Value,
}

/// Request body for `POST /v3/rooms/{room_id}/report`.
#[derive(utoipa::ToSchema, serde::Deserialize)]
#[allow(dead_code)]
pub struct ApiRoomReportRequest {
    /// The `reason` field.
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `score` field.
    pub score: Option<i32>,
}

/// Response body of `POST /v3/rooms/{room_id}/report`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiReportAcceptedResponse {
    /// The `accepted` field.
    pub accepted: bool,
}

/// Request body for `POST /v3/rooms/{room_id}/send/{event_type}/{txn_id}`-style state event.
#[derive(utoipa::ToSchema, serde::Deserialize)]
#[allow(dead_code)]
pub struct ApiStateEventRequest {
    #[serde(flatten)]
    /// The `content` field.
    pub content: serde_json::Value,
}

/// Response body of `PUT /v3/rooms/{room_id}/state/{event_type}`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiStateEventPutResponse {
    /// The `event_id` field.
    pub event_id: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// P2-Admin: User / room admin endpoints
// ─────────────────────────────────────────────────────────────────────────────

/// Per-user admin listing entry (`GET /_synapse/admin/v1/users`).
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminUserEntry {
    /// The `user_id` field.
    pub user_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `displayname` field.
    pub displayname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `is_admin` field.
    pub is_admin: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `deactivated` field.
    pub deactivated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `user_type` field.
    pub user_type: Option<String>,
}

/// Response body of `GET /_synapse/admin/v1/users`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminUserListResponse {
    /// The `users` field.
    pub users: Vec<ApiAdminUserEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `next_token` field.
    pub next_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `total` field.
    pub total: Option<i64>,
}

/// Per-room admin listing entry (`GET /_synapse/admin/v1/rooms`).
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminRoomEntry {
    /// The `room_id` field.
    pub room_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `name` field.
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `canonical_alias` field.
    pub canonical_alias: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `joined_members` field.
    pub joined_members: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `joined_local_members` field.
    pub joined_local_members: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `version` field.
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `creator` field.
    pub creator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `encryption` field.
    pub encryption: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `is_public` field.
    pub is_public: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `topic` field.
    pub topic: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `avatar_url` field.
    pub avatar_url: Option<String>,
}

/// Response body of `GET /_synapse/admin/v1/rooms`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminRoomListResponse {
    /// The `rooms` field.
    pub rooms: Vec<ApiAdminRoomEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `next_batch` field.
    pub next_batch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `prev_batch` field.
    pub prev_batch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `total_rooms` field.
    pub total_rooms: Option<i64>,
}

/// Generic admin success response (`{"success": true}`).
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminSuccess {
    /// The `success` field.
    pub success: bool,
}

/// Response body of `DELETE /_synapse/admin/v1/users/{user_id}`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminUserDeleteResponse {
    /// The `user_id` field.
    pub user_id: String,
    /// The `deleted` field.
    pub deleted: bool,
}

/// Response body of `POST /_synapse/admin/v1/users/{user_id}/evict`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminUserEvictResponse {
    /// The `user_id` field.
    pub user_id: String,
    /// The `rooms_evicted` field.
    pub rooms_evicted: i64,
    /// The `rooms` field.
    pub rooms: Vec<String>,
    /// The `failures` field.
    pub failures: Vec<serde_json::Value>,
}

/// Request body for `PUT /_synapse/admin/v1/users/{user_id}/admin`.
#[derive(utoipa::ToSchema, serde::Deserialize)]
#[allow(dead_code)]
pub struct ApiAdminSetAdminRequest {
    /// The `admin` field.
    pub admin: bool,
}

/// Request body for `POST /_synapse/admin/v1/users/{user_id}/password`.
#[derive(utoipa::ToSchema, serde::Deserialize)]
#[allow(dead_code)]
pub struct ApiAdminResetPasswordRequest {
    /// The `new_password` field.
    pub new_password: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `logout_devices` field.
    pub logout_devices: Option<bool>,
}

/// Response body of `POST /_synapse/admin/v1/users/{user_id}/deactivate`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminDeactivateResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `id_server_unbind_result` field.
    pub id_server_unbind_result: Option<String>,
}

/// Per-device entry in admin device listings.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminDeviceEntry {
    /// The `device_id` field.
    pub device_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `display_name` field.
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `last_seen_ts` field.
    pub last_seen_ts: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `last_seen_ip` field.
    pub last_seen_ip: Option<String>,
}

/// Response body of `GET /_synapse/admin/v1/users/{user_id}/devices`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminUserDevicesResponse {
    /// The `devices` field.
    pub devices: Vec<ApiAdminDeviceEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `total` field.
    pub total: Option<i64>,
}

/// Response body of `GET /_synapse/admin/v1/users/{user_id}/rooms`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminUserRoomsResponse {
    /// The `joined_rooms` field.
    pub joined_rooms: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `total` field.
    pub total: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `next_batch` field.
    pub next_batch: Option<String>,
}

/// Response body of `POST /_synapse/admin/v1/users/{user_id}/login`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminLoginAsUserResponse {
    /// The `access_token` field.
    pub access_token: String,
    /// The `device_id` field.
    pub device_id: String,
    /// The `user_id` field.
    pub user_id: String,
}

/// Response body of `POST /_synapse/admin/v1/users/{user_id}/logout`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminLogoutUserDevicesResponse {
    /// The `devices_deleted` field.
    pub devices_deleted: i64,
}

/// Response body of `GET /_synapse/admin/v1/user_stats`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminUserStats {
    /// The `total_users` field.
    pub total_users: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `active_users` field.
    pub active_users: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `admin_users` field.
    pub admin_users: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `deactivated_users` field.
    pub deactivated_users: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `guest_users` field.
    pub guest_users: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `average_rooms_per_user` field.
    pub average_rooms_per_user: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `user_registration_enabled` field.
    pub user_registration_enabled: Option<bool>,
}

/// Response body of `GET /_synapse/admin/v1/users/{user_id}/stats`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminSingleUserStats {
    /// The `user_id` field.
    pub user_id: String,
    /// The `rooms_joined` field.
    pub rooms_joined: i64,
    /// The `messages_sent` field.
    pub messages_sent: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `last_seen_ts` field.
    pub last_seen_ts: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `creation_ts` field.
    pub creation_ts: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `is_admin` field.
    pub is_admin: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `dashboard` field.
    pub dashboard: Option<serde_json::Value>,
}

/// Response body of `POST /_synapse/admin/v1/users/batch` and `/batch_deactivate`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminBatchUserResponse {
    /// The `created` field.
    pub created: Option<i64>,
    /// The `deactivated` field.
    pub deactivated: Option<i64>,
    /// The `failed` field.
    pub failed: i64,
    /// The `total` field.
    pub total: i64,
}

/// Response body of `GET /_synapse/admin/v2/users/{user_id}`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminUserDetail {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `name` field.
    pub name: Option<String>,
    /// The `user_id` field.
    pub user_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `is_guest` field.
    pub is_guest: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `admin` field.
    pub admin: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `deactivated` field.
    pub deactivated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `displayname` field.
    pub displayname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `avatar_url` field.
    pub avatar_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `created_ts` field.
    pub created_ts: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `user_type` field.
    pub user_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `devices` field.
    pub devices: Option<Vec<ApiAdminDeviceEntry>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `threepids` field.
    pub threepids: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `external_ids` field.
    pub external_ids: Option<Vec<serde_json::Value>>,
}

/// Request body for `PUT /_synapse/admin/v2/users/{user_id}`.
#[derive(utoipa::ToSchema, serde::Deserialize)]
#[allow(dead_code)]
pub struct ApiAdminUpsertUserRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `password` field.
    pub password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `displayname` field.
    pub displayname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `avatar_url` field.
    pub avatar_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `admin` field.
    pub admin: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `deactivated` field.
    pub deactivated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `user_type` field.
    pub user_type: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// P3-Admin batch 2: session / account / room / spaces endpoints
// ─────────────────────────────────────────────────────────────────────────────

/// Per-session entry in admin user session listings.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminSessionEntry {
    /// The `device_id` field.
    pub device_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `display_name` field.
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `last_seen_ts` field.
    pub last_seen_ts: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `last_seen_ip` field.
    pub last_seen_ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `session_id` field.
    pub session_id: Option<String>,
}

/// Response body of `GET /_synapse/admin/v1/user_sessions/{user_id}`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminUserSessionsResponse {
    /// The `user_id` field.
    pub user_id: String,
    /// The `sessions` field.
    pub sessions: Vec<ApiAdminSessionEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `total` field.
    pub total: Option<i64>,
}

/// Response body of `POST /_synapse/admin/v1/user_sessions/{user_id}/invalidate`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminSessionsInvalidated {
    /// The `invalidated` field.
    pub invalidated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `sessions_removed` field.
    pub sessions_removed: Option<i64>,
}

/// Request body for `POST /_synapse/admin/v1/account/{user_id}` (generic JSON).
#[derive(utoipa::ToSchema, serde::Deserialize)]
#[allow(dead_code)]
pub struct ApiAdminAccountUpdateRequest {
    #[serde(flatten)]
    /// The `fields` field.
    pub fields: serde_json::Value,
}

/// Response body of `GET /_synapse/admin/v1/account/{user_id}`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminAccountDetails {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `name` field.
    pub name: Option<String>,
    /// The `user_id` field.
    pub user_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `displayname` field.
    pub displayname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `admin` field.
    pub admin: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `deactivated` field.
    pub deactivated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `creation_ts` field.
    pub creation_ts: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `device_count` field.
    pub device_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `room_count` field.
    pub room_count: Option<i64>,
}

/// Response body of `POST /_synapse/admin/v1/account/{user_id}` update.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminAccountUpdated {
    /// The `user_id` field.
    pub user_id: String,
    /// The `updated` field.
    pub updated: bool,
}

/// Response body of `GET /_synapse/admin/v1/rooms/{room_id}` (admin view).
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminRoomDetail {
    /// The `room_id` field.
    pub room_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `name` field.
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `topic` field.
    pub topic: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `creator` field.
    pub creator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `member_count` field.
    pub member_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `room_version` field.
    pub room_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `encryption` field.
    pub encryption: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `is_public` field.
    pub is_public: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `join_rule` field.
    pub join_rule: Option<String>,
}

/// Member entry in admin room member listings.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminRoomMemberEntry {
    /// The `user_id` field.
    pub user_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `displayname` field.
    pub displayname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `avatar_url` field.
    pub avatar_url: Option<String>,
    /// The `membership` field.
    pub membership: String,
}

/// Response body of `GET /_synapse/admin/v1/rooms/{room_id}/members`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminRoomMembersResponse {
    /// The `members` field.
    pub members: Vec<ApiAdminRoomMemberEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `total` field.
    pub total: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `next_batch` field.
    pub next_batch: Option<String>,
}

/// State event entry in admin room state listings.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminStateEventEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "type")]
    /// The `event_type` field.
    pub event_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `state_key` field.
    pub state_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `content` field.
    pub content: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `sender` field.
    pub sender: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `event_id` field.
    pub event_id: Option<String>,
}

/// Response body of `GET /_synapse/admin/v1/rooms/{room_id}/state`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminRoomStateResponse {
    /// The `state` field.
    pub state: Vec<ApiAdminStateEventEntry>,
}

/// Space entry in admin space listings.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminSpaceEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `space_id` field.
    pub space_id: Option<String>,
    /// The `room_id` field.
    pub room_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `name` field.
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `topic` field.
    pub topic: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `creator` field.
    pub creator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `created_ts` field.
    pub created_ts: Option<i64>,
}

/// Response body of `GET /_synapse/admin/v1/spaces`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminSpacesResponse {
    /// The `spaces` field.
    pub spaces: Vec<ApiAdminSpaceEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `total` field.
    pub total: Option<i64>,
}

/// Response body of `GET /_synapse/admin/v1/spaces/{space_id}/users`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminSpaceUsersResponse {
    /// The `users` field.
    pub users: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `total` field.
    pub total: Option<i64>,
}

/// Response body of `GET /_synapse/admin/v1/spaces/{space_id}/rooms`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminSpaceRoomsResponse {
    /// The `rooms` field.
    pub rooms: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `total` field.
    pub total: Option<i64>,
}

/// Response body of `GET /_synapse/admin/v1/spaces/{space_id}/stats`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminSpaceStats {
    /// The `space_id` field.
    pub space_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `member_count` field.
    pub member_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `child_room_count` field.
    pub child_room_count: Option<i64>,
}

/// Response body of `GET /_synapse/admin/v1/rooms/{room_id}/listings`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminRoomListingStatus {
    /// The `room_id` field.
    pub room_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `public` field.
    pub public: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `in_directory` field.
    pub in_directory: Option<bool>,
}

/// Response body of `PUT /DELETE /_synapse/admin/v1/rooms/{room_id}/listings/public`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminRoomVisibilityUpdated {
    /// The `room_id` field.
    pub room_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `public` field.
    pub public: Option<bool>,
}

/// Response body of `GET /_synapse/admin/v1/rooms/{room_id}/block`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminRoomBlockStatus {
    /// The `block` field.
    pub block: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `blocked_at` field.
    pub blocked_at: Option<i64>,
}

/// Request body for `POST /_synapse/admin/v1/rooms/{room_id}/block`.
#[derive(utoipa::ToSchema, serde::Deserialize)]
#[allow(dead_code)]
pub struct ApiAdminBlockRoomRequest {
    /// The `block` field.
    pub block: bool,
}

/// Response body of `POST /_synapse/admin/v1/rooms/{room_id}/block` and `/unblock`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminRoomBlockUpdated {
    /// The `block` field.
    pub block: bool,
}

/// Request body for `POST /_synapse/admin/v1/rooms/{room_id}/make_admin`.
#[derive(utoipa::ToSchema, serde::Deserialize)]
#[allow(dead_code)]
pub struct ApiAdminMakeRoomAdminRequest {
    /// The `user_id` field.
    pub user_id: String,
}

/// Request body for `POST /_synapse/admin/v1/purge_history`.
#[derive(utoipa::ToSchema, serde::Deserialize)]
#[allow(dead_code)]
pub struct ApiAdminPurgeHistoryRequest {
    /// The `room_id` field.
    pub room_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `before_ts` field.
    pub before_ts: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `delete_local_events` field.
    pub delete_local_events: Option<bool>,
}

/// Response body of `POST /_synapse/admin/v1/purge_history`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminPurgeHistoryResponse {
    /// The `success` field.
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `deleted_events` field.
    pub deleted_events: Option<i64>,
}

/// Request body for `POST /_synapse/admin/v1/purge_room`.
#[derive(utoipa::ToSchema, serde::Deserialize)]
#[allow(dead_code)]
pub struct ApiAdminPurgeRoomRequest {
    /// The `room_id` field.
    pub room_id: String,
}

/// Response body of `POST /_synapse/admin/v1/purge_room`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminPurgeRoomResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `purge_id` field.
    pub purge_id: Option<String>,
    /// The `success` field.
    pub success: bool,
}

/// Response body of `PUT /_synapse/admin/v1/rooms/{room_id}/members/{user_id}`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminMembershipUpdated {
    /// The `user_id` field.
    pub user_id: String,
    /// The `room_id` field.
    pub room_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `membership` field.
    pub membership: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `removed` field.
    pub removed: Option<bool>,
}

/// Request body for `POST /_synapse/admin/v1/rooms/cleanup`.
#[derive(utoipa::ToSchema, serde::Deserialize)]
#[allow(dead_code)]
pub struct ApiAdminCleanupRoomsRequest {
    #[serde(flatten)]
    /// The `options` field.
    pub options: serde_json::Value,
}

/// Response body of `GET /_synapse/admin/v1/server_version`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminServerVersion {
    /// The `server_version` field.
    pub server_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `python_version` field.
    pub python_version: Option<String>,
    /// The `server_name` field.
    pub server_name: String,
}

/// Response body of `GET /_synapse/admin/info`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminServerInfo {
    /// The `server_name` field.
    pub server_name: String,
    /// The `server_version` field.
    pub server_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `implementation` field.
    pub implementation: Option<String>,
}

/// Response body of `GET /_synapse/admin/v1/whoami`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminWhoAmI {
    /// The `user_id` field.
    pub user_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `name` field.
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `is_admin` field.
    pub is_admin: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `role` field.
    pub role: Option<String>,
}

/// Response body of `GET /_synapse/admin/v1/statistics`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminStatistics {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `total_users` field.
    pub total_users: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `total_rooms` field.
    pub total_rooms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `daily_active_users` field.
    pub daily_active_users: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `monthly_active_users` field.
    pub monthly_active_users: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `r30_users` field.
    pub r30_users: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `r30v2_users` field.
    pub r30v2_users: Option<i64>,
}

/// Response body of `GET /_synapse/admin/v1/status`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminStatus {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `db_ok` field.
    pub db_ok: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `server_ok` field.
    pub server_ok: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `up` field.
    pub up: Option<bool>,
}

/// Device last-seen entry in whois responses.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminWhoisDeviceEntry {
    /// The `device_id` field.
    pub device_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `display_name` field.
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `last_seen` field.
    pub last_seen: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `ip` field.
    pub ip: Option<String>,
}

/// Response body of `GET /_synapse/admin/v1/whois/{user_id}`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminWhoisResponse {
    /// The `user_id` field.
    pub user_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `devices` field.
    pub devices: Option<Vec<ApiAdminWhoisDeviceEntry>>,
}

/// Response body of `GET /_synapse/admin/v1/whois/{user_id}/{device_id}`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminWhoisDeviceResponse {
    /// The `user_id` field.
    pub user_id: String,
    /// The `device_id` field.
    pub device_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `display_name` field.
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `last_seen` field.
    pub last_seen: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `ip` field.
    pub ip: Option<String>,
}

/// Response body of `POST /_synapse/admin/v1/purge_media_cache`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminPurgeMediaCacheResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `deleted` field.
    pub deleted: Option<i64>,
}

/// Response body of `GET /_synapse/admin/v1/config`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminConfigResponse {
    /// The `server_name` field.
    pub server_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `public_baseurl` field.
    pub public_baseurl: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `registration_enabled` field.
    pub registration_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `max_upload_size` field.
    pub max_upload_size: Option<i64>,
}

/// Response body of `GET /_synapse/admin/v1/jitsi/config`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminJitsiConfig {
    /// The `domain` field.
    pub domain: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `app_id` field.
    pub app_id: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `jwt_enabled` field.
    pub jwt_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `jwt_asap_enabled` field.
    pub jwt_asap_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `jwt_auth_type` field.
    pub jwt_auth_type: Option<String>,
    /// The `server_name` field.
    pub server_name: String,
}

/// Response body of `GET /_synapse/admin/v1/invite/blocklist`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminInviteBlocklist {
    /// The `blocklist` field.
    pub blocklist: Vec<String>,
}

/// Response body of `GET /_synapse/admin/v1/invite/allowlist`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminInviteAllowlist {
    /// The `allowlist` field.
    pub allowlist: Vec<String>,
}

/// Federation destination entry.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminFederationDestinationEntry {
    /// The `destination` field.
    pub destination: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `retry_last_ts` field.
    pub retry_last_ts: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `retry_interval` field.
    pub retry_interval: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `failure_ts` field.
    pub failure_ts: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `last_successful_stream_ordering` field.
    pub last_successful_stream_ordering: Option<i64>,
}

/// Response body of `GET /_synapse/admin/v1/federation/destinations`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminFederationDestinationsResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `destinations` field.
    pub destinations: Option<Vec<ApiAdminFederationDestinationEntry>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `total` field.
    pub total: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `total_count` field.
    pub total_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `next_batch` field.
    pub next_batch: Option<String>,
}

/// Response body of `GET /_synapse/admin/v1/federation/destinations/{destination}/rooms`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminFederationDestinationRoomsResponse {
    /// The `rooms` field.
    pub rooms: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `total` field.
    pub total: Option<i64>,
}

/// Report entry in admin moderation listings.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminReportEntry {
    /// The `id` field.
    pub id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `room_id` field.
    pub room_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `event_id` field.
    pub event_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `user_id` field.
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `reported_user_id` field.
    pub reported_user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `reason` field.
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `content` field.
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `status` field.
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `score` field.
    pub score: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `received_ts` field.
    pub received_ts: Option<i64>,
}

/// Response body of `GET /_synapse/admin/v1/reports`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminReportsResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `reports` field.
    pub reports: Option<Vec<ApiAdminReportEntry>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `total` field.
    pub total: Option<i64>,
}

/// Response body of `GET /_synapse/admin/v1/retention/policy`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminRetentionPolicy {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `max_lifetime` field.
    pub max_lifetime: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `min_lifetime` field.
    pub min_lifetime: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `is_expire_on_clients` field.
    pub is_expire_on_clients: Option<bool>,
}

/// Request body for `POST /_synapse/admin/v1/retention/policy`.
#[derive(utoipa::ToSchema, serde::Deserialize)]
#[allow(dead_code)]
pub struct ApiAdminSetRetentionPolicyRequest {
    #[serde(flatten)]
    /// The `fields` field.
    pub fields: serde_json::Value,
}

/// Response body of `GET /_synapse/admin/v1/retention/policy/{room_id}`.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminRoomRetentionPolicy {
    /// The `room_id` field.
    pub room_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `max_lifetime` field.
    pub max_lifetime: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `min_lifetime` field.
    pub min_lifetime: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `is_expire_on_clients` field.
    pub is_expire_on_clients: Option<bool>,
}

/// Request body for `POST /_synapse/admin/v1/purge_media_cache`.
#[derive(utoipa::ToSchema, serde::Deserialize)]
#[allow(dead_code)]
pub struct ApiAdminPurgeMediaCacheRequest {
    /// The `before_ts` field.
    pub before_ts: i64,
}

/// Response body of `GET /_synapse/admin/v1/room_stats` (generic stats).
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminRoomStatsOverview {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `total_rooms` field.
    pub total_rooms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `public_rooms` field.
    pub public_rooms: Option<i64>,
}

/// Generic request body used across multiple admin endpoints.
#[derive(utoipa::ToSchema, serde::Deserialize)]
#[allow(dead_code)]
pub struct ApiAdminGenericRequest {
    #[serde(flatten)]
    /// The `fields` field.
    pub fields: serde_json::Value,
}

// ─────────────────────────────────────────────────────────────────────────────
// P3-Client: Generic client schemas for remaining push_rules / server_version /
//            turnserver endpoints
// ─────────────────────────────────────────────────────────────────────────────

/// Generic client JSON response (push rules, server version, etc.).
#[derive(utoipa::ToSchema, serde::Serialize, serde::Deserialize)]
#[allow(dead_code)]
pub struct ApiClientGenericJson {
    #[serde(flatten)]
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `extra` field.
    pub extra: Option<serde_json::Value>,
}

/// Generic client JSON request body.
#[derive(utoipa::ToSchema, serde::Deserialize)]
#[allow(dead_code)]
pub struct ApiClientGenericRequest {
    #[serde(flatten)]
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `extra` field.
    pub extra: Option<serde_json::Value>,
}

/// Generic JSON response body used across multiple admin endpoints.
#[derive(utoipa::ToSchema, serde::Serialize)]
#[allow(dead_code)]
pub struct ApiAdminGenericJson {
    #[serde(flatten)]
    /// The `fields` field.
    pub fields: serde_json::Value,
}
