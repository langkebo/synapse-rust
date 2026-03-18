use crate::services::directory_service::DirectoryService;
use crate::web::routes::{ApiError, AppState, AuthenticatedUser};
use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct PublicRoomsQuery {
    #[serde(default = "default_limit")]
    pub limit: i32,
    #[serde(default)]
    pub since: Option<String>,
}

fn default_limit() -> i32 {
    20
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
    _auth_user: AuthenticatedUser,
    Path(room_alias): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let room_id = body
        .get("room_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing room_id".to_string()))?;

    state
        .services
        .directory_service
        .set_room_alias(room_id, &room_alias)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to set alias: {}", e)))?;

    Ok(Json(json!({})))
}

pub async fn remove_room_alias(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(room_alias): Path<String>,
) -> Result<Json<Value>, ApiError> {
    state
        .services
        .directory_service
        .remove_room_alias(&room_alias)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to remove alias: {}", e)))?;

    Ok(Json(json!({})))
}

pub async fn get_alias_servers(
    State(state): State<AppState>,
    Path(room_alias): Path<String>,
) -> Result<Json<Value>, ApiError> {
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

pub async fn set_canonical_alias(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let alias = body.get("alias").and_then(|v| v.as_str());

    if let Some(alias_str) = alias {
        state
            .services
            .room_storage
            .update_canonical_alias(&room_id, alias_str)
            .await
            .map_err(|e| ApiError::internal(format!("Failed: {}", e)))?;
    }

    Ok(Json(json!({})))
}

pub async fn get_canonical_alias(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let alias = state
        .services
        .room_storage
        .get_room_alias(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed: {}", e)))?;

    Ok(Json(json!({
        "alias": alias,
    })))
}
