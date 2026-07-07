use crate::common::ApiError;
use crate::web::routes::context::AdminContext;
use crate::web::routes::AdminUser;
use axum::{
    extract::{Path, State},
    Json,
};
use serde_json::{json, Value};

async fn resolve_space_id(ctx: &AdminContext, identifier: &str) -> Result<String, ApiError> {
    ctx.space_service
        .resolve_space_id(identifier)
        .await?
        .ok_or_else(|| ApiError::not_found("Space not found".to_string()))
}

#[axum::debug_handler]
pub async fn get_spaces(_admin: AdminUser, State(ctx): State<AdminContext>) -> Result<Json<Value>, ApiError> {
    let spaces = ctx.space_service.get_all_spaces_for_admin().await?;

    let space_list: Vec<Value> = spaces
        .iter()
        .map(|s| {
            json!({
                "space_id": s.space_id,
                "room_id": s.room_id,
                "name": s.name,
                "topic": s.topic,
                "creator": s.creator,
                "created_ts": s.created_ts
            })
        })
        .collect();

    Ok(Json(json!({ "spaces": space_list, "total": space_list.len() })))
}

#[axum::debug_handler]
pub async fn get_space(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(space_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let space = ctx.space_service.get_space_by_identifier(&space_id).await?;

    match space {
        Some(s) => Ok(Json(json!({
            "space_id": s.space_id,
            "room_id": s.room_id,
            "name": s.name,
            "topic": s.topic,
            "creator": s.creator,
            "created_ts": s.created_ts
        }))),
        None => Err(ApiError::not_found("Space not found".to_string())),
    }
}

#[axum::debug_handler]
pub async fn delete_space(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(space_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let resolved_space_id = resolve_space_id(&ctx, &space_id).await?;
    let rows_affected = ctx.space_service.delete_space_returning_count(&resolved_space_id).await?;

    if rows_affected == 0 {
        return Err(ApiError::not_found("Space not found".to_string()));
    }

    Ok(Json(json!({ "deleted": true })))
}

#[axum::debug_handler]
pub async fn get_space_users(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(space_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let resolved_space_id = resolve_space_id(&ctx, &space_id).await?;

    let user_list = ctx.space_service.get_space_user_ids(&resolved_space_id).await?;

    Ok(Json(json!({ "users": user_list, "total": user_list.len() })))
}

#[axum::debug_handler]
pub async fn get_space_rooms(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(space_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let resolved_space_id = resolve_space_id(&ctx, &space_id).await?;

    let room_list = ctx.space_service.get_space_room_ids(&resolved_space_id).await?;

    Ok(Json(json!({ "rooms": room_list, "total": room_list.len() })))
}

#[axum::debug_handler]
pub async fn get_space_stats(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(space_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let resolved_space_id = resolve_space_id(&ctx, &space_id).await?;

    let (member_count, child_count) = ctx.space_service.get_space_member_and_child_count(&resolved_space_id).await?;

    Ok(Json(json!({
        "space_id": resolved_space_id,
        "member_count": member_count,
        "child_room_count": child_count
    })))
}

/// Get overall room statistics
#[axum::debug_handler]
pub async fn get_room_stats(_admin: AdminUser, State(ctx): State<AdminContext>) -> Result<Json<Value>, ApiError> {
    let stats = ctx.room_service.state.get_room_stats_overview().await?;

    Ok(Json(stats))
}

/// Get statistics for a single room
#[axum::debug_handler]
pub async fn get_single_room_stats(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let stats = ctx.room_service.state.get_single_room_stats(&room_id).await?;

    match stats {
        Some(stats) => Ok(Json(stats)),
        None => Err(ApiError::not_found("Room not found".to_string())),
    }
}

/// Get room listings (public/directory status)
#[axum::debug_handler]
pub async fn get_room_listings(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let listing = ctx.room_service.state.get_room_listings_status(&room_id).await?;

    let Some((is_public, in_directory)) = listing else {
        return Err(ApiError::not_found("Room not found".to_string()));
    };

    Ok(Json(json!({
        "room_id": room_id,
        "public": is_public,
        "in_directory": in_directory
    })))
}

/// Set room as public
#[axum::debug_handler]
pub async fn set_room_public(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let found = ctx.room_service.state.set_room_public_with_directory(&room_id).await?;

    if !found {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    Ok(Json(json!({
        "room_id": room_id,
        "public": true
    })))
}

/// Set room as private
#[axum::debug_handler]
pub async fn set_room_private(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let found = ctx.room_service.state.set_room_private_with_directory(&room_id).await?;

    if !found {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    Ok(Json(json!({
        "room_id": room_id,
        "public": false
    })))
}
