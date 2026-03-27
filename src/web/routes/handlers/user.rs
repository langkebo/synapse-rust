//! 用户相关处理器

use crate::common::{ApiError, AppState};
use axum::{
    extract::{Path, State},
    Json,
};
use serde_json::json;

/// 获取用户资料
pub async fn get_profile(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(user_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    validate_user_id(&user_id)?;

    // 简化实现
    Ok(Json(json!({
        "user_id": user_id,
        "displayname": null,
        "avatar_url": null
    })))
}

/// 设置用户资料
pub async fn set_profile(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    validate_user_id(&user_id)?;
    Ok(Json(json!({ "ok": true })))
}

/// 获取用户头像
pub async fn get_avatar_url(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    validate_user_id(&user_id)?;
    Ok(Json(json!({ "avatar_url": serde_json::Value::Null })))
}

/// 设置用户头像
pub async fn set_avatar_url(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    validate_user_id(&user_id)?;
    Ok(Json(json!({ "ok": true })))
}

/// 获取用户显示名
pub async fn get_displayname(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    validate_user_id(&user_id)?;
    Ok(Json(json!({ "displayname": serde_json::Value::Null })))
}

/// 设置用户显示名
pub async fn set_displayname(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    validate_user_id(&user_id)?;
    Ok(Json(json!({ "ok": true })))
}

/// 验证 user_id 格式
fn validate_user_id(user_id: &str) -> Result<(), ApiError> {
    if !user_id.starts_with('@') {
        return Err(ApiError::bad_request("Invalid user ID format".to_string()));
    }
    Ok(())
}