#![cfg(feature = "openapi-docs")]


/// `GET /_synapse/admin/v1/users` — List registered users (Admin only).
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/users",
    tag = "Admin",
    params(
        ("from" = Option<String>, Query, description = "Pagination token"),
        ("limit" = Option<i64>, Query, description = "Maximum number of users to return"),
        ("name" = Option<String>, Query, description = "Filter by username (wildcard)")
    ),
    responses(
        (status = 200, description = "List of users",
            body = serde_json::Value,
            example = json!({
                "users": [
                    {
                        "user_id": "@alice:example.com",
                        "displayname": "Alice",
                        "is_admin": false,
                        "deactivated": false
                    }
                ],
                "next_token": "12345",
                "total": 1
            })
        ),
        (status = 403, description = "Admin only")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn list_users_admin_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/rooms` — List rooms on the server (Admin only).
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/rooms",
    tag = "Admin",
    params(
        ("from" = Option<String>, Query, description = "Pagination token"),
        ("limit" = Option<i64>, Query, description = "Maximum number of rooms to return"),
        ("order_by" = Option<String>, Query, description = "Field to order by (name, canonical_alias, joined_members, joined_local_members, version, creator, encryption, is_public)"),
        ("dir" = Option<String>, Query, description = "Direction (f, b)")
    ),
    responses(
        (status = 200, description = "List of rooms",
            body = serde_json::Value,
            example = json!({
                "rooms": [
                    {
                        "room_id": "!abc:example.com",
                        "name": "General",
                        "canonical_alias": "#general:example.com",
                        "joined_members": 42,
                        "is_public": true
                    }
                ],
                "next_batch": "56789",
                "total_rooms": 1
            })
        ),
        (status = 403, description = "Admin only")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn list_rooms_admin_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_synapse/admin/v1/users/{user_id}` — Delete a user account.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_synapse/admin/v1/users/{user_id}",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "User deleted",
            body = serde_json::Value,
            example = json!({
                "user_id": "@alice:example.com",
                "deleted": true
            })
        ),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_delete_user_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/admin/v1/users/{user_id}/evict` — Evict a user from all joined rooms.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/admin/v1/users/{user_id}/evict",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "User evicted from joined rooms",
            body = serde_json::Value,
            example = json!({
                "user_id": "@alice:example.com",
                "rooms_evicted": 2,
                "rooms": ["!room1:example.com", "!room2:example.com"],
                "failures": []
            })
        ),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_evict_user_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_synapse/admin/v1/users/{user_id}/admin` — Update whether a user is an admin.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_synapse/admin/v1/users/{user_id}/admin",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Admin flag updated", body = serde_json::Value, example = json!({"success": true})),
        (status = 400, description = "Missing admin field"),
        (status = 403, description = "Only super_admin can change privileges"),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_set_user_admin_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/admin/v1/users/{user_id}/deactivate` — Deactivate a user account.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/admin/v1/users/{user_id}/deactivate",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "User deactivated",
            body = serde_json::Value,
            example = json!({"id_server_unbind_result": "success"})
        ),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_deactivate_user_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/admin/v1/users/{user_id}/password` — Reset a user's password.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/admin/v1/users/{user_id}/password",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Password reset completed", body = serde_json::Value),
        (status = 400, description = "Password does not satisfy validation requirements"),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_reset_user_password_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v2/users/{user_id}` — Read detailed information for one user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v2/users/{user_id}",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "Detailed user information",
            body = serde_json::Value,
            example = json!({
                "name": "@alice:example.com",
                "user_id": "@alice:example.com",
                "is_guest": false,
                "admin": false,
                "deactivated": false,
                "displayname": "Alice",
                "avatar_url": "mxc://example.com/alice",
                "created_ts": 1718000000000_i64,
                "user_type": null,
                "devices": [{
                    "device_id": "DEVICEID",
                    "display_name": "Alice phone",
                    "last_seen_ts": 1718000000000_i64
                }],
                "threepids": [],
                "external_ids": []
            })
        ),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_user_v2_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_synapse/admin/v2/users/{user_id}` — Create or update one user's admin-facing profile.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_synapse/admin/v2/users/{user_id}",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "User created or updated", body = serde_json::Value),
        (status = 403, description = "Only super_admin can change admin or user_type")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_upsert_user_v2_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/users/{user_id}/rooms` — List rooms joined by a user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/users/{user_id}/rooms",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID"),
        ("limit" = Option<i64>, Query, description = "Maximum number of rooms to return"),
        ("from" = Option<String>, Query, description = "Pagination token")
    ),
    responses(
        (status = 200, description = "Rooms joined by the user",
            body = serde_json::Value,
            example = json!({
                "joined_rooms": ["!room:example.com", "!other:example.com"],
                "total": 2,
                "next_batch": null
            })
        ),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_user_rooms_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/users/{user_id}/devices` — List devices owned by a user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/users/{user_id}/devices",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "User devices",
            body = serde_json::Value,
            example = json!({
                "devices": [{
                    "device_id": "DEVICEID",
                    "display_name": "Alice phone",
                    "last_seen_ts": 1718000000000_i64,
                    "last_seen_ip": "203.0.113.10"
                }],
                "total": 1
            })
        ),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_user_devices_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_synapse/admin/v1/users/{user_id}/devices/{device_id}` — Revoke one user device.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_synapse/admin/v1/users/{user_id}/devices/{device_id}",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID"),
        ("device_id" = String, Path, description = "Matrix device ID")
    ),
    responses(
        (status = 200, description = "Device revoked", body = serde_json::Value),
        (status = 404, description = "User or device not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_delete_user_device_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/admin/v1/users/{user_id}/login` — Mint an access token to impersonate a user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/admin/v1/users/{user_id}/login",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "Impersonation token issued",
            body = serde_json::Value,
            example = json!({
                "access_token": "access_token_value",
                "device_id": "ABCDEFGHIJ",
                "user_id": "@alice:example.com"
            })
        ),
        (status = 400, description = "User is deactivated"),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_login_as_user_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/admin/v1/users/{user_id}/logout` — Invalidate all sessions for a user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/admin/v1/users/{user_id}/logout",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "All user sessions invalidated",
            body = serde_json::Value,
            example = json!({"devices_deleted": 3})
        ),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_logout_user_devices_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/user_stats` — Read aggregate user statistics.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/user_stats",
    tag = "Admin",
    responses(
        (status = 200, description = "Aggregate user statistics",
            body = serde_json::Value,
            example = json!({
                "total_users": 120,
                "active_users": 80,
                "admin_users": 2,
                "deactivated_users": 5,
                "guest_users": 10,
                "average_rooms_per_user": 3.5,
                "user_registration_enabled": true
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_user_stats_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/users/{user_id}/stats` — Read one user's statistics dashboard.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/users/{user_id}/stats",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "Per-user statistics dashboard",
            body = serde_json::Value,
            example = json!({
                "user_id": "@alice:example.com",
                "rooms_joined": 12,
                "messages_sent": 345,
                "last_seen_ts": 1718000000000_i64,
                "creation_ts": 1717000000000_i64,
                "is_admin": false,
                "dashboard": {
                    "total_rooms": 12,
                    "total_messages": 345,
                    "last_seen": 1718000000000_i64
                }
            })
        ),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_single_user_stats_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/admin/v1/users/batch` — Create multiple users in one request.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/admin/v1/users/batch",
    tag = "Admin",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Batch user creation result",
            body = serde_json::Value,
            example = json!({
                "created": 3,
                "failed": 0,
                "total": 3
            })
        ),
        (status = 403, description = "Only super_admin can create admin users")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_batch_create_users_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/admin/v1/users/batch_deactivate` — Deactivate multiple users in one request.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/admin/v1/users/batch_deactivate",
    tag = "Admin",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Batch deactivation result",
            body = serde_json::Value,
            example = json!({
                "deactivated": 3,
                "failed": 0,
                "total": 3
            })
        ),
        (status = 400, description = "Too many users in batch request")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_batch_deactivate_users_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/user_sessions/{user_id}` — List active sessions for a user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/user_sessions/{user_id}",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "User session listing",
            body = serde_json::Value,
            example = json!({
                "user_id": "@alice:example.com",
                "sessions": [{
                    "device_id": "DEVICEID",
                    "display_name": "Alice phone",
                    "last_seen_ts": 1718000000000_i64,
                    "last_seen_ip": "203.0.113.10",
                    "session_id": "DEVICEID"
                }],
                "total": 1
            })
        ),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_user_sessions_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/admin/v1/user_sessions/{user_id}/invalidate` — Invalidate all sessions for a user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/admin/v1/user_sessions/{user_id}/invalidate",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "User sessions invalidated",
            body = serde_json::Value,
            example = json!({
                "invalidated": true,
                "sessions_removed": 3
            })
        ),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_invalidate_user_sessions_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/account/{user_id}` — Read admin-facing account details for a user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/account/{user_id}",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "Admin-facing account details",
            body = serde_json::Value,
            example = json!({
                "name": "alice",
                "user_id": "@alice:example.com",
                "displayname": "Alice",
                "admin": false,
                "deactivated": false,
                "creation_ts": 1717000000000_i64,
                "device_count": 2,
                "room_count": 12
            })
        ),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_account_details_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/admin/v1/account/{user_id}` — Update admin-facing account details for a user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/admin/v1/account/{user_id}",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Account updated",
            body = serde_json::Value,
            example = json!({
                "user_id": "@alice:example.com",
                "updated": true
            })
        ),
        (status = 403, description = "Only super_admin can change admin privilege"),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_update_account_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/rooms/{room_id}` — Read admin-visible room details.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/rooms/{room_id}",
    tag = "Admin",
    params(
        ("room_id" = String, Path, description = "Matrix room ID")
    ),
    responses(
        (status = 200, description = "Room details",
            body = serde_json::Value,
            example = json!({
                "room_id": "!room:example.com",
                "name": "General",
                "topic": "Main room",
                "creator": "@admin:example.com",
                "member_count": 42,
                "room_version": "11",
                "encryption": "m.megolm.v1.aes-sha2",
                "is_public": true,
                "join_rule": "public"
            })
        ),
        (status = 404, description = "Room not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_room_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/rooms/{room_id}/members` — List room members for moderation.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/rooms/{room_id}/members",
    tag = "Admin",
    params(
        ("room_id" = String, Path, description = "Matrix room ID"),
        ("limit" = Option<i64>, Query, description = "Maximum number of members to return"),
        ("from" = Option<String>, Query, description = "Pagination token")
    ),
    responses(
        (status = 200, description = "Room members",
            body = serde_json::Value,
            example = json!({
                "members": [{
                    "user_id": "@alice:example.com",
                    "displayname": "Alice",
                    "avatar_url": "mxc://example.com/alice",
                    "membership": "join"
                }],
                "total": 42,
                "next_batch": null
            })
        ),
        (status = 404, description = "Room not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_room_members_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/rooms/{room_id}/state` — List state events for a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/rooms/{room_id}/state",
    tag = "Admin",
    params(
        ("room_id" = String, Path, description = "Matrix room ID")
    ),
    responses(
        (status = 200, description = "Room state events",
            body = serde_json::Value,
            example = json!({
                "state": [{
                    "type": "m.room.name",
                    "state_key": "",
                    "content": { "name": "General" },
                    "sender": "@admin:example.com",
                    "event_id": "$event:example.com"
                }]
            })
        ),
        (status = 404, description = "Room not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_room_state_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/spaces` — List all spaces for administration.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/spaces",
    tag = "Admin",
    responses(
        (status = 200, description = "Spaces",
            body = serde_json::Value,
            example = json!({
                "spaces": [{
                    "space_id": "!space:example.com",
                    "room_id": "!space:example.com",
                    "name": "Workspace",
                    "topic": "Team space",
                    "creator": "@admin:example.com",
                    "created_ts": 1718000000000_i64
                }],
                "total": 1
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_spaces_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/spaces/{space_id}` — Read one space.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/spaces/{space_id}",
    tag = "Admin",
    params(
        ("space_id" = String, Path, description = "Space ID or identifier")
    ),
    responses(
        (status = 200, description = "Space details",
            body = serde_json::Value,
            example = json!({
                "space_id": "!space:example.com",
                "room_id": "!space:example.com",
                "name": "Workspace",
                "topic": "Team space",
                "creator": "@admin:example.com",
                "created_ts": 1718000000000_i64
            })
        ),
        (status = 404, description = "Space not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_space_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_synapse/admin/v1/spaces/{space_id}` — Delete a space.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_synapse/admin/v1/spaces/{space_id}",
    tag = "Admin",
    params(
        ("space_id" = String, Path, description = "Space ID or identifier")
    ),
    responses(
        (status = 200, description = "Space deleted",
            body = serde_json::Value,
            example = json!({
                "deleted": true
            })
        ),
        (status = 404, description = "Space not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_delete_space_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/spaces/{space_id}/users` — List users in a space.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/spaces/{space_id}/users",
    tag = "Admin",
    params(
        ("space_id" = String, Path, description = "Space ID or identifier")
    ),
    responses(
        (status = 200, description = "Space users",
            body = serde_json::Value,
            example = json!({
                "users": ["@alice:example.com", "@bob:example.com"],
                "total": 2
            })
        ),
        (status = 404, description = "Space not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_space_users_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/spaces/{space_id}/rooms` — List rooms under a space.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/spaces/{space_id}/rooms",
    tag = "Admin",
    params(
        ("space_id" = String, Path, description = "Space ID or identifier")
    ),
    responses(
        (status = 200, description = "Space rooms",
            body = serde_json::Value,
            example = json!({
                "rooms": ["!room:example.com", "!other:example.com"],
                "total": 2
            })
        ),
        (status = 404, description = "Space not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_space_rooms_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/spaces/{space_id}/stats` — Read aggregate statistics for a space.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/spaces/{space_id}/stats",
    tag = "Admin",
    params(
        ("space_id" = String, Path, description = "Space ID or identifier")
    ),
    responses(
        (status = 200, description = "Space statistics",
            body = serde_json::Value,
            example = json!({
                "space_id": "!space:example.com",
                "member_count": 12,
                "child_room_count": 4
            })
        ),
        (status = 404, description = "Space not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_space_stats_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/room_stats` — Read global room statistics overview.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/room_stats",
    tag = "Admin",
    responses(
        (status = 200, description = "Global room statistics overview", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_room_stats_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/room_stats/{room_id}` — Read statistics for one room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/room_stats/{room_id}",
    tag = "Admin",
    params(
        ("room_id" = String, Path, description = "Matrix room ID")
    ),
    responses(
        (status = 200, description = "Room statistics", body = serde_json::Value),
        (status = 404, description = "Room not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_single_room_stats_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/rooms/{room_id}/listings` — Read room directory visibility status.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/rooms/{room_id}/listings",
    tag = "Admin",
    params(
        ("room_id" = String, Path, description = "Matrix room ID")
    ),
    responses(
        (status = 200, description = "Room listing status",
            body = serde_json::Value,
            example = json!({
                "room_id": "!room:example.com",
                "public": true,
                "in_directory": true
            })
        ),
        (status = 404, description = "Room not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_room_listings_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_synapse/admin/v1/rooms/{room_id}/listings/public` — Set a room as public.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_synapse/admin/v1/rooms/{room_id}/listings/public",
    tag = "Admin",
    params(
        ("room_id" = String, Path, description = "Matrix room ID")
    ),
    responses(
        (status = 200, description = "Room visibility updated",
            body = serde_json::Value,
            example = json!({
                "room_id": "!room:example.com",
                "public": true
            })
        ),
        (status = 404, description = "Room not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_set_room_public_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_synapse/admin/v1/rooms/{room_id}/listings/public` — Set a room as private.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_synapse/admin/v1/rooms/{room_id}/listings/public",
    tag = "Admin",
    params(
        ("room_id" = String, Path, description = "Matrix room ID")
    ),
    responses(
        (status = 200, description = "Room visibility updated",
            body = serde_json::Value,
            example = json!({
                "room_id": "!room:example.com",
                "public": false
            })
        ),
        (status = 404, description = "Room not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_set_room_private_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/rooms/{room_id}/block` — Read block status for a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/rooms/{room_id}/block",
    tag = "Admin",
    params(
        ("room_id" = String, Path, description = "Matrix room ID")
    ),
    responses(
        (status = 200, description = "Room block status",
            body = serde_json::Value,
            example = json!({
                "block": true,
                "blocked_at": 1718000000000_i64
            })
        ),
        (status = 404, description = "Room not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_room_block_status_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/admin/v1/rooms/{room_id}/block` — Block a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/admin/v1/rooms/{room_id}/block",
    tag = "Admin",
    params(
        ("room_id" = String, Path, description = "Matrix room ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Room block state updated",
            body = serde_json::Value,
            example = json!({
                "block": true
            })
        ),
        (status = 404, description = "Room not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_block_room_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/admin/v1/rooms/{room_id}/unblock` — Unblock a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/admin/v1/rooms/{room_id}/unblock",
    tag = "Admin",
    params(
        ("room_id" = String, Path, description = "Matrix room ID")
    ),
    responses(
        (status = 200, description = "Room block state updated",
            body = serde_json::Value,
            example = json!({
                "block": false
            })
        ),
        (status = 404, description = "Room not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_unblock_room_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/admin/v1/rooms/{room_id}/make_admin` — Grant room admin power to a user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/admin/v1/rooms/{room_id}/make_admin",
    tag = "Admin",
    params(
        ("room_id" = String, Path, description = "Matrix room ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Power levels updated", body = serde_json::Value),
        (status = 404, description = "Room or user not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_make_room_admin_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/admin/v1/purge_history` — Purge room history before a timestamp.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/admin/v1/purge_history",
    tag = "Admin",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "History purged",
            body = serde_json::Value,
            example = json!({
                "success": true,
                "deleted_events": 123
            })
        ),
        (status = 400, description = "Missing room_id"),
        (status = 404, description = "Room not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_purge_history_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/admin/v1/purge_room` — Permanently delete a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/admin/v1/purge_room",
    tag = "Admin",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Room purged",
            body = serde_json::Value,
            example = json!({
                "purge_id": "550e8400-e29b-41d4-a716-446655440000",
                "success": true
            })
        ),
        (status = 400, description = "Missing room_id"),
        (status = 404, description = "Room not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_purge_room_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_synapse/admin/v1/rooms/{room_id}/members/{user_id}` — Force-join a user to a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_synapse/admin/v1/rooms/{room_id}/members/{user_id}",
    tag = "Admin",
    params(
        ("room_id" = String, Path, description = "Matrix room ID"),
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "Membership updated",
            body = serde_json::Value,
            example = json!({
                "user_id": "@alice:example.com",
                "room_id": "!room:example.com",
                "membership": "join"
            })
        ),
        (status = 404, description = "Room or user not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_join_room_member_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_synapse/admin/v1/rooms/{room_id}/members/{user_id}` — Remove a user from a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_synapse/admin/v1/rooms/{room_id}/members/{user_id}",
    tag = "Admin",
    params(
        ("room_id" = String, Path, description = "Matrix room ID"),
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "Membership updated",
            body = serde_json::Value,
            example = json!({
                "user_id": "@alice:example.com",
                "room_id": "!room:example.com",
                "removed": true
            })
        ),
        (status = 404, description = "Room or user not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_remove_room_member_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/admin/v1/rooms/cleanup` — Clean up abnormal room data.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/admin/v1/rooms/cleanup",
    tag = "Admin",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Cleanup results", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_cleanup_abnormal_rooms_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/server_version` — Return admin-visible server version metadata.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/server_version",
    tag = "Admin",
    responses(
        (status = 200, description = "Admin server version metadata",
            body = serde_json::Value,
            example = json!({
                "server_version": "6.0.4",
                "python_version": "Rust",
                "server_name": "example.com"
            })
        ),
        (status = 403, description = "Admin only")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_server_version_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/info` — Return privileged homeserver metadata.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/info",
    tag = "Admin",
    responses(
        (status = 200, description = "Privileged homeserver metadata",
            body = serde_json::Value,
            example = json!({
                "server_name": "example.com",
                "server_version": "6.0.4",
                "implementation": "synapse-rust"
            })
        ),
        (status = 403, description = "Only super_admin can access server information")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_info_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/whoami` — Return the current admin principal.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/whoami",
    tag = "Admin",
    responses(
        (status = 200, description = "Authenticated admin principal",
            body = serde_json::Value,
            example = json!({
                "user_id": "@admin:example.com",
                "name": "@admin:example.com",
                "is_admin": true,
                "role": "super_admin"
            })
        ),
        (status = 403, description = "Admin only")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_whoami_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/statistics` — Return aggregate homeserver statistics.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/statistics",
    tag = "Admin",
    responses(
        (status = 200, description = "Aggregate server statistics",
            body = serde_json::Value,
            example = json!({
                "total_users": 120,
                "total_rooms": 45,
                "daily_active_users": 120,
                "monthly_active_users": 120,
                "r30_users": 120,
                "r30v2_users": 120
            })
        ),
        (status = 403, description = "Admin only")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_statistics_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/status` — Return high-level server health status.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/status",
    tag = "Admin",
    responses(
        (status = 200, description = "Admin health status",
            body = serde_json::Value,
            example = json!({
                "db_ok": true,
                "server_ok": true,
                "up": true
            })
        ),
        (status = 403, description = "Admin only")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_status_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/whois/{user_id}` — Inspect devices and connections for a user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/whois/{user_id}",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "User connection summary",
            body = serde_json::Value,
            example = json!({
                "user_id": "@alice:example.com",
                "devices": [{
                    "device_id": "DEVICEID",
                    "display_name": "Alice phone",
                    "last_seen": 1718000000000_i64,
                    "ip": "203.0.113.10"
                }]
            })
        ),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_whois_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/whois/{user_id}/{device_id}` — Inspect one device connection for a user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/whois/{user_id}/{device_id}",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID"),
        ("device_id" = String, Path, description = "Matrix device ID")
    ),
    responses(
        (status = 200, description = "Device connection summary",
            body = serde_json::Value,
            example = json!({
                "user_id": "@alice:example.com",
                "device_id": "DEVICEID",
                "display_name": "Alice phone",
                "last_seen": 1718000000000_i64,
                "ip": "203.0.113.10"
            })
        ),
        (status = 404, description = "User or device not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_whois_device_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/admin/v1/purge_media_cache` — Purge cached media older than a given timestamp.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/admin/v1/purge_media_cache",
    tag = "Admin",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Media cache purge summary",
            body = serde_json::Value,
            example = json!({
                "deleted": 42
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_purge_media_cache_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/config` — Read selected homeserver configuration values.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/config",
    tag = "Admin",
    responses(
        (status = 200, description = "Selected homeserver configuration",
            body = serde_json::Value,
            example = json!({
                "server_name": "example.com",
                "public_baseurl": "https://matrix.example.com",
                "registration_enabled": true,
                "max_upload_size": 10485760
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_config_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/jitsi/config` — Read Jitsi integration settings.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/jitsi/config",
    tag = "Admin",
    responses(
        (status = 200, description = "Jitsi integration configuration",
            body = serde_json::Value,
            example = json!({
                "domain": "meet.jit.si",
                "app_id": null,
                "jwt_enabled": false,
                "jwt_asap_enabled": false,
                "jwt_auth_type": "none",
                "server_name": "example.com"
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_jitsi_config_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/invite/blocklist` — Read the global invite blocklist.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/invite/blocklist",
    tag = "Admin",
    responses(
        (status = 200, description = "Global invite blocklist",
            body = serde_json::Value,
            example = json!({
                "blocklist": ["bad.example.com", "spam.example.net"]
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_invite_blocklist_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/invite/allowlist` — Read the global invite allowlist.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/invite/allowlist",
    tag = "Admin",
    responses(
        (status = 200, description = "Global invite allowlist",
            body = serde_json::Value,
            example = json!({
                "allowlist": ["trusted.example.com"]
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_invite_allowlist_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/federation/destinations` — List known federation destinations.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/federation/destinations",
    tag = "Admin",
    params(
        ("from" = Option<String>, Query, description = "Keyset pagination cursor"),
        ("limit" = Option<i32>, Query, description = "Maximum number of destinations to return")
    ),
    responses(
        (status = 200, description = "Federation destination list",
            body = serde_json::Value,
            example = json!({
                "destinations": [{
                    "destination": "matrix.org",
                    "retry_last_ts": 1718000000000_i64,
                    "retry_interval": 0,
                    "failure_ts": null,
                    "last_successful_stream_ordering": 12345
                }],
                "total": 1,
                "total_count": 1,
                "next_batch": null
            })
        ),
        (status = 400, description = "Invalid pagination cursor"),
        (status = 403, description = "Admin only")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_federation_destinations_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/federation/destinations/{destination}` — Read one federation destination.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/federation/destinations/{destination}",
    tag = "Admin",
    params(
        ("destination" = String, Path, description = "Remote server name")
    ),
    responses(
        (status = 200, description = "Federation destination details", body = serde_json::Value),
        (status = 404, description = "Destination not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_federation_destination_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/federation/destinations/{destination}/rooms` — List rooms tied to a destination.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/federation/destinations/{destination}/rooms",
    tag = "Admin",
    params(
        ("destination" = String, Path, description = "Remote server name")
    ),
    responses(
        (status = 200, description = "Destination room list",
            body = serde_json::Value,
            example = json!({
                "rooms": ["!room:example.com"],
                "total": 1
            })
        ),
        (status = 403, description = "Admin only")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_federation_destination_rooms_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/reports` — List moderation reports.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/reports",
    tag = "Admin",
    params(
        ("limit" = Option<i32>, Query, description = "Maximum number of reports to return"),
        ("since_score" = Option<i32>, Query, description = "Score cursor"),
        ("since_ts" = Option<i64>, Query, description = "Timestamp cursor"),
        ("since_id" = Option<i64>, Query, description = "Report ID cursor")
    ),
    responses(
        (status = 200, description = "Moderation reports",
            body = serde_json::Value,
            example = json!({
                "reports": [{
                    "id": 1,
                    "room_id": "!room:example.com",
                    "event_id": "$event:example.com",
                    "user_id": "@alice:example.com",
                    "reported_user_id": "@bob:example.com",
                    "reason": "spam",
                    "content": "Unwanted content",
                    "status": "open",
                    "score": -100,
                    "received_ts": 1718000000000_i64
                }],
                "total": 1
            })
        ),
        (status = 400, description = "Legacy offset pagination is not supported")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_reports_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/reports/{report_id}` — Read one moderation report.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/reports/{report_id}",
    tag = "Admin",
    params(
        ("report_id" = i64, Path, description = "Moderation report ID")
    ),
    responses(
        (status = 200, description = "Moderation report details", body = serde_json::Value),
        (status = 404, description = "Report not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_report_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/retention/policy` — Read the server retention policy.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/retention/policy",
    tag = "Admin",
    responses(
        (status = 200, description = "Server retention policy",
            body = serde_json::Value,
            example = json!({
                "max_lifetime": 7776000000_i64,
                "min_lifetime": 86400000_i64,
                "is_expire_on_clients": false
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_retention_policy_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/admin/v1/retention/policy` — Update the server retention policy.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/admin/v1/retention/policy",
    tag = "Admin",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Server retention policy updated", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_set_retention_policy_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/retention/policy/{room_id}` — Read room-specific retention policy.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/retention/policy/{room_id}",
    tag = "Admin",
    params(
        ("room_id" = String, Path, description = "Matrix room ID")
    ),
    responses(
        (status = 200, description = "Room retention policy",
            body = serde_json::Value,
            example = json!({
                "room_id": "!room:example.com",
                "max_lifetime": 2592000000_i64,
                "min_lifetime": null,
                "is_expire_on_clients": false
            })
        ),
        (status = 404, description = "Room not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_room_retention_policy_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/retention/status` — Read retention subsystem status summary.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/retention/status",
    tag = "Admin",
    responses(
        (status = 200, description = "Retention subsystem status",
            body = serde_json::Value,
            example = json!({
                "server_policy_enabled": true,
                "rooms_with_custom_policy": 5,
                "lifecycle_cleanup_enabled": true,
                "cleanup_batch_size": 500,
                "audit_retention_days": 30,
                "queue_retention_days": 7,
                "last_run": {
                    "started_ts": 1718000000000_i64,
                    "completed_ts": 1718000005000_i64,
                    "duration_ms": 5000,
                    "expired_events_deleted": 100,
                    "expired_beacons_deleted": 0,
                    "expired_uploads_deleted": 0,
                    "expired_audit_events_deleted": 0,
                    "cleanup_queue_items_processed": 10,
                    "cleanup_queue_rows_pruned": 10,
                    "failed_tasks": 0
                }
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_retention_status_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/registration_tokens` — List registration tokens with cursor pagination.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/registration_tokens",
    tag = "Admin",
    params(
        ("limit" = Option<i64>, Query, description = "Maximum number of tokens to return"),
        ("from" = Option<String>, Query, description = "Opaque cursor for the next page")
    ),
    responses(
        (status = 200, description = "Registration token listing",
            body = serde_json::Value,
            example = json!({
                "registration_tokens": [{
                    "token": "invite-token",
                    "uses_allowed": 10,
                    "pending": 0,
                    "completed": 2,
                    "expiry_time": 1719000000000_i64,
                    "created_ts": 1718000000000_i64
                }],
                "next_batch": "cursor"
            })
        ),
        (status = 400, description = "Invalid from cursor")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_registration_tokens_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/admin/v1/registration_tokens` — Create a registration token.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/admin/v1/registration_tokens",
    tag = "Admin",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Registration token created",
            body = serde_json::Value,
            example = json!({
                "token": "invite-token",
                "uses_allowed": 10,
                "pending": 0,
                "completed": 0,
                "expiry_time": 1719000000000_i64,
                "created_ts": 1718000000000_i64
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_create_registration_token_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/registration_tokens/{token}` — Read one registration token.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/registration_tokens/{token}",
    tag = "Admin",
    params(
        ("token" = String, Path, description = "Registration token string")
    ),
    responses(
        (status = 200, description = "Registration token details", body = serde_json::Value),
        (status = 404, description = "Token not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_registration_token_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_synapse/admin/v1/registration_tokens/{token}` — Delete one registration token.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_synapse/admin/v1/registration_tokens/{token}",
    tag = "Admin",
    params(
        ("token" = String, Path, description = "Registration token string")
    ),
    responses(
        (status = 200, description = "Registration token deleted", body = serde_json::Value),
        (status = 404, description = "Token not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_delete_registration_token_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/admin/v1/registration_tokens/{token}` — Update one registration token.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/admin/v1/registration_tokens/{token}",
    tag = "Admin",
    params(
        ("token" = String, Path, description = "Registration token string")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Registration token updated", body = serde_json::Value),
        (status = 404, description = "Token not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_update_registration_token_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/users/{user_id}/tokens` — List access tokens for one user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/users/{user_id}/tokens",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "Access token listing",
            body = serde_json::Value,
            example = json!({
                "tokens": [{
                    "id": 1,
                    "device_id": "DEVICEID",
                    "created_ts": 1718000000000_i64,
                    "expires_at": 1719000000000_i64,
                    "is_revoked": false
                }],
                "total": 1
            })
        ),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_user_tokens_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_synapse/admin/v1/users/{user_id}/tokens/{token_id}` — Delete one access token for a user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_synapse/admin/v1/users/{user_id}/tokens/{token_id}",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID"),
        ("token_id" = i64, Path, description = "Access token ID")
    ),
    responses(
        (status = 200, description = "Access token deleted", body = serde_json::Value),
        (status = 404, description = "User or token not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_delete_user_token_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/users/{user_id}/refresh_tokens` — List refresh tokens for one user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/users/{user_id}/refresh_tokens",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "Refresh token listing",
            body = serde_json::Value,
            example = json!({
                "refresh_tokens": [{
                    "id": 1,
                    "device_id": "DEVICEID",
                    "created_ts": 1718000000000_i64,
                    "expires_at": 1719000000000_i64,
                    "is_revoked": false
                }],
                "total": 1
            })
        ),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_user_refresh_tokens_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_synapse/admin/v1/users/{user_id}/refresh_tokens/{token_id}` — Delete one refresh token for a user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_synapse/admin/v1/users/{user_id}/refresh_tokens/{token_id}",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID"),
        ("token_id" = i64, Path, description = "Refresh token ID")
    ),
    responses(
        (status = 200, description = "Refresh token deleted", body = serde_json::Value),
        (status = 404, description = "User or token not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_delete_refresh_token_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/media` — List uploaded media with cursor pagination.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/media",
    tag = "Admin",
    params(
        ("limit" = Option<i64>, Query, description = "Maximum number of media objects to return"),
        ("from" = Option<String>, Query, description = "Opaque cursor for the next page")
    ),
    responses(
        (status = 200, description = "Media listing",
            body = serde_json::Value,
            example = json!({
                "media": [{
                    "media_id": "abcdef",
                    "media_type": "image/png",
                    "upload_name": "avatar.png",
                    "created_ts": 1718000000000_i64,
                    "last_access_ts": 1718000100000_i64,
                    "media_length": 2048,
                    "user_id": "@alice:example.com",
                    "quarantined": false
                }],
                "total": 1,
                "next_batch": "cursor"
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_media_list_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/media/{media_id}` — Read one media object's metadata.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/media/{media_id}",
    tag = "Admin",
    params(
        ("media_id" = String, Path, description = "Media identifier")
    ),
    responses(
        (status = 200, description = "Media metadata", body = serde_json::Value),
        (status = 404, description = "Media not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_media_info_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_synapse/admin/v1/media/{media_id}` — Delete one media object.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_synapse/admin/v1/media/{media_id}",
    tag = "Admin",
    params(
        ("media_id" = String, Path, description = "Media identifier")
    ),
    responses(
        (status = 200, description = "Media deleted", body = serde_json::Value),
        (status = 404, description = "Media not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_delete_media_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/media/quota` — Read global media quota statistics.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/media/quota",
    tag = "Admin",
    responses(
        (status = 200, description = "Media quota summary",
            body = serde_json::Value,
            example = json!({
                "total_size": 1048576,
                "total_count": 120,
                "default_size_limit": 10000000000_i64,
                "default_count_limit": 100
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_media_quota_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/users/{user_id}/media` — List media uploaded by one user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/users/{user_id}/media",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "User media listing",
            body = serde_json::Value,
            example = json!({
                "media": [{
                    "media_id": "abcdef",
                    "media_type": "image/png",
                    "upload_name": "avatar.png",
                    "created_ts": 1718000000000_i64,
                    "media_length": 2048
                }],
                "total": 1
            })
        ),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_user_media_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_synapse/admin/v1/users/{user_id}/media` — Delete all media uploaded by one user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_synapse/admin/v1/users/{user_id}/media",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "User media deleted",
            body = serde_json::Value,
            example = json!({
                "deleted": 3
            })
        ),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_delete_user_media_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/admin/v1/users/{user_id}/shadow_ban` — Enable shadow-ban mode for a user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/admin/v1/users/{user_id}/shadow_ban",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "Shadow-ban enabled", body = serde_json::Value),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_shadow_ban_user_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_synapse/admin/v1/users/{user_id}/shadow_ban` — Disable shadow-ban mode for a user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_synapse/admin/v1/users/{user_id}/shadow_ban",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "Shadow-ban disabled", body = serde_json::Value),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_unshadow_ban_user_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/users/{user_id}/rate_limit` — Read the per-user rate limit.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/users/{user_id}/rate_limit",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "Per-user rate limit",
            body = serde_json::Value,
            example = json!({
                "messages_per_second": 5.0,
                "burst_count": 10
            })
        ),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_user_rate_limit_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_synapse/admin/v1/users/{user_id}/rate_limit` — Update the per-user rate limit.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_synapse/admin/v1/users/{user_id}/rate_limit",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Per-user rate limit updated", body = serde_json::Value),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_set_user_rate_limit_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_synapse/admin/v1/users/{user_id}/rate_limit` — Remove the per-user rate limit override.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_synapse/admin/v1/users/{user_id}/rate_limit",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "Per-user rate limit deleted", body = serde_json::Value),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_delete_user_rate_limit_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/users/{user_id}/override_ratelimit` — Read the legacy override rate-limit view for a user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/users/{user_id}/override_ratelimit",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "Override rate-limit view",
            body = serde_json::Value,
            example = json!({
                "messages_per_second": 5.0,
                "burst_count": 10
            })
        ),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_override_rate_limit_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/admin/v1/users/{user_id}/override_ratelimit` — Update the legacy override rate-limit view for a user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/admin/v1/users/{user_id}/override_ratelimit",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Override rate-limit updated", body = serde_json::Value),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_set_override_rate_limit_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_synapse/admin/v1/users/{user_id}/override_ratelimit` — Remove the legacy override rate-limit view for a user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_synapse/admin/v1/users/{user_id}/override_ratelimit",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "Override rate-limit deleted", body = serde_json::Value),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_delete_override_rate_limit_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}
