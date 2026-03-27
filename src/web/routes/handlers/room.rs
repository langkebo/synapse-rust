//! 房间相关处理器

use crate::common::{ApiError, AppState};
use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::json;

/// 房间消息参数
#[derive(Debug, Deserialize)]
pub struct RoomMessagesParams {
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub to: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub dir: Option<String>,
}

/// 获取房间消息
pub async fn get_messages(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    Query(params): Query<RoomMessagesParams>,
) -> Result<Json<serde_json::Value>, ApiError> {
    validate_room_id(&room_id)?;

    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check room: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    // 简化返回
    Ok(Json(json!({
        "chunk": [],
        "start": "s0",
        "end": "s0"
    })))
}

/// 发送消息
pub async fn send_message(
    State(state): State<AppState>,
    Path((room_id, event_type, txn_id)): Path<(String, String, String)>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    validate_room_id(&room_id)?;

    let content = body
        .get("body")
        .ok_or_else(|| ApiError::bad_request("Message body required".to_string()))?;

    // 长度检查
    if let Some(s) = content.as_str() {
        if s.len() > 65536 {
            return Err(ApiError::bad_request("Message body too long (max 64KB)".to_string()));
        }
    }

    Ok(json!({
        "event_id": "$placeholder"
    }))
}

/// 加入房间
pub async fn join_room(
    State(state): State<AppState>,
    Path(room_id_or_alias): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    validate_room_id_or_alias(&room_id_or_alias)?;
    Ok(json!({ "room_id": room_id_or_alias }))
}

/// 离开房间
pub async fn leave_room(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    validate_room_id(&room_id)?;
    Ok(json!({ "ok": true }))
}

/// 获取房间信息
pub async fn get_room_info(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    validate_room_id(&room_id)?;

    // 简化返回
    Ok(json!({
        "room_id": room_id,
        "name": null,
        "topic": null,
        "avatar_url": null
    }))
}

/// 获取已加入的房间列表
pub async fn get_joined_rooms(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(json!({ "joined_rooms": [] }))
}

/// 获取我的房间列表
pub async fn get_my_rooms(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(json!({ "rooms": [] }))
}

/// 验证 room_id 格式
fn validate_room_id(room_id: &str) -> Result<(), ApiError> {
    if !room_id.starts_with('!') {
        return Err(ApiError::bad_request("Invalid room ID format".to_string()));
    }
    Ok(())
}

/// 验证 room_id 或 room_alias 格式
fn validate_room_id_or_alias(id: &str) -> Result<(), ApiError> {
    if !id.starts_with('!') && !id.starts_with('#') {
        return Err(ApiError::bad_request("Invalid room ID or alias format".to_string()));
    }
    Ok(())
}