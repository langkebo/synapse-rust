#![cfg(feature = "openapi-docs")]

use super::schemas::*;

/// `GET /_health` — Detailed health check with component statuses.
#[cfg(feature = "openapi-docs")]
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

/// `PUT /_matrix/client/v3/devices/{device_id}` — Update device metadata.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v3/devices/{device_id}",
    tag = "Client-Server",
    params(
        ("device_id" = String, Path, description = "Matrix device ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Device updated",
            body = serde_json::Value,
            example = json!({
                "device_id": "DEVICEID",
                "display_name": "Updated device name",
                "updated_ts": 1718000000000_i64
            })
        ),
        (status = 404, description = "Device not found"),
        (status = 400, description = "Invalid display_name")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn update_device_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_matrix/client/v3/devices/{device_id}` — Delete one device with UIA.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_matrix/client/v3/devices/{device_id}",
    tag = "Client-Server",
    params(
        ("device_id" = String, Path, description = "Matrix device ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Device deleted", body = serde_json::Value),
        (status = 401, description = "UIA challenge required"),
        (status = 404, description = "Device not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn delete_device_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/delete_devices` — Delete multiple devices with UIA.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/delete_devices",
    tag = "Client-Server",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Devices deleted", body = serde_json::Value),
        (status = 401, description = "UIA challenge required"),
        (status = 400, description = "Missing or invalid device_ids")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn delete_devices_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/profile/{user_id}` — Read the public profile for a Matrix user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/profile/{user_id}",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "User profile",
            body = serde_json::Value,
            example = json!({
                "displayname": "Alice",
                "avatar_url": "mxc://example.com/avatar"
            })
        ),
        (status = 403, description = "Profile is not visible to the caller"),
        (status = 404, description = "User not found")
    ),
)]
pub fn get_profile_info() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/profile/{user_id}/displayname` — Read the public display name for a Matrix user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/profile/{user_id}/displayname",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "Display name",
            body = serde_json::Value,
            example = json!({
                "displayname": "Alice"
            })
        ),
        (status = 403, description = "Profile is not visible to the caller"),
        (status = 404, description = "User not found")
    ),
)]
pub fn get_profile_displayname() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/profile/{user_id}/avatar_url` — Read the public avatar URL for a Matrix user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/profile/{user_id}/avatar_url",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "Avatar URL",
            body = serde_json::Value,
            example = json!({
                "avatar_url": "mxc://example.com/avatar"
            })
        ),
        (status = 403, description = "Profile is not visible to the caller"),
        (status = 404, description = "User not found")
    ),
)]
pub fn get_profile_avatar_url() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v3/profile/{user_id}/avatar_url` — Set user avatar URL.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v3/profile/{user_id}/avatar_url",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "The user ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Avatar URL updated", body = serde_json::Value),
        (status = 403, description = "Cannot update another user's profile")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn update_avatar_url_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/presence/{user_id}/status` — Read presence for one user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/presence/{user_id}/status",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "Presence state",
            body = serde_json::Value,
            example = json!({
                "presence": "online",
                "status_msg": "Available",
                "last_active_ago": 1200,
                "currently_active": true
            })
        ),
        (status = 403, description = "Presence is not visible to the caller"),
        (status = 404, description = "User not found")
    ),
)]
pub fn get_presence_status() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/presence/list` — Read the caller's current presence subscriptions.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/presence/list",
    tag = "Client-Server",
    responses(
        (status = 200, description = "Presence subscription list",
            body = serde_json::Value,
            example = json!({
                "presences": [{
                    "user_id": "@alice:example.com",
                    "presence": "online",
                    "status_msg": "Working",
                    "last_active_ago": 500,
                    "currently_active": true
                }]
            })
        )
    ),
)]
pub fn get_presence_list_current_user() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/presence/list/{user_id}` — Read presence subscriptions for a specific user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/presence/list/{user_id}",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "Presence subscription list for the target user",
            body = serde_json::Value,
            example = json!({
                "presences": [{
                    "user_id": "@bob:example.com",
                    "presence": "offline",
                    "status_msg": null,
                    "last_active_ago": null,
                    "currently_active": null
                }]
            })
        ),
        (status = 403, description = "Access denied")
    ),
)]
pub fn get_presence_list_for_user() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device/status` — Read dehydrated device status for the authenticated user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device/status",
    tag = "Client-Server",
    responses(
        (status = 200, description = "Dehydrated device status",
            body = serde_json::Value,
            example = json!({
                "device_id": "DEHYDRATEDDEVICE",
                "device_data": {
                    "algorithm": "m.dehydration.v1.olm"
                }
            })
        ),
        (status = 404, description = "No dehydrated device exists")
    ),
)]
pub fn get_dehydrated_device_status_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/unstable/org.matrix.msc4143/rtc/transports` — Read configured MatrixRTC transport information.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/unstable/org.matrix.msc4143/rtc/transports",
    tag = "Client-Server",
    responses(
        (status = 200, description = "RTC transports",
            body = serde_json::Value,
            example = json!({
                "transports": [{
                    "type": "org.matrix.msc4403.ice-server-transport",
                    "ice_servers": [{
                        "urls": ["stun:stun.example.com:3478"]
                    }, {
                        "urls": ["turn:turn.example.com:3478?transport=udp"],
                        "username": "@alice:example.com",
                        "credential": "turn-secret"
                    }]
                }]
            })
        )
    ),
)]
pub fn get_rtc_transports_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/media/config` — Get Matrix media configuration (max upload size).
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/media/config",
    tag = "Client-Server",
    responses(
        (status = 200, description = "Media configuration",
            body = serde_json::Value,
            example = json!({
                "m.upload.size": 52428800
            })
        )
    ),
)]
pub fn get_media_config() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/joined_rooms` — List rooms the user is joined to.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/joined_rooms",
    tag = "Client-Server",
    responses(
        (status = 200, description = "List of joined rooms",
            body = serde_json::Value,
            example = json!({
                "joined_rooms": ["!room1:example.com", "!room2:example.com"]
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_joined_rooms_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/publicRooms` — Get the public room directory.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/publicRooms",
    tag = "Client-Server",
    params(
        ("limit" = Option<i32>, Query, description = "Maximum number of rooms to return"),
        ("since" = Option<String>, Query, description = "Pagination token")
    ),
    responses(
        (status = 200, description = "Public room directory",
            body = serde_json::Value,
            example = json!({
                "chunk": [
                    {
                        "room_id": "!room:example.com",
                        "name": "Public Room",
                        "num_joined_members": 100,
                        "world_readable": true,
                        "guest_can_join": false
                    }
                ],
                "next_batch": "next_token",
                "total_room_count_estimate": 10
            })
        )
    ),
)]
pub fn get_public_rooms_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/createRoom` — Create a new room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/createRoom",
    tag = "Client-Server",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Room created",
            body = serde_json::Value,
            example = json!({
                "room_id": "!abc:example.com"
            })
        ),
        (status = 400, description = "Invalid parameters")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn create_room_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v3/rooms/{room_id}/send/{event_type}/{txn_id}` — Send a message event.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v3/rooms/{room_id}/send/{event_type}/{txn_id}",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "The room ID"),
        ("event_type" = String, Path, description = "The type of event (e.g. m.room.message)"),
        ("txn_id" = String, Path, description = "Client-generated transaction ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Event sent",
            body = serde_json::Value,
            example = json!({
                "event_id": "$event:example.com"
            })
        ),
        (status = 403, description = "Not in room"),
        (status = 400, description = "Invalid event content")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn send_message_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/rooms/{room_id}/join` — Join a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/rooms/{room_id}/join",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "The room ID or alias")
    ),
    request_body = Option<serde_json::Value>,
    responses(
        (status = 200, description = "Joined successfully",
            body = serde_json::Value,
            example = json!({
                "room_id": "!room:example.com"
            })
        ),
        (status = 403, description = "Banned or join rules restricted"),
        (status = 404, description = "Room not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn join_room_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/rooms/{room_id}/leave` — Leave a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/rooms/{room_id}/leave",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "The room ID")
    ),
    responses(
        (status = 200, description = "Left room", body = serde_json::Value),
        (status = 404, description = "Room not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn leave_room_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/rooms/{room_id}/forget` — Forget a room after leaving.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/rooms/{room_id}/forget",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "The room ID")
    ),
    responses(
        (status = 200, description = "Room forgotten",
            body = serde_json::Value,
            example = json!({
                "room_id": "!room:example.com",
                "is_forgotten": true,
                "updated_ts": 1718000000000_i64
            })
        ),
        (status = 404, description = "Room not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn forget_room_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/rooms/{room_id}/invite` — Invite a user to a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/rooms/{room_id}/invite",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "The room ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "User invited",
            body = serde_json::Value,
            example = json!({
                "room_id": "!room:example.com",
                "invited_user_id": "@bob:example.com",
                "invited_ts": 1718000000000_i64
            })
        ),
        (status = 400, description = "Missing or invalid user_id"),
        (status = 403, description = "Caller cannot invite into this room")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn invite_user_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/rooms/{room_id}/joined_members` — List joined members with profile snippets.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/rooms/{room_id}/joined_members",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "The room ID")
    ),
    responses(
        (status = 200, description = "Joined members",
            body = serde_json::Value,
            example = json!({
                "joined": {
                    "@alice:example.com": {
                        "display_name": "Alice",
                        "avatar_url": "mxc://example.com/alice"
                    },
                    "@bob:example.com": {
                        "display_name": "Bob",
                        "avatar_url": null
                    }
                }
            })
        ),
        (status = 403, description = "Not allowed to view joined members")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_joined_members_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v3/profile/{user_id}/displayname` — Set user display name.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v3/profile/{user_id}/displayname",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "The user ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Display name updated", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn update_displayname_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/user_directory/profiles/{user_id}` — Fetch a visible user profile snapshot.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/user_directory/profiles/{user_id}",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Target user ID")
    ),
    responses(
        (status = 200, description = "User directory profile",
            body = serde_json::Value,
            example = json!({
                "user_id": "@alice:example.com",
                "displayname": "Alice",
                "avatar_url": "mxc://example.com/alice"
            })
        ),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_user_directory_profile_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/user_directory/search` — Search discoverable users.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/user_directory/search",
    tag = "Client-Server",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Search results",
            body = serde_json::Value,
            example = json!({
                "limited": false,
                "results": [
                    {
                        "user_id": "@alice:example.com",
                        "display_name": "Alice",
                        "avatar_url": "mxc://example.com/alice"
                    }
                ]
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn search_user_directory_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/directory/list/room/{room_id}` — Read room directory visibility.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/directory/list/room/{room_id}",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "Target room ID")
    ),
    responses(
        (status = 200, description = "Visibility value",
            body = serde_json::Value,
            example = json!({
                "visibility": "public"
            })
        ),
        (status = 404, description = "Room not found")
    )
)]
pub fn get_room_visibility_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v3/directory/list/room/{room_id}` — Update room directory visibility.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v3/directory/list/room/{room_id}",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "Target room ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Visibility updated",
            body = serde_json::Value,
            example = json!({
                "room_id": "!room:example.com",
                "visibility": "private"
            })
        ),
        (status = 403, description = "Caller cannot update visibility")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn set_room_visibility_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/directory/room/{room_alias}` — Resolve a room alias.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/directory/room/{room_alias}",
    tag = "Client-Server",
    params(
        ("room_alias" = String, Path, description = "Room alias, e.g. #room:example.com")
    ),
    responses(
        (status = 200, description = "Alias target room",
            body = serde_json::Value,
            example = json!({
                "room_id": "!room:example.com"
            })
        ),
        (status = 404, description = "Alias not found")
    )
)]
pub fn get_room_by_alias_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v3/directory/room/{room_alias}` — Create or replace a room alias mapping.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v3/directory/room/{room_alias}",
    tag = "Client-Server",
    params(
        ("room_alias" = String, Path, description = "Room alias to create")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Alias created",
            body = serde_json::Value,
            example = json!({
                "room_id": "!room:example.com",
                "alias": "#room:example.com",
                "created_ts": 1718000000000_i64
            })
        ),
        (status = 403, description = "Caller cannot manage aliases")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn set_room_alias_direct_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_matrix/client/v3/directory/room/{room_alias}` — Delete a room alias mapping.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_matrix/client/v3/directory/room/{room_alias}",
    tag = "Client-Server",
    params(
        ("room_alias" = String, Path, description = "Room alias to delete")
    ),
    responses(
        (status = 200, description = "Alias removed",
            body = serde_json::Value,
            example = json!({
                "removed": true,
                "alias": "#room:example.com"
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn delete_room_alias_direct_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/publicRooms` — Query the public room directory with a request body.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/publicRooms",
    tag = "Client-Server",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Public room directory",
            body = serde_json::Value,
            example = json!({
                "chunk": [
                    {
                        "room_id": "!room:example.com",
                        "name": "Public Room",
                        "num_joined_members": 42,
                        "world_readable": true,
                        "guest_can_join": true
                    }
                ],
                "total_room_count_estimate": 1,
                "next_batch": null
            })
        )
    )
)]
pub fn query_public_rooms_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/directory/room/{room_id}/alias` — List aliases bound to a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/directory/room/{room_id}/alias",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "Target room ID")
    ),
    responses(
        (status = 200, description = "Aliases for the room",
            body = serde_json::Value,
            example = json!({
                "aliases": ["#room:example.com"]
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_room_aliases_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/r0/directory/room/{room_id}/alias/{room_alias}` — Add an alias to a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/r0/directory/room/{room_id}/alias/{room_alias}",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "Target room ID"),
        ("room_alias" = String, Path, description = "Alias to add")
    ),
    responses(
        (status = 200, description = "Alias created",
            body = serde_json::Value,
            example = json!({
                "room_id": "!room:example.com",
                "alias": "#room:example.com",
                "created_ts": 1718000000000_i64
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn set_room_alias_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_matrix/client/r0/directory/room/{room_id}/alias/{room_alias}` — Remove a room alias.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_matrix/client/r0/directory/room/{room_id}/alias/{room_alias}",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "Target room ID"),
        ("room_alias" = String, Path, description = "Alias to remove")
    ),
    responses(
        (status = 200, description = "Alias removed", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn delete_room_alias_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/sync` — Perform an incremental or initial sync.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/sync",
    tag = "Client-Server",
    params(
        ("since" = Option<String>, Query, description = "Opaque sync token"),
        ("timeout" = Option<u64>, Query, description = "Long-poll timeout in milliseconds"),
        ("filter" = Option<String>, Query, description = "Filter ID or inline JSON filter"),
        ("full_state" = Option<bool>, Query, description = "Whether to force a full-state sync"),
        ("set_presence" = Option<String>, Query, description = "Presence override")
    ),
    responses(
        (status = 200, description = "Sync response", body = serde_json::Value),
        (status = 429, description = "Rate limited")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn sync_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/events` — Poll event updates using the legacy events stream.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/events",
    tag = "Client-Server",
    params(
        ("from" = Option<String>, Query, description = "Pagination token"),
        ("timeout" = Option<u64>, Query, description = "Long-poll timeout in milliseconds")
    ),
    responses(
        (status = 200, description = "Event stream chunk", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_events_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/my_rooms` — Return the caller's room list summary.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/my_rooms",
    tag = "Client-Server",
    responses(
        (status = 200, description = "Rooms visible to the caller",
            body = serde_json::Value,
            example = json!({
                "rooms": [
                    {
                        "room_id": "!room:example.com",
                        "name": "General"
                    }
                ],
                "total": 1
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_my_rooms_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/search` — Search room events and users.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/search",
    tag = "Client-Server",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Search response", body = serde_json::Value),
        (status = 400, description = "Invalid search request")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn search_room_events_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/search_recipients` — Search DM recipients.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/search_recipients",
    tag = "Client-Server",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Recipient search response", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn search_recipients_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/search_rooms` — Search room metadata.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/search_rooms",
    tag = "Client-Server",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Room search response", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn search_rooms_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/rooms/{room_id}/context/{event_id}` — Fetch event context around a target event.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/rooms/{room_id}/context/{event_id}",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "Room ID"),
        ("event_id" = String, Path, description = "Anchor event ID"),
        ("limit" = Option<i64>, Query, description = "Context event limit")
    ),
    responses(
        (status = 200, description = "Context events around the anchor", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_event_context_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/rooms/{room_id}/hierarchy` — Return hierarchy information for a space.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/rooms/{room_id}/hierarchy",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "Space room ID"),
        ("from" = Option<String>, Query, description = "Pagination token"),
        ("limit" = Option<u32>, Query, description = "Maximum number of children"),
        ("max_depth" = Option<u32>, Query, description = "Maximum traversal depth"),
        ("suggested_only" = Option<bool>, Query, description = "Return suggested rooms only")
    ),
    responses(
        (status = 200, description = "Space hierarchy", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_room_hierarchy_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/rooms/{room_id}/timestamp_to_event` — Resolve the closest event for a timestamp.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/rooms/{room_id}/timestamp_to_event",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "Room ID"),
        ("ts" = i64, Query, description = "Target timestamp in milliseconds"),
        ("dir" = String, Query, description = "Search direction, such as `f` or `b`")
    ),
    responses(
        (status = 200, description = "Closest event",
            body = serde_json::Value,
            example = json!({
                "event_id": "$event:example.com",
                "origin_server_ts": 1718000000000_i64
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn timestamp_to_event_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/media/v3/upload` — Upload media content.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/media/v3/upload",
    tag = "Client-Server",
    params(
        ("filename" = Option<String>, Query, description = "Original filename"),
        ("content_type" = Option<String>, Query, description = "Declared media content type")
    ),
    request_body(content = String, content_type = "application/octet-stream", description = "Raw media bytes"),
    responses(
        (status = 200, description = "Media uploaded",
            body = serde_json::Value,
            example = json!({
                "content_uri": "mxc://example.com/abcdef"
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn upload_media_v3_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/media/v3/download/{server_name}/{media_id}` — Download media content.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/media/v3/download/{server_name}/{media_id}",
    tag = "Client-Server",
    params(
        ("server_name" = String, Path, description = "Owning homeserver name"),
        ("media_id" = String, Path, description = "Opaque media ID")
    ),
    responses(
        (status = 200, description = "Raw media bytes"),
        (status = 404, description = "Media not found")
    )
)]
pub fn download_media_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/media/v3/thumbnail/{server_name}/{media_id}` — Fetch a media thumbnail.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/media/v3/thumbnail/{server_name}/{media_id}",
    tag = "Client-Server",
    params(
        ("server_name" = String, Path, description = "Owning homeserver name"),
        ("media_id" = String, Path, description = "Opaque media ID"),
        ("width" = Option<u32>, Query, description = "Desired width"),
        ("height" = Option<u32>, Query, description = "Desired height"),
        ("method" = Option<String>, Query, description = "Thumbnail method, e.g. `crop` or `scale`")
    ),
    responses(
        (status = 200, description = "Thumbnail bytes"),
        (status = 404, description = "Media not found")
    )
)]
pub fn get_thumbnail_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/media/v3/preview_url` — Generate or fetch a URL preview.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/media/v3/preview_url",
    tag = "Client-Server",
    params(
        ("url" = String, Query, description = "Absolute URL to preview"),
        ("ts" = Option<i64>, Query, description = "Optional preview timestamp override")
    ),
    responses(
        (status = 200, description = "Preview metadata", body = serde_json::Value),
        (status = 400, description = "Invalid URL")
    )
)]
pub fn preview_url_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/rooms/{room_id}/report` — Report a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/rooms/{room_id}/report",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "Room to report")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Room report accepted", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn report_room_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/rooms/{room_id}/report/{event_id}` — Report a specific event.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/rooms/{room_id}/report/{event_id}",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "Room containing the event"),
        ("event_id" = String, Path, description = "Reported event ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Event report accepted", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn report_event_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v3/rooms/{room_id}/report/{event_id}/score` — Update a report score.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v3/rooms/{room_id}/report/{event_id}/score",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "Room containing the event"),
        ("event_id" = String, Path, description = "Reported event ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Report score updated", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn update_report_score_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/rooms/{room_id}/relations/{event_id}` — List all relations for an event.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/rooms/{room_id}/relations/{event_id}",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "Room containing the event"),
        ("event_id" = String, Path, description = "Target event ID"),
        ("limit" = Option<i64>, Query, description = "Maximum relations to return"),
        ("from" = Option<String>, Query, description = "Pagination token"),
        ("dir" = Option<String>, Query, description = "Direction")
    ),
    responses(
        (status = 200, description = "Relations response", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_relations_by_event_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/rooms/{room_id}/relations/{event_id}/{rel_type}` — List relations of one type.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/rooms/{room_id}/relations/{event_id}/{rel_type}",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "Room containing the event"),
        ("event_id" = String, Path, description = "Target event ID"),
        ("rel_type" = String, Path, description = "Relation type"),
        ("limit" = Option<i64>, Query, description = "Maximum relations to return"),
        ("from" = Option<String>, Query, description = "Pagination token"),
        ("dir" = Option<String>, Query, description = "Direction")
    ),
    responses(
        (status = 200, description = "Relations response", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_relations_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v3/rooms/{room_id}/relations/{event_id}/{rel_type}/{txn_id}` — Send a related event.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v3/rooms/{room_id}/relations/{event_id}/{rel_type}/{txn_id}",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "Room containing the target event"),
        ("event_id" = String, Path, description = "Target event ID"),
        ("rel_type" = String, Path, description = "Relation type"),
        ("txn_id" = String, Path, description = "Client transaction ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Relation event created",
            body = serde_json::Value,
            example = json!({
                "event_id": "$relation:example.com",
                "room_id": "!room:example.com"
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn send_relation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/rooms/{room_id}/aggregations/{event_id}/{rel_type}` — Aggregate relation counts.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/rooms/{room_id}/aggregations/{event_id}/{rel_type}",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "Room containing the event"),
        ("event_id" = String, Path, description = "Target event ID"),
        ("rel_type" = String, Path, description = "Relation type"),
        ("limit" = Option<i64>, Query, description = "Maximum groups to return"),
        ("from" = Option<String>, Query, description = "Pagination token"),
        ("dir" = Option<String>, Query, description = "Direction")
    ),
    responses(
        (status = 200, description = "Aggregation response", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_aggregations_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v3/rooms/{room_id}/send/m.reaction/{txn_id}` — Send an `m.reaction` event.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v3/rooms/{room_id}/send/m.reaction/{txn_id}",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "Target room ID"),
        ("txn_id" = String, Path, description = "Client transaction ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Reaction event created",
            body = serde_json::Value,
            example = json!({
                "event_id": "$reaction:example.com"
            })
        ),
        (status = 400, description = "Invalid relation payload")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn add_reaction_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/config/client` — Return homeserver client configuration.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/config/client",
    tag = "Health",
    responses(
        (status = 200, description = "Client configuration",
            body = serde_json::Value,
            example = json!({
                "homeserver": {
                    "base_url": "https://example.com",
                    "server_name": "example.com"
                },
                "identity_server": {
                    "base_url": "https://example.com"
                }
            })
        )
    )
)]
pub fn get_client_config_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/account/guest` — Return information about the current guest account.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/account/guest",
    tag = "Authentication",
    responses(
        (status = 200, description = "Guest account information",
            body = serde_json::Value,
            example = json!({
                "user_id": "@guest-1:example.com",
                "is_guest": true
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_guest_info_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/account/guest/upgrade` — Upgrade a guest account into a regular account.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/account/guest/upgrade",
    tag = "Authentication",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Guest account upgraded",
            body = serde_json::Value,
            example = json!({
                "success": true,
                "user_id": "@guest-1:example.com",
                "access_token": "new_access_token"
            })
        ),
        (status = 400, description = "Invalid upgrade payload")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn upgrade_guest_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/thirdparty/protocols` — List supported third-party protocols.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/thirdparty/protocols",
    tag = "Client-Server",
    responses(
        (status = 200, description = "Third-party protocol registry", body = serde_json::Value)
    )
)]
pub fn get_thirdparty_protocols_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/thirdparty/protocol/{protocol}` — Get metadata for one third-party protocol.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/thirdparty/protocol/{protocol}",
    tag = "Client-Server",
    params(
        ("protocol" = String, Path, description = "Third-party protocol identifier")
    ),
    responses(
        (status = 200, description = "Third-party protocol metadata",
            body = serde_json::Value,
            example = json!({
                "instances": [],
                "user_fields": [],
                "location_fields": []
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_thirdparty_protocol_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/thirdparty/location/{protocol}` — Search bridged locations via a protocol adapter.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/thirdparty/location/{protocol}",
    tag = "Client-Server",
    params(
        ("protocol" = String, Path, description = "Third-party protocol identifier"),
        ("alias" = Option<String>, Query, description = "Alias to look up"),
        ("search" = Option<String>, Query, description = "Search term"),
        ("server" = Option<String>, Query, description = "Third-party server"),
        ("channel" = Option<String>, Query, description = "Third-party channel")
    ),
    responses(
        (status = 200, description = "Location search result", body = serde_json::Value),
        (status = 404, description = "No bridge configured")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_thirdparty_location_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/thirdparty/location` — Search bridged locations by query only.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/thirdparty/location",
    tag = "Client-Server",
    params(
        ("alias" = Option<String>, Query, description = "Alias to look up"),
        ("search" = Option<String>, Query, description = "Search term"),
        ("server" = Option<String>, Query, description = "Third-party server"),
        ("channel" = Option<String>, Query, description = "Third-party channel")
    ),
    responses(
        (status = 200, description = "Location search result", body = serde_json::Value),
        (status = 404, description = "No bridge configured")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_thirdparty_location_by_alias_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/thirdparty/user` — Search bridged users by query only.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/thirdparty/user",
    tag = "Client-Server",
    params(
        ("userid" = Option<String>, Query, description = "Third-party user identifier"),
        ("search" = Option<String>, Query, description = "Search term"),
        ("nickname" = Option<String>, Query, description = "Nickname"),
        ("server" = Option<String>, Query, description = "Third-party server")
    ),
    responses(
        (status = 200, description = "User search result", body = serde_json::Value),
        (status = 404, description = "No bridge configured")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_thirdparty_user_by_id_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v3/pushrules/{scope}/{kind}/{rule_id}/actions` — Update push rule actions.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v3/pushrules/{scope}/{kind}/{rule_id}/actions",
    tag = "Client-Server",
    params(
        ("scope" = String, Path, description = "Push rule scope"),
        ("kind" = String, Path, description = "Push rule kind"),
        ("rule_id" = String, Path, description = "Push rule identifier")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Push rule actions updated",
            body = serde_json::Value,
            example = json!({
                "rule_id": ".m.rule.message",
                "actions": ["notify"],
                "updated_ts": 1718000000000_i64
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn set_push_rule_actions_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/pushrules/{scope}/{kind}/{rule_id}/enabled` — Read push rule enabled state.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/pushrules/{scope}/{kind}/{rule_id}/enabled",
    tag = "Client-Server",
    params(
        ("scope" = String, Path, description = "Push rule scope"),
        ("kind" = String, Path, description = "Push rule kind"),
        ("rule_id" = String, Path, description = "Push rule identifier")
    ),
    responses(
        (status = 200, description = "Push rule enabled flag",
            body = serde_json::Value,
            example = json!({
                "enabled": true
            })
        ),
        (status = 404, description = "Push rule not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_push_rule_enabled_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v3/pushrules/{scope}/{kind}/{rule_id}/enabled` — Update push rule enabled state.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v3/pushrules/{scope}/{kind}/{rule_id}/enabled",
    tag = "Client-Server",
    params(
        ("scope" = String, Path, description = "Push rule scope"),
        ("kind" = String, Path, description = "Push rule kind"),
        ("rule_id" = String, Path, description = "Push rule identifier")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Push rule enabled flag updated",
            body = serde_json::Value,
            example = json!({
                "rule_id": ".m.rule.message",
                "enabled": true,
                "updated_ts": 1718000000000_i64
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn set_push_rule_enabled_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/pushrules/` — Default push rule collection entrypoint.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/pushrules/",
    tag = "Client-Server",
    responses(
        (status = 200, description = "Default push rules", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_push_rules_default_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/pushrules/global/` — Default global push rule collection entrypoint.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/pushrules/global/",
    tag = "Client-Server",
    responses(
        (status = 200, description = "Default global push rules", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_push_rules_global_default_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/pushrules/` — Default push rule collection entrypoint on the r0 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/pushrules/",
    tag = "Client-Server",
    responses(
        (status = 200, description = "Default push rules", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_push_rules_default_r0_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/pushrules/global/` — Default global push rule collection entrypoint on the r0 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/pushrules/global/",
    tag = "Client-Server",
    responses(
        (status = 200, description = "Default global push rules", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_push_rules_global_default_r0_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/presence/{user_id}/status` — Read presence status using the v1 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/presence/{user_id}/status",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Target user ID")
    ),
    responses(
        (status = 200, description = "Presence status", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_presence_status_v1_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v1/presence/{user_id}/status` — Update presence status using the v1 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v1/presence/{user_id}/status",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Caller user ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Presence updated", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn set_presence_status_v1_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/presence/{user_id}/status` — Read presence status using the r0 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/presence/{user_id}/status",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Target user ID")
    ),
    responses(
        (status = 200, description = "Presence status", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_presence_status_r0_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/r0/presence/{user_id}/status` — Update presence status using the r0 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/r0/presence/{user_id}/status",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Caller user ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Presence updated", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn set_presence_status_r0_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/rooms/{room_id}/typing` — List currently typing users in a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/rooms/{room_id}/typing",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "Target room ID")
    ),
    responses(
        (status = 200, description = "Typing users",
            body = serde_json::Value,
            example = json!({
                "typing": ["@alice:example.com", "@bob:example.com"]
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_typing_users_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/rooms/{room_id}/typing/{user_id}` — Read a single user's typing state.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/rooms/{room_id}/typing/{user_id}",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "Target room ID"),
        ("user_id" = String, Path, description = "Target user ID")
    ),
    responses(
        (status = 200, description = "Typing flag",
            body = serde_json::Value,
            example = json!({
                "typing": true
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_user_typing_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v3/rooms/{room_id}/typing/{user_id}` — Set a user's typing state.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v3/rooms/{room_id}/typing/{user_id}",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "Target room ID"),
        ("user_id" = String, Path, description = "Caller user ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Typing state updated", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn set_typing_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/rooms/typing` — Bulk fetch typing state for multiple rooms.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/rooms/typing",
    tag = "Client-Server",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Per-room typing states",
            body = serde_json::Value,
            example = json!({
                "!room:example.com": {
                    "typing": ["@alice:example.com"]
                }
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn bulk_get_typing_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/rooms/{room_id}/typing` — List currently typing users in a room on the r0 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/rooms/{room_id}/typing",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "Target room ID")
    ),
    responses(
        (status = 200, description = "Typing users",
            body = serde_json::Value,
            example = json!({
                "typing": ["@alice:example.com", "@bob:example.com"]
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_typing_users_r0_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/rooms/{room_id}/typing/{user_id}` — Read a single user's typing state on the r0 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/rooms/{room_id}/typing/{user_id}",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "Target room ID"),
        ("user_id" = String, Path, description = "Target user ID")
    ),
    responses(
        (status = 200, description = "Typing flag",
            body = serde_json::Value,
            example = json!({
                "typing": true
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_user_typing_r0_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/r0/rooms/{room_id}/typing/{user_id}` — Set a user's typing state on the r0 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/r0/rooms/{room_id}/typing/{user_id}",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "Target room ID"),
        ("user_id" = String, Path, description = "Caller user ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Typing state updated", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn set_typing_r0_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/r0/rooms/typing` — Bulk fetch typing state for multiple rooms on the r0 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/r0/rooms/typing",
    tag = "Client-Server",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Per-room typing states",
            body = serde_json::Value,
            example = json!({
                "!room:example.com": {
                    "typing": ["@alice:example.com"]
                }
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn bulk_get_typing_r0_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v1/rendezvous` — Create a rendezvous session.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v1/rendezvous",
    tag = "Authentication",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Rendezvous session created",
            body = serde_json::Value,
            example = json!({
                "url": "matrix://rendezvous/example.com/session123",
                "session_id": "session123",
                "key": "secret"
            })
        ),
        (status = 400, description = "Invalid rendezvous request")
    )
)]
pub fn create_rendezvous_session_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/rendezvous/{session_id}` — Fetch rendezvous session metadata.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/rendezvous/{session_id}",
    tag = "Authentication",
    params(
        ("session_id" = String, Path, description = "Rendezvous session ID"),
        ("x-matrix-rendezvous-key" = Option<String>, Header, description = "Session access key")
    ),
    responses(
        (status = 200, description = "Rendezvous session",
            body = serde_json::Value,
            example = json!({
                "session_id": "session123",
                "intent": "login.start",
                "transport": "http.v1",
                "status": "pending",
                "created_ts": 1718000000000_i64,
                "expires_at": 1718000300000_i64
            })
        ),
        (status = 404, description = "Session not found")
    )
)]
pub fn get_rendezvous_session_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v1/rendezvous/{session_id}` — Update rendezvous session state.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v1/rendezvous/{session_id}",
    tag = "Authentication",
    params(
        ("session_id" = String, Path, description = "Rendezvous session ID"),
        ("x-matrix-rendezvous-key" = Option<String>, Header, description = "Session access key")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Rendezvous session updated", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn update_rendezvous_session_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_matrix/client/v1/rendezvous/{session_id}` — Delete a rendezvous session.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_matrix/client/v1/rendezvous/{session_id}",
    tag = "Authentication",
    params(
        ("session_id" = String, Path, description = "Rendezvous session ID"),
        ("x-matrix-rendezvous-key" = Option<String>, Header, description = "Session access key")
    ),
    responses(
        (status = 200, description = "Rendezvous session deleted", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn delete_rendezvous_session_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v1/rendezvous/{session_id}/messages` — Send a rendezvous message.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v1/rendezvous/{session_id}/messages",
    tag = "Authentication",
    params(
        ("session_id" = String, Path, description = "Rendezvous session ID"),
        ("x-matrix-rendezvous-key" = Option<String>, Header, description = "Session access key")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Rendezvous message stored",
            body = serde_json::Value,
            example = json!({
                "session_id": "session123",
                "message_id": "session123_1718000000000",
                "sent_ts": 1718000000000_i64
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn send_rendezvous_message_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/rendezvous/{session_id}/messages` — Fetch rendezvous messages.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/rendezvous/{session_id}/messages",
    tag = "Authentication",
    params(
        ("session_id" = String, Path, description = "Rendezvous session ID"),
        ("x-matrix-rendezvous-key" = Option<String>, Header, description = "Session access key")
    ),
    responses(
        (status = 200, description = "Rendezvous messages", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_rendezvous_messages_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/push/devices` — List registered push devices.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/push/devices",
    tag = "Client-Server",
    responses(
        (status = 200, description = "Registered push devices", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_push_devices_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/r0/push/devices` — Register a push device.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/r0/push/devices",
    tag = "Client-Server",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Push device registered", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn register_push_device_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_matrix/client/r0/push/devices/{device_id}` — Unregister a push device.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_matrix/client/r0/push/devices/{device_id}",
    tag = "Client-Server",
    params(
        ("device_id" = String, Path, description = "Push device identifier")
    ),
    responses(
        (status = 200, description = "Push device unregistered", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn unregister_push_device_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/r0/push/send` — Queue a push notification for delivery.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/r0/push/send",
    tag = "Client-Server",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Push notification queued",
            body = serde_json::Value,
            example = json!({
                "message": "Notification queued"
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn send_push_notification_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/push/rules` — List custom push notification rules.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/push/rules",
    tag = "Client-Server",
    responses(
        (status = 200, description = "Push notification rules", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_push_notification_rules_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/r0/push/rules` — Create a custom push notification rule.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/r0/push/rules",
    tag = "Client-Server",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Push notification rule created", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn create_push_notification_rule_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_matrix/client/r0/push/rules/{scope}/{kind}/{rule_id}` — Delete a custom push notification rule.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_matrix/client/r0/push/rules/{scope}/{kind}/{rule_id}",
    tag = "Client-Server",
    params(
        ("scope" = String, Path, description = "Push rule scope"),
        ("kind" = String, Path, description = "Push rule kind"),
        ("rule_id" = String, Path, description = "Push rule identifier")
    ),
    responses(
        (status = 200, description = "Push notification rule deleted", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn delete_push_notification_rule_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/thirdparty/location` — Query third-party locations without specifying a protocol.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/thirdparty/location",
    tag = "Client-Server",
    params(
        ("alias" = Option<String>, Query, description = "Third-party room alias"),
        ("search" = Option<String>, Query, description = "Free-text search string"),
        ("server" = Option<String>, Query, description = "Remote server hint"),
        ("channel" = Option<String>, Query, description = "Third-party channel identifier")
    ),
    responses(
        (status = 200, description = "Third-party locations", body = serde_json::Value),
        (status = 400, description = "No location bridge configured")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_thirdparty_location_r0_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/thirdparty/user` — Query third-party users without specifying a protocol.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/thirdparty/user",
    tag = "Client-Server",
    params(
        ("userid" = Option<String>, Query, description = "Third-party user identifier"),
        ("search" = Option<String>, Query, description = "Free-text search string"),
        ("nickname" = Option<String>, Query, description = "Third-party nickname"),
        ("server" = Option<String>, Query, description = "Remote server hint")
    ),
    responses(
        (status = 200, description = "Third-party users", body = serde_json::Value),
        (status = 400, description = "No user bridge configured")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_thirdparty_user_r0_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/oidc/userinfo` — Read OpenID Connect-compatible user information.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/oidc/userinfo",
    tag = "Authentication",
    responses(
        (status = 200, description = "OIDC userinfo",
            body = serde_json::Value,
            example = json!({
                "sub": "@alice:example.com",
                "name": "Alice",
                "picture": "mxc://example.com/avatar",
                "email": "alice@example.com"
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn oidc_userinfo_v3_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/oidc/token` — Exchange an authorization code or refresh token for a Matrix-compatible token response.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/oidc/token",
    tag = "Authentication",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "OIDC token response",
            body = serde_json::Value,
            example = json!({
                "access_token": "syt_abcdef",
                "token_type": "Bearer",
                "expires_in": 3600,
                "refresh_token": "syr_refresh",
                "scope": "openid profile email",
                "matrix_user_id": "@alice:example.com",
                "device_id": "OIDC1234"
            })
        ),
        (status = 400, description = "OIDC is not enabled or request validation failed")
    )
)]
pub fn oidc_token_v3_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/oidc/authorize` — Build an upstream OIDC authorization request.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/oidc/authorize",
    tag = "Authentication",
    params(
        ("response_type" = String, Query, description = "Must be `code`"),
        ("client_id" = String, Query, description = "OIDC client identifier"),
        ("redirect_uri" = String, Query, description = "OIDC callback URL"),
        ("scope" = Option<String>, Query, description = "Requested scopes"),
        ("state" = Option<String>, Query, description = "Optional client-supplied state"),
        ("nonce" = Option<String>, Query, description = "Optional OIDC nonce")
    ),
    responses(
        (status = 200, description = "Authorization request details",
            body = serde_json::Value,
            example = json!({
                "authorization_url": "https://idp.example.com/authorize?...",
                "state": "state123",
                "nonce": "nonce123",
                "code_verifier": "pkce-verifier"
            })
        ),
        (status = 400, description = "OIDC is not enabled or response_type is unsupported")
    )
)]
pub fn oidc_authorize_v3_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/oidc/callback` — Consume the upstream OIDC authorization callback.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/oidc/callback",
    tag = "Authentication",
    params(
        ("code" = Option<String>, Query, description = "Authorization code"),
        ("state" = Option<String>, Query, description = "Opaque state value"),
        ("error" = Option<String>, Query, description = "OIDC provider error"),
        ("error_description" = Option<String>, Query, description = "OIDC provider error details")
    ),
    responses(
        (status = 200, description = "Completed OIDC login",
            body = serde_json::Value,
            example = json!({
                "access_token": "syt_abcdef",
                "refresh_token": "syr_refresh",
                "expires_in": 3600,
                "device_id": "OIDC1234",
                "user_id": "@alice:example.com"
            })
        ),
        (status = 400, description = "OIDC callback parameters are invalid"),
        (status = 401, description = "OIDC state is missing, expired, or invalid")
    )
)]
pub fn oidc_callback_v3_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/oidc/userinfo` — Read OpenID Connect-compatible user information on the r0 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/oidc/userinfo",
    tag = "Authentication",
    responses(
        (status = 200, description = "OIDC userinfo",
            body = serde_json::Value,
            example = json!({
                "sub": "@alice:example.com",
                "name": "Alice",
                "picture": "mxc://example.com/avatar",
                "email": "alice@example.com"
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn oidc_userinfo_r0_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/r0/oidc/token` — Exchange an authorization code or refresh token on the r0 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/r0/oidc/token",
    tag = "Authentication",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "OIDC token response",
            body = serde_json::Value,
            example = json!({
                "access_token": "syt_abcdef",
                "token_type": "Bearer",
                "expires_in": 3600,
                "refresh_token": "syr_refresh",
                "scope": "openid profile email",
                "matrix_user_id": "@alice:example.com",
                "device_id": "OIDC1234"
            })
        ),
        (status = 400, description = "OIDC is not enabled or request validation failed")
    )
)]
pub fn oidc_token_r0_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/oidc/authorize` — Build an upstream OIDC authorization request on the r0 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/oidc/authorize",
    tag = "Authentication",
    params(
        ("response_type" = String, Query, description = "Must be `code`"),
        ("client_id" = String, Query, description = "OIDC client identifier"),
        ("redirect_uri" = String, Query, description = "OIDC callback URL"),
        ("scope" = Option<String>, Query, description = "Requested scopes"),
        ("state" = Option<String>, Query, description = "Optional client-supplied state"),
        ("nonce" = Option<String>, Query, description = "Optional OIDC nonce")
    ),
    responses(
        (status = 200, description = "Authorization request details",
            body = serde_json::Value,
            example = json!({
                "authorization_url": "https://idp.example.com/authorize?...",
                "state": "state123",
                "nonce": "nonce123",
                "code_verifier": "pkce-verifier"
            })
        ),
        (status = 400, description = "OIDC is not enabled or response_type is unsupported")
    )
)]
pub fn oidc_authorize_r0_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/oidc/callback` — Consume the upstream OIDC authorization callback on the r0 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/oidc/callback",
    tag = "Authentication",
    params(
        ("code" = Option<String>, Query, description = "Authorization code"),
        ("state" = Option<String>, Query, description = "Opaque state value"),
        ("error" = Option<String>, Query, description = "OIDC provider error"),
        ("error_description" = Option<String>, Query, description = "OIDC provider error details")
    ),
    responses(
        (status = 200, description = "Completed OIDC login",
            body = serde_json::Value,
            example = json!({
                "access_token": "syt_abcdef",
                "refresh_token": "syr_refresh",
                "expires_in": 3600,
                "device_id": "OIDC1234",
                "user_id": "@alice:example.com"
            })
        ),
        (status = 400, description = "OIDC callback parameters are invalid"),
        (status = 401, description = "OIDC state is missing, expired, or invalid")
    )
)]
pub fn oidc_callback_r0_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/app/v1/ping` — Verify application service access token validity.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/app/v1/ping",
    tag = "Application Service",
    responses(
        (status = 200, description = "Application service identity",
            body = serde_json::Value,
            example = json!({
                "as_id": "bridge_example"
            })
        ),
        (status = 401, description = "Missing or invalid bearer token")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn appservice_ping_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/app/v1/transactions/{as_id}/{txn_id}` — Receive a transaction for an application service.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/app/v1/transactions/{as_id}/{txn_id}",
    tag = "Application Service",
    params(
        ("as_id" = String, Path, description = "Application service ID"),
        ("txn_id" = String, Path, description = "Transaction ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Transaction accepted", body = serde_json::Value),
        (status = 401, description = "Missing or invalid bearer token"),
        (status = 403, description = "Application service ID mismatch")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn appservice_transactions_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/app/v1/users/{user_id}` — Query whether a user is in the application service namespace.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/app/v1/users/{user_id}",
    tag = "Application Service",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "User exists in the namespace", body = serde_json::Value),
        (status = 401, description = "Missing or invalid bearer token"),
        (status = 403, description = "User not in application service namespace")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn appservice_user_query_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/app/v1/rooms/{alias}` — Query whether a room alias is in the application service namespace.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/app/v1/rooms/{alias}",
    tag = "Application Service",
    params(
        ("alias" = String, Path, description = "Room alias")
    ),
    responses(
        (status = 200, description = "Alias exists in the namespace", body = serde_json::Value),
        (status = 401, description = "Missing or invalid bearer token"),
        (status = 403, description = "Room alias not in application service namespace")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn appservice_room_alias_query_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/app/v1/{as_id}` — Query application service metadata by ID.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/app/v1/{as_id}",
    tag = "Application Service",
    params(
        ("as_id" = String, Path, description = "Application service ID")
    ),
    responses(
        (status = 200, description = "Application service metadata",
            body = serde_json::Value,
            example = json!({
                "id": "bridge_example",
                "url": "https://bridge.example.com",
                "sender": "@bridgebot:example.com",
                "description": "Example bridge",
                "is_enabled": true,
                "protocols": ["irc"]
            })
        ),
        (status = 404, description = "Application service not found")
    )
)]
pub fn appservice_query_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/keys/rotation/status` — Read key rotation status.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/keys/rotation/status",
    tag = "Client-Server",
    responses(
        (status = 200, description = "Key rotation status",
            body = serde_json::Value,
            example = json!({
                "enabled": true,
                "status": {
                    "rotation_enabled": true
                },
                "user_last_rotation": 1718000000000_i64
            })
        ),
        (status = 403, description = "Admin privileges required")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn key_rotation_status_get_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v1/keys/rotation/status` — POST compatibility variant for reading key rotation status.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v1/keys/rotation/status",
    tag = "Client-Server",
    responses(
        (status = 200, description = "Key rotation status", body = serde_json::Value),
        (status = 403, description = "Admin privileges required")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn key_rotation_status_post_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v1/keys/rotation/rotate` — Force a key rotation.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v1/keys/rotation/rotate",
    tag = "Client-Server",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Rotation result",
            body = serde_json::Value,
            example = json!({
                "success": true,
                "message": "Keys rotated successfully",
                "has_new_key": true
            })
        ),
        (status = 403, description = "Admin privileges required")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn key_rotation_rotate_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/keys/rotation/history/{device_id}` — Read key rotation history for a device.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/keys/rotation/history/{device_id}",
    tag = "Client-Server",
    params(
        ("device_id" = String, Path, description = "Target device ID")
    ),
    responses(
        (status = 200, description = "Rotation history",
            body = serde_json::Value,
            example = json!({
                "device_id": "DEVICE123",
                "rotations": [
                    {
                        "key_id": "ed25519:1",
                        "rotated_ts": 1718000000000_i64
                    }
                ]
            })
        ),
        (status = 403, description = "Admin privileges required")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn key_rotation_history_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v1/keys/rotation/revoke` — Revoke an old key.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v1/keys/rotation/revoke",
    tag = "Client-Server",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Revocation result",
            body = serde_json::Value,
            example = json!({
                "success": true,
                "revoked": 1,
                "message": "Successfully revoked key ed25519:1"
            })
        ),
        (status = 400, description = "key_id is required"),
        (status = 403, description = "Admin privileges required")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn key_rotation_revoke_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v1/keys/rotation/config` — Update key rotation configuration.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v1/keys/rotation/config",
    tag = "Client-Server",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Persisted key rotation configuration",
            body = serde_json::Value,
            example = json!({
                "enabled": true,
                "interval_ms": 86400000_i64,
                "rotation_interval_days": 7,
                "rotation_threshold_days": 1,
                "grace_period_minutes": 30
            })
        ),
        (status = 403, description = "Admin privileges required")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn key_rotation_config_put_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v1/keys/rotation/config` — POST compatibility variant for updating key rotation configuration.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v1/keys/rotation/config",
    tag = "Client-Server",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Persisted key rotation configuration", body = serde_json::Value),
        (status = 403, description = "Admin privileges required")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn key_rotation_config_post_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/keys/rotation/check` — Check whether key rotation is needed.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/keys/rotation/check",
    tag = "Client-Server",
    params(
        ("key_id" = Option<String>, Query, description = "Optional key ID to check")
    ),
    responses(
        (status = 200, description = "Rotation requirement",
            body = serde_json::Value,
            example = json!({
                "needs_rotation": true,
                "last_rotation": 1718000000000_i64,
                "interval_ms": 86400000
            })
        ),
        (status = 403, description = "Admin privileges required")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn key_rotation_check_get_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v1/keys/rotation/check` — POST compatibility variant for checking whether key rotation is needed.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v1/keys/rotation/check",
    tag = "Client-Server",
    params(
        ("key_id" = Option<String>, Query, description = "Optional key ID to check")
    ),
    responses(
        (status = 200, description = "Rotation requirement", body = serde_json::Value),
        (status = 403, description = "Admin privileges required")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn key_rotation_check_post_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/saml/metadata` — Fetch SAML IdP metadata using the r0 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/saml/metadata",
    tag = "Authentication",
    responses(
        (status = 200, description = "SAML IdP metadata",
            body = serde_json::Value,
            example = json!({
                "entity_id": "https://idp.example.com/metadata",
                "sso_url": "https://idp.example.com/sso",
                "slo_url": "https://idp.example.com/slo",
                "certificate": "MIIC..."
            })
        ),
        (status = 400, description = "SAML authentication is not enabled")
    )
)]
pub fn get_saml_metadata_r0_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/saml/sp_metadata` — Fetch generated SP metadata using the r0 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/saml/sp_metadata",
    tag = "Authentication",
    responses(
        (status = 200, description = "SAML SP metadata XML", body = String),
        (status = 400, description = "SAML authentication is not enabled")
    )
)]
pub fn get_sp_metadata_r0_doc() -> String {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/saml/metadata` — Fetch SAML IdP metadata using the v3 path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/saml/metadata",
    tag = "Authentication",
    responses(
        (status = 200, description = "SAML IdP metadata",
            body = serde_json::Value,
            example = json!({
                "entity_id": "https://idp.example.com/metadata",
                "sso_url": "https://idp.example.com/sso",
                "slo_url": "https://idp.example.com/slo",
                "certificate": "MIIC..."
            })
        ),
        (status = 400, description = "SAML authentication is not enabled")
    )
)]
pub fn get_saml_metadata_v3_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/saml/sp_metadata` — Fetch generated SP metadata using the v3 path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/saml/sp_metadata",
    tag = "Authentication",
    responses(
        (status = 200, description = "SAML SP metadata XML", body = String),
        (status = 400, description = "SAML authentication is not enabled")
    )
)]
pub fn get_sp_metadata_v3_doc() -> String {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/unstable/org.matrix.msc2965/auth_metadata` — Return OAuth/OIDC auth metadata.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/unstable/org.matrix.msc2965/auth_metadata",
    tag = "Authentication",
    responses(
        (status = 200, description = "OIDC discovery metadata", body = serde_json::Value),
        (status = 404, description = "OIDC not configured")
    )
)]
pub fn get_auth_metadata_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/unstable/org.matrix.msc2965/auth_issuer` — Return the configured OIDC issuer.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/unstable/org.matrix.msc2965/auth_issuer",
    tag = "Authentication",
    responses(
        (status = 200, description = "OIDC issuer", body = serde_json::Value),
        (status = 404, description = "OIDC not configured")
    )
)]
pub fn get_auth_issuer_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device` — Fetch the caller's dehydrated device.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device",
    tag = "Client-Server",
    responses(
        (status = 200, description = "Dehydrated device", body = serde_json::Value),
        (status = 404, description = "No dehydrated device")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_dehydrated_device_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device` — Upload or replace a dehydrated device.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device",
    tag = "Client-Server",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Dehydrated device stored",
            body = serde_json::Value,
            example = json!({
                "device_id": "DEHYDRATED1"
            })
        ),
        (status = 400, description = "Invalid dehydrated device payload"),
        (status = 403, description = "Cross-signing or secret storage prerequisites not met")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn put_dehydrated_device_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device` — Delete the caller's dehydrated device.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device",
    tag = "Client-Server",
    responses(
        (status = 200, description = "Dehydrated device deleted",
            body = serde_json::Value,
            example = json!({
                "device_id": "DEHYDRATED1"
            })
        ),
        (status = 404, description = "No dehydrated device")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn delete_dehydrated_device_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device/{device_id}/events` — Claim queued to-device events for a dehydrated device.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device/{device_id}/events",
    tag = "Client-Server",
    params(
        ("device_id" = String, Path, description = "Dehydrated device identifier")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Queued to-device events", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn post_dehydrated_device_events_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/version` — Compatibility alias for server version metadata.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/version",
    tag = "Health",
    responses(
        (status = 200, description = "Server implementation version",
            body = serde_json::Value,
            example = json!({
                "server": {
                    "name": "synapse-rust",
                    "version": env!("CARGO_PKG_VERSION")
                }
            })
        )
    )
)]
pub fn get_server_version_r0_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v1/sync` — Sliding sync entrypoint exposed on the v1 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v1/sync",
    tag = "Client-Server",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Sliding sync response", body = serde_json::Value),
        (status = 429, description = "Rate limited")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn sliding_sync_v1_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/rooms/{room_id}/ephemeral` — Fetch ephemeral room events.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/rooms/{room_id}/ephemeral",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "Target room ID"),
        ("limit" = Option<i64>, Query, description = "Maximum number of ephemeral events to return")
    ),
    responses(
        (status = 200, description = "Ephemeral room events",
            body = serde_json::Value,
            example = json!({
                "chunk": [
                    {
                        "type": "m.typing",
                        "content": {
                            "user_ids": ["@alice:example.com"]
                        }
                    }
                ],
                "start": null,
                "end": null
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_ephemeral_events_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v1/rooms/{room_id}/replies/{event_id}/redact` — Redact a thread reply.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v1/rooms/{room_id}/replies/{event_id}/redact",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "Target room ID"),
        ("event_id" = String, Path, description = "Reply event ID to redact")
    ),
    request_body = Option<serde_json::Value>,
    responses(
        (status = 200, description = "Reply redacted",
            body = serde_json::Value,
            example = json!({
                "redacted": true,
                "event_id": "$reply:example.com"
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn redact_thread_reply_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/room_auth/{room_id}` — Get the auth chain for a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/federation/v1/room_auth/{room_id}",
    tag = "Federation",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
        ("auth_event_ids" = Vec<String>, Query, description = "The event IDs to get auth for")
    ),
    responses(
        (status = 200, description = "Auth chain", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn get_room_auth_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/get_joining_rules/{room_id}` — Get the joining rules for a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/federation/v1/get_joining_rules/{room_id}",
    tag = "Federation",
    params(
        ("room_id" = String, Path, description = "The ID of the room")
    ),
    responses(
        (status = 200, description = "Joining rules", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn get_joining_rules_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/federation/v1/event_auth` — Get auth events for a list of events.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/federation/v1/event_auth",
    tag = "Federation",
    params(
        ("room_id" = String, Query, description = "The ID of the room"),
        ("event_ids" = Vec<String>, Query, description = "The event IDs to get auth for")
    ),
    responses(
        (status = 200, description = "Auth events", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn event_auth_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/federation/v1/keys/claim` — Claim one-time keys (legacy extension).
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/federation/v1/keys/claim",
    tag = "Federation",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Claimed keys", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn legacy_keys_claim_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/federation/v1/keys/query` — Query device keys (legacy extension).
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/federation/v1/keys/query",
    tag = "Federation",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Device keys", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn legacy_keys_query_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/federation/v1/keys/upload` — Upload device keys (legacy extension).
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/federation/v1/keys/upload",
    tag = "Federation",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Success", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn keys_upload_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/federation/v2/key/clone` — Clone server keys (trusted extension).
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/federation/v2/key/clone",
    tag = "Federation",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Success")
    )
)]
pub fn key_clone_federation_doc() -> String {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/unstable/org.matrix.msc3575/sync` — Sliding sync endpoint (MSC3575).
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/unstable/org.matrix.msc3575/sync",
    tag = "Unstable MSC",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Sync response", body = serde_json::Value),
        (status = 401, description = "Unauthorized")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn sliding_sync_msc3575_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/unstable/org.matrix.simplified_msc3575/sync` — Simplified sliding sync endpoint.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/unstable/org.matrix.simplified_msc3575/sync",
    tag = "Unstable MSC",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Sync response", body = serde_json::Value),
        (status = 401, description = "Unauthorized")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn sliding_sync_simplified_msc3575_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/unstable/uk.tcpip.msc4133/profile/{user_id}` — Get extended profile (MSC4133).
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/unstable/uk.tcpip.msc4133/profile/{user_id}",
    tag = "Unstable MSC",
    params(
        ("user_id" = String, Path, description = "The ID of the user")
    ),
    responses(
        (status = 200, description = "Profile document", body = serde_json::Value),
        (status = 404, description = "Not Found")
    )
)]
pub fn get_extended_profile_msc4133_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/unstable/uk.tcpip.msc4133/profile/{user_id}/{key_name}` — Get extended profile field.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/unstable/uk.tcpip.msc4133/profile/{user_id}/{key_name}",
    tag = "Unstable MSC",
    params(
        ("user_id" = String, Path, description = "The ID of the user"),
        ("key_name" = String, Path, description = "The field name")
    ),
    responses(
        (status = 200, description = "Field value", body = serde_json::Value),
        (status = 404, description = "Not Found")
    )
)]
pub fn get_extended_profile_field_msc4133_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/unstable/uk.tcpip.msc4133/profile/{user_id}/{key_name}` — Update extended profile field.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/unstable/uk.tcpip.msc4133/profile/{user_id}/{key_name}",
    tag = "Unstable MSC",
    params(
        ("user_id" = String, Path, description = "The ID of the user"),
        ("key_name" = String, Path, description = "The field name")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Updated", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn put_extended_profile_field_msc4133_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_matrix/client/unstable/uk.tcpip.msc4133/profile/{user_id}/{key_name}` — Delete extended profile field.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_matrix/client/unstable/uk.tcpip.msc4133/profile/{user_id}/{key_name}",
    tag = "Unstable MSC",
    params(
        ("user_id" = String, Path, description = "The ID of the user"),
        ("key_name" = String, Path, description = "The field name")
    ),
    responses(
        (status = 200, description = "Deleted", body = serde_json::Value),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Not Found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn delete_extended_profile_field_msc4133_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/r0/create_dm` — Create a direct message room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/r0/create_dm",
    tag = "Private Extension - DM",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "DM room created", body = serde_json::Value),
        (status = 401, description = "Unauthorized"),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn create_dm_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/direct` — Get direct message rooms map.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/direct",
    tag = "Private Extension - DM",
    responses(
        (status = 200, description = "Direct rooms map", body = serde_json::Value),
        (status = 401, description = "Unauthorized"),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_direct_map_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/r0/direct/{room_id}` — Update direct message room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/r0/direct/{room_id}",
    tag = "Private Extension - DM",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Updated", body = serde_json::Value),
        (status = 401, description = "Unauthorized"),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn update_direct_map_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/voice/config` — Get voice configuration.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/voice/config",
    tag = "Private Extension - Voice",
    responses(
        (status = 200, description = "Voice config", body = serde_json::Value),
    ),
)]
pub fn get_voice_config_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/r0/voice/upload` — Upload a voice message.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/r0/voice/upload",
    tag = "Private Extension - Voice",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Uploaded", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn upload_voice_message_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/voice/stats` — Get voice stats for current user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/voice/stats",
    tag = "Private Extension - Voice",
    responses(
        (status = 200, description = "Voice stats", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_voice_stats_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/voice/room/{room_id}/stats` — Get voice stats for a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/voice/room/{room_id}/stats",
    tag = "Private Extension - Voice",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
    ),
    responses(
        (status = 200, description = "Room voice stats", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_room_voice_stats_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/voice/room/{room_id}` — List voice messages in a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/voice/room/{room_id}",
    tag = "Private Extension - Voice",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
        ("limit" = Option<i64>, Query, description = "Maximum messages to return"),
        ("from" = Option<i64>, Query, description = "Pagination offset"),
    ),
    responses(
        (status = 200, description = "Voice messages", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_room_voice_messages_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/voice/{media_id}` — Get a voice message by media ID.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/voice/{media_id}",
    tag = "Private Extension - Voice",
    params(
        ("media_id" = String, Path, description = "The media ID"),
    ),
    responses(
        (status = 200, description = "Voice content"),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_voice_message_content_doc() -> String {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/voice/{media_id}/convert` — Convert a voice message.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/voice/{media_id}/convert",
    tag = "Private Extension - Voice",
    params(
        ("media_id" = String, Path, description = "The media ID"),
    ),
    responses(
        (status = 200, description = "Converted", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn convert_voice_message_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/voice/{media_id}/optimize` — Optimize a voice message.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/voice/{media_id}/optimize",
    tag = "Private Extension - Voice",
    params(
        ("media_id" = String, Path, description = "The media ID"),
    ),
    responses(
        (status = 200, description = "Optimized", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn optimize_voice_message_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/voice/{media_id}/transcribe` — Transcribe a voice message.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/voice/{media_id}/transcribe",
    tag = "Private Extension - Voice",
    params(
        ("media_id" = String, Path, description = "The media ID"),
    ),
    responses(
        (status = 200, description = "Transcribed", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn transcribe_voice_message_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v1/widgets` — Create a widget.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v1/widgets",
    tag = "Private Extension - Widget",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Widget created", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn create_widget_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/widgets/{widget_id}` — Get a widget.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/widgets/{widget_id}",
    tag = "Private Extension - Widget",
    params(
        ("widget_id" = String, Path, description = "The ID of the widget"),
    ),
    responses(
        (status = 200, description = "Widget details", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_widget_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v1/widgets/{widget_id}` — Update a widget.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v1/widgets/{widget_id}",
    tag = "Private Extension - Widget",
    params(
        ("widget_id" = String, Path, description = "The ID of the widget"),
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Widget updated", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn update_widget_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_matrix/client/v1/widgets/{widget_id}` — Delete a widget.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_matrix/client/v1/widgets/{widget_id}",
    tag = "Private Extension - Widget",
    params(
        ("widget_id" = String, Path, description = "The ID of the widget"),
    ),
    responses(
        (status = 200, description = "Widget deleted", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn delete_widget_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/widgets/{widget_id}/config` — Get widget configuration.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/widgets/{widget_id}/config",
    tag = "Private Extension - Widget",
    params(
        ("widget_id" = String, Path, description = "The ID of the widget"),
    ),
    responses(
        (status = 200, description = "Widget config", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_widget_config_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/rooms/{room_id}/widgets` — Get room widgets.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/rooms/{room_id}/widgets",
    tag = "Private Extension - Widget",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
    ),
    responses(
        (status = 200, description = "Room widgets", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_room_widgets_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/rooms/{room_id}/widgets/jitsi/config` — Get Jitsi config.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/rooms/{room_id}/widgets/jitsi/config",
    tag = "Private Extension - Widget",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
    ),
    responses(
        (status = 200, description = "Jitsi config", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_jitsi_config_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/rooms/{room_id}/widgets/{widget_id}/send` — Send a widget message.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/rooms/{room_id}/widgets/{widget_id}/send",
    tag = "Private Extension - Widget",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
        ("widget_id" = String, Path, description = "The ID of the widget"),
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Message sent", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn send_room_widget_message_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v1/widgets/{widget_id}/permissions` — Set widget permissions.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v1/widgets/{widget_id}/permissions",
    tag = "Private Extension - Widget",
    params(
        ("widget_id" = String, Path, description = "The ID of the widget"),
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Permissions set", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn set_widget_permission_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/widgets/{widget_id}/permissions` — Get widget permissions.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/widgets/{widget_id}/permissions",
    tag = "Private Extension - Widget",
    params(
        ("widget_id" = String, Path, description = "The ID of the widget"),
    ),
    responses(
        (status = 200, description = "Permissions", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_widget_permissions_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_matrix/client/v1/widgets/{widget_id}/permissions/{user_id}` — Delete widget permissions.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_matrix/client/v1/widgets/{widget_id}/permissions/{user_id}",
    tag = "Private Extension - Widget",
    params(
        ("widget_id" = String, Path, description = "The ID of the widget"),
        ("user_id" = String, Path, description = "The ID of the user"),
    ),
    responses(
        (status = 200, description = "Deleted", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn delete_widget_permission_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v1/widgets/{widget_id}/sessions` — Create a widget session.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v1/widgets/{widget_id}/sessions",
    tag = "Private Extension - Widget",
    params(
        ("widget_id" = String, Path, description = "The ID of the widget"),
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Session created", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn create_widget_session_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/widgets/{widget_id}/sessions` — Get widget sessions.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/widgets/{widget_id}/sessions",
    tag = "Private Extension - Widget",
    params(
        ("widget_id" = String, Path, description = "The ID of the widget"),
    ),
    responses(
        (status = 200, description = "Session list", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_widget_sessions_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/widgets/sessions/{session_id}` — Get a widget session.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/widgets/sessions/{session_id}",
    tag = "Private Extension - Widget",
    params(
        ("session_id" = String, Path, description = "The ID of the session"),
    ),
    responses(
        (status = 200, description = "Session details", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_widget_session_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_matrix/client/v1/widgets/sessions/{session_id}` — Terminate a widget session.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_matrix/client/v1/widgets/sessions/{session_id}",
    tag = "Private Extension - Widget",
    params(
        ("session_id" = String, Path, description = "The ID of the session"),
    ),
    responses(
        (status = 200, description = "Session terminated", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn terminate_widget_session_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/admin/v1/external_services` — Register an external service.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/admin/v1/external_services",
    tag = "Private Extension - External Services",
    request_body = serde_json::Value,
    responses(
        (status = 201, description = "Registered", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn register_external_service_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/admin/v1/external_services` — List external services.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/admin/v1/external_services",
    tag = "Private Extension - External Services",
    params(
        ("service_type" = Option<String>, Query, description = "Filter by service type"),
    ),
    responses(
        (status = 200, description = "Service list", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn list_external_services_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_matrix/admin/v1/external_services/{as_id}` — Unregister an external service.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_matrix/admin/v1/external_services/{as_id}",
    tag = "Private Extension - External Services",
    params(
        ("as_id" = String, Path, description = "The ID of the service"),
    ),
    responses(
        (status = 200, description = "Unregistered", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn unregister_external_service_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/admin/v1/external_services/{as_id}` — Update an external service.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/admin/v1/external_services/{as_id}",
    tag = "Private Extension - External Services",
    params(
        ("as_id" = String, Path, description = "The ID of the service"),
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Updated", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn update_external_service_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/admin/v1/external_services/{as_id}` — Get an external service.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/admin/v1/external_services/{as_id}",
    tag = "Private Extension - External Services",
    params(
        ("as_id" = String, Path, description = "The ID of the service"),
    ),
    responses(
        (status = 200, description = "Service details", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_external_service_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v1/rooms/{room_id}/burn` — Enable burn after read in a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v1/rooms/{room_id}/burn",
    tag = "Private Extension - Burn",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Updated", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn enable_burn_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/rooms/{room_id}/burn` — Get burn settings for a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/rooms/{room_id}/burn",
    tag = "Private Extension - Burn",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
    ),
    responses(
        (status = 200, description = "Burn settings", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_burn_settings_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/rooms/{room_id}/burn/pending` — Get pending burn messages.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/rooms/{room_id}/burn/pending",
    tag = "Private Extension - Burn",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
    ),
    responses(
        (status = 200, description = "Pending burns", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_pending_burns_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v1/rooms/{room_id}/burn/{event_id}` — Mark message as read (trigger burn).
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v1/rooms/{room_id}/burn/{event_id}",
    tag = "Private Extension - Burn",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
        ("event_id" = String, Path, description = "The ID of the event"),
    ),
    responses(
        (status = 200, description = "Marked as read", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn mark_burn_read_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_matrix/client/v1/rooms/{room_id}/burn/{event_id}` — Cancel a pending burn.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_matrix/client/v1/rooms/{room_id}/burn/{event_id}",
    tag = "Private Extension - Burn",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
        ("event_id" = String, Path, description = "The ID of the event"),
    ),
    responses(
        (status = 200, description = "Cancelled", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn cancel_burn_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/friends` — Get friends list.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/friends",
    tag = "Private Extension - Friends",
    responses(
        (status = 200, description = "Friends list", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_friends_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v1/friends` — Send a friend request.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v1/friends",
    tag = "Private Extension - Friends",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Request sent", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn send_friend_request_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/friends/search` — Search the friend directory.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/friends/search",
    tag = "Private Extension - Friends",
    responses(
        (status = 200, description = "Search results", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn search_friend_directory_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/friends/requests/incoming` — Get incoming friend requests.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/friends/requests/incoming",
    tag = "Private Extension - Friends",
    responses(
        (status = 200, description = "Incoming requests", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_incoming_requests_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/friends/requests/outgoing` — Get outgoing friend requests.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/friends/requests/outgoing",
    tag = "Private Extension - Friends",
    responses(
        (status = 200, description = "Outgoing requests", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_outgoing_requests_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/friends/request/received` — Get received friend requests.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/friends/request/received",
    tag = "Private Extension - Friends",
    responses(
        (status = 200, description = "Received requests", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_received_requests_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v1/friends/request/{user_id}/accept` — Accept a friend request.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v1/friends/request/{user_id}/accept",
    tag = "Private Extension - Friends",
    params(
        ("user_id" = String, Path, description = "The ID of the user"),
    ),
    responses(
        (status = 200, description = "Accepted", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn accept_friend_request_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v1/friends/request/{user_id}/reject` — Reject a friend request.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v1/friends/request/{user_id}/reject",
    tag = "Private Extension - Friends",
    params(
        ("user_id" = String, Path, description = "The ID of the user"),
    ),
    responses(
        (status = 200, description = "Rejected", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn reject_friend_request_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v1/friends/request/{user_id}/cancel` — Cancel a friend request.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v1/friends/request/{user_id}/cancel",
    tag = "Private Extension - Friends",
    params(
        ("user_id" = String, Path, description = "The ID of the user"),
    ),
    responses(
        (status = 200, description = "Cancelled", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn cancel_friend_request_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/friends/check/{user_id}` — Check friendship status.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/friends/check/{user_id}",
    tag = "Private Extension - Friends",
    params(
        ("user_id" = String, Path, description = "The ID of the user"),
    ),
    responses(
        (status = 200, description = "Status", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn check_friendship_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/friends/suggestions` — Get friend suggestions.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/friends/suggestions",
    tag = "Private Extension - Friends",
    responses(
        (status = 200, description = "Suggestions", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_friend_suggestions_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_matrix/client/v1/friends/{user_id}` — Remove a friend.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_matrix/client/v1/friends/{user_id}",
    tag = "Private Extension - Friends",
    params(
        ("user_id" = String, Path, description = "The ID of the user"),
    ),
    responses(
        (status = 200, description = "Removed", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn remove_friend_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v1/friends/{user_id}/note` — Update a friend note.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v1/friends/{user_id}/note",
    tag = "Private Extension - Friends",
    params(
        ("user_id" = String, Path, description = "The ID of the user"),
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Updated", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn update_friend_note_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/friends/{user_id}/status` — Get a friend status.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/friends/{user_id}/status",
    tag = "Private Extension - Friends",
    params(
        ("user_id" = String, Path, description = "The ID of the user"),
    ),
    responses(
        (status = 200, description = "Status", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_friend_status_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v1/friends/{user_id}/status` — Update a friend status.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v1/friends/{user_id}/status",
    tag = "Private Extension - Friends",
    params(
        ("user_id" = String, Path, description = "The ID of the user"),
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Updated", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn update_friend_status_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/friends/{user_id}/info` — Get a friend info.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/friends/{user_id}/info",
    tag = "Private Extension - Friends",
    params(
        ("user_id" = String, Path, description = "The ID of the user"),
    ),
    responses(
        (status = 200, description = "Info", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_friend_info_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v1/friends/{user_id}/displayname` — Update a friend display name.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v1/friends/{user_id}/displayname",
    tag = "Private Extension - Friends",
    params(
        ("user_id" = String, Path, description = "The ID of the user"),
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Updated", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn update_friend_displayname_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/friends/groups` — Get friend groups.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/friends/groups",
    tag = "Private Extension - Friends",
    responses(
        (status = 200, description = "Groups list", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_friend_groups_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v1/friends/groups` — Create a friend group.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v1/friends/groups",
    tag = "Private Extension - Friends",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Group created", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn create_friend_group_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_matrix/client/v1/friends/groups/{group_id}` — Delete a friend group.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_matrix/client/v1/friends/groups/{group_id}",
    tag = "Private Extension - Friends",
    params(
        ("group_id" = String, Path, description = "The ID of the group"),
    ),
    responses(
        (status = 200, description = "Deleted", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn delete_friend_group_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v1/friends/groups/{group_id}/name` — Rename a friend group.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v1/friends/groups/{group_id}/name",
    tag = "Private Extension - Friends",
    params(
        ("group_id" = String, Path, description = "The ID of the group"),
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Renamed", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn rename_friend_group_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/friends/groups/{group_id}/friends` — Get friends in a group.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/friends/groups/{group_id}/friends",
    tag = "Private Extension - Friends",
    params(
        ("group_id" = String, Path, description = "The ID of the group"),
    ),
    responses(
        (status = 200, description = "Friends in group", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_friend_group_friends_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v1/friends/groups/{group_id}/add/{user_id}` — Add a friend to a group.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v1/friends/groups/{group_id}/add/{user_id}",
    tag = "Private Extension - Friends",
    params(
        ("group_id" = String, Path, description = "The ID of the group"),
        ("user_id" = String, Path, description = "The ID of the user"),
    ),
    responses(
        (status = 200, description = "Added", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn add_friend_to_group_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v1/friends/groups/{group_id}/remove/{user_id}` — Remove a friend from a group.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v1/friends/groups/{group_id}/remove/{user_id}",
    tag = "Private Extension - Friends",
    params(
        ("group_id" = String, Path, description = "The ID of the group"),
        ("user_id" = String, Path, description = "The ID of the user"),
    ),
    responses(
        (status = 200, description = "Removed", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn remove_friend_from_group_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/friends/{user_id}/groups` — Get groups a friend is in.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/friends/{user_id}/groups",
    tag = "Private Extension - Friends",
    params(
        ("user_id" = String, Path, description = "The ID of the user"),
    ),
    responses(
        (status = 200, description = "Groups list", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_friend_groups_for_user_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/friends/dm/{user_id}` — Get or create a DM with a friend.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/friends/dm/{user_id}",
    tag = "Private Extension - Friends",
    params(
        ("user_id" = String, Path, description = "The ID of the user"),
    ),
    responses(
        (status = 200, description = "DM room", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_friend_dm_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}
