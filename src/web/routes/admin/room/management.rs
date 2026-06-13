use super::types::*;
use crate::common::ApiError;
use crate::web::routes::admin::audit::{record_audit_event, resolve_request_id};
use crate::web::routes::{AdminUser, AppState};
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use serde_json::{json, Value};

#[axum::debug_handler]
pub async fn cleanup_abnormal_rooms(
    _admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let min_age_ms = body.get("min_age_ms").and_then(|v| v.as_i64());

    let results = state
        .services.rooms.room_storage
        .cleanup_abnormal_data(min_age_ms)
        .await
        .map_err(|e| ApiError::internal_with_log("Cleanup failed", &e))?;

    Ok(Json(results))
}

#[axum::debug_handler]
pub async fn block_room(
    admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<BlockRoomRequest>,
) -> Result<Json<Value>, ApiError> {
    if !state.services.rooms.room_storage.room_exists(&room_id).await.map_err(|e| {
        tracing::error!("Database error: {e}");
        ApiError::database("A database error occurred".to_string())
    })? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let now = chrono::Utc::now().timestamp_millis();

    state.services.rooms.room_storage.block_room(&room_id, now, &admin.user_id, body.reason.as_deref()).await.map_err(
        |e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        },
    )?;

    record_audit_event(
        &state,
        &admin.user_id,
        "admin.room.block",
        "room",
        &room_id,
        resolve_request_id(&headers),
        json!({
            "block": body.block,
            "reason": body.reason
        }),
    )
    .await?;

    Ok(Json(json!({ "block": body.block })))
}

#[axum::debug_handler]
pub async fn get_room_block_status(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if !state.services.rooms.room_storage.room_exists(&room_id).await.map_err(|e| {
        tracing::error!("Database error: {e}");
        ApiError::database("A database error occurred".to_string())
    })? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let blocked_at = state.services.rooms.room_storage.get_room_block_status(&room_id).await.map_err(|e| {
        tracing::error!("Database error: {e}");
        ApiError::database("A database error occurred".to_string())
    })?;

    match blocked_at {
        Some(blocked_at) => Ok(Json(json!({
            "block": true,
            "blocked_at": blocked_at
        }))),
        None => Ok(Json(json!({ "block": false }))),
    }
}

#[axum::debug_handler]
pub async fn unblock_room(
    admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    if !state.services.rooms.room_storage.room_exists(&room_id).await.map_err(|e| {
        tracing::error!("Database error: {e}");
        ApiError::database("A database error occurred".to_string())
    })? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    state.services.rooms.room_storage.unblock_room(&room_id).await.map_err(|e| {
        tracing::error!("Database error: {e}");
        ApiError::database("A database error occurred".to_string())
    })?;

    record_audit_event(
        &state,
        &admin.user_id,
        "admin.room.unblock",
        "room",
        &room_id,
        resolve_request_id(&headers),
        json!({ "block": false }),
    )
    .await?;

    Ok(Json(json!({ "block": false })))
}

#[axum::debug_handler]
pub async fn make_room_admin(
    admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    Json(body): Json<MakeRoomAdminRequest>,
) -> Result<Json<Value>, ApiError> {
    crate::web::routes::admin::ensure_super_admin_for_privilege_change(&admin)?;
    if !state.services.rooms.room_storage.room_exists(&room_id).await.map_err(|e| {
        tracing::error!("Database error: {e}");
        ApiError::database("A database error occurred".to_string())
    })? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    if state
        .services
        .account
        .user_storage
        .get_user_by_id(&body.user_id)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?
        .is_none()
    {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    let now = chrono::Utc::now().timestamp_millis();
    let event_id = crate::common::crypto::generate_event_id(&state.services.core.config.server.name);
    let user_id = body.user_id.clone();
    let admin_user = "@admin:".to_string() + &state.services.core.config.server.name;
    let power_levels = json!({
        "users": {
            user_id.clone(): 100
        },
        "users_default": 0,
        "events_default": 0,
        "state_default": 50,
        "ban": 50,
        "kick": 50,
        "redact": 50,
        "invite": 0
    });

    state
        .services.rooms.event_storage
        .upsert_power_levels_event(&event_id, &room_id, &user_id, power_levels, now, &admin_user)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

    Ok(Json(json!({})))
}

#[axum::debug_handler]
pub async fn purge_history(
    admin: AdminUser,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);
    let room_id = body
        .get("room_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing 'room_id' field".to_string()))?;
    let timestamp = body
        .get("purge_up_to_ts")
        .and_then(|v| v.as_i64())
        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis() - (30 * 24 * 60 * 60 * 1000));

    if !state.services.rooms.room_storage.room_exists(room_id).await.map_err(|e| {
        tracing::error!("Database error: {e}");
        ApiError::database("A database error occurred".to_string())
    })? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    // P2 #33: 审计日志 - purge_history 操作
    tracing::warn!(
        request_id = %request_id,
        action = "admin.purge_history",
        admin_user_id = %admin.user_id,
        target_room_id = %room_id,
        purge_up_to_ts = timestamp,
        timestamp_ms = chrono::Utc::now().timestamp_millis(),
        "Admin purge history operation"
    );

    let deleted_count = state
        .services.rooms.event_storage
        .delete_events_before(room_id, timestamp)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to purge history", &e))?;

    Ok(Json(json!({
        "success": true,
        "deleted_events": deleted_count
    })))
}

#[axum::debug_handler]
pub async fn purge_history_by_room(
    admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let merged_body = match body {
        Value::Object(mut map) => {
            map.insert("room_id".to_string(), Value::String(room_id));
            Value::Object(map)
        }
        _ => json!({ "room_id": room_id }),
    };

    purge_history(admin, State(state), headers, Json(merged_body)).await
}

#[axum::debug_handler]
pub async fn purge_room(
    admin: AdminUser,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);
    let room_id = body
        .get("room_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing 'room_id' field".to_string()))?;

    if !state.services.rooms.room_storage.room_exists(room_id).await.map_err(|e| {
        tracing::error!("Database error: {e}");
        ApiError::database("A database error occurred".to_string())
    })? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    // P2 #33: 审计日志 - delete_room 操作
    tracing::warn!(
        request_id = %request_id,
        action = "admin.delete_room",
        admin_user_id = %admin.user_id,
        target_room_id = %room_id,
        timestamp_ms = chrono::Utc::now().timestamp_millis(),
        "Admin delete room operation"
    );

    state
        .services.rooms.room_storage
        .delete_room(room_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to purge room", &e))?;

    Ok(Json(json!({
        "purge_id": uuid::Uuid::new_v4().to_string(),
        "success": true
    })))
}

/// Join a user to a room (force join)
#[axum::debug_handler]
pub async fn join_room_member(
    _admin: AdminUser,
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((room_id, user_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);
    Ok(Json(join_room_member_internal(&state, &room_id, &user_id, &request_id).await?))
}

/// Remove a user from a room
#[axum::debug_handler]
pub async fn remove_room_member(
    _admin: AdminUser,
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((room_id, user_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);
    Ok(Json(remove_room_member_internal(&state, &room_id, &user_id, &request_id).await?))
}

#[axum::debug_handler]
pub async fn ban_user(
    admin: AdminUser,
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((room_id, user_id)): Path<(String, String)>,
    Json(body): Json<BanRequest>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);
    Ok(Json(
        ban_user_internal(&state, &room_id, &user_id, &admin.user_id, body.reason.as_deref(), &request_id).await?,
    ))
}

#[axum::debug_handler]
pub async fn ban_user_by_body(
    admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<RoomUserActionRequest>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);
    Ok(Json(
        ban_user_internal(&state, &room_id, &body.user_id, &admin.user_id, body.reason.as_deref(), &request_id)
            .await?,
    ))
}

/// Unban a user from a room
#[axum::debug_handler]
pub async fn unban_user(
    _admin: AdminUser,
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((room_id, user_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);
    Ok(Json(unban_user_internal(&state, &room_id, &user_id, &request_id).await?))
}

/// Kick a user from a room
#[axum::debug_handler]
pub async fn kick_user(
    admin: AdminUser,
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((room_id, user_id)): Path<(String, String)>,
    Json(body): Json<BanRequest>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);
    Ok(Json(
        kick_user_internal(&state, &room_id, &user_id, &admin.user_id, body.reason.as_deref(), &request_id).await?,
    ))
}

#[axum::debug_handler]
pub async fn kick_user_by_body(
    admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<RoomUserActionRequest>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);
    Ok(Json(
        kick_user_internal(&state, &room_id, &body.user_id, &admin.user_id, body.reason.as_deref(), &request_id)
            .await?,
    ))
}

// Internal helpers

async fn join_room_member_internal(
    state: &AppState,
    room_id: &str,
    user_id: &str,
    request_id: &str,
) -> Result<Value, ApiError> {
    if !state
        .services.rooms.room_storage
        .room_exists(room_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to check room", &e))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    if !state
        .services
        .account
        .user_storage
        .user_exists(user_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to check user", &e))?
    {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    let existing_membership = state
        .services.rooms.member_storage
        .get_room_member(room_id, user_id)
        .await
        .map_err(|e| {
            tracing::error!(
                request_id = %request_id,
                room_id = %room_id,
                user_id = %user_id,
                error = %e,
                "Database error while getting room member for join"
            );
            ApiError::database("A database error occurred".to_string())
        })?
        .map(|member| member.membership);

    if existing_membership.as_deref() != Some("join") {
        state.services.rooms.room_service.join_room(room_id, user_id).await?;
    }

    Ok(json!({
        "user_id": user_id,
        "room_id": room_id,
        "membership": "join"
    }))
}

async fn remove_room_member_internal(
    state: &AppState,
    room_id: &str,
    user_id: &str,
    request_id: &str,
) -> Result<Value, ApiError> {
    if !state
        .services.rooms.room_storage
        .room_exists(room_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to check room", &e))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    if !state
        .services
        .account
        .user_storage
        .user_exists(user_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to check user", &e))?
    {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    let existing_membership = state
        .services.rooms.member_storage
        .get_room_member(room_id, user_id)
        .await
        .map_err(|e| {
            tracing::error!(
                request_id = %request_id,
                room_id = %room_id,
                user_id = %user_id,
                error = %e,
                "Database error while getting room member for removal"
            );
            ApiError::database("A database error occurred".to_string())
        })?
        .map(|member| member.membership);

    if existing_membership.as_deref() == Some("join") {
        state.services.rooms.room_service.leave_room(room_id, user_id).await?;
    }

    Ok(json!({
        "user_id": user_id,
        "room_id": room_id,
        "removed": true
    }))
}

async fn ban_user_internal(
    state: &AppState,
    room_id: &str,
    user_id: &str,
    actor_user_id: &str,
    reason: Option<&str>,
    request_id: &str,
) -> Result<Value, ApiError> {
    if !state
        .services.rooms.room_storage
        .room_exists(room_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to check room", &e))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    if !state
        .services
        .account
        .user_storage
        .user_exists(user_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to check user", &e))?
    {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    let existing_membership = state
        .services.rooms.member_storage
        .get_room_member(room_id, user_id)
        .await
        .map_err(|e| {
            tracing::error!(
                request_id = %request_id,
                room_id = %room_id,
                user_id = %user_id,
                actor_user_id = %actor_user_id,
                error = %e,
                "Database error while getting room member for ban"
            );
            ApiError::database("A database error occurred".to_string())
        })?
        .map(|member| member.membership);

    let actor_is_admin = state
        .services
        .account
        .user_storage
        .get_user_by_id(actor_user_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to check actor", &e))?
        .is_some_and(|user| user.is_admin);

    if actor_is_admin {
        state
            .services.rooms.member_storage
            .ban_member(room_id, user_id, actor_user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to ban user", &e))?;
    } else {
        state.services.rooms.room_service.ban_user(room_id, user_id, actor_user_id, reason).await?;
    }

    if existing_membership.as_deref() == Some("join") {
        state
            .services.rooms.room_storage
            .decrement_member_count(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to update member count", &e))?;
    }

    if let Some(reason) = reason {
        state.services.rooms.member_storage.set_ban_reason(room_id, user_id, reason).await.map_err(|e| {
            tracing::error!(
                request_id = %request_id,
                room_id = %room_id,
                user_id = %user_id,
                actor_user_id = %actor_user_id,
                error = %e,
                "Database error while setting ban reason"
            );
            ApiError::database("A database error occurred".to_string())
        })?;
    }

    #[cfg(feature = "friends")]
    if let Err(error) = state
        .services
        .friend_room_service
        .sync_dm_room_membership_change(room_id, user_id, "banned", Some(actor_user_id), reason)
        .await
    {
        ::tracing::warn!(
            request_id = %request_id,
            room_id = %room_id,
            user_id = %user_id,
            actor_user_id = %actor_user_id,
            error = %error,
            "Failed to sync friend DM ban state"
        );
    }

    Ok(json!({
        "user_id": user_id,
        "room_id": room_id,
        "membership": "ban"
    }))
}

async fn unban_user_internal(
    state: &AppState,
    room_id: &str,
    user_id: &str,
    request_id: &str,
) -> Result<Value, ApiError> {
    if !state
        .services.rooms.room_storage
        .room_exists(room_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to check room", &e))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    if !state
        .services
        .account
        .user_storage
        .user_exists(user_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to check user", &e))?
    {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    state.services.rooms.member_storage.unban_member(room_id, user_id).await.map_err(|e| {
        tracing::error!(
            request_id = %request_id,
            room_id = %room_id,
            user_id = %user_id,
            error = %e,
            "Database error while unbanning user"
        );
        ApiError::database("A database error occurred".to_string())
    })?;

    Ok(json!({
        "user_id": user_id,
        "room_id": room_id,
        "unbanned": true
    }))
}

async fn kick_user_internal(
    state: &AppState,
    room_id: &str,
    user_id: &str,
    actor_user_id: &str,
    reason: Option<&str>,
    request_id: &str,
) -> Result<Value, ApiError> {
    let existing_membership = state
        .services.rooms.member_storage
        .get_room_member(room_id, user_id)
        .await
        .map_err(|e| {
            tracing::error!(
                request_id = %request_id,
                room_id = %room_id,
                user_id = %user_id,
                actor_user_id = %actor_user_id,
                error = %e,
                "Database error while getting room member for kick"
            );
            ApiError::database("A database error occurred".to_string())
        })?
        .map(|member| member.membership);

    if !state
        .services.rooms.room_storage
        .room_exists(room_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to check room", &e))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    if !state
        .services
        .account
        .user_storage
        .user_exists(user_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to check user", &e))?
    {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    match existing_membership.as_deref() {
        Some("join") => {
            state.services.rooms.room_service.leave_room(room_id, user_id).await?;
        }
        Some(_) => {
            let now = chrono::Utc::now().timestamp_millis();
            state.services.rooms.member_storage.force_leave_membership(room_id, user_id, now).await.map_err(|e| {
                tracing::error!(
                    request_id = %request_id,
                    room_id = %room_id,
                    user_id = %user_id,
                    actor_user_id = %actor_user_id,
                    error = %e,
                    "Database error while forcing leave membership for kick"
                );
                ApiError::database("A database error occurred".to_string())
            })?;
        }
        None => {}
    }

    #[cfg(feature = "friends")]
    if let Err(error) = state
        .services
        .friend_room_service
        .sync_dm_room_membership_change(room_id, user_id, "kicked", Some(actor_user_id), reason)
        .await
    {
        ::tracing::warn!(
            request_id = %request_id,
            room_id = %room_id,
            user_id = %user_id,
            actor_user_id = %actor_user_id,
            error = %error,
            "Failed to sync friend DM kick state"
        );
    }

    Ok(json!({
        "user_id": user_id,
        "room_id": room_id,
        "kicked": true,
        "reason": reason
    }))
}
