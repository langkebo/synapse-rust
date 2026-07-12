use crate::common::ApiError;
use crate::web::extractors::{AuthenticatedUser, OptionalAuthenticatedUser};
use crate::web::routes::context::AdminContext;
use crate::web::routes::{
    account_compat::{can_view_profile_for_requester_batch, enforce_profile_visibility},
    ensure_room_member_admin, validate_event_id, validate_room_alias, validate_room_id, validate_user_id,
};
use crate::web::utils::auth::resolve_request_id;
use axum::{
    extract::{Json, Path, Query, State},
    http::HeaderMap,
};
use serde_json::{json, Value};

fn decode_public_rooms_cursor(cursor: Option<&str>) -> Option<(i64, &str)> {
    let cursor = cursor?;
    let (ts, room_id) = cursor.split_once('|')?;
    let ts = ts.parse::<i64>().ok()?;
    if room_id.is_empty() {
        return None;
    }
    Some((ts, room_id))
}

fn encode_public_rooms_cursor(created_ts: i64, room_id: &str) -> String {
    format!("{created_ts}|{room_id}")
}

async fn ensure_room_alias_write_allowed(
    ctx: &AdminContext,
    auth_user: &AuthenticatedUser,
    room_id: &str,
) -> Result<(), ApiError> {
    ensure_room_member_admin(ctx, auth_user, room_id, "You must be a member of this room to manage aliases").await?;

    let is_creator = ctx
        .room_service
        .state
        .is_room_creator(room_id, &auth_user.user_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to check room creator", &e))?;

    if !is_creator {
        return Err(ApiError::forbidden("Only room admins can manage aliases".to_string()));
    }

    Ok(())
}

pub(crate) async fn get_user_directory_profile(
    State(ctx): State<AdminContext>,
    _auth_user: AuthenticatedUser,
    headers: HeaderMap,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&user_id)?;
    enforce_profile_visibility(ctx.auth_service.as_ref(), &ctx.account_identity_service, &headers, &user_id).await?;

    let user = ctx
        .account_identity_service
        .get_user_by_identifier(&user_id)
        .await?
        .ok_or_else(|| ApiError::not_found("User not found".to_string()))?;

    Ok(Json(json!({
        "user_id": user.user_id,
        "displayname": user.displayname,
        "avatar_url": user.avatar_url
    })))
}

pub(crate) async fn search_user_directory(
    State(ctx): State<AdminContext>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let search_query = body.get("search_term").and_then(|v| v.as_str()).unwrap_or("").to_string();

    let limit = body.get("limit").and_then(|v| v.as_u64()).unwrap_or(10).clamp(1, 100) as i64;

    let results = ctx.account_identity_service.search_users(&search_query, limit).await?;

    let target_user_ids: Vec<String> = results.iter().map(|u| u.user_id.clone()).collect();
    let visibility =
        can_view_profile_for_requester_batch(&ctx.account_identity_service, Some(&auth_user.user_id), &target_user_ids)
            .await?;

    let mut users = Vec::new();
    for u in results {
        if !visibility.get(&u.user_id).copied().unwrap_or(true) {
            continue;
        }

        users.push(json!({
            "user_id": u.user_id,
            "display_name": u.displayname,
            "avatar_url": u.avatar_url
        }));
    }

    Ok(Json(json!({
        "limited": users.len() >= limit as usize,
        "results": users
    })))
}

fn decode_user_cursor(cursor: Option<&str>) -> Option<(i64, &str)> {
    let cursor = cursor?;
    let (ts, user_id) = cursor.split_once('|')?;
    let ts = ts.parse::<i64>().ok()?;
    if user_id.is_empty() {
        return None;
    }
    Some((ts, user_id))
}

fn encode_user_cursor(created_ts: i64, user_id: &str) -> String {
    format!("{created_ts}|{user_id}")
}

pub(crate) async fn list_user_directory(
    State(ctx): State<AdminContext>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let limit = body.get("limit").and_then(|v| v.as_u64()).unwrap_or(50).clamp(1, 200) as i64;
    let cursor = decode_user_cursor(body.get("since").and_then(|v| v.as_str()));

    let total_count = ctx.account_identity_service.get_user_count().await?;

    let users = ctx
        .account_identity_service
        .get_users_paginated(limit, cursor.map(|(ts, _)| ts), cursor.map(|(_, user_id)| user_id))
        .await?;

    let next_batch = if users.len() as i64 == limit {
        users.last().map(|user| encode_user_cursor(user.created_ts, &user.user_id))
    } else {
        None
    };

    let target_user_ids: Vec<String> = users.iter().map(|u| u.user_id.clone()).collect();
    let visibility =
        can_view_profile_for_requester_batch(&ctx.account_identity_service, Some(&auth_user.user_id), &target_user_ids)
            .await?;

    let mut users_json = Vec::new();
    for u in users {
        if !visibility.get(&u.user_id).copied().unwrap_or(true) {
            continue;
        }

        users_json.push(json!({
            "user_id": u.user_id,
            "display_name": u.displayname,
            "avatar_url": u.avatar_url
        }));
    }

    Ok(Json(json!({
        "total": total_count,
        "next_batch": next_batch,
        "users": users_json
    })))
}

pub(crate) async fn report_event(
    State(ctx): State<AdminContext>,
    headers: HeaderMap,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);
    validate_room_id(&room_id)?;
    validate_event_id(&event_id)?;
    ensure_room_member_admin(&ctx, &auth_user, &room_id, "You must be a room member to report events in this room")
        .await?;

    let event = ctx.room_service.messaging.get_event_record(&event_id).await?;
    let Some(event) = event.filter(|event| event.room_id == room_id) else {
        return Err(ApiError::not_found("Event not found".to_string()));
    };

    let reason = body.get("reason").and_then(|v| v.as_str());
    let score = body.get("score").and_then(|v| v.as_i64()).unwrap_or(-100) as i32;

    let report_id = ctx
        .room_service
        .messaging
        .report_event(&event.event_id, &event.room_id, &auth_user.user_id, reason, score)
        .await?;

    ::tracing::info!(
        request_id = %request_id,
        room_id = %room_id,
        event_id = %event.event_id,
        reporter_user_id = %auth_user.user_id,
        report_id,
        "Created event report"
    );

    Ok(Json(json!({
        "report_id": report_id
    })))
}

pub(crate) async fn update_report_score(
    State(_ctx): State<AdminContext>,
    _auth_user: AuthenticatedUser,
    Path((_room_id, event_id)): Path<(String, String)>,
    Json(_body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_event_id(&event_id)?;
    Err(ApiError::forbidden("Report score updates are not available via the client API".to_string()))
}

pub(crate) async fn report_room(
    State(ctx): State<AdminContext>,
    headers: HeaderMap,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);
    validate_room_id(&room_id)?;
    ensure_room_member_admin(&ctx, &auth_user, &room_id, "You must be a room member to report this room").await?;

    let reason = body.get("reason").and_then(|v| v.as_str()).map(str::to_string);
    let description = body.get("description").and_then(|v| v.as_str()).map(str::to_string);

    let request = synapse_storage::event_report::CreateEventReportRequest {
        event_id: format!("room_report:{room_id}"),
        room_id: room_id.clone(),
        reporter_user_id: auth_user.user_id.clone(),
        reported_user_id: None,
        event_json: None,
        reason,
        description,
        score: Some(0),
    };

    let report = ctx.event_report_service.create_report(request).await?;

    ::tracing::info!(
        request_id = %request_id,
        room_id = %room_id,
        reporter_user_id = %auth_user.user_id,
        report_id = report.id,
        "Created room report"
    );

    Ok(Json(json!({
        "report_id": report.id,
        "room_id": room_id,
        "status": "submitted"
    })))
}

pub(crate) async fn get_scanner_info(
    State(ctx): State<AdminContext>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    validate_event_id(&event_id)?;
    ensure_room_member_admin(&ctx, &auth_user, &room_id, "You must be a room member to view scanner info").await?;

    let event = ctx.room_service.messaging.get_event_record(&event_id).await?;
    let Some(event) = event.filter(|event| event.room_id == room_id) else {
        return Err(ApiError::not_found("Event not found".to_string()));
    };

    Ok(Json(json!({
        "scanner_enabled": false,
        "room_id": room_id,
        "event_id": event.event_id,
        "status": "not_configured",
        "message": "Content scanner is not enabled on this server"
    })))
}

pub(crate) async fn get_room_aliases(
    State(ctx): State<AdminContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    if !ctx.room_service.state.room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_member_admin(&ctx, &auth_user, &room_id, "You must be a room member to view aliases").await?;

    let aliases = ctx.room_service.state.get_room_aliases(&room_id).await?;
    Ok(Json(json!({ "aliases": aliases })))
}

pub(crate) async fn set_room_alias(
    State(ctx): State<AdminContext>,
    headers: HeaderMap,
    auth_user: AuthenticatedUser,
    Path((room_id, room_alias)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);
    validate_room_alias(&room_alias)?;

    if room_alias.len() > 255 {
        return Err(ApiError::bad_request("Alias too long (max 255 characters)".to_string()));
    }

    if !ctx.room_service.state.room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_alias_write_allowed(&ctx, &auth_user, &room_id).await?;

    ctx.room_service.state.set_room_alias(&room_id, &room_alias, &auth_user.user_id).await?;
    ::tracing::info!(
        request_id = %request_id,
        room_id = %room_id,
        room_alias = %room_alias,
        user_id = %auth_user.user_id,
        "Set room alias"
    );
    Ok(Json(json!({
        "room_id": room_id,
        "alias": room_alias,
        "created_ts": chrono::Utc::now().timestamp_millis()
    })))
}

pub(crate) async fn delete_room_alias(
    State(ctx): State<AdminContext>,
    headers: HeaderMap,
    auth_user: AuthenticatedUser,
    Path((room_id, _room_alias)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);
    ensure_room_alias_write_allowed(&ctx, &auth_user, &room_id).await?;
    ctx.room_service.state.remove_room_alias(&room_id).await?;
    ::tracing::info!(request_id = %request_id, room_id = %room_id, user_id = %auth_user.user_id, "Deleted room alias by room id");
    Ok(Json(json!({})))
}

pub(crate) async fn get_room_by_alias(
    State(ctx): State<AdminContext>,
    _auth_user: OptionalAuthenticatedUser,
    Path(room_alias): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_alias(&room_alias)?;
    let room_id = ctx.room_service.state.get_room_by_alias(&room_alias).await?;
    match room_id {
        Some(rid) => Ok(Json(json!({ "room_id": rid }))),
        None => Err(ApiError::not_found("Room alias not found".to_string())),
    }
}

#[axum::debug_handler]
pub(crate) async fn set_room_alias_direct(
    State(ctx): State<AdminContext>,
    headers: HeaderMap,
    auth_user: AuthenticatedUser,
    Path(room_alias): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);
    validate_room_alias(&room_alias)?;
    let room_id = body
        .get("room_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing room_id field".to_string()))?;

    if !room_id.starts_with('!') {
        return Err(ApiError::bad_request("Invalid room_id format".to_string()));
    }

    if !ctx.room_service.state.room_exists(room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ensure_room_alias_write_allowed(&ctx, &auth_user, room_id).await?;

    ctx.room_service.state.set_room_alias(room_id, &room_alias, &auth_user.user_id).await?;
    ::tracing::info!(
        request_id = %request_id,
        room_id,
        room_alias = %room_alias,
        user_id = %auth_user.user_id,
        "Set room alias by alias endpoint"
    );
    Ok(Json(json!({
        "room_id": room_id,
        "alias": room_alias,
        "created_ts": chrono::Utc::now().timestamp_millis()
    })))
}

#[axum::debug_handler]
pub(crate) async fn delete_room_alias_direct(
    State(ctx): State<AdminContext>,
    headers: HeaderMap,
    auth_user: AuthenticatedUser,
    Path(room_alias): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);
    validate_room_alias(&room_alias)?;
    if let Some(room_id) = ctx.room_service.state.get_room_by_alias(&room_alias).await? {
        ensure_room_alias_write_allowed(&ctx, &auth_user, &room_id).await?;
    }
    ctx.room_service.state.remove_room_alias_by_name(&room_alias).await?;
    ::tracing::info!(request_id = %request_id, room_alias = %room_alias, user_id = %auth_user.user_id, "Deleted room alias by alias");
    Ok(Json(json!({
        "removed": true,
        "alias": room_alias
    })))
}

pub(crate) async fn get_public_rooms(
    State(ctx): State<AdminContext>,
    _auth_user: OptionalAuthenticatedUser,
    Query(params): Query<Value>,
) -> Result<Json<Value>, ApiError> {
    let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(20).clamp(1, 1000) as i64;
    let cursor = decode_public_rooms_cursor(params.get("since").and_then(|v| v.as_str()));

    let (rooms, total) = tokio::try_join!(
        async {
            ctx.room_service
                .state
                .get_public_rooms_paginated(limit, cursor.map(|(ts, _)| ts), cursor.map(|(_, room_id)| room_id))
                .await
        },
        async { ctx.room_service.state.count_public_rooms().await }
    )?;

    let next_batch = if rooms.len() as i64 == limit {
        rooms.last().map(|room| encode_public_rooms_cursor(room.created_ts, &room.room_id))
    } else {
        None
    };

    let chunk: Vec<Value> = rooms
        .into_iter()
        .map(|r| {
            let world_readable = r.history_visibility == "world_readable";
            let guest_can_join = r.join_rule == "public";
            json!({
                "room_id": r.room_id,
                "name": r.name,
                "topic": r.topic,
                "avatar_url": r.avatar_url,
                "canonical_alias": r.canonical_alias,
                "num_joined_members": r.member_count,
                "world_readable": world_readable,
                "guest_can_join": guest_can_join,
                "join_rule": r.join_rule,
                "room_type": Option::<String>::None,
            })
        })
        .collect();

    Ok(Json(json!({
        "chunk": chunk,
        "total_room_count_estimate": total,
        "next_batch": next_batch,
    })))
}

#[cfg(test)]
mod cursor_tests {
    use super::{decode_public_rooms_cursor, decode_user_cursor, encode_public_rooms_cursor, encode_user_cursor};

    #[test]
    fn test_public_rooms_cursor_round_trip() {
        let cursor = encode_public_rooms_cursor(1_700_000_000_000, "!room:example.com");
        assert_eq!(decode_public_rooms_cursor(Some(&cursor)), Some((1_700_000_000_000, "!room:example.com")));
    }

    #[test]
    fn test_user_directory_cursor_round_trip() {
        let cursor = encode_user_cursor(1_700_000_000_000, "@alice:example.com");
        assert_eq!(decode_user_cursor(Some(&cursor)), Some((1_700_000_000_000, "@alice:example.com")));
    }

    #[test]
    fn test_user_directory_cursor_rejects_invalid_value() {
        assert_eq!(decode_user_cursor(Some("bad-cursor")), None);
        assert_eq!(decode_user_cursor(Some("123|")), None);
    }
}

#[axum::debug_handler]
pub(crate) async fn query_public_rooms(
    State(ctx): State<AdminContext>,
    _auth_user: OptionalAuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let limit = body.get("limit").and_then(|v| v.as_u64()).unwrap_or(20).clamp(1, 1000) as i64;
    let cursor = decode_public_rooms_cursor(body.get("since").and_then(|v| v.as_str()));
    let _filter = body.get("filter");

    let (rooms, total) = tokio::try_join!(
        async {
            ctx.room_service
                .state
                .get_public_rooms_paginated(limit, cursor.map(|(ts, _)| ts), cursor.map(|(_, room_id)| room_id))
                .await
        },
        async { ctx.room_service.state.count_public_rooms().await }
    )?;

    let next_batch = if rooms.len() as i64 == limit {
        rooms.last().map(|room| encode_public_rooms_cursor(room.created_ts, &room.room_id))
    } else {
        None
    };

    let chunk: Vec<Value> = rooms
        .into_iter()
        .map(|r| {
            let world_readable = r.history_visibility == "world_readable";
            let guest_can_join = r.join_rule == "public";
            json!({
                "room_id": r.room_id,
                "name": r.name,
                "topic": r.topic,
                "avatar_url": r.avatar_url,
                "canonical_alias": r.canonical_alias,
                "num_joined_members": r.member_count,
                "world_readable": world_readable,
                "guest_can_join": guest_can_join,
                "join_rule": r.join_rule,
                "room_type": Option::<String>::None,
            })
        })
        .collect();

    Ok(Json(json!({
        "chunk": chunk,
        "total_room_count_estimate": total,
        "next_batch": next_batch,
    })))
}
