// 房间级邀请名单（本仓扩展）—— 路由层视图。
//
// ⚠️ 归号订正（2026-10-16）：本文件**不是** MSC4380。MSC4380（Matrix v1.18 已纳入）定义的是
// **用户自己**拒收邀请的单个开关（`default_action: "block" | "allow"`），其正文把
// 「按 user ID / 服务器 / Space 屏蔽」列为**未来扩展**并指向 MSC4155。本文件实现的是
// **房主控制谁能被邀请进本房间**的名单，属本仓扩展。
// 依据：docs/audit/INVITE_PERUSER_POLICY_PROPOSAL_2026-10-16.md §7.2。
// Allows room admins to control who can be invited to a room

use crate::routes::context::AdminContext;
use crate::routes::extractors::MatrixPath;
use crate::routes::extractors::RoomId;
use crate::routes::MatrixJson;
use crate::routes::{ensure_room_member_strict_admin, ApiError, AuthenticatedUser};
use axum::{extract::State, Json};
use serde_json::{json, Value};
use synapse_common::current_timestamp_millis;

async fn ensure_invite_list_view_access(
    ctx: &AdminContext,
    auth_user: &AuthenticatedUser,
    room_id: &str,
) -> Result<(), ApiError> {
    ensure_room_member_strict_admin(
        ctx,
        auth_user,
        room_id,
        "You must be a member of this room to view invite restrictions",
    )
    .await
}

async fn ensure_invite_list_manage_access(
    ctx: &AdminContext,
    auth_user: &AuthenticatedUser,
    room_id: &str,
) -> Result<(), ApiError> {
    ensure_room_member_strict_admin(
        ctx,
        auth_user,
        room_id,
        "You must be a member of this room to manage invite restrictions",
    )
    .await?;

    let is_admin = ctx.room_auth.verify_room_admin(room_id, &auth_user.user_id).await.is_ok();

    if !is_admin {
        return Err(ApiError::forbidden("Only room admins can set invite blocklist".to_string()));
    }

    Ok(())
}

/// Get room invite blocklist
/// GET /_matrix/vendor/v1/rooms/{room_id}/invite_blocklist
pub async fn get_invite_blocklist(
    State(ctx): State<AdminContext>,
    auth_user: AuthenticatedUser,
    MatrixPath(room_id): MatrixPath<RoomId>,
) -> Result<Json<Value>, ApiError> {
    ensure_invite_list_view_access(&ctx, &auth_user, &room_id).await?;

    let blocklist = ctx.invite_blocklist_service.get_invite_blocklist(&room_id).await?;

    Ok(Json(json!({
        "blocklist": blocklist,
        "blocked_users": blocklist
    })))
}

/// Set room invite blocklist
/// POST /_matrix/vendor/v1/rooms/{room_id}/invite_blocklist
pub async fn set_invite_blocklist(
    State(ctx): State<AdminContext>,
    auth_user: AuthenticatedUser,
    MatrixPath(room_id): MatrixPath<RoomId>,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, ApiError> {
    ensure_invite_list_manage_access(&ctx, &auth_user, &room_id).await?;

    let user_ids: Vec<String> = body
        .get("user_ids")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    ctx.invite_blocklist_service.set_invite_blocklist(&room_id, user_ids.clone()).await?;

    Ok(Json(json!({
        "room_id": room_id.to_string(),
        "blocklist": user_ids.clone(),
        "blocked_users": user_ids,
        "updated_ts": current_timestamp_millis()
    })))
}

/// Get room invite allowlist
/// GET /_matrix/vendor/v1/rooms/{room_id}/invite_allowlist
pub async fn get_invite_allowlist(
    State(ctx): State<AdminContext>,
    auth_user: AuthenticatedUser,
    MatrixPath(room_id): MatrixPath<RoomId>,
) -> Result<Json<Value>, ApiError> {
    ensure_invite_list_view_access(&ctx, &auth_user, &room_id).await?;

    let allowlist = ctx.invite_blocklist_service.get_invite_allowlist(&room_id).await?;

    Ok(Json(json!({
        "allowlist": allowlist,
        "allowed_users": allowlist
    })))
}

/// Set room invite allowlist
/// POST /_matrix/vendor/v1/rooms/{room_id}/invite_allowlist
pub async fn set_invite_allowlist(
    State(ctx): State<AdminContext>,
    auth_user: AuthenticatedUser,
    MatrixPath(room_id): MatrixPath<RoomId>,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, ApiError> {
    ensure_invite_list_manage_access(&ctx, &auth_user, &room_id).await?;

    let user_ids: Vec<String> = body
        .get("user_ids")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    ctx.invite_blocklist_service.set_invite_allowlist(&room_id, user_ids.clone()).await?;

    Ok(Json(json!({
        "room_id": room_id.to_string(),
        "allowlist": user_ids.clone(),
        "allowed_users": user_ids,
        "updated_ts": current_timestamp_millis()
    })))
}
