pub mod admin;
pub mod e2ee_routes;
pub mod federation;
pub mod friend;
pub mod key_backup;
pub mod media;
pub mod private_chat;
pub mod voice;

pub use admin::create_admin_router;
pub use e2ee_routes::create_e2ee_router;
pub use federation::create_federation_router;
pub use friend::create_friend_router;
pub use key_backup::create_key_backup_router;
pub use media::create_media_router;
pub use private_chat::create_private_chat_router;
pub use voice::create_voice_router;

use crate::cache::*;
use crate::common::*;
use crate::services::*;
use crate::storage::CreateEventParams;
use axum::{
    extract::{FromRequestParts, Json, Path, Query, State},
    http::{request::Parts, HeaderMap},
    routing::{delete, get, post, put},
    Router,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tower_http::compression::CompressionLayer;

#[derive(Clone)]
pub struct AppState {
    pub services: ServiceContainer,
    pub cache: Arc<CacheManager>,
}

impl AppState {
    pub fn new(services: ServiceContainer, cache: Arc<CacheManager>) -> Self {
        Self { services, cache }
    }
}

#[derive(Clone)]
pub struct AuthenticatedUser {
    pub user_id: String,
    pub device_id: Option<String>,
    pub is_admin: bool,
    pub access_token: String,
}

#[derive(Clone)]
pub struct AdminUser {
    pub user_id: String,
    pub device_id: Option<String>,
    pub access_token: String,
}

impl FromRequestParts<AppState> for AuthenticatedUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = extract_token_from_headers(&parts.headers)?;

        let (user_id, device_id, is_admin) =
            state.services.auth_service.validate_token(&token).await?;

        Ok(AuthenticatedUser {
            user_id,
            device_id,
            is_admin,
            access_token: token,
        })
    }
}

impl FromRequestParts<AppState> for AdminUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let auth = AuthenticatedUser::from_request_parts(parts, state).await?;
        if !auth.is_admin {
            return Err(ApiError::forbidden("Admin access required".to_string()));
        }
        Ok(AdminUser {
            user_id: auth.user_id,
            device_id: auth.device_id,
            access_token: auth.access_token,
        })
    }
}

fn extract_token_from_headers(headers: &HeaderMap) -> Result<String, ApiError> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.to_string())
        .ok_or_else(|| {
            ApiError::unauthorized("Missing or invalid authorization header".to_string())
        })
}

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route(
            "/",
            get(|| async {
                Json(json!({"msg": "Synapse Rust Matrix Server", "version": "0.1.0"}))
            }),
        )
        .route("/_matrix/client/versions", get(get_client_versions))
        .route("/_matrix/client/r0/register", post(register))
        .route(
            "/_matrix/client/r0/register/available",
            get(check_username_availability),
        )
        .route("/_matrix/client/r0/login", post(login))
        .route("/_matrix/client/r0/logout", post(logout))
        .route("/_matrix/client/r0/logout/all", post(logout_all))
        .route("/_matrix/client/r0/refresh", post(refresh_token))
        .route("/_matrix/client/r0/account/whoami", get(whoami))
        .route(
            "/_matrix/client/r0/account/profile/{user_id}",
            get(get_profile),
        )
        .route(
            "/_matrix/client/r0/account/profile/{user_id}/displayname",
            put(update_displayname),
        )
        .route(
            "/_matrix/client/r0/account/profile/{user_id}/avatar_url",
            put(update_avatar),
        )
        .route("/_matrix/client/r0/account/password", post(change_password))
        .route(
            "/_matrix/client/r0/account/deactivate",
            post(deactivate_account),
        )
        .route("/_matrix/client/r0/sync", get(sync))
        .route(
            "/_matrix/client/r0/rooms/{room_id}/messages",
            get(get_messages),
        )
        .route(
            "/_matrix/client/r0/rooms/{room_id}/send/{event_type}",
            post(send_message),
        )
        .route("/_matrix/client/r0/rooms/{room_id}/join", post(join_room))
        .route("/_matrix/client/r0/rooms/{room_id}/leave", post(leave_room))
        .route(
            "/_matrix/client/r0/rooms/{room_id}/members",
            get(get_room_members),
        )
        .route(
            "/_matrix/client/r0/rooms/{room_id}/invite",
            post(invite_user),
        )
        .route("/_matrix/client/r0/createRoom", post(create_room))
        .route("/_matrix/client/r0/directory/room/{room_id}", get(get_room))
        .route(
            "/_matrix/client/r0/directory/room/{room_id}",
            delete(delete_room),
        )
        .route("/_matrix/client/r0/publicRooms", get(get_public_rooms))
        .route("/_matrix/client/r0/publicRooms", post(create_public_room))
        .route(
            "/_matrix/client/r0/user/{user_id}/rooms",
            get(get_user_rooms),
        )
        .route("/_matrix/client/r0/devices", get(get_devices))
        .route("/_matrix/client/r0/delete_devices", post(delete_devices))
        .route("/_matrix/client/r0/devices/{device_id}", get(get_device))
        .route("/_matrix/client/r0/devices/{device_id}", put(update_device))
        .route(
            "/_matrix/client/r0/devices/{device_id}",
            delete(delete_device),
        )
        .route(
            "/_matrix/client/r0/presence/{user_id}/status",
            get(get_presence),
        )
        .route(
            "/_matrix/client/r0/presence/{user_id}/status",
            put(set_presence),
        )
        .route(
            "/_matrix/client/r0/rooms/{room_id}/state",
            get(get_room_state),
        )
        .route(
            "/_matrix/client/r0/rooms/{room_id}/state/{event_type}",
            get(get_state_by_type),
        )
        .route(
            "/_matrix/client/r0/rooms/{room_id}/state/{event_type}/{state_key}",
            get(get_state_event),
        )
        .route(
            "/_matrix/client/r0/rooms/{room_id}/redact/{event_id}",
            put(redact_event),
        )
        .route("/_matrix/client/r0/rooms/{room_id}/kick", post(kick_user))
        .route("/_matrix/client/r0/rooms/{room_id}/ban", post(ban_user))
        .route("/_matrix/client/r0/rooms/{room_id}/unban", post(unban_user))
        .merge(create_friend_router(state.clone()))
        .merge(create_private_chat_router(state.clone()))
        .merge(create_voice_router(state.clone()))
        .merge(create_media_router(state.clone()))
        .merge(create_e2ee_router(state.clone()))
        .merge(create_key_backup_router(state.clone()))
        .merge(create_admin_router(state.clone()))
        .merge(create_federation_router(state.clone()))
        .layer(CompressionLayer::new())
        .with_state(state)
}

async fn get_client_versions() -> Json<Value> {
    Json(json!({
        "versions": ["r0.0.1", "r0.1.0", "r0.2.0", "r0.3.0", "r0.4.0", "r0.5.0", "r0.6.0"],
        "unstable_features": {
            "m.lazy_load_members": true,
            "m.require_identity_server": false,
            "m.supports_login_via_phone_number": true
        }
    }))
}

async fn register(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let username = body
        .get("username")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Username required".to_string()))?;
    let password = body
        .get("password")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Password required".to_string()))?;
    let admin = body.get("admin").and_then(|v| v.as_bool()).unwrap_or(false);
    let displayname = body.get("displayname").and_then(|v| v.as_str());

    let registration_service = RegistrationService::new(&state.services);
    Ok(Json(
        registration_service
            .register_user(username, password, admin, displayname)
            .await?,
    ))
}

async fn check_username_availability(
    State(state): State<AppState>,
    Query(params): Query<Value>,
) -> Result<Json<Value>, ApiError> {
    let username = params
        .get("username")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Username required".to_string()))?;

    let exists = state
        .services
        .user_storage
        .user_exists(&format!("@{}:{}", username, state.services.server_name))
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(json!({
        "available": !exists,
        "username": username
    })))
}

async fn login(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let username = body
        .get("user")
        .or(body.get("username"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Username required".to_string()))?;
    let password = body
        .get("password")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Password required".to_string()))?;
    let device_id = body.get("device_id").and_then(|v| v.as_str());
    let initial_display_name = body.get("initial_display_name").and_then(|v| v.as_str());

    let (user, access_token, refresh_token, device_id) = state
        .services
        .auth_service
        .login(username, password, device_id, initial_display_name)
        .await?;

    Ok(Json(json!({
        "access_token": access_token,
        "refresh_token": refresh_token,
        "expires_in": state.services.auth_service.token_expiry,
        "device_id": device_id,
        "user_id": user.user_id(),
        "well_known": {
            "m.homeserver": {
                "base_url": format!("http://{}:8008", state.services.server_name)
            }
        }
    })))
}

async fn logout(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(_body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    state
        .services
        .auth_service
        .logout(&auth_user.access_token, auth_user.device_id.as_deref())
        .await?;

    Ok(Json(json!({})))
}

async fn logout_all(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(_body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    state
        .services
        .auth_service
        .logout_all(&auth_user.user_id)
        .await?;

    Ok(Json(json!({})))
}

async fn refresh_token(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let refresh_token = body
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Refresh token required".to_string()))?;

    let (new_access, new_refresh, device_id) = state
        .services
        .auth_service
        .refresh_token(refresh_token)
        .await?;

    Ok(Json(json!({
        "access_token": new_access,
        "refresh_token": new_refresh,
        "expires_in": state.services.auth_service.token_expiry,
        "device_id": device_id
    })))
}

async fn whoami(auth_user: AuthenticatedUser) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({
        "user_id": auth_user.user_id,
        "displayname": None::<String>,
        "admin": auth_user.is_admin
    })))
}

async fn get_profile(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let registration_service = RegistrationService::new(&state.services);
    Ok(Json(registration_service.get_profile(&user_id).await?))
}

async fn update_displayname(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    if user_id != auth_user.user_id && !auth_user.is_admin {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }

    let displayname = body
        .get("displayname")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Displayname required".to_string()))?;

    let registration_service = RegistrationService::new(&state.services);
    registration_service
        .update_user_profile(&user_id, Some(displayname), None)
        .await?;
    Ok(Json(json!({})))
}

async fn update_avatar(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    if user_id != auth_user.user_id && !auth_user.is_admin {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }

    let avatar_url = body
        .get("avatar_url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Avatar URL required".to_string()))?;

    let registration_service = RegistrationService::new(&state.services);
    registration_service
        .update_user_profile(&user_id, None, Some(avatar_url))
        .await?;
    Ok(Json(json!({})))
}

async fn change_password(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let new_password = body
        .get("new_password")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("New password required".to_string()))?;

    let registration_service = RegistrationService::new(&state.services);
    registration_service
        .change_password(&auth_user.user_id, new_password)
        .await?;

    Ok(Json(json!({})))
}

async fn deactivate_account(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(_body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let registration_service = RegistrationService::new(&state.services);
    registration_service
        .deactivate_account(&auth_user.user_id)
        .await?;
    Ok(Json(json!({})))
}

async fn sync(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<Value>,
) -> Result<Json<Value>, ApiError> {
    let token = extract_token_from_headers(&headers)?;
    let (user_id, _, _) = state.services.auth_service.validate_token(&token).await?;

    let timeout = params
        .get("timeout")
        .and_then(|v| v.as_u64())
        .unwrap_or(30000);
    let full_state = params
        .get("full_state")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let set_presence = params
        .get("set_presence")
        .and_then(|v| v.as_str())
        .unwrap_or("online");

    let sync_service = SyncService::new(&state.services);
    Ok(Json(
        sync_service
            .sync(&user_id, timeout, full_state, set_presence)
            .await?,
    ))
}

async fn get_messages(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Query(params): Query<Value>,
) -> Result<Json<Value>, ApiError> {
    let from = params
        .get("from")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(10);
    let direction = params.get("dir").and_then(|v| v.as_str()).unwrap_or("b");

    let room_service = RoomService::new(&state.services);
    Ok(Json(
        room_service
            .get_room_messages(&room_id, from as i64, limit as i64, direction)
            .await?,
    ))
}

async fn send_message(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, _event_type)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let msgtype = body
        .get("msgtype")
        .and_then(|v| v.as_str())
        .unwrap_or("m.room.message");
    let content = body
        .get("body")
        .ok_or_else(|| ApiError::bad_request("Message body required".to_string()))?;

    let room_service = RoomService::new(&state.services);
    Ok(Json(
        room_service
            .send_message(&room_id, &auth_user.user_id, msgtype, content)
            .await?,
    ))
}

async fn join_room(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let token = extract_token_from_headers(&headers)?;
    let (user_id, _, _) = state.services.auth_service.validate_token(&token).await?;

    let room_service = RoomService::new(&state.services);
    room_service.join_room(&room_id, &user_id).await?;
    Ok(Json(json!({})))
}

async fn leave_room(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let room_service = RoomService::new(&state.services);
    room_service
        .leave_room(&room_id, &auth_user.user_id)
        .await?;
    Ok(Json(json!({})))
}

async fn get_room_members(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let token = extract_token_from_headers(&headers)?;
    let (user_id, _, _) = state.services.auth_service.validate_token(&token).await?;

    let room_service = RoomService::new(&state.services);
    let members = room_service.get_room_members(&room_id, &user_id).await?;
    Ok(Json(members))
}

async fn invite_user(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let invitee = body
        .get("user_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("User ID required".to_string()))?;

    let room_service = RoomService::new(&state.services);
    room_service
        .invite_user(&room_id, &auth_user.user_id, invitee)
        .await?;
    Ok(Json(json!({})))
}

async fn create_room(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let token = extract_token_from_headers(&headers)?;
    let (user_id, _, _) = state.services.auth_service.validate_token(&token).await?;

    let visibility = body.get("visibility").and_then(|v| v.as_str());
    let room_alias = body.get("room_alias_name").and_then(|v| v.as_str());
    let name = body.get("name").and_then(|v| v.as_str());
    let topic = body.get("topic").and_then(|v| v.as_str());
    let invite = body.get("invite").and_then(|v| v.as_array()).map(|arr| {
        arr.iter()
            .filter_map(|x| x.as_str().map(String::from))
            .collect()
    });
    let preset = body.get("preset").and_then(|v| v.as_str());

    let config = CreateRoomConfig {
        visibility: visibility.map(|s| s.to_string()),
        room_alias_name: room_alias.map(|s| s.to_string()),
        name: name.map(|s| s.to_string()),
        topic: topic.map(|s| s.to_string()),
        invite_list: invite,
        preset: preset.map(|s| s.to_string()),
        ..Default::default()
    };

    let room_service = RoomService::new(&state.services);
    Ok(Json(room_service.create_room(&user_id, config).await?))
}

async fn get_room(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let room_service = RoomService::new(&state.services);
    Ok(Json(room_service.get_room(&room_id).await?))
}

async fn delete_room(
    State(_state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(_room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.is_admin {
        return Err(ApiError::forbidden("Admin access required".to_string()));
    }

    Ok(Json(json!({})))
}

async fn get_public_rooms(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Query(params): Query<Value>,
) -> Result<Json<Value>, ApiError> {
    let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(10);
    let _since = params.get("since").and_then(|v| v.as_str());

    let room_service = RoomService::new(&state.services);
    Ok(Json(room_service.get_public_rooms(limit as i64).await?))
}

#[axum::debug_handler]
async fn create_public_room(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let visibility = body.get("visibility").and_then(|v| v.as_str());
    let room_alias = body.get("room_alias_name").and_then(|v| v.as_str());
    let name = body.get("name").and_then(|v| v.as_str());
    let topic = body.get("topic").and_then(|v| v.as_str());
    let invite = body.get("invite").and_then(|v| v.as_array()).map(|arr| {
        arr.iter()
            .filter_map(|x| x.as_str().map(String::from))
            .collect()
    });
    let preset = body.get("preset").and_then(|v| v.as_str());

    let config = CreateRoomConfig {
        visibility: visibility.map(|s| s.to_string()),
        room_alias_name: room_alias.map(|s| s.to_string()),
        name: name.map(|s| s.to_string()),
        topic: topic.map(|s| s.to_string()),
        invite_list: invite,
        preset: preset.map(|s| s.to_string()),
        ..Default::default()
    };

    let room_service = RoomService::new(&state.services);
    Ok(Json(
        room_service.create_room(&auth_user.user_id, config).await?,
    ))
}

async fn get_user_rooms(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if auth_user.user_id != user_id && !auth_user.is_admin {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }

    let room_service = RoomService::new(&state.services);
    let rooms = room_service.get_joined_rooms(&user_id).await?;

    Ok(Json(json!({
        "joined_rooms": rooms
    })))
}

async fn get_devices(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let token = extract_token_from_headers(&headers)?;
    let (user_id, _, _) = state.services.auth_service.validate_token(&token).await?;

    let devices = state
        .services
        .device_storage
        .get_user_devices(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get devices: {}", e)))?;

    let device_list: Vec<Value> = devices
        .iter()
        .map(|d| {
            json!({
                "device_id": d.device_id,
                "display_name": d.display_name,
                "last_seen_ts": d.last_seen_ts.unwrap_or(0),
                "user_id": d.user_id
            })
        })
        .collect();

    Ok(Json(json!({ "devices": device_list })))
}

async fn get_device(
    State(_state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(device_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let device = _state
        .services
        .device_storage
        .get_device(&device_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get device: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Device not found".to_string()))?;

    if device.user_id != _auth_user.user_id && !_auth_user.is_admin {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }

    Ok(Json(json!({
        "device_id": device.device_id,
        "display_name": device.display_name,
        "last_seen_ts": device.last_seen_ts.unwrap_or(0),
        "user_id": device.user_id
    })))
}

async fn update_device(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(device_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let display_name = body
        .get("display_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Display name required".to_string()))?;

    state
        .services
        .device_storage
        .update_device_display_name(&device_id, display_name)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to update device: {}", e)))?;

    Ok(Json(json!({})))
}

async fn delete_device(
    State(_state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(device_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    _state
        .services
        .device_storage
        .delete_device(&device_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to delete device: {}", e)))?;

    Ok(Json(json!({})))
}

async fn delete_devices(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let devices = body
        .get("devices")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ApiError::bad_request("Devices array required".to_string()))?;

    let device_ids: Vec<String> = devices
        .iter()
        .filter_map(|v| v.as_str())
        .map(String::from)
        .collect();

    if device_ids.is_empty() {
        return Ok(Json(json!({})));
    }

    state
        .services
        .device_storage
        .delete_devices_batch(&device_ids)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to delete devices: {}", e)))?;

    Ok(Json(json!({})))
}

async fn get_presence(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let presence = state
        .services
        .presence_storage
        .get_presence(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get presence: {}", e)))?;

    match presence {
        Some((presence, status_msg)) => Ok(Json(json!({
            "presence": presence,
            "status_msg": status_msg
        }))),
        _ => Ok(Json(json!({
            "presence": "offline",
            "status_msg": Option::<String>::None
        }))),
    }
}

async fn set_presence(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    if auth_user.user_id != user_id && !auth_user.is_admin {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }

    let presence = body
        .get("presence")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Presence required".to_string()))?;
    let status_msg = body.get("status_msg").and_then(|v| v.as_str());

    state
        .services
        .presence_storage
        .set_presence(&user_id, presence, status_msg)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to set presence: {}", e)))?;

    Ok(Json(json!({})))
}

async fn get_room_state(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let events = state
        .services
        .event_storage
        .get_state_events(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get state: {}", e)))?;

    let state_events: Vec<Value> = events
        .iter()
        .map(|e| {
            json!({
                "type": e.event_type,
                "event_id": e.event_id,
                "sender": e.user_id,
                "content": e.content,
                "state_key": e.state_key
            })
        })
        .collect();

    Ok(Json(json!({ "state": state_events })))
}

async fn get_state_by_type(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path((room_id, event_type)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let events = state
        .services
        .event_storage
        .get_state_events_by_type(&room_id, &event_type)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get state: {}", e)))?;

    let state_events: Vec<Value> = events
        .iter()
        .map(|e| {
            json!({
                "type": e.event_type,
                "event_id": e.event_id,
                "sender": e.user_id,
                "content": e.content,
                "state_key": e.state_key
            })
        })
        .collect();

    Ok(Json(json!({ "events": state_events })))
}

async fn get_state_event(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path((room_id, event_type, state_key)): Path<(String, String, String)>,
) -> Result<Json<Value>, ApiError> {
    let events = state
        .services
        .event_storage
        .get_state_events_by_type(&room_id, &event_type)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get state: {}", e)))?;

    let event = events
        .iter()
        .find(|e| e.state_key == Some(state_key.clone()))
        .ok_or_else(|| ApiError::not_found("State event not found".to_string()))?;

    Ok(Json(json!({
        "type": event.event_type,
        "event_id": event.event_id,
        "sender": event.user_id,
        "content": event.content,
        "state_key": event.state_key
    })))
}

async fn redact_event(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, _event_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let reason = body.get("reason").and_then(|v| v.as_str());

    let new_event_id = crate::common::crypto::generate_event_id(&state.services.server_name);
    let now = chrono::Utc::now().timestamp_millis();

    let content = json!({
        "reason": reason
    });

    state
        .services
        .event_storage
        .create_event(CreateEventParams {
            event_id: new_event_id.clone(),
            room_id: room_id.clone(),
            user_id: auth_user.user_id,
            event_type: "m.room.redaction".to_string(),
            content,
            state_key: None,
            origin_server_ts: now,
        })
        .await
        .map_err(|e| ApiError::internal(format!("Failed to redact event: {}", e)))?;

    Ok(Json(json!({
        "event_id": new_event_id
    })))
}

async fn kick_user(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let target = body
        .get("user_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("User ID required".to_string()))?;
    let reason = body.get("reason").and_then(|v| v.as_str());

    let event_id = crate::common::crypto::generate_event_id(&state.services.server_name);
    let content = json!({
        "membership": "leave",
        "reason": reason.unwrap_or("")
    });

    state
        .services
        .member_storage
        .remove_member(&room_id, target)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to kick user: {}", e)))?;

    state
        .services
        .event_storage
        .create_event(CreateEventParams {
            event_id,
            room_id: room_id.clone(),
            user_id: auth_user.user_id,
            event_type: "m.room.member".to_string(),
            content,
            state_key: Some(target.to_string()),
            origin_server_ts: chrono::Utc::now().timestamp_millis(),
        })
        .await
        .ok();

    Ok(Json(json!({})))
}

async fn ban_user(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let target = body
        .get("user_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("User ID required".to_string()))?;
    let reason = body.get("reason").and_then(|v| v.as_str());

    let event_id = crate::common::crypto::generate_event_id(&state.services.server_name);
    let content = json!({
        "membership": "ban",
        "reason": reason.unwrap_or("")
    });

    state
        .services
        .member_storage
        .add_member(&room_id, target, "ban", None, None)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to ban user: {}", e)))?;

    state
        .services
        .event_storage
        .create_event(CreateEventParams {
            event_id,
            room_id: room_id.clone(),
            user_id: auth_user.user_id,
            event_type: "m.room.member".to_string(),
            content,
            state_key: Some(target.to_string()),
            origin_server_ts: chrono::Utc::now().timestamp_millis(),
        })
        .await
        .ok();

    Ok(Json(json!({})))
}

async fn unban_user(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let target = body
        .get("user_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("User ID required".to_string()))?;

    state
        .services
        .member_storage
        .remove_member(&room_id, target)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to unban user: {}", e)))?;

    let event_id = crate::common::crypto::generate_event_id(&state.services.server_name);
    let content = json!({
        "membership": "leave"
    });

    state
        .services
        .event_storage
        .create_event(CreateEventParams {
            event_id,
            room_id: room_id.clone(),
            user_id: auth_user.user_id,
            event_type: "m.room.member".to_string(),
            content,
            state_key: Some(target.to_string()),
            origin_server_ts: chrono::Utc::now().timestamp_millis(),
        })
        .await
        .ok();

    Ok(Json(json!({})))
}
