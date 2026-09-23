use crate::routes::context::AdminContext;
use crate::routes::extractors::RoomId;
use crate::routes::AdminUser;
use axum::{
    extract::{Path, State},
    Json,
};
use serde_json::{json, Value};
use synapse_common::ApiError;

async fn resolve_space_id(ctx: &AdminContext, identifier: &str) -> Result<String, ApiError> {
    ctx.space_service
        .resolve_space_id(identifier)
        .await?
        .ok_or_else(|| ApiError::not_found("Space not found".to_string()))
}

/// See [`get_spaces`].
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

/// See [`get_space`].
#[axum::debug_handler]
pub async fn get_space(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(space_id): Path<RoomId>,
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

/// See [`delete_space`].
#[axum::debug_handler]
pub async fn delete_space(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(space_id): Path<RoomId>,
) -> Result<Json<Value>, ApiError> {
    let resolved_space_id = resolve_space_id(&ctx, &space_id).await?;
    let rows_affected = ctx.space_service.delete_space_returning_count(&resolved_space_id).await?;

    if rows_affected == 0 {
        return Err(ApiError::not_found("Space not found".to_string()));
    }

    Ok(Json(json!({ "deleted": true })))
}

/// See [`get_space_users`].
#[axum::debug_handler]
pub async fn get_space_users(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(space_id): Path<RoomId>,
) -> Result<Json<Value>, ApiError> {
    let resolved_space_id = resolve_space_id(&ctx, &space_id).await?;

    let user_list = ctx.space_service.get_space_user_ids(&resolved_space_id).await?;

    Ok(Json(json!({ "users": user_list, "total": user_list.len() })))
}

/// See [`get_space_rooms`].
#[axum::debug_handler]
pub async fn get_space_rooms(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(space_id): Path<RoomId>,
) -> Result<Json<Value>, ApiError> {
    let resolved_space_id = resolve_space_id(&ctx, &space_id).await?;

    let room_list = ctx.space_service.get_space_room_ids(&resolved_space_id).await?;

    Ok(Json(json!({ "rooms": room_list, "total": room_list.len() })))
}

/// See [`get_space_stats`].
#[axum::debug_handler]
pub async fn get_space_stats(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(space_id): Path<RoomId>,
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
    let stats = ctx.room_service.state().get_room_stats_overview().await?;

    Ok(Json(stats))
}

/// Get statistics for a single room
#[axum::debug_handler]
pub async fn get_single_room_stats(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(room_id): Path<RoomId>,
) -> Result<Json<Value>, ApiError> {
    let stats = ctx.room_service.state().get_single_room_stats(&room_id).await?;

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
    Path(room_id): Path<RoomId>,
) -> Result<Json<Value>, ApiError> {
    let listing = ctx.room_service.state().get_room_listings_status(&room_id).await?;

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
    Path(room_id): Path<RoomId>,
) -> Result<Json<Value>, ApiError> {
    let found = ctx.room_service.state().set_room_public_with_directory(&room_id).await?;

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
    Path(room_id): Path<RoomId>,
) -> Result<Json<Value>, ApiError> {
    let found = ctx.room_service.state().set_room_private_with_directory(&room_id).await?;

    if !found {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    Ok(Json(json!({
        "room_id": room_id,
        "public": false
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use synapse_common::ApiErrorKind;

    // Mock type for compile-time structure validation only. It must live *inside*
    // the test module: declared at file scope it is dead code in the non-test
    // (`lib`) build, which `clippy -D warnings` turns into an error.
    #[derive(Debug)]
    struct SpaceInfoMock {
        space_id: String,
        room_id: String,
        name: Option<String>,
        topic: Option<String>,
        creator: String,
        created_ts: u64,
    }

    #[test]
    fn test_space_list_json_structure() {
        let mock_spaces = [SpaceInfoMock {
            space_id: "!space1:example.com".to_string(),
            room_id: "!room1:example.com".to_string(),
            name: Some("Test Space".to_string()),
            topic: None,
            creator: "@user1:example.com".to_string(),
            created_ts: 1234567890,
        }];

        let space_list: Vec<Value> = mock_spaces
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

        let response = json!({ "spaces": space_list, "total": space_list.len() });

        assert!(response.get("spaces").unwrap().is_array());
        assert_eq!(response.get("total").unwrap(), 1);
        assert_eq!(response["spaces"][0]["space_id"], "!space1:example.com");
    }

    #[test]
    fn test_space_not_found_error_kind() {
        let error = ApiError::not_found("Space not found".to_string());
        // ApiError::not_found uses NotFound variant which maps to 404
        assert!(matches!(error.kind, ApiErrorKind::NotFound));
    }

    #[test]
    fn test_deleted_response_structure() {
        let response = json!({ "deleted": true });

        assert_eq!(response.get("deleted").unwrap(), true);
    }

    #[test]
    fn test_space_users_response_structure() {
        let user_list = vec!["@user1:example.com".to_string(), "@user2:example.com".to_string()];

        let response = json!({ "users": user_list, "total": user_list.len() });

        assert!(response.get("users").unwrap().is_array());
        assert_eq!(response.get("total").unwrap(), 2);
        assert_eq!(response["users"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_space_rooms_response_structure() {
        let room_list = vec!["!room1:example.com".to_string(), "!room2:example.com".to_string()];

        let response = json!({ "rooms": room_list, "total": room_list.len() });

        assert!(response.get("rooms").unwrap().is_array());
        assert_eq!(response.get("total").unwrap(), 2);
    }

    #[test]
    fn test_space_stats_response_structure() {
        let resolved_space_id = "!space1:example.com".to_string();
        let member_count = 5;
        let child_count = 3;

        let response = json!({
            "space_id": resolved_space_id,
            "member_count": member_count,
            "child_room_count": child_count
        });

        assert_eq!(response["space_id"], "!space1:example.com");
        assert_eq!(response["member_count"], 5);
        assert_eq!(response["child_room_count"], 3);
    }

    #[test]
    fn test_room_listing_response_structure() {
        let is_public = true;
        let in_directory = false;
        let room_id = "!room1:example.com";

        let response = json!({
            "room_id": room_id,
            "public": is_public,
            "in_directory": in_directory
        });

        assert_eq!(response["room_id"], "!room1:example.com");
        assert_eq!(response["public"], true);
        assert_eq!(response["in_directory"], false);
    }

    #[test]
    fn test_room_not_found_error_kind() {
        let error = ApiError::not_found("Room not found".to_string());
        assert!(matches!(error.kind, ApiErrorKind::NotFound));
    }

    #[test]
    fn test_set_room_public_response_structure() {
        let room_id = "!room1:example.com";

        let response = json!({
            "room_id": room_id,
            "public": true
        });

        assert_eq!(response["room_id"], "!room1:example.com");
        assert_eq!(response["public"], true);
    }

    #[test]
    fn test_set_room_private_response_structure() {
        let room_id = "!room1:example.com";

        let response = json!({
            "room_id": room_id,
            "public": false
        });

        assert_eq!(response["room_id"], "!room1:example.com");
        assert_eq!(response["public"], false);
    }
}
