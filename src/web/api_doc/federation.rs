#![cfg(feature = "openapi-docs")]

/// `GET /_matrix/federation/v1/version` — Get the server version.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1/version",
    tag = "Federation",
    responses(
        (status = 200, description = "Success",
            body = serde_json::Value,
            example = json!({
                "server": {
                    "name": "synapse-rust",
                    "version": "0.1.0"
                }
            })
        )
    )
)]
pub fn get_federation_version_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1` — Get server capabilities and discovery information.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1",
    tag = "Federation",
    responses(
        (status = 200, description = "Success",
            body = serde_json::Value,
            example = json!({
                "version": "0.1.0",
                "server_name": "example.com",
                "capabilities": {
                    "m.change_password": true,
                    "m.room_versions": {
                        "default": "10",
                        "available": {
                            "1": "stable",
                            "2": "stable",
                            "3": "stable",
                            "4": "stable",
                            "5": "stable",
                            "6": "stable",
                            "7": "stable",
                            "8": "stable",
                            "9": "stable",
                            "10": "stable",
                            "11": "stable",
                            "12": "stable",
                            "13": "stable"
                        }
                    }
                }
            })
        )
    )
)]
pub fn get_federation_discovery_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/publicRooms` — Get the public room list from this server.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1/publicRooms",
    tag = "Federation",
    params(
        ("limit" = Option<i64>, Query, description = "Maximum number of rooms to return"),
        ("since" = Option<String>, Query, description = "Pagination token")
    ),
    responses(
        (status = 200, description = "Public rooms list", body = serde_json::Value)
    )
)]
pub fn get_public_rooms_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/query/destination` — Query if a destination is reachable.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1/query/destination",
    tag = "Federation",
    params(
        ("server_name" = String, Query, description = "The server name to query")
    ),
    responses(
        (status = 200, description = "Success")
    )
)]
pub fn query_destination_doc() -> String {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/openid/userinfo` — Exchange an OpenID token for user information.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1/openid/userinfo",
    tag = "Federation",
    params(
        ("access_token" = String, Query, description = "The OpenID access token")
    ),
    responses(
        (status = 200, description = "User information",
            body = serde_json::Value,
            example = json!({ "sub": "@alice:example.com" })
        ),
        (status = 401, description = "Invalid or expired token")
    )
)]
pub fn openid_userinfo_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/members/{room_id}` — Get the full membership list for a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1/members/{room_id}",
    tag = "Federation",
    params(
        ("room_id" = String, Path, description = "The ID of the room")
    ),
    responses(
        (status = 200, description = "Membership list", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn get_room_members_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/members/{room_id}/joined` — Get the joined membership list for a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1/members/{room_id}/joined",
    tag = "Federation",
    params(
        ("room_id" = String, Path, description = "The ID of the room")
    ),
    responses(
        (status = 200, description = "Joined membership list", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn get_joined_room_members_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/user/devices/{user_id}` — Get the device list for a user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1/user/devices/{user_id}",
    tag = "Federation",
    params(
        ("user_id" = String, Path, description = "The ID of the user")
    ),
    responses(
        (status = 200, description = "Device list", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn get_user_devices_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/federation/v1/knock/{room_id}/{user_id}` — Submit a knock request to a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/federation/v1/knock/{room_id}/{user_id}",
    tag = "Federation",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
        ("user_id" = String, Path, description = "The ID of the user knocking")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Knock accepted", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn knock_room_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/federation/v1/thirdparty/invite` — Submit a third-party invite.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/federation/v1/thirdparty/invite",
    tag = "Federation",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Invite accepted", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn thirdparty_invite_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/federation/v2/invite/{room_id}/{event_id}` — Submit an invite (v2).
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/federation/v2/invite/{room_id}/{event_id}",
    tag = "Federation",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
        ("event_id" = String, Path, description = "The ID of the invite event")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Invite accepted", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn invite_v2_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/federation/v1/send/{txn_id}` — Submit a transaction of events.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/federation/v1/send/{txn_id}",
    tag = "Federation",
    params(
        ("txn_id" = String, Path, description = "The ID of the transaction")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Transaction accepted", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn send_transaction_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/make_join/{room_id}/{user_id}` — Request to join a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1/make_join/{room_id}/{user_id}",
    tag = "Federation",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
        ("user_id" = String, Path, description = "The ID of the user joining"),
        ("ver" = Option<Vec<String>>, Query, description = "Supported room versions")
    ),
    responses(
        (status = 200, description = "Join template", body = serde_json::Value),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Not Found")
    )
)]
pub fn make_join_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/make_leave/{room_id}/{user_id}` — Request to leave a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1/make_leave/{room_id}/{user_id}",
    tag = "Federation",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
        ("user_id" = String, Path, description = "The ID of the user leaving")
    ),
    responses(
        (status = 200, description = "Leave template", body = serde_json::Value),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Not Found")
    )
)]
pub fn make_leave_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/federation/v1/send_join/{room_id}/{event_id}` — Submit a join event.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/federation/v1/send_join/{room_id}/{event_id}",
    tag = "Federation",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
        ("event_id" = String, Path, description = "The ID of the join event")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Join accepted", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn send_join_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/federation/v1/send_leave/{room_id}/{event_id}` — Submit a leave event.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/federation/v1/send_leave/{room_id}/{event_id}",
    tag = "Federation",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
        ("event_id" = String, Path, description = "The ID of the leave event")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Leave accepted", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn send_leave_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/federation/v1/invite/{room_id}/{event_id}` — Submit an invite event.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/federation/v1/invite/{room_id}/{event_id}",
    tag = "Federation",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
        ("event_id" = String, Path, description = "The ID of the invite event")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Invite accepted", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn invite_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/federation/v1/get_missing_events/{room_id}` — Get missing events for a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/federation/v1/get_missing_events/{room_id}",
    tag = "Federation",
    params(
        ("room_id" = String, Path, description = "The ID of the room")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Missing events", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn get_missing_events_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/room/{room_id}/{event_id}` — Get a specific event from a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1/room/{room_id}/{event_id}",
    tag = "Federation",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
        ("event_id" = String, Path, description = "The ID of the event")
    ),
    responses(
        (status = 200, description = "Event data", body = serde_json::Value),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Not Found")
    )
)]
pub fn get_room_event_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/timestamp_to_event/{room_id}` — Get the event closest to a timestamp.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1/timestamp_to_event/{room_id}",
    tag = "Federation",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
        ("ts" = i64, Query, description = "The timestamp in milliseconds"),
        ("dir" = String, Query, description = "Direction to search: f or b")
    ),
    responses(
        (status = 200, description = "Event ID", body = serde_json::Value),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Not Found")
    )
)]
pub fn timestamp_to_event_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/get_event_auth/{room_id}/{event_id}` — Get the auth chain for an event.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1/get_event_auth/{room_id}/{event_id}",
    tag = "Federation",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
        ("event_id" = String, Path, description = "The ID of the event")
    ),
    responses(
        (status = 200, description = "Auth chain", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn get_event_auth_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/query/auth` — Query auth for an event.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1/query/auth",
    tag = "Federation",
    params(
        ("room_id" = String, Query, description = "The ID of the room"),
        ("event_id" = String, Query, description = "The ID of the event")
    ),
    responses(
        (status = 200, description = "Auth response", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn query_auth_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/state/{room_id}` — Get the state of a room at a given event.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1/state/{room_id}",
    tag = "Federation",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
        ("event_id" = String, Query, description = "The ID of the event to get state at")
    ),
    responses(
        (status = 200, description = "Room state", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn get_state_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/event/{event_id}` — Get a specific event.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1/event/{event_id}",
    tag = "Federation",
    params(
        ("event_id" = String, Path, description = "The ID of the event")
    ),
    responses(
        (status = 200, description = "Event data", body = serde_json::Value),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Not Found")
    )
)]
pub fn get_event_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/state_ids/{room_id}` — Get the state IDs of a room at a given event.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1/state_ids/{room_id}",
    tag = "Federation",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
        ("event_id" = String, Query, description = "The ID of the event to get state at")
    ),
    responses(
        (status = 200, description = "State IDs", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn get_state_ids_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/query/directory/room/{room_id}` — Query the room directory for a room ID.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1/query/directory/room/{room_id}",
    tag = "Federation",
    params(
        ("room_id" = String, Path, description = "The ID of the room")
    ),
    responses(
        (status = 200, description = "Room aliases", body = serde_json::Value),
        (status = 404, description = "Not Found")
    )
)]
pub fn room_directory_query_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/query/profile` — Query a user's profile.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1/query/profile",
    tag = "Federation",
    params(
        ("user_id" = String, Query, description = "The ID of the user"),
        ("field" = Option<String>, Query, description = "The profile field to query")
    ),
    responses(
        (status = 200, description = "Profile data", body = serde_json::Value),
        (status = 404, description = "Not Found")
    )
)]
pub fn profile_query_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/query/profile/{user_id}` — Query a user's profile (legacy).
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1/query/profile/{user_id}",
    tag = "Federation",
    params(
        ("user_id" = String, Path, description = "The ID of the user")
    ),
    responses(
        (status = 200, description = "Profile data", body = serde_json::Value),
        (status = 404, description = "Not Found")
    )
)]
pub fn profile_query_legacy_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/hierarchy/{room_id}` — Get the room hierarchy.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1/hierarchy/{room_id}",
    tag = "Federation",
    params(
        ("room_id" = String, Path, description = "The ID of the room")
    ),
    responses(
        (status = 200, description = "Room hierarchy", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn get_room_hierarchy_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/backfill/{room_id}` — Backfill events for a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1/backfill/{room_id}",
    tag = "Federation",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
        ("v" = Vec<String>, Query, description = "The event IDs to backfill from"),
        ("limit" = i64, Query, description = "Maximum number of events to return")
    ),
    responses(
        (status = 200, description = "Backfilled events", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn backfill_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/federation/v1/user/keys/upload` — Upload device keys (user path).
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/federation/v1/user/keys/upload",
    tag = "Federation",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Success", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn user_keys_upload_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/federation/v1/user/keys/claim` — Claim one-time keys.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/federation/v1/user/keys/claim",
    tag = "Federation",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Claimed keys", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn keys_claim_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/federation/v1/user/keys/query` — Query device keys.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/federation/v1/user/keys/query",
    tag = "Federation",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Device keys", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn keys_query_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/federation/v2/user/keys/query` — Query device keys (v2).
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/federation/v2/user/keys/query",
    tag = "Federation",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Device keys", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn v2_keys_query_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/federation/v2/send_join/{room_id}/{event_id}` — Submit a join event (v2).
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/federation/v2/send_join/{room_id}/{event_id}",
    tag = "Federation",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
        ("event_id" = String, Path, description = "The ID of the join event")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Join accepted", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn send_join_v2_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/federation/v2/send_leave/{room_id}/{event_id}` — Submit a leave event (v2).
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/federation/v2/send_leave/{room_id}/{event_id}",
    tag = "Federation",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
        ("event_id" = String, Path, description = "The ID of the leave event")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Leave accepted", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn send_leave_v2_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/federation/v1/publicRooms` — Query the public room list.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/federation/v1/publicRooms",
    tag = "Federation",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Public rooms list", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn post_public_rooms_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/query/directory` — Query the room directory for an alias.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1/query/directory",
    tag = "Federation",
    params(
        ("room_alias" = String, Query, description = "The room alias to query")
    ),
    responses(
        (status = 200, description = "Room ID", body = serde_json::Value),
        (status = 404, description = "Not Found")
    )
)]
pub fn query_directory_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/media/download/{server_name}/{media_id}` — Download media from a remote server.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1/media/download/{server_name}/{media_id}",
    tag = "Federation",
    params(
        ("server_name" = String, Path, description = "The server name"),
        ("media_id" = String, Path, description = "The media ID")
    ),
    responses(
        (status = 200, description = "Media content"),
        (status = 404, description = "Not Found")
    )
)]
pub fn media_download_federation_doc() -> String {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/media/thumbnail/{server_name}/{media_id}` — Get a thumbnail for media from a remote server.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1/media/thumbnail/{server_name}/{media_id}",
    tag = "Federation",
    params(
        ("server_name" = String, Path, description = "The server name"),
        ("media_id" = String, Path, description = "The media ID"),
        ("width" = Option<i64>, Query, description = "Thumbnail width"),
        ("height" = Option<i64>, Query, description = "Thumbnail height"),
        ("method" = Option<String>, Query, description = "Thumbnailing method")
    ),
    responses(
        (status = 200, description = "Thumbnail content"),
        (status = 404, description = "Not Found")
    )
)]
pub fn media_thumbnail_federation_doc() -> String {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/federation/v1/exchange_third_party_invite/{room_id}` — Exchange a third-party invite.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/federation/v1/exchange_third_party_invite/{room_id}",
    tag = "Federation",
    params(
        ("room_id" = String, Path, description = "The ID of the room")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Exchange accepted", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn exchange_third_party_invite_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v2/server` — Get the server's signing keys (v2).
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v2/server",
    tag = "Federation",
    responses(
        (status = 200, description = "Server keys", body = serde_json::Value)
    )
)]
pub fn server_key_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/key/v2/server` — Get the server's signing keys (v2, key path).
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/key/v2/server",
    tag = "Federation",
    responses(
        (status = 200, description = "Server keys", body = serde_json::Value)
    )
)]
pub fn server_key_v2_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v2/query/{server_name}/{key_id}` — Query server keys (v2).
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v2/query/{server_name}/{key_id}",
    tag = "Federation",
    params(
        ("server_name" = String, Path, description = "The server name"),
        ("key_id" = String, Path, description = "The key ID")
    ),
    responses(
        (status = 200, description = "Server keys", body = serde_json::Value),
        (status = 404, description = "Not Found")
    )
)]
pub fn key_query_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/key/v2/query/{server_name}/{key_id}` — Query server keys (v2, key path).
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/key/v2/query/{server_name}/{key_id}",
    tag = "Federation",
    params(
        ("server_name" = String, Path, description = "The server name"),
        ("key_id" = String, Path, description = "The key ID")
    ),
    responses(
        (status = 200, description = "Server keys", body = serde_json::Value),
        (status = 404, description = "Not Found")
    )
)]
pub fn key_query_v2_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}
