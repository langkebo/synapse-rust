use crate::common::ApiError;
use crate::web::extractors::{AuthenticatedUser, OptionalAuthenticatedUser};
use crate::web::routes::{
    extract_token_from_headers, validate_event_id, validate_room_id, AppState,
};
use axum::{
    extract::{Json, Path, Query, State},
    http::HeaderMap,
};
use serde_json::{json, Value};

pub(crate) async fn search_user_directory(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let token = extract_token_from_headers(&headers)?;
    let (_, _, _) = state.services.auth_service.validate_token(&token).await?;

    let search_query = body
        .get("search_term")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let limit = body.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as i64;

    let results = state
        .services
        .user_storage
        .search_users(&search_query, limit)
        .await?;

    let users: Vec<Value> = results
        .into_iter()
        .map(|u| {
            json!({
                "user_id": u.user_id,
                "display_name": u.displayname.unwrap_or_else(|| u.username.clone()),
                "avatar_url": u.avatar_url
            })
        })
        .collect();

    Ok(Json(json!({
        "limited": users.len() >= limit as usize,
        "results": users
    })))
}

pub(crate) async fn list_user_directory(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let token = extract_token_from_headers(&headers)?;
    let (_, _, _) = state.services.auth_service.validate_token(&token).await?;

    let limit = body.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as i64;
    let offset = body.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as i64;

    let total_count = state.services.user_storage.get_user_count().await?;

    let users = state
        .services
        .user_storage
        .get_users_paginated(limit, offset)
        .await?;

    let users_json: Vec<Value> = users
        .into_iter()
        .map(|u| {
            json!({
                "user_id": u.user_id,
                "display_name": u.displayname.unwrap_or_else(|| u.username.clone()),
                "avatar_url": u.avatar_url
            })
        })
        .collect();

    Ok(Json(json!({
        "total": total_count,
        "offset": offset,
        "users": users_json
    })))
}

pub(crate) async fn report_event(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    validate_event_id(&event_id)?;

    let reason = body.get("reason").and_then(|v| v.as_str());
    let score = body.get("score").and_then(|v| v.as_i64()).unwrap_or(-100) as i32;

    let event = state
        .services
        .event_storage
        .get_event(&event_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Event not found".to_string()))?;

    if event.room_id != room_id {
        return Err(ApiError::bad_request(
            "Event does not belong to the specified room".to_string(),
        ));
    }

    let report_id = state
        .services
        .event_storage
        .report_event(
            &event_id,
            &room_id,
            &event.user_id,
            &auth_user.user_id,
            reason,
            score,
        )
        .await?;

    Ok(Json(json!({
        "report_id": report_id
    })))
}

pub(crate) async fn update_report_score(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path((_room_id, event_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_event_id(&event_id)?;

    let score =
        body.get("score")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| ApiError::bad_request("Score is required".to_string()))? as i32;

    state
        .services
        .event_storage
        .update_event_report_score_by_event(&event_id, score)
        .await?;

    Ok(Json(json!({})))
}

pub(crate) async fn report_room(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let members = sqlx::query("SELECT room_id FROM rooms WHERE room_id = $1 LIMIT 1")
        .bind(&room_id)
        .fetch_optional(&*state.services.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    if members.is_none() {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let reason = body
        .get("reason")
        .and_then(|v| v.as_str())
        .unwrap_or("No reason provided");
    let description = body.get("description").and_then(|v| v.as_str());

    ::tracing::info!(
        "Room report submitted: room_id={}, user_id={}, reason={}",
        room_id,
        auth_user.user_id,
        reason
    );

    let report_id = format!("{}_{}", room_id, chrono::Utc::now().timestamp_millis());

    Ok(Json(json!({
        "report_id": report_id,
        "room_id": room_id,
        "reason": reason,
        "description": description,
        "status": "submitted"
    })))
}

pub(crate) async fn get_scanner_info(
    State(_state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path((room_id, event_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    Ok(Json(json!({
        "scanner_enabled": false,
        "room_id": room_id,
        "event_id": event_id,
        "status": "not_configured",
        "message": "Content scanner is not enabled on this server"
    })))
}

pub(crate) async fn get_room_aliases(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let aliases = state
        .services
        .room_service
        .get_room_aliases(&room_id)
        .await?;
    Ok(Json(json!({ "aliases": aliases })))
}

pub(crate) async fn set_room_alias(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, room_alias)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    if room_alias.len() > 255 {
        return Err(ApiError::bad_request(
            "Alias too long (max 255 characters)".to_string(),
        ));
    }

    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let is_member = state
        .services
        .member_storage
        .is_member(&room_id, &auth_user.user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check membership: {}", e)))?;

    if !is_member && !auth_user.is_admin {
        return Err(ApiError::forbidden(
            "You must be a room member to set an alias".to_string(),
        ));
    }

    state
        .services
        .room_service
        .set_room_alias(&room_id, &room_alias, &auth_user.user_id)
        .await?;
    Ok(Json(json!({})))
}

pub(crate) async fn delete_room_alias(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path((room_id, _room_alias)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    state
        .services
        .room_service
        .remove_room_alias(&room_id)
        .await?;
    Ok(Json(json!({})))
}

pub(crate) async fn get_room_by_alias(
    State(state): State<AppState>,
    _auth_user: OptionalAuthenticatedUser,
    Path(room_alias): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let room_id = state
        .services
        .room_service
        .get_room_by_alias(&room_alias)
        .await?;
    match room_id {
        Some(rid) => Ok(Json(json!({ "room_id": rid }))),
        None => Err(ApiError::not_found("Room alias not found".to_string())),
    }
}

#[axum::debug_handler]
pub(crate) async fn set_room_alias_direct(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_alias): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let room_id = body
        .get("room_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing room_id field".to_string()))?;

    if !room_id.starts_with('!') {
        return Err(ApiError::bad_request("Invalid room_id format".to_string()));
    }

    if room_alias.len() > 255 {
        return Err(ApiError::bad_request(
            "Alias too long (max 255 characters)".to_string(),
        ));
    }

    if !state
        .services
        .room_storage
        .room_exists(room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let is_member = state
        .services
        .member_storage
        .is_member(room_id, &auth_user.user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check membership: {}", e)))?;

    if !is_member && !auth_user.is_admin {
        return Err(ApiError::forbidden(
            "You must be a room member to set an alias".to_string(),
        ));
    }

    state
        .services
        .room_service
        .set_room_alias(room_id, &room_alias, &auth_user.user_id)
        .await?;
    Ok(Json(json!({})))
}

#[axum::debug_handler]
pub(crate) async fn delete_room_alias_direct(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(room_alias): Path<String>,
) -> Result<Json<Value>, ApiError> {
    state
        .services
        .room_service
        .remove_room_alias_by_name(&room_alias)
        .await?;
    Ok(Json(json!({})))
}

pub(crate) async fn get_public_rooms(
    State(state): State<AppState>,
    _auth_user: OptionalAuthenticatedUser,
    Query(params): Query<Value>,
) -> Result<Json<Value>, ApiError> {
    let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(10);
    let _since = params.get("since").and_then(|v| v.as_str());

    Ok(Json(
        state
            .services
            .room_service
            .get_public_rooms(limit as i64)
            .await?,
    ))
}

#[axum::debug_handler]
pub(crate) async fn query_public_rooms(
    State(state): State<AppState>,
    _auth_user: OptionalAuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let limit = body.get("limit").and_then(|v| v.as_u64()).unwrap_or(10);
    let _since = body.get("since").and_then(|v| v.as_str());
    let _filter = body.get("filter");

    Ok(Json(
        state
            .services
            .room_service
            .get_public_rooms(limit as i64)
            .await?,
    ))
}
