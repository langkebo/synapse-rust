use super::types::*;
use crate::routes::admin::audit::{record_audit_event, resolve_request_id};
use crate::routes::context::AdminContext;
use crate::routes::extractors::{RoomId, UserId};
use crate::routes::validate_moderation_reason;
use crate::routes::AdminUser;
use crate::routes::MatrixJson;
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use synapse_common::current_timestamp_millis;
use synapse_common::ApiError;

/// `purge_history` 的两个边界参数**恰好要一个**；这两条消息在校验与取值两处共用，
/// 避免同一事实写两份而漂移。
const ONLY_ONE_PURGE_BOUNDARY: &str = "Only one of 'purge_up_to_ts' and 'purge_up_to_event_id' may be provided";
const MISSING_PURGE_BOUNDARY: &str = "One of 'purge_up_to_ts' or 'purge_up_to_event_id' is required";

/// Request body for the abnormal-room cleanup endpoints.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CleanupAbnormalRoomsRequest {
    /// Only clean up rooms older than this many milliseconds.
    pub min_age_ms: Option<i64>,
}

/// Request body for [`purge_history`].
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PurgeHistoryRequest {
    /// Room to purge; required in the body for the un-scoped endpoint.
    pub room_id: Option<String>,
    /// Purge events older than this millisecond timestamp.
    ///
    /// Mutually exclusive with `purge_up_to_event_id`; one of the two is required.
    pub purge_up_to_ts: Option<i64>,
    /// Purge events older than this event (upstream-compatible alternative to
    /// `purge_up_to_ts`).
    pub purge_up_to_event_id: Option<String>,
    /// Upstream flag, accepted for compatibility but **not honoured**: only
    /// remote/federated events are purged, local events are always preserved.
    pub delete_local_events: Option<bool>,
    /// Only report what would be deleted.
    pub dry_run: Option<bool>,
}

/// Request body for the path-scoped purge-history endpoint.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PurgeHistoryByRoomRequest {
    /// Purge events older than this millisecond timestamp.
    ///
    /// Mutually exclusive with `purge_up_to_event_id`; one of the two is required.
    pub purge_up_to_ts: Option<i64>,
    /// Purge events older than this event (upstream-compatible alternative to
    /// `purge_up_to_ts`).
    pub purge_up_to_event_id: Option<String>,
    /// Upstream flag, accepted for compatibility but **not honoured**: only
    /// remote/federated events are purged, local events are always preserved.
    pub delete_local_events: Option<bool>,
    /// Only report what would be deleted.
    pub dry_run: Option<bool>,
}

/// Request body for [`purge_room`].
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PurgeRoomRequest {
    /// Room to delete; required in the body.
    pub room_id: Option<String>,
}

/// Request body for [`backfill_room`].
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackfillRoomRequest {
    /// Maximum number of events to backfill.
    pub limit: Option<u64>,
}

/// See [`cleanup_abnormal_rooms`].
#[axum::debug_handler]
pub async fn cleanup_abnormal_rooms(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    MatrixJson(body): MatrixJson<CleanupAbnormalRoomsRequest>,
) -> Result<Json<Value>, ApiError> {
    let min_age_ms = body.min_age_ms;

    let results = ctx.room_service.state().cleanup_abnormal_data(min_age_ms).await?;

    Ok(Json(results))
}

/// See [`block_room`].
#[axum::debug_handler]
pub async fn block_room(
    admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(room_id): Path<RoomId>,
    headers: HeaderMap,
    MatrixJson(body): MatrixJson<BlockRoomRequest>,
) -> Result<Json<Value>, ApiError> {
    if !ctx.room_service.state().room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ctx.room_service.state().block_room(&room_id, &admin.user_id, body.reason.as_deref()).await?;

    record_audit_event(
        &ctx,
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

/// See [`get_room_block_status`].
#[axum::debug_handler]
pub async fn get_room_block_status(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(room_id): Path<RoomId>,
) -> Result<Json<Value>, ApiError> {
    if !ctx.room_service.state().room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let blocked_at = ctx.room_service.state().get_room_block_status(&room_id).await?;

    match blocked_at {
        Some(blocked_at) => Ok(Json(json!({
            "block": true,
            "blocked_at": blocked_at
        }))),
        None => Ok(Json(json!({ "block": false }))),
    }
}

/// See [`unblock_room`].
#[axum::debug_handler]
pub async fn unblock_room(
    admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(room_id): Path<RoomId>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    if !ctx.room_service.state().room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    ctx.room_service.state().unblock_room(&room_id).await?;

    record_audit_event(
        &ctx,
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

/// See [`make_room_admin`].
#[axum::debug_handler]
pub async fn make_room_admin(
    admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(room_id): Path<RoomId>,
    MatrixJson(body): MatrixJson<MakeRoomAdminRequest>,
) -> Result<Json<Value>, ApiError> {
    crate::routes::admin::ensure_super_admin_for_privilege_change(&admin)?;
    if !ctx.room_service.state().room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    if ctx.account_identity_service.get_user_by_id(&body.user_id).await?.is_none() {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    ctx.room_service.state().grant_room_admin(&room_id, &body.user_id).await?;

    Ok(Json(json!({})))
}

/// See [`purge_history`].
#[axum::debug_handler]
pub async fn purge_history(
    admin: AdminUser,
    State(ctx): State<AdminContext>,
    headers: HeaderMap,
    MatrixJson(body): MatrixJson<PurgeHistoryRequest>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);
    let room_id =
        body.room_id.as_deref().ok_or_else(|| ApiError::bad_request("Missing 'room_id' field".to_string()))?;
    // 两个边界参数**恰好一个**：校验放在这里（先于 room_exists，保证"参数错"优先于"房间不存在"），
    // 取值在下面那个 match 里。两处用同一组常量，避免消息漂移。
    match (body.purge_up_to_ts, body.purge_up_to_event_id.as_deref()) {
        (Some(_), Some(_)) => return Err(ApiError::bad_request(ONLY_ONE_PURGE_BOUNDARY.to_string())),
        (None, None) => return Err(ApiError::bad_request(MISSING_PURGE_BOUNDARY.to_string())),
        _ => {}
    }
    let dry_run = body.dry_run.unwrap_or(false);
    let delete_local_events = body.delete_local_events.unwrap_or(false);

    if !ctx.room_service.state().room_exists(room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    // 取值。尾臂**不再**是 `unreachable!()`：类型上它是可达的（两个边界同时给 / 都不给），
    // 只是上面刚拒绝过。写成 panic 的话，哪天有人调整上面的校验（或挪走它），
    // 一个 400 就会变成 500 + 进程内 panic —— 而面板上分不清这两者。
    let timestamp = match (body.purge_up_to_ts, body.purge_up_to_event_id.as_deref()) {
        (Some(timestamp), None) => timestamp,
        (None, Some(event_id)) => ctx
            .room_service
            .state()
            .event_origin_server_ts_in_room(room_id, event_id)
            .await?
            .ok_or_else(|| ApiError::not_found("Event not found".to_string()))?,
        (Some(_), Some(_)) => return Err(ApiError::bad_request(ONLY_ONE_PURGE_BOUNDARY.to_string())),
        (None, None) => return Err(ApiError::bad_request(MISSING_PURGE_BOUNDARY.to_string())),
    };

    if delete_local_events {
        tracing::warn!(
            request_id = %request_id,
            action = "admin.purge_history",
            admin_user_id = %admin.user_id,
            target_room_id = %room_id,
            "delete_local_events=true is not supported; only remote/federated events are purged"
        );
    }

    // P2 #33: 审计日志 - purge_history 操作
    tracing::warn!(
        request_id = %request_id,
        action = "admin.purge_history",
        admin_user_id = %admin.user_id,
        target_room_id = %room_id,
        purge_up_to_ts = timestamp,
        purge_up_to_event_id = body.purge_up_to_event_id.as_deref(),
        delete_local_events = delete_local_events,
        dry_run = dry_run,
        timestamp_ms = current_timestamp_millis(),
        "Admin purge history operation"
    );

    let affected_count = ctx.room_service.state().purge_history_before(room_id, timestamp, dry_run).await?;

    Ok(Json(json!({
        "success": true,
        "deleted_events": affected_count,
        "dry_run": dry_run
    })))
}

/// See [`purge_history_by_room`].
#[axum::debug_handler]
pub async fn purge_history_by_room(
    admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(room_id): Path<RoomId>,
    headers: HeaderMap,
    MatrixJson(body): MatrixJson<PurgeHistoryByRoomRequest>,
) -> Result<Json<Value>, ApiError> {
    let merged_body = PurgeHistoryRequest {
        room_id: Some(room_id.to_string()),
        purge_up_to_ts: body.purge_up_to_ts,
        purge_up_to_event_id: body.purge_up_to_event_id,
        delete_local_events: body.delete_local_events,
        dry_run: body.dry_run,
    };

    purge_history(admin, State(ctx), headers, MatrixJson(merged_body)).await
}

/// `POST /_synapse/admin/v1/rooms/{room_id}/backfill`
///
/// Manually triggers outbound federation backfill for a room.  The server
/// contacts federated peers (servers with joined members in the room) and
/// requests historical events preceding the most recent locally-known
/// events.  Fetched events are persisted with full DAG metadata via
/// `create_event_with_graph`.
///
/// Request body (all optional):
///   - `limit`: number of events to request per candidate (default 100)
///
/// Response:
///   ```json
///   {
///     "room_id": "!room:server",
///     "source_server": "remote.example",
///     "persisted_events": 42,
///     "candidates_tried": 3
///   }
///   ```
#[axum::debug_handler]
pub async fn backfill_room(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(room_id): Path<RoomId>,
    MatrixJson(body): MatrixJson<BackfillRoomRequest>,
) -> Result<Json<Value>, ApiError> {
    if !ctx.room_service.state().room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let limit = body.limit.map(|n| n.min(u64::from(u32::MAX)) as u32);

    let outcome = ctx.room_service.backfill_room_history(&ctx.federation_client, &room_id, limit).await?;

    Ok(Json(json!({
        "room_id": room_id,
        "source_server": outcome.source_server,
        "persisted_events": outcome.persisted_events,
        "candidates_tried": outcome.candidates_tried,
    })))
}

/// See [`purge_room`].
#[axum::debug_handler]
pub async fn purge_room(
    admin: AdminUser,
    State(ctx): State<AdminContext>,
    headers: HeaderMap,
    MatrixJson(body): MatrixJson<PurgeRoomRequest>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);
    let room_id =
        body.room_id.as_deref().ok_or_else(|| ApiError::bad_request("Missing 'room_id' field".to_string()))?;

    if !ctx.room_service.state().room_exists(room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    // P2 #33: 审计日志 - delete_room 操作
    tracing::warn!(
        request_id = %request_id,
        action = "admin.delete_room",
        admin_user_id = %admin.user_id,
        target_room_id = %room_id,
        timestamp_ms = current_timestamp_millis(),
        "Admin delete room operation"
    );

    ctx.room_service.state().delete_room(room_id, &admin.user_id).await?;

    Ok(Json(json!({
        "purge_id": uuid::Uuid::new_v4().to_string(),
        "success": true
    })))
}

/// Request body for [`admin_join_room`].
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminJoinRoomRequest {
    /// The user to force-join into the room.
    pub user_id: String,
}

/// Force-join a user into a room addressed by room ID or alias.
///
/// Mirrors upstream Synapse `POST /_synapse/admin/v1/join/{room_id_or_alias}`.
#[axum::debug_handler]
pub async fn admin_join_room(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(room_id_or_alias): Path<String>,
    headers: HeaderMap,
    MatrixJson(body): MatrixJson<AdminJoinRoomRequest>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);

    let room_id = if room_id_or_alias.starts_with('!') {
        room_id_or_alias
    } else if room_id_or_alias.starts_with('#') {
        ctx.room_service
            .state()
            .get_room_by_alias(&room_id_or_alias)
            .await?
            .ok_or_else(|| ApiError::not_found("Room alias not found".to_string()))?
    } else {
        return Err(ApiError::bad_request("room_id_or_alias must start with '!' or '#'".to_string()));
    };

    Ok(Json(join_room_member_internal(&ctx, &room_id, &body.user_id, &request_id).await?))
}

/// Join a user to a room (force join)
#[axum::debug_handler]
pub async fn join_room_member(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    headers: HeaderMap,
    Path((room_id, user_id)): Path<(RoomId, UserId)>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);
    Ok(Json(join_room_member_internal(&ctx, &room_id, &user_id, &request_id).await?))
}

/// Remove a user from a room
#[axum::debug_handler]
pub async fn remove_room_member(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    headers: HeaderMap,
    Path((room_id, user_id)): Path<(RoomId, UserId)>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);
    Ok(Json(remove_room_member_internal(&ctx, &room_id, &user_id, &request_id).await?))
}

/// Ban a user from a room (admin API).
#[axum::debug_handler]
pub async fn admin_ban_user(
    admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path((room_id, user_id)): Path<(RoomId, UserId)>,
    MatrixJson(body): MatrixJson<BanRequest>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(ban_user_internal(&ctx, &room_id, &user_id, &admin.user_id, body.reason.as_deref()).await?))
}

/// See [`ban_user_by_body`].
#[axum::debug_handler]
pub async fn ban_user_by_body(
    admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(room_id): Path<RoomId>,
    MatrixJson(body): MatrixJson<RoomUserActionRequest>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(ban_user_internal(&ctx, &room_id, &body.user_id, &admin.user_id, body.reason.as_deref()).await?))
}

/// Unban a user from a room
#[axum::debug_handler]
pub async fn admin_unban_user(
    admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path((room_id, user_id)): Path<(RoomId, UserId)>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(unban_user_internal(&ctx, &room_id, &user_id, &admin.user_id).await?))
}

/// Kick a user from a room
#[axum::debug_handler]
pub async fn admin_kick_user(
    admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path((room_id, user_id)): Path<(RoomId, UserId)>,
    MatrixJson(body): MatrixJson<BanRequest>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(kick_user_internal(&ctx, &room_id, &user_id, &admin.user_id, body.reason.as_deref()).await?))
}

/// See [`kick_user_by_body`].
#[axum::debug_handler]
pub async fn kick_user_by_body(
    admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(room_id): Path<RoomId>,
    MatrixJson(body): MatrixJson<RoomUserActionRequest>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(kick_user_internal(&ctx, &room_id, &body.user_id, &admin.user_id, body.reason.as_deref()).await?))
}

// Internal helpers

/// Shared precondition for the admin membership-moderation endpoints: the room
/// must exist and the target must be an account able to hold membership.
///
/// U-2: this must be the active predicate (row-existence would let deactivated
/// accounts be force-joined / banned / kicked).
async fn ensure_room_and_active_user(ctx: &AdminContext, room_id: &str, user_id: &str) -> Result<(), ApiError> {
    if !ctx.room_service.state().room_exists(room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    if !ctx.account_identity_service.active_user_exists(user_id).await? {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    Ok(())
}

async fn join_room_member_internal(
    ctx: &AdminContext,
    room_id: &str,
    user_id: &str,
    _request_id: &str,
) -> Result<Value, ApiError> {
    ensure_room_and_active_user(ctx, room_id, user_id).await?;

    let existing_membership = ctx.room_service.membership().get_room_membership(room_id, user_id).await?;

    if existing_membership.as_deref() != Some("join") {
        ctx.room_service.membership().join_room(room_id, user_id).await?;
    }

    Ok(json!({
        "user_id": user_id,
        "room_id": room_id,
        "membership": "join"
    }))
}

async fn remove_room_member_internal(
    ctx: &AdminContext,
    room_id: &str,
    user_id: &str,
    _request_id: &str,
) -> Result<Value, ApiError> {
    ensure_room_and_active_user(ctx, room_id, user_id).await?;

    let existing_membership = ctx.room_service.membership().get_room_membership(room_id, user_id).await?;

    if existing_membership.as_deref() == Some("join") {
        ctx.room_service.membership().leave_room(room_id, user_id).await?;
    }

    Ok(json!({
        "user_id": user_id,
        "room_id": room_id,
        "removed": true
    }))
}

async fn ban_user_internal(
    ctx: &AdminContext,
    room_id: &str,
    user_id: &str,
    actor_user_id: &str,
    reason: Option<&str>,
) -> Result<Value, ApiError> {
    validate_moderation_reason(reason)?;

    ensure_room_and_active_user(ctx, room_id, user_id).await?;

    // The member counter is moved by the service's shared moderation tail (it
    // has to know the previous membership anyway); decrementing here as well
    // would count a single ban twice.
    ctx.room_service.membership().admin_ban_user_membership(room_id, user_id, actor_user_id, reason).await?;

    Ok(json!({
        "user_id": user_id,
        "room_id": room_id,
        "membership": "ban"
    }))
}

async fn unban_user_internal(
    ctx: &AdminContext,
    room_id: &str,
    user_id: &str,
    actor_user_id: &str,
) -> Result<Value, ApiError> {
    ensure_room_and_active_user(ctx, room_id, user_id).await?;

    ctx.room_service.membership().admin_unban_user_membership(room_id, user_id, actor_user_id).await?;

    Ok(json!({
        "user_id": user_id,
        "room_id": room_id,
        "unbanned": true
    }))
}

async fn kick_user_internal(
    ctx: &AdminContext,
    room_id: &str,
    user_id: &str,
    actor_user_id: &str,
    reason: Option<&str>,
) -> Result<Value, ApiError> {
    validate_moderation_reason(reason)?;

    ensure_room_and_active_user(ctx, room_id, user_id).await?;

    ctx.room_service.membership().admin_kick_user_membership(room_id, user_id, actor_user_id, reason).await?;

    Ok(json!({
        "user_id": user_id,
        "room_id": room_id,
        "kicked": true,
        "reason": reason
    }))
}

/// Request body for [`redact_room_events`].
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RedactRoomEventsRequest {
    pub before_ts: Option<i64>,
    pub after_ts: Option<i64>,
    pub limit: Option<i64>,
    pub reason: Option<String>,
}

/// `POST /_matrix/client/v3/admin/room/{room_id}/redact`
///
/// Admin Redact API with time filter: batch-redact events in a room within
/// an optional time range. Matches Element Synapse's admin redact endpoint.
///
/// Request body:
/// - `before_ts` (i64, optional): redact events with `origin_server_ts < before_ts`
/// - `after_ts` (i64, optional): redact events with `origin_server_ts > after_ts`
/// - `limit` (i64, optional, default 1000): cap number of events to redact
/// - `reason` (string, optional): reason recorded in audit log
#[axum::debug_handler]
pub async fn redact_room_events(
    admin: AdminUser,
    State(ctx): State<AdminContext>,
    headers: HeaderMap,
    Path(room_id): Path<RoomId>,
    MatrixJson(body): MatrixJson<RedactRoomEventsRequest>,
) -> Result<Json<Value>, ApiError> {
    let before_ts = body.before_ts;
    let after_ts = body.after_ts;
    let limit = body.limit.unwrap_or(1000);
    let reason = body.reason.clone();

    // Validate limit is positive and bounded (prevent excessive batch sizes)
    if limit <= 0 || limit > 10_000 {
        return Err(ApiError::bad_request("limit must be between 1 and 10000".to_string()));
    }

    // Verify room exists (fail-closed: don't reveal whether room has events)
    if !ctx.room_service.state().room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let request_id = resolve_request_id(&headers);

    tracing::warn!(
        request_id = %request_id,
        action = "admin.redact_room_events",
        admin_user_id = %admin.user_id,
        target_room_id = %room_id,
        before_ts = ?before_ts,
        after_ts = ?after_ts,
        limit = limit,
        reason = ?reason,
        timestamp_ms = current_timestamp_millis(),
        "Admin batch redact operation started"
    );

    // Find events matching the time filter
    let event_ids =
        ctx.event_redaction_service.find_event_ids_for_redaction(&room_id, before_ts, after_ts, limit).await?;

    let found = event_ids.len() as u64;

    // Batch redact the matched events. Operator action: no `m.room.redaction`
    // event is persisted, and `events.redacted_by` is a self-referential FK to
    // `events.event_id` (`fk_events_redacted_by`), so the correct value is NULL.
    // Passing `admin.user_id` violated the constraint and made the endpoint 500;
    // the `admin.redact_room_events` warn! entries (with `admin_user_id`) are the
    // attribution record.
    let redacted = ctx.event_redaction_service.batch_redact_events(&room_id, &event_ids, None).await?;

    tracing::warn!(
        request_id = %request_id,
        action = "admin.redact_room_events.complete",
        admin_user_id = %admin.user_id,
        target_room_id = %room_id,
        events_found = found,
        events_redacted = redacted,
        timestamp_ms = current_timestamp_millis(),
        "Admin batch redact operation completed"
    );

    Ok(Json(json!({
        "redacted": redacted
    })))
}
