use crate::services::directory_service::DirectoryService;
use crate::web::routes::{
    ensure_room_member, validate_room_alias, ApiError, AppState, AuthenticatedUser,
};
use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use validator::Validate;

#[derive(Debug, Deserialize, Validate)]
pub struct PublicRoomsQuery {
    #[validate(range(min = 0, max = 100))]
    #[serde(default = "default_limit")]
    pub limit: i32,
    #[validate(length(max = 256))]
    #[serde(default)]
    pub since: Option<String>,
}

fn default_limit() -> i32 {
    20
}

async fn ensure_room_alias_write_allowed(
    state: &AppState,
    auth_user: &AuthenticatedUser,
    room_id: &str,
) -> Result<(), ApiError> {
    ensure_room_member(
        state,
        auth_user,
        room_id,
        "You must be a member of this room to manage aliases",
    )
    .await?;

    let is_creator = state
        .services
        .room_service
        .is_room_creator(room_id, &auth_user.user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check room creator: {}", e)))?;

    if !is_creator {
        return Err(ApiError::forbidden(
            "Only room admins can manage aliases".to_string(),
        ));
    }

    Ok(())
}

#[derive(Debug, Deserialize, Validate)]
pub struct SetRoomAliasBody {
    #[validate(length(min = 1, max = 255))]
    pub room_id: String,
}

#[derive(Debug, Serialize)]
pub struct PublicRoom {
    pub room_id: String,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub avatar_url: Option<String>,
    pub member_count: i64,
    pub world_readable: bool,
    pub guest_can_join: bool,
}

pub async fn get_directory_room(
    State(state): State<AppState>,
    Path(room_alias): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_alias(&room_alias)?;

    let room_id = state
        .services
        .directory_service
        .get_room_id_by_alias(&room_alias)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to lookup room: {}", e)))?;

    match room_id {
        Some(rid) => Ok(Json(json!({
            "room_id": rid,
            "servers": [state.services.server_name.clone()],
        }))),
        None => Err(ApiError::not_found("Room not found".to_string())),
    }
}

pub async fn set_room_alias_handler(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_alias): Path<String>,
    Json(body): Json<SetRoomAliasBody>,
) -> Result<Json<Value>, ApiError> {
    validate_room_alias(&room_alias)?;

    let room_id = &body.room_id;

    ensure_room_alias_write_allowed(&state, &auth_user, room_id).await?;

    state
        .services
        .directory_service
        .set_room_alias(room_id, &room_alias)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to set alias: {}", e)))?;

    Ok(Json(json!({
        "room_id": room_id,
        "alias": room_alias,
        "created_ts": chrono::Utc::now().timestamp_millis()
    })))
}

pub async fn remove_room_alias(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_alias): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_alias(&room_alias)?;

    let existing = state
        .services
        .directory_service
        .get_room_id_by_alias(&room_alias)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get alias: {}", e)))?;

    if let Some(room_id) = &existing {
        ensure_room_alias_write_allowed(&state, &auth_user, room_id).await?;
    }

    state
        .services
        .directory_service
        .remove_room_alias(&room_alias)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to remove alias: {}", e)))?;

    Ok(Json(json!({
        "removed": true,
        "alias": room_alias
    })))
}

pub async fn get_alias_servers(
    State(state): State<AppState>,
    Path(room_alias): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_alias(&room_alias)?;

    let room_id = state
        .services
        .directory_service
        .get_room_id_by_alias(&room_alias)
        .await
        .map_err(|e| ApiError::internal(format!("Failed: {}", e)))?;

    match room_id {
        Some(_) => Ok(Json(json!({
            "servers": [state.services.server_name.clone()]
        }))),
        None => Err(ApiError::not_found("Room not found".to_string())),
    }
}

pub async fn get_public_rooms_handler(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let limit = params
        .get("limit")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(20);

    let rooms = state
        .services
        .room_storage
        .get_public_rooms(limit)
        .await
        .map_err(|e| ApiError::internal(format!("Failed: {}", e)))?;

    let chunk: Vec<Value> = rooms
        .into_iter()
        .map(|r| {
            json!({
                "room_id": r.room_id,
                "name": r.name,
                "topic": r.topic,
                "avatar_url": r.avatar_url,
                "num_joined_members": r.member_count,
                "world_readable": r.is_public,
                "guest_can_join": true,
            })
        })
        .collect();

    Ok(Json(json!({
        "chunk": chunk,
        "total_room_count_estimate": chunk.len(),
    })))
}

pub async fn search_public_rooms(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let _filter = body.get("filter").and_then(|v| v.as_str());
    let limit = body.get("limit").and_then(|v| v.as_i64()).unwrap_or(20);

    let rooms = state
        .services
        .room_storage
        .get_public_rooms(limit)
        .await
        .map_err(|e| ApiError::internal(format!("Failed: {}", e)))?;

    let chunk: Vec<Value> = rooms
        .into_iter()
        .map(|r| {
            json!({
                "room_id": r.room_id,
                "name": r.name,
                "topic": r.topic,
                "avatar_url": r.avatar_url,
                "num_joined_members": r.member_count,
                "world_readable": r.is_public,
                "guest_can_join": true,
            })
        })
        .collect();

    Ok(Json(json!({
        "chunk": chunk,
        "total_room_count_estimate": chunk.len(),
    })))
}
