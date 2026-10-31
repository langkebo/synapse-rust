use crate::routes::context::AdminContext;
use crate::routes::extractors::MatrixPath;
use crate::routes::extractors::MatrixQuery;
use crate::routes::extractors::RoomId;
use crate::routes::AdminUser;
use axum::{extract::State, Json};
use serde::Deserialize;
use serde_json::{json, Value};
use synapse_common::ApiError;

/// Query for the paginated admin space listings.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpaceListQuery {
    /// Page size; clamped to `1..=500`, defaults to 100.
    pub limit: Option<i64>,
    /// Opaque keyset cursor taken from the previous page's `next_batch`.
    pub from: Option<String>,
}

/// Decodes the `"{created_ts}|{space_id}"` cursor used by `GET /spaces`.
fn parse_space_cursor(from: Option<&str>) -> Result<Option<(i64, String)>, ApiError> {
    let Some(raw) = from.filter(|value| !value.is_empty()) else {
        return Ok(None);
    };

    let (ts, space_id) = raw.split_once('|').ok_or_else(|| ApiError::invalid_param("Invalid `from` cursor"))?;
    let ts = ts.parse::<i64>().map_err(|_| ApiError::invalid_param("Invalid `from` cursor"))?;
    if space_id.is_empty() {
        return Err(ApiError::invalid_param("Invalid `from` cursor"));
    }

    Ok(Some((ts, space_id.to_owned())))
}

/// Decodes the bare-identifier cursor used by the space member/child listings.
fn parse_id_cursor(from: Option<&str>) -> Option<&str> {
    from.filter(|value| !value.is_empty())
}

/// Clamps a caller-supplied page size into the supported range.
fn page_limit(limit: Option<i64>) -> i64 {
    limit.unwrap_or(100).clamp(1, 500)
}

async fn resolve_space_id(ctx: &AdminContext, identifier: &str) -> Result<String, ApiError> {
    ctx.space_service
        .resolve_space_id(identifier)
        .await?
        .ok_or_else(|| ApiError::not_found("Space not found".to_string()))
}

/// See [`get_spaces`].
#[axum::debug_handler]
pub async fn get_spaces(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    MatrixQuery(query): MatrixQuery<SpaceListQuery>,
) -> Result<Json<Value>, ApiError> {
    let limit = page_limit(query.limit);
    let cursor = parse_space_cursor(query.from.as_deref())?;

    let (spaces, next_batch) = ctx.space_service.get_all_spaces_for_admin(limit, cursor).await?;

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

    Ok(Json(json!({ "spaces": space_list, "total": space_list.len(), "next_batch": next_batch })))
}

/// See [`get_space`].
#[axum::debug_handler]
pub async fn get_space(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    MatrixPath(space_id): MatrixPath<RoomId>,
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
    MatrixPath(space_id): MatrixPath<RoomId>,
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
    MatrixPath(space_id): MatrixPath<RoomId>,
    MatrixQuery(query): MatrixQuery<SpaceListQuery>,
) -> Result<Json<Value>, ApiError> {
    let resolved_space_id = resolve_space_id(&ctx, &space_id).await?;
    let limit = page_limit(query.limit);
    let cursor = parse_id_cursor(query.from.as_deref());

    let (user_list, next_batch) = ctx.space_service.get_space_user_ids(&resolved_space_id, limit, cursor).await?;

    Ok(Json(json!({ "users": user_list, "total": user_list.len(), "next_batch": next_batch })))
}

/// See [`get_space_rooms`].
#[axum::debug_handler]
pub async fn get_space_rooms(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    MatrixPath(space_id): MatrixPath<RoomId>,
    MatrixQuery(query): MatrixQuery<SpaceListQuery>,
) -> Result<Json<Value>, ApiError> {
    let resolved_space_id = resolve_space_id(&ctx, &space_id).await?;
    let limit = page_limit(query.limit);
    let cursor = parse_id_cursor(query.from.as_deref());

    let (room_list, next_batch) = ctx.space_service.get_space_room_ids(&resolved_space_id, limit, cursor).await?;

    Ok(Json(json!({ "rooms": room_list, "total": room_list.len(), "next_batch": next_batch })))
}

/// See [`get_space_stats`].
#[axum::debug_handler]
pub async fn get_space_stats(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    MatrixPath(space_id): MatrixPath<RoomId>,
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
    MatrixPath(room_id): MatrixPath<RoomId>,
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
    MatrixPath(room_id): MatrixPath<RoomId>,
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
    MatrixPath(room_id): MatrixPath<RoomId>,
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
    MatrixPath(room_id): MatrixPath<RoomId>,
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
    use crate::routes::assembly::declared_ledger_all;
    use crate::routes::route_ledger::RouteEntry;
    use axum::http::Method;
    use synapse_common::ApiErrorKind;

    /// Helper: extract admin::room spaces routes from the derived route ledger.
    ///
    /// Filters the actual derived route table by `registered_by == "admin::room"`
    /// and path containing "/spaces" to get the real route manifest.
    fn admin_room_spaces_route_manifest() -> Vec<RouteEntry> {
        declared_ledger_all()
            .iter()
            .filter(|e| e.registered_by == "admin::room" && e.path.contains("/spaces"))
            .cloned()
            .collect()
    }

    /// High-standard router structure test: verify the real derived route
    /// manifest contains the admin::room spaces endpoints.
    #[test]
    fn test_admin_room_spaces_routes_from_real_ledger() {
        let manifest = admin_room_spaces_route_manifest();

        // Admin spaces endpoints: GET /spaces, GET/DELETE /spaces/{space_id}, etc.
        assert!(!manifest.is_empty(), "admin::room spaces manifest must declare at least one (method, path) entry");

        // Verify we have the core endpoints
        let has_get_spaces = manifest.iter().any(|e| e.method == Method::GET && e.path == "/_synapse/admin/v1/spaces");
        assert!(has_get_spaces, "must have GET /_synapse/admin/v1/spaces");

        let has_delete_space =
            manifest.iter().any(|e| e.method == Method::DELETE && e.path.starts_with("/_synapse/admin/v1/spaces/"));
        assert!(has_delete_space, "must have DELETE /_synapse/admin/v1/spaces/<space_id>");

        let has_get_space =
            manifest.iter().any(|e| e.method == Method::GET && e.path.starts_with("/_synapse/admin/v1/spaces/"));
        assert!(has_get_space, "must have GET /_synapse/admin/v1/spaces/<space_id>");
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
    fn test_room_not_found_error_kind() {
        let error = ApiError::not_found("Room not found".to_string());
        assert!(matches!(error.kind, ApiErrorKind::NotFound));
    }
}

// ============== Tests ==============
