//! 全局邀请名单端点：整体替换 + per-user 增量（交接单 Q1，2026-10-16）。
//!
//! 两条语义**并存**，不要互相替代：
//!
//! * **整体替换**（`GET|POST /_synapse/admin/v1/invite/{blocklist,allowlist}`）——
//!   声明式全量设置：调用方给出什么名单就存什么名单，**不做跨表互斥**；
//! * **per-user 增量**（`POST|DELETE …/{user_id}`）—— 单条、**幂等**，并做**跨表互斥**
//!   （加入一侧会把同一 user 从另一侧移除）。
//!
//! 决策依据与实测记录：`docs/audit/INVITE_PERUSER_POLICY_PROPOSAL_2026-10-16.md` §4–§6。
//!
//! ⚠️ per-user 端点的**整个 body 都可缺**：SDK 在"只给 user_id、没有 reason"时不发 body、
//! 也不设 `Content-Type` ⇒ body 提取器必须是 `Option<Json<…>>`；写成必选会让那条调用直接 415。

use crate::routes::context::AdminContext;
use crate::routes::AdminUser;
use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use synapse_common::types::UserId;
use synapse_common::ApiError;

/// See [`get_invite_blocklist_admin`].
#[axum::debug_handler]
pub async fn get_invite_blocklist_admin(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Query(params): Query<InviteListParams>,
) -> Result<Json<Value>, ApiError> {
    let blocklist = ctx.invite_blocklist_service.get_global_invite_blocklist(params.limit, params.offset).await?;
    let count = ctx.invite_blocklist_service.get_global_invite_blocklist_count().await?;

    Ok(Json(json!({
        "blocklist": blocklist,
        "total_count": count,
        "limit": params.limit,
        "offset": params.offset
    })))
}

/// See [`get_invite_allowlist_admin`].
#[axum::debug_handler]
pub async fn get_invite_allowlist_admin(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Query(params): Query<InviteListParams>,
) -> Result<Json<Value>, ApiError> {
    let allowlist = ctx.invite_blocklist_service.get_global_invite_allowlist(params.limit, params.offset).await?;
    let count = ctx.invite_blocklist_service.get_global_invite_allowlist_count().await?;

    Ok(Json(json!({
        "allowlist": allowlist,
        "total_count": count,
        "limit": params.limit,
        "offset": params.offset
    })))
}

/// Pagination parameters for the invite blocklist/allowlist admin endpoints.
#[derive(Deserialize, Debug, Clone, Copy)]
pub struct InviteListParams {
    /// Maximum number of rows to return. Defaults to 100.
    #[serde(default = "default_limit")]
    pub limit: i64,
    /// Number of rows to skip for pagination. Defaults to 0.
    #[serde(default)]
    pub offset: i64,
}

/// Request body for setting the global invite list.
#[derive(Deserialize, Debug)]
pub struct InviteListWriteBody {
    /// User IDs to include in the global list.
    pub user_ids: Vec<String>,
}

/// Replace the global invite blocklist.
/// POST /_synapse/admin/v1/invite/blocklist
#[axum::debug_handler]
pub async fn set_global_invite_blocklist_admin(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Json(body): Json<InviteListWriteBody>,
) -> Result<Json<Value>, ApiError> {
    ctx.invite_blocklist_service.set_global_invite_blocklist(body.user_ids).await?;

    Ok(Json(json!({
        "status": "ok",
        "action": "set_global_invite_blocklist"
    })))
}

/// Replace the global invite allowlist.
/// POST /_synapse/admin/v1/invite/allowlist
#[axum::debug_handler]
pub async fn set_global_invite_allowlist_admin(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Json(body): Json<InviteListWriteBody>,
) -> Result<Json<Value>, ApiError> {
    ctx.invite_blocklist_service.set_global_invite_allowlist(body.user_ids).await?;

    Ok(Json(json!({
        "status": "ok",
        "action": "set_global_invite_allowlist"
    })))
}

/// Request body for the per-user invite-list endpoints（Q1，2026-10-16）。
///
/// `reason` 可空、**整个 body 也可缺**：SDK 在"只给 user_id、没有理由"时根本不发 body
/// （也不设 `Content-Type`）⇒ handler 必须接受"无 body"。
///
/// `deny_unknown_fields` 是刻意的：未知字段一律 400，而不是被静默丢弃 —— 这正是交接单 Q4
/// 的教训（那个黑名单端点曾把整个 body 丢过一次）。
#[derive(Deserialize, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct InviteListEntryBody {
    /// 为什么拉黑 / 放行（落库，供审计）。
    pub reason: Option<String>,
}

/// Add one user to the global invite blocklist（per-user, idempotent）。
/// POST /_synapse/admin/v1/invite/blocklist/{user_id}
///
/// 重复调用 ⇒ `200` + `already_present: true`（**不覆盖**既有 reason / created_by）。
/// 若该用户原本在全局白名单里，则同时把它从白名单移除（`removed_from_other_list: true`）。
#[axum::debug_handler]
pub async fn add_global_invite_blocklist_user(
    admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(user_id): Path<UserId>,
    body: Option<Json<InviteListEntryBody>>,
) -> Result<Json<Value>, ApiError> {
    let reason = body.and_then(|Json(body)| body.reason);
    let (already_present, removed_from_other_list) =
        ctx.invite_blocklist_service.add_to_global_invite_blocklist(user_id.as_str(), reason, &admin.user_id).await?;

    Ok(Json(json!({
        "status": "ok",
        "action": "add_global_invite_blocklist_user",
        "user_id": user_id.as_str(),
        "already_present": already_present,
        "removed_from_other_list": removed_from_other_list
    })))
}

/// Remove one user from the global invite blocklist（per-user, idempotent）。
/// DELETE /_synapse/admin/v1/invite/blocklist/{user_id}
///
/// 目标用户不在名单里 ⇒ 同样 `200` + `already_absent: true`（幂等删除）。
#[axum::debug_handler]
pub async fn remove_global_invite_blocklist_user(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(user_id): Path<UserId>,
) -> Result<Json<Value>, ApiError> {
    let removed = ctx.invite_blocklist_service.remove_from_global_invite_blocklist(user_id.as_str()).await?;

    Ok(Json(json!({
        "status": "ok",
        "action": "remove_global_invite_blocklist_user",
        "user_id": user_id.as_str(),
        "already_absent": !removed
    })))
}

/// Add one user to the global invite allowlist（per-user, idempotent）。
/// POST /_synapse/admin/v1/invite/allowlist/{user_id}
#[axum::debug_handler]
pub async fn add_global_invite_allowlist_user(
    admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(user_id): Path<UserId>,
    body: Option<Json<InviteListEntryBody>>,
) -> Result<Json<Value>, ApiError> {
    let reason = body.and_then(|Json(body)| body.reason);
    let (already_present, removed_from_other_list) =
        ctx.invite_blocklist_service.add_to_global_invite_allowlist(user_id.as_str(), reason, &admin.user_id).await?;

    Ok(Json(json!({
        "status": "ok",
        "action": "add_global_invite_allowlist_user",
        "user_id": user_id.as_str(),
        "already_present": already_present,
        "removed_from_other_list": removed_from_other_list
    })))
}

/// Remove one user from the global invite allowlist（per-user, idempotent）。
/// DELETE /_synapse/admin/v1/invite/allowlist/{user_id}
#[axum::debug_handler]
pub async fn remove_global_invite_allowlist_user(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path(user_id): Path<UserId>,
) -> Result<Json<Value>, ApiError> {
    let removed = ctx.invite_blocklist_service.remove_from_global_invite_allowlist(user_id.as_str()).await?;

    Ok(Json(json!({
        "status": "ok",
        "action": "remove_global_invite_allowlist_user",
        "user_id": user_id.as_str(),
        "already_absent": !removed
    })))
}

fn default_limit() -> i64 {
    100
}
