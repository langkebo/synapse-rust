// Direct Message Routes - 直接消息路由
// DM room creation and management

use crate::services::room_service::CreateRoomConfig;
use crate::web::routes::{ApiError, AppState, AuthenticatedUser};
use axum::{
    extract::{Path, State},
    routing::{get, post, put},
    Json, Router,
};
use serde_json::{json, Value};

#[derive(Debug, serde::Deserialize)]
pub struct CreateDmRequest {
    pub invite: Option<Vec<String>>,
    pub is_direct: Option<bool>,
    pub name: Option<String>,
    pub visibility: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct UpdateDmRequest {
    pub users: Option<Value>,
}

pub async fn create_dm_room(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<CreateDmRequest>,
) -> Result<Json<Value>, ApiError> {
    let invite_list = body.invite.unwrap_or_default();

    if invite_list.is_empty() {
        return Err(ApiError::bad_request(
            "At least one user must be invited to create a DM",
        ));
    }

    let config = CreateRoomConfig {
        name: body.name.clone(),
        visibility: Some(
            body.visibility
                .clone()
                .unwrap_or_else(|| "private".to_string()),
        ),
        preset: Some("private_chat".to_string()),
        invite_list: Some(invite_list.clone()),
        is_direct: Some(true),
        room_type: Some("m.direct".to_string()),
        ..Default::default()
    };

    let response = state
        .services
        .room_service
        .create_room(&auth_user.user_id, config)
        .await?;

    let room_id = response
        .get("room_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::internal("Failed to get room_id from create_room response"))?;

    let mut dm_users: Vec<String> = Vec::new();
    for u in &invite_list {
        let clean_user = u
            .replace("@", "")
            .split(':')
            .next()
            .unwrap_or("")
            .to_string();
        dm_users.push(format!("@{}:cjystx.top", clean_user));
    }

    let dm_content = serde_json::json!(dm_users);

    let direct_event_id = format!("direct_{}", room_id.trim_start_matches('!'));
    state
        .services
        .room_service
        .create_event(
            crate::storage::CreateEventParams {
                event_id: direct_event_id,
                room_id: room_id.to_string(),
                user_id: auth_user.user_id.clone(),
                event_type: "m.direct".to_string(),
                content: dm_content,
                state_key: Some(auth_user.user_id.clone()),
                origin_server_ts: chrono::Utc::now().timestamp_millis(),
            },
            None,
        )
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create m.direct event: {}", e)))?;

    Ok(Json(json!({
        "room_id": room_id
    })))
}

pub async fn get_dm_rooms(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let user_id = &auth_user.user_id;

    let rooms = state
        .services
        .room_storage
        .get_user_rooms(user_id)
        .await
        .map_err(|e| ApiError::database(format!("Failed to get user rooms: {}", e)))?;

    let mut dm_rooms = serde_json::Map::new();

    for room_id in rooms {
        let join_members = state
            .services
            .member_storage
            .get_room_members(&room_id, "join")
            .await
            .map_err(|e| ApiError::database(format!("Failed to get room join members: {}", e)))?;

        let invited_members = state
            .services
            .member_storage
            .get_room_members(&room_id, "invite")
            .await
            .map_err(|e| ApiError::database(format!("Failed to get room invite members: {}", e)))?;

        let total_members = join_members.len() + invited_members.len();

        if total_members == 2 {
            let other_member = join_members
                .iter()
                .find(|m| m.user_id.as_str() != user_id)
                .or_else(|| {
                    invited_members
                        .iter()
                        .find(|m| m.user_id.as_str() != user_id)
                });

            if let Some(member) = other_member {
                dm_rooms.insert(room_id, json!([member.user_id.clone()]));
            }
        }
    }

    Ok(Json(json!({
        "rooms": dm_rooms
    })))
}

pub async fn update_dm_room(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<UpdateDmRequest>,
) -> Result<Json<Value>, ApiError> {
    let join_members = state
        .services
        .member_storage
        .get_room_members(&room_id, "join")
        .await
        .map_err(|e| ApiError::database(format!("Failed to get room join members: {}", e)))?;

    let invited_members = state
        .services
        .member_storage
        .get_room_members(&room_id, "invite")
        .await
        .map_err(|e| ApiError::database(format!("Failed to get room invite members: {}", e)))?;

    let total_members = join_members.len() + invited_members.len();

    if total_members != 2 {
        return Err(ApiError::bad_request("Room is not a DM room"));
    }

    if let Some(users) = body.users {
        let event_id = format!("direct_update_{}", room_id.trim_start_matches('!'));
        state
            .services
            .room_service
            .create_event(
                crate::storage::CreateEventParams {
                    event_id,
                    room_id: room_id.clone(),
                    user_id: auth_user.user_id.clone(),
                    event_type: "m.direct".to_string(),
                    content: users,
                    state_key: Some(auth_user.user_id.clone()),
                    origin_server_ts: chrono::Utc::now().timestamp_millis(),
                },
                None,
            )
            .await
            .map_err(|e| ApiError::internal(format!("Failed to update m.direct event: {}", e)))?;
    }

    Ok(Json(json!({})))
}

pub async fn check_room_dm(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let join_members = state
        .services
        .member_storage
        .get_room_members(&room_id, "join")
        .await
        .map_err(|e| ApiError::database(format!("Failed to get room join members: {}", e)))?;

    let invited_members = state
        .services
        .member_storage
        .get_room_members(&room_id, "invite")
        .await
        .map_err(|e| ApiError::database(format!("Failed to get room invite members: {}", e)))?;

    let total_members = join_members.len() + invited_members.len();

    if total_members == 2 {
        Ok(Json(json!({
            "room_id": room_id,
            "m.direct": true
        })))
    } else {
        Err(ApiError::not_found("Room is not a DM".to_string()))
    }
}

pub async fn get_dm_partner_route(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let user_id = &auth_user.user_id;

    let join_members = state
        .services
        .member_storage
        .get_room_members(&room_id, "join")
        .await
        .map_err(|e| ApiError::database(format!("Failed to get room join members: {}", e)))?;

    let invited_members = state
        .services
        .member_storage
        .get_room_members(&room_id, "invite")
        .await
        .map_err(|e| ApiError::database(format!("Failed to get room invite members: {}", e)))?;

    let total_members = join_members.len() + invited_members.len();

    if total_members != 2 {
        return Err(ApiError::not_found("Room is not a DM".to_string()));
    }

    let other_member = join_members
        .iter()
        .find(|m| m.user_id.as_str() != user_id)
        .or_else(|| {
            invited_members
                .iter()
                .find(|m| m.user_id.as_str() != user_id)
        })
        .ok_or_else(|| ApiError::not_found("DM partner not found".to_string()))?;

    Ok(Json(json!({
        "room_id": room_id,
        "user_id": other_member.user_id,
        "display_name": other_member.display_name.clone().unwrap_or_default(),
        "avatar_url": other_member.avatar_url.clone().unwrap_or_default()
    })))
}

pub fn create_dm_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/_matrix/client/r0/create_dm", post(create_dm_room))
        .route("/_matrix/client/v3/direct", get(get_dm_rooms))
        .route("/_matrix/client/v3/direct/{room_id}", put(update_dm_room))
        .route("/_matrix/client/v3/rooms/{room_id}/dm", get(check_room_dm))
        .route(
            "/_matrix/client/v3/rooms/{room_id}/dm/partner",
            get(get_dm_partner_route),
        )
        .with_state(state)
}
