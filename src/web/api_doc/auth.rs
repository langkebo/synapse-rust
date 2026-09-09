#![cfg(feature = "openapi-docs")]

use super::schemas;

/// `GET /_matrix/client/v3/user/{user_id}/account_data/` — List account data for the authenticated user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/user/{user_id}/account_data/",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Authenticated Matrix user ID")
    ),
    responses(
        (status = 200, description = "Account data map",
            body = schemas::ApiAdminGenericJson,
            example = json!({
                "account_data": {
                    "m.push_rules": {
                        "global": {
                            "override": []
                        }
                    }
                }
            })
        ),
    ),
)]
/// See [`list_account_data`].
pub fn list_account_data() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/user/{user_id}/account_data/{type}` — Read one account data event.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/user/{user_id}/account_data/{type}",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Authenticated Matrix user ID"),
        ("type" = String, Path, description = "Account data event type")
    ),
    responses(
        (status = 200, description = "Account data content", body = schemas::ApiAdminGenericJson),
        (status = 404, description = "Account data not found")
    ),
)]
/// See [`get_account_data`].
pub fn get_account_data() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v3/user/{user_id}/account_data/{type}` — Set one account data event.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v3/user/{user_id}/account_data/{type}",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Authenticated Matrix user ID"),
        ("type" = String, Path, description = "Account data event type")
    ),
    request_body = schemas::ApiAdminGenericJson,
    responses(
        (status = 200, description = "Account data updated", body = schemas::ApiAdminGenericJson),
        (status = 403, description = "Cannot modify account data for another user")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
/// See [`set_account_data_doc`].
pub fn set_account_data_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_matrix/client/v3/user/{user_id}/account_data/{type}` — Delete one account data event.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_matrix/client/v3/user/{user_id}/account_data/{type}",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Authenticated Matrix user ID"),
        ("type" = String, Path, description = "Account data event type")
    ),
    responses(
        (status = 200, description = "Account data deleted", body = schemas::ApiAdminGenericJson),
        (status = 404, description = "Account data not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
/// See [`delete_account_data_doc`].
pub fn delete_account_data_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/user/{user_id}/rooms/{room_id}/account_data/{type}` — Read one room-scoped account data event.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/user/{user_id}/rooms/{room_id}/account_data/{type}",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Authenticated Matrix user ID"),
        ("room_id" = String, Path, description = "Matrix room ID"),
        ("type" = String, Path, description = "Room account data event type")
    ),
    responses(
        (status = 200, description = "Room account data content", body = schemas::ApiAdminGenericJson),
        (status = 404, description = "Room account data not found")
    ),
)]
/// See [`get_room_account_data`].
pub fn get_room_account_data() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v3/user/{user_id}/rooms/{room_id}/account_data/{type}` — Set one room-scoped account data event.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v3/user/{user_id}/rooms/{room_id}/account_data/{type}",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Authenticated Matrix user ID"),
        ("room_id" = String, Path, description = "Matrix room ID"),
        ("type" = String, Path, description = "Room account data event type")
    ),
    request_body = schemas::ApiAdminGenericJson,
    responses(
        (status = 200, description = "Room account data updated", body = schemas::ApiAdminGenericJson),
        (status = 403, description = "Cannot modify room account data for another user")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
/// See [`set_room_account_data_doc`].
pub fn set_room_account_data_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_matrix/client/v3/user/{user_id}/rooms/{room_id}/account_data/{type}` — Delete one room-scoped account data event.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_matrix/client/v3/user/{user_id}/rooms/{room_id}/account_data/{type}",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Authenticated Matrix user ID"),
        ("room_id" = String, Path, description = "Matrix room ID"),
        ("type" = String, Path, description = "Room account data event type")
    ),
    responses(
        (status = 200, description = "Room account data deleted", body = schemas::ApiAdminGenericJson),
        (status = 404, description = "Room account data not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
/// See [`delete_room_account_data_doc`].
pub fn delete_room_account_data_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v3/user/{user_id}/filter` — Save a new sync filter.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v3/user/{user_id}/filter",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Authenticated Matrix user ID")
    ),
    request_body = schemas::ApiAdminGenericJson,
    responses(
        (status = 200, description = "Filter created",
            body = schemas::ApiAdminGenericJson,
            example = json!({
                "filter_id": "12345"
            })
        ),
        (status = 403, description = "Cannot create a filter for another user")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
/// See [`create_filter_doc`].
pub fn create_filter_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/user/{user_id}/filter/{filter_id}` — Read one saved sync filter.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/user/{user_id}/filter/{filter_id}",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Authenticated Matrix user ID"),
        ("filter_id" = String, Path, description = "Filter ID")
    ),
    responses(
        (status = 200, description = "Saved filter document", body = schemas::ApiAdminGenericJson),
        (status = 404, description = "Filter not found")
    ),
)]
/// See [`get_filter`].
pub fn get_filter() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_matrix/client/v3/user/{user_id}/filter/{filter_id}` — Delete one saved sync filter.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_matrix/client/v3/user/{user_id}/filter/{filter_id}",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Authenticated Matrix user ID"),
        ("filter_id" = String, Path, description = "Filter ID")
    ),
    responses(
        (status = 200, description = "Filter deleted", body = schemas::ApiAdminGenericJson),
        (status = 404, description = "Filter not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
/// See [`delete_filter_doc`].
pub fn delete_filter_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/user/{user_id}/openid/request_token` — Issue an OpenID token for the caller.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/user/{user_id}/openid/request_token",
    tag = "Authentication",
    params(
        ("user_id" = String, Path, description = "Authenticated Matrix user ID")
    ),
    responses(
        (status = 200, description = "OpenID token issued",
            body = schemas::ApiAdminGenericJson,
            example = json!({
                "access_token": "openid_access_token",
                "token_type": "Bearer",
                "matrix_server_name": "example.com",
                "expires_in": 3600
            })
        ),
        (status = 403, description = "Cannot issue a token for another user")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
/// See [`get_openid_token_doc`].
pub fn get_openid_token_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/user/mutual_rooms` — Get rooms in common with another user (MSC2666).
///
/// Returns a list of rooms where both the authenticated user and the `user_id` query
/// parameter user have `join` membership. Supports pagination via `from` parameter.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/user/mutual_rooms",
    tag = "Client-Server",
    params(
        ("user_id" = String, Query, description = "MXID of the user to find mutual rooms with"),
        ("from" = Option<String>, Query, description = "Pagination token for next batch"),
        ("limit" = Option<i64>, Query, description = "Maximum number of rooms to return (default 100, max 1000)")
    ),
    responses(
        (status = 200, description = "Mutual rooms list",
            body = schemas::ApiAdminGenericJson,
            example = json!({
                "joined": ["!room1:example.com", "!room2:example.com"],
                "next_batch_token": "next_page_token"
            })
        ),
        (status = 403, description = "Access denied or querying mutual rooms with yourself")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
/// See [`get_mutual_rooms_doc`].
pub fn get_mutual_rooms_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/user/{user_id}/tags` — List all tags grouped by room for the authenticated user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/user/{user_id}/tags",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Authenticated Matrix user ID")
    ),
    responses(
        (status = 200, description = "All room tags for the user",
            body = schemas::ApiAdminGenericJson,
            example = json!({
                "tags": {
                    "!room:example.com": {
                        "m.favourite": {
                            "order": 0.5
                        }
                    }
                }
            })
        ),
        (status = 403, description = "Access denied")
    ),
)]
/// See [`get_global_tags`].
pub fn get_global_tags() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/user/{user_id}/rooms/{room_id}/tags` — List tags for one room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/user/{user_id}/rooms/{room_id}/tags",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Authenticated Matrix user ID"),
        ("room_id" = String, Path, description = "Matrix room ID")
    ),
    responses(
        (status = 200, description = "Room tags",
            body = schemas::ApiAdminGenericJson,
            example = json!({
                "tags": {
                    "m.favourite": {
                        "order": 0.5
                    },
                    "u.work": {
                        "order": 1.0
                    }
                }
            })
        ),
        (status = 403, description = "Access denied")
    ),
)]
/// See [`get_room_tags`].
pub fn get_room_tags() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v3/user/{user_id}/rooms/{room_id}/tags/{tag}` — Add or update one room tag.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v3/user/{user_id}/rooms/{room_id}/tags/{tag}",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Authenticated Matrix user ID"),
        ("room_id" = String, Path, description = "Matrix room ID"),
        ("tag" = String, Path, description = "Tag name")
    ),
    request_body = schemas::ApiAdminGenericJson,
    responses(
        (status = 200, description = "Room tag updated", body = schemas::ApiAdminGenericJson),
        (status = 403, description = "Access denied")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
/// See [`put_room_tag_doc`].
pub fn put_room_tag_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_matrix/client/v3/user/{user_id}/rooms/{room_id}/tags/{tag}` — Remove one room tag.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_matrix/client/v3/user/{user_id}/rooms/{room_id}/tags/{tag}",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Authenticated Matrix user ID"),
        ("room_id" = String, Path, description = "Matrix room ID"),
        ("tag" = String, Path, description = "Tag name")
    ),
    responses(
        (status = 200, description = "Room tag deleted", body = schemas::ApiAdminGenericJson),
        (status = 403, description = "Access denied")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
/// See [`delete_room_tag_doc`].
pub fn delete_room_tag_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/account/whoami` — Return the authenticated user and device.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/account/whoami",
    tag = "Client-Server",
    responses(
        (status = 200, description = "Authenticated Matrix principal",
            body = schemas::ApiAdminGenericJson,
            example = json!({
                "user_id": "@alice:example.com",
                "device_id": "DEVICEID",
                "is_guest": false
            })
        )
    ),
)]
/// See [`get_whoami`].
pub fn get_whoami() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/account/3pid` — List bound third-party identifiers for the authenticated user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/account/3pid",
    tag = "Client-Server",
    responses(
        (status = 200, description = "Bound third-party identifiers",
            body = schemas::ApiAdminGenericJson,
            example = json!({
                "threepids": [{
                    "medium": "email",
                    "address": "alice@example.com",
                    "validated_ts": 1718000000000_i64,
                    "added_at": 1717990000000_i64
                }]
            })
        )
    ),
)]
/// See [`get_threepids`].
pub fn get_threepids() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/login/qrcode/{session_id}/status` — Get QR login transaction status.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/login/qrcode/{session_id}/status",
    tag = "Client-Server",
    params(
        ("session_id" = String, Path, description = "QR login transaction ID")
    ),
    responses(
        (status = 200, description = "QR login transaction status",
            body = schemas::ApiAdminGenericJson,
            example = json!({
                "transaction_id": "qr_1234",
                "user_id": "@alice:example.com",
                "status": "pending_confirmation"
            })
        ),
        (status = 404, description = "Transaction not found")
    ),
)]
/// See [`get_qr_status`].
pub fn get_qr_status() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/register` — Register a new account.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/register",
    tag = "Authentication",
    request_body = schemas::ApiAdminGenericJson,
    responses(
        (status = 200, description = "Registration successful",
            body = schemas::ApiAdminGenericJson,
            example = json!({
                "user_id": "@alice:example.com",
                "access_token": "access_token_value",
                "device_id": "ABCDEFGHIJ"
            })
        ),
        (status = 401, description = "Interactive authentication required"),
        (status = 400, description = "Invalid request or username taken")
    ),
)]
/// See [`register_doc`].
pub fn register_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/login` — Log in to an account.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/login",
    tag = "Authentication",
    request_body = schemas::ApiAdminGenericJson,
    responses(
        (status = 200, description = "Login successful",
            body = schemas::ApiAdminGenericJson,
            example = json!({
                "user_id": "@alice:example.com",
                "access_token": "access_token_value",
                "device_id": "ABCDEFGHIJ"
            })
        ),
        (status = 401, description = "Invalid credentials"),
        (status = 403, description = "Account deactivated")
    ),
)]
/// See [`login_doc`].
pub fn login_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/logout` — Log out of a session.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/logout",
    tag = "Authentication",
    responses(
        (status = 200, description = "Logout successful", body = schemas::ApiAdminGenericJson)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
/// See [`logout_doc`].
pub fn logout_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/refresh` — Refresh an access token.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/refresh",
    tag = "Authentication",
    request_body = schemas::ApiAdminGenericJson,
    responses(
        (status = 200, description = "Token refreshed",
            body = schemas::ApiAdminGenericJson,
            example = json!({
                "access_token": "new_access_token",
                "refresh_token": "new_refresh_token",
                "expires_in_ms": 3600000
            })
        ),
        (status = 401, description = "Invalid or expired refresh token")
    ),
)]
/// See [`refresh_token_doc`].
pub fn refresh_token_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/register` — Get supported registration flows.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/register",
    tag = "Authentication",
    responses(
        (status = 200, description = "Supported registration stages",
            body = schemas::ApiAdminGenericJson,
            example = json!({
                "flows": [
                    {"type": "m.login.dummy"},
                    {"type": "m.login.password"}
                ],
                "params": {}
            })
        )
    )
)]
/// See [`get_register_flows_doc`].
pub fn get_register_flows_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/register/available` — Check username availability.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/register/available",
    tag = "Authentication",
    params(
        ("username" = String, Query, description = "Desired localpart")
    ),
    responses(
        (status = 200, description = "Availability result",
            body = schemas::ApiAdminGenericJson,
            example = json!({
                "available": true,
                "username": "alice"
            })
        ),
        (status = 400, description = "Invalid username")
    )
)]
/// See [`check_username_availability_doc`].
pub fn check_username_availability_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/login` — Get supported login flows.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/login",
    tag = "Authentication",
    responses(
        (status = 200, description = "Supported login flows",
            body = schemas::ApiAdminGenericJson,
            example = json!({
                "flows": [
                    {"type": "m.login.password"},
                    {"type": "m.login.token"}
                ]
            })
        )
    )
)]
/// See [`get_login_flows_doc`].
pub fn get_login_flows_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/logout/all` — Log out every device for the caller.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/logout/all",
    tag = "Authentication",
    responses(
        (status = 200, description = "All sessions revoked", body = schemas::ApiAdminGenericJson)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
/// See [`logout_all_doc`].
pub fn logout_all_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/account/password` — Change the current password.
///
/// MSC4204 / Matrix v1.3: after a successful password change the server, by
/// default, revokes all of the user's access tokens (logout_devices=true).
/// Clients that want to keep the current session alive must set
/// `logout_devices=false` and supply the current `device_id` in the auth
/// section.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/account/password",
    tag = "Authentication",
    request_body = schemas::ApiAdminGenericJson,
    responses(
        (status = 200, description = "Password changed", body = schemas::ApiAdminGenericJson,
            example = json!({
                "new_password": "hunter2",
                "auth": {
                    "type": "m.login.password",
                    "identifier": { "user": "j橙" },
                    "password": "old_password",
                    "logout_devices": true
                }
            })
        ),
        (status = 401, description = "Authentication failed (wrong current password)"),
        (status = 400, description = "Invalid payload (weak password, logout_devices=false without device)")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
/// See [`change_password_doc`].
pub fn change_password_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/account/deactivate` — Deactivate the current account.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/account/deactivate",
    tag = "Authentication",
    request_body = Option<schemas::ApiAdminGenericRequest>,
    responses(
        (status = 200, description = "Account deactivated",
            body = schemas::ApiAdminGenericJson,
            example = json!({
                "id_server_unbind_result": "no-support"
            })
        ),
        (status = 401, description = "Authentication failed")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
/// See [`deactivate_account_doc`].
pub fn deactivate_account_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/account/3pid` — Add a third-party identifier.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/account/3pid",
    tag = "Authentication",
    request_body = schemas::ApiAdminGenericJson,
    responses(
        (status = 200, description = "Third-party identifier added", body = schemas::ApiAdminGenericJson),
        (status = 400, description = "Invalid 3PID payload")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
/// See [`add_threepid_doc`].
pub fn add_threepid_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/account/3pid/delete` — Remove a third-party identifier.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/account/3pid/delete",
    tag = "Authentication",
    request_body = schemas::ApiAdminGenericJson,
    responses(
        (status = 200, description = "Third-party identifier removed",
            body = schemas::ApiAdminGenericJson,
            example = json!({
                "id_server_unbind_result": "success"
            })
        ),
        (status = 400, description = "Invalid request")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
/// See [`delete_threepid_doc`].
pub fn delete_threepid_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/account/3pid/unbind` — Unbind a third-party identifier from an identity server.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/account/3pid/unbind",
    tag = "Authentication",
    request_body = schemas::ApiAdminGenericJson,
    responses(
        (status = 200, description = "Third-party identifier unbound",
            body = schemas::ApiAdminGenericJson,
            example = json!({
                "id_server_unbind_result": "success"
            })
        ),
        (status = 400, description = "Invalid request")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
/// See [`unbind_threepid_doc`].
pub fn unbind_threepid_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/register/guest` — Register a guest account.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/register/guest",
    tag = "Authentication",
    responses(
        (status = 200, description = "Guest registration successful",
            body = schemas::ApiAdminGenericJson,
            example = json!({
                "access_token": "guest_access_token",
                "device_id": "GUESTDEVICE",
                "user_id": "@guest-1:example.com",
                "expires_in": 3600000
            })
        ),
        (status = 403, description = "Guest registration disabled")
    )
)]
/// See [`register_guest_doc`].
pub fn register_guest_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/login/get_qr_code` — Generate a QR login challenge for the current user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/login/get_qr_code",
    tag = "Authentication",
    responses(
        (status = 200, description = "QR login challenge",
            body = schemas::ApiAdminGenericJson,
            example = json!({
                "transaction_id": "qr_xxx",
                "mode": "login",
                "challenge": "uuid-challenge",
                "expires_in": 300
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
/// See [`get_qr_code_doc`].
pub fn get_qr_code_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v1/login/qr/confirm` — Confirm a QR login on the source device.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v1/login/qr/confirm",
    tag = "Authentication",
    request_body = schemas::ApiAdminGenericJson,
    responses(
        (status = 200, description = "QR login confirmed",
            body = schemas::ApiAdminGenericJson,
            example = json!({
                "transaction_id": "qr_xxx",
                "status": "confirmed"
            })
        ),
        (status = 404, description = "Transaction not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
/// See [`confirm_qr_login_doc`].
pub fn confirm_qr_login_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v1/login/qr/start` — Start QR login on the scanning device.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v1/login/qr/start",
    tag = "Authentication",
    request_body = schemas::ApiAdminGenericJson,
    responses(
        (status = 200, description = "QR login started",
            body = schemas::ApiAdminGenericJson,
            example = json!({
                "transaction_id": "qr_xxx",
                "user_id": "@alice:example.com",
                "device_id": "DEVICEID",
                "initial_display_name": "Alice iPhone",
                "status": "pending_confirmation"
            })
        ),
        (status = 404, description = "Transaction not found")
    )
)]
/// See [`start_qr_login_doc`].
pub fn start_qr_login_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/login/qr/{transaction_id}/status` — Fetch QR login transaction status.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/login/qr/{transaction_id}/status",
    tag = "Authentication",
    params(
        ("transaction_id" = String, Path, description = "QR login transaction ID")
    ),
    responses(
        (status = 200, description = "QR login status",
            body = schemas::ApiAdminGenericJson,
            example = json!({
                "transaction_id": "qr_xxx",
                "user_id": "@alice:example.com",
                "status": "pending"
            })
        ),
        (status = 404, description = "Transaction not found")
    )
)]
/// See [`get_qr_status_doc`].
pub fn get_qr_status_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v1/login/qr/invalidate` — Cancel a QR login transaction.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v1/login/qr/invalidate",
    tag = "Authentication",
    request_body = schemas::ApiAdminGenericJson,
    responses(
        (status = 200, description = "QR login invalidated",
            body = schemas::ApiAdminGenericJson,
            example = json!({
                "transaction_id": "qr_xxx",
                "status": "invalidated"
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
/// See [`invalidate_qr_login_doc`].
pub fn invalidate_qr_login_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v1/login/qrcode/new` — Frontend compatibility alias for QR login generation.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v1/login/qrcode/new",
    tag = "Authentication",
    responses(
        (status = 200, description = "QR login challenge", body = schemas::ApiAdminGenericJson)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
/// See [`get_qrcode_new_doc`].
pub fn get_qrcode_new_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/login/qrcode/{session_id}` — Frontend compatibility alias for QR login status.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/login/qrcode/{session_id}",
    tag = "Authentication",
    params(
        ("session_id" = String, Path, description = "QR login session ID")
    ),
    responses(
        (status = 200, description = "QR login status", body = schemas::ApiAdminGenericJson)
    )
)]
/// See [`get_qrcode_status_alias_doc`].
pub fn get_qrcode_status_alias_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/thirdparty/user/{protocol}` — Search bridged users via a protocol adapter.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/thirdparty/user/{protocol}",
    tag = "Client-Server",
    params(
        ("protocol" = String, Path, description = "Third-party protocol identifier"),
        ("userid" = Option<String>, Query, description = "Third-party user identifier"),
        ("search" = Option<String>, Query, description = "Search term"),
        ("nickname" = Option<String>, Query, description = "Nickname"),
        ("server" = Option<String>, Query, description = "Third-party server")
    ),
    responses(
        (status = 200, description = "User search result", body = schemas::ApiAdminGenericJson),
        (status = 404, description = "No bridge configured")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
/// See [`get_thirdparty_user_doc`].
pub fn get_thirdparty_user_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/register/captcha/send` — Request a captcha challenge.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/register/captcha/send",
    tag = "Authentication",
    request_body = schemas::ApiAdminGenericJson,
    responses(
        (status = 200, description = "Captcha issued",
            body = schemas::ApiAdminGenericJson,
            example = json!({
                "captcha_id": "captcha-123",
                "expires_in": 300,
                "captcha_type": "email"
            })
        )
    )
)]
/// See [`send_captcha_doc`].
pub fn send_captcha_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/register/captcha/verify` — Verify a captcha response.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/register/captcha/verify",
    tag = "Authentication",
    request_body = schemas::ApiAdminGenericJson,
    responses(
        (status = 200, description = "Captcha verification result",
            body = schemas::ApiAdminGenericJson,
            example = json!({
                "verified": true
            })
        )
    )
)]
/// See [`verify_captcha_doc`].
pub fn verify_captcha_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/register/captcha/status` — Query captcha state.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/register/captcha/status",
    tag = "Authentication",
    params(
        ("captcha_id" = String, Query, description = "Captcha identifier")
    ),
    responses(
        (status = 200, description = "Captcha status", body = schemas::ApiAdminGenericJson),
        (status = 404, description = "Captcha not found")
    )
)]
/// See [`get_captcha_status_doc`].
pub fn get_captcha_status_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/r0/register/captcha/send` — Request a captcha challenge on the r0 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/r0/register/captcha/send",
    tag = "Authentication",
    request_body = schemas::ApiAdminGenericJson,
    responses(
        (status = 200, description = "Captcha issued",
            body = schemas::ApiAdminGenericJson,
            example = json!({
                "captcha_id": "captcha-123",
                "expires_in": 300,
                "captcha_type": "email"
            })
        )
    )
)]
/// See [`send_captcha_r0_doc`].
pub fn send_captcha_r0_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/r0/register/captcha/verify` — Verify a captcha response on the r0 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/r0/register/captcha/verify",
    tag = "Authentication",
    request_body = schemas::ApiAdminGenericJson,
    responses(
        (status = 200, description = "Captcha verification result",
            body = schemas::ApiAdminGenericJson,
            example = json!({
                "verified": true
            })
        )
    )
)]
/// See [`verify_captcha_r0_doc`].
pub fn verify_captcha_r0_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/register/captcha/status` — Query captcha state on the r0 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/register/captcha/status",
    tag = "Authentication",
    params(
        ("captcha_id" = String, Query, description = "Captcha identifier")
    ),
    responses(
        (status = 200, description = "Captcha status", body = schemas::ApiAdminGenericJson),
        (status = 404, description = "Captcha not found")
    )
)]
/// See [`get_captcha_status_r0_doc`].
pub fn get_captcha_status_r0_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/login/sso/redirect` — Redirect to the configured SSO provider.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/login/sso/redirect",
    tag = "Authentication",
    params(
        ("redirectUrl" = Option<String>, Query, description = "Preferred redirect URL after SSO login"),
        ("redirect_url" = Option<String>, Query, description = "Compatibility alias for redirectUrl")
    ),
    responses(
        (status = 307, description = "Temporary redirect to the SSO provider"),
        (status = 400, description = "SSO is not enabled")
    )
)]
/// See [`login_sso_redirect_v3_doc`].
pub fn login_sso_redirect_v3_doc() -> String {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/login/sso/redirect` — Redirect to the configured SSO provider on the r0 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/login/sso/redirect",
    tag = "Authentication",
    params(
        ("redirectUrl" = Option<String>, Query, description = "Preferred redirect URL after SSO login"),
        ("redirect_url" = Option<String>, Query, description = "Compatibility alias for redirectUrl")
    ),
    responses(
        (status = 307, description = "Temporary redirect to the SSO provider"),
        (status = 400, description = "SSO is not enabled")
    )
)]
/// See [`login_sso_redirect_r0_doc`].
pub fn login_sso_redirect_r0_doc() -> String {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/login/sso/userinfo` — Read the authenticated user's SSO profile.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/login/sso/userinfo",
    tag = "Authentication",
    responses(
        (status = 200, description = "OIDC-compatible userinfo",
            body = schemas::ApiAdminGenericJson,
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
/// See [`login_sso_userinfo_v3_doc`].
pub fn login_sso_userinfo_v3_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/login/sso/userinfo` — Read the authenticated user's SSO profile on the r0 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/login/sso/userinfo",
    tag = "Authentication",
    responses(
        (status = 200, description = "OIDC-compatible userinfo",
            body = schemas::ApiAdminGenericJson,
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
/// See [`login_sso_userinfo_r0_doc`].
pub fn login_sso_userinfo_r0_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/login/sso/redirect/cas` — Redirect to the configured CAS entrypoint.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/login/sso/redirect/cas",
    tag = "Authentication",
    params(
        ("redirect_after" = Option<String>, Query, description = "Where to continue after CAS login")
    ),
    responses(
        (status = 302, description = "Redirect to CAS login"),
        (status = 400, description = "CAS SSO is not configured")
    )
)]
/// See [`login_sso_redirect_cas_v3_doc`].
pub fn login_sso_redirect_cas_v3_doc() -> String {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/login/sso/redirect/cas` — Redirect to the configured CAS entrypoint on the r0 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/login/sso/redirect/cas",
    tag = "Authentication",
    params(
        ("redirect_after" = Option<String>, Query, description = "Where to continue after CAS login")
    ),
    responses(
        (status = 302, description = "Redirect to CAS login"),
        (status = 400, description = "CAS SSO is not configured")
    )
)]
/// See [`login_sso_redirect_cas_r0_doc`].
pub fn login_sso_redirect_cas_r0_doc() -> String {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/login/sso/redirect/saml` — Redirect to the configured SAML IdP.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/login/sso/redirect/saml",
    tag = "Authentication",
    params(
        ("redirect_url" = Option<String>, Query, description = "Optional post-login redirect URL")
    ),
    responses(
        (status = 307, description = "Temporary redirect to the SAML IdP"),
        (status = 400, description = "SAML authentication is not enabled")
    )
)]
/// See [`login_sso_redirect_saml_v3_doc`].
pub fn login_sso_redirect_saml_v3_doc() -> String {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/login/sso/redirect/saml` — Redirect to the configured SAML IdP on the r0 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/login/sso/redirect/saml",
    tag = "Authentication",
    params(
        ("redirect_url" = Option<String>, Query, description = "Optional post-login redirect URL")
    ),
    responses(
        (status = 307, description = "Temporary redirect to the SAML IdP"),
        (status = 400, description = "SAML authentication is not enabled")
    )
)]
/// See [`login_sso_redirect_saml_r0_get_doc`].
pub fn login_sso_redirect_saml_r0_get_doc() -> String {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/r0/login/sso/redirect/saml` — Request a SAML login redirect URL as JSON.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/r0/login/sso/redirect/saml",
    tag = "Authentication",
    request_body = schemas::ApiAdminGenericJson,
    responses(
        (status = 200, description = "SAML redirect URL",
            body = schemas::ApiAdminGenericJson,
            example = json!({
                "redirect_url": "https://idp.example.com/sso?SAMLRequest=..."
            })
        ),
        (status = 400, description = "SAML authentication is not enabled")
    )
)]
/// See [`login_sso_redirect_saml_r0_post_doc`].
pub fn login_sso_redirect_saml_r0_post_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/login/saml/callback` — Handle a browser-based SAML login callback.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/login/saml/callback",
    tag = "Authentication",
    params(
        ("saml_response" = Option<String>, Query, description = "SAML response payload"),
        ("saml_request" = Option<String>, Query, description = "SAML request payload"),
        ("relay_state" = Option<String>, Query, description = "SAML relay state")
    ),
    responses(
        (status = 200, description = "Completed SAML login",
            body = schemas::ApiAdminGenericJson,
            example = json!({
                "user_id": "@alice:example.com",
                "access_token": "syt_abcdef",
                "device_id": "SAML_DEVICE",
                "expires_in": 3600,
                "refresh_token": "syr_refresh"
            })
        ),
        (status = 400, description = "Missing or invalid SAML response")
    )
)]
/// See [`login_saml_callback_v3_get_doc`].
pub fn login_saml_callback_v3_get_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/login/saml/callback` — Handle a posted SAML login callback.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/login/saml/callback",
    tag = "Authentication",
    request_body = schemas::ApiAdminGenericJson,
    responses(
        (status = 200, description = "Completed SAML login",
            body = schemas::ApiAdminGenericJson,
            example = json!({
                "user_id": "@alice:example.com",
                "access_token": "syt_abcdef",
                "device_id": "SAML_DEVICE",
                "expires_in": 3600,
                "refresh_token": "syr_refresh"
            })
        ),
        (status = 400, description = "Missing or invalid SAML response")
    )
)]
/// See [`login_saml_callback_v3_post_doc`].
pub fn login_saml_callback_v3_post_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/login/saml/callback` — Handle a browser-based SAML login callback on the r0 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/login/saml/callback",
    tag = "Authentication",
    params(
        ("saml_response" = Option<String>, Query, description = "SAML response payload"),
        ("saml_request" = Option<String>, Query, description = "SAML request payload"),
        ("relay_state" = Option<String>, Query, description = "SAML relay state")
    ),
    responses(
        (status = 200, description = "Completed SAML login",
            body = schemas::ApiAdminGenericJson,
            example = json!({
                "user_id": "@alice:example.com",
                "access_token": "syt_abcdef",
                "device_id": "SAML_DEVICE",
                "expires_in": 3600,
                "refresh_token": "syr_refresh"
            })
        ),
        (status = 400, description = "Missing or invalid SAML response")
    )
)]
/// See [`login_saml_callback_r0_get_doc`].
pub fn login_saml_callback_r0_get_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/r0/login/saml/callback` — Handle a posted SAML login callback on the r0 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/r0/login/saml/callback",
    tag = "Authentication",
    request_body = schemas::ApiAdminGenericJson,
    responses(
        (status = 200, description = "Completed SAML login",
            body = schemas::ApiAdminGenericJson,
            example = json!({
                "user_id": "@alice:example.com",
                "access_token": "syt_abcdef",
                "device_id": "SAML_DEVICE",
                "expires_in": 3600,
                "refresh_token": "syr_refresh"
            })
        ),
        (status = 400, description = "Missing or invalid SAML response")
    )
)]
/// See [`login_saml_callback_r0_post_doc`].
pub fn login_saml_callback_r0_post_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/logout/saml` — Initiate SAML single logout for the authenticated user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/logout/saml",
    tag = "Authentication",
    responses(
        (status = 200, description = "SAML logout status",
            body = schemas::ApiAdminGenericJson,
            example = json!({
                "redirect_url": "https://idp.example.com/slo?SAMLRequest=..."
            })
        ),
        (status = 400, description = "SAML authentication is not enabled")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
/// See [`logout_saml_r0_doc`].
pub fn logout_saml_r0_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/logout/saml/callback` — Complete SAML single logout.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/logout/saml/callback",
    tag = "Authentication",
    params(
        ("saml_response" = Option<String>, Query, description = "SAML logout response"),
        ("relay_state" = Option<String>, Query, description = "SAML relay state")
    ),
    responses(
        (status = 200, description = "Logout completed",
            body = schemas::ApiAdminGenericJson,
            example = json!({
                "message": "Logout successful"
            })
        ),
        (status = 400, description = "SAML authentication is not enabled or response is missing")
    )
)]
/// See [`logout_saml_callback_r0_doc`].
pub fn logout_saml_callback_r0_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/oidc/logout` — Revoke OIDC session state for the authenticated user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/oidc/logout",
    tag = "Authentication",
    request_body = schemas::ApiAdminGenericJson,
    responses(
        (status = 200, description = "OIDC logout completed",
            body = schemas::ApiAdminSuccess,
            example = json!({
                "success": true
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
/// See [`oidc_logout_v3_doc`].
pub fn oidc_logout_v3_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/r0/oidc/logout` — Revoke OIDC session state on the r0 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/r0/oidc/logout",
    tag = "Authentication",
    request_body = schemas::ApiAdminGenericJson,
    responses(
        (status = 200, description = "OIDC logout completed",
            body = schemas::ApiAdminSuccess,
            example = json!({
                "success": true
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
/// See [`oidc_logout_r0_doc`].
pub fn oidc_logout_r0_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/oidc/login` — Built-in OIDC provider login helper.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/oidc/login",
    tag = "Authentication",
    request_body = schemas::ApiAdminGenericJson,
    responses(
        (status = 200, description = "Authorization code issued by the built-in OIDC provider",
            body = schemas::ApiAdminGenericJson,
            example = json!({
                "code": "oidc-auth-code"
            })
        ),
        (status = 400, description = "Built-in OIDC provider is not enabled"),
        (status = 401, description = "Authorization failed")
    )
)]
/// See [`oidc_login_v3_doc`].
pub fn oidc_login_v3_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_matrix/client/v3/register/captcha/clean` — Clean expired captchas.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_matrix/client/v3/register/captcha/clean",
    tag = "Authentication",
    responses(
        (status = 200, description = "Expired captchas cleaned", body = schemas::ApiAdminGenericJson)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
/// See [`cleanup_captcha_doc`].
pub fn cleanup_captcha_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/static/client/login/` — Browser login fallback page.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/static/client/login/",
    tag = "Authentication",
    responses(
        (status = 200, description = "HTML login fallback page", body = String)
    )
)]
/// See [`login_fallback_page_doc`].
pub fn login_fallback_page_doc() -> String {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/voice/user/{user_id}/stats` — Get voice stats for a user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/voice/user/{user_id}/stats",
    tag = "Private Extension - Voice",
    params(
        ("user_id" = String, Path, description = "The ID of the user"),
    ),
    responses(
        (status = 200, description = "User voice stats", body = schemas::ApiAdminGenericJson),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
/// See [`get_user_voice_stats_doc`].
pub fn get_user_voice_stats_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/voice/user/{user_id}` — List voice messages for a user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/voice/user/{user_id}",
    tag = "Private Extension - Voice",
    params(
        ("user_id" = String, Path, description = "The ID of the user"),
        ("limit" = Option<i64>, Query, description = "Maximum messages to return"),
        ("from" = Option<i64>, Query, description = "Pagination offset"),
    ),
    responses(
        (status = 200, description = "Voice messages", body = schemas::ApiAdminGenericJson),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
/// See [`get_user_voice_messages_doc`].
pub fn get_user_voice_messages_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v1/user/burn/config` — Set global burn config.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v1/user/burn/config",
    tag = "Private Extension - Burn",
    request_body = schemas::ApiAdminGenericJson,
    responses(
        (status = 200, description = "Updated", body = schemas::ApiAdminGenericJson),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
/// See [`set_global_burn_config_doc`].
pub fn set_global_burn_config_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/user/burn/stats` — Get user burn stats.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/user/burn/stats",
    tag = "Private Extension - Burn",
    responses(
        (status = 200, description = "Burn stats", body = schemas::ApiAdminGenericJson),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
/// See [`get_burn_stats_doc`].
pub fn get_burn_stats_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}
