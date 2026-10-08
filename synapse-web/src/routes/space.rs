use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post, put},
    Json, Router,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde::{Deserialize, Serialize};
use std::future::Future;
use validator::Validate;

use crate::routes::context::RoomContext;
pub(super) use crate::routes::response_helpers::{created_json_from, json_from, json_vec_from};
use crate::routes::{AppState, AuthenticatedUser, OptionalAuthenticatedUser};
use synapse_common::ApiError;

/// The `children_hierarchy` module.
pub mod children_hierarchy;
mod lifecycle_query;
mod membership_state;
mod summary;
mod types;

use children_hierarchy::{create_space_children_private_routes, create_space_hierarchy_spec_routes};
use lifecycle_query::create_space_lifecycle_query_routes;
use membership_state::create_space_membership_state_routes;
use summary::create_space_summary_routes;
pub(super) use types::*;

/// See [`resolve_space_by_room`].
pub(super) async fn resolve_space_by_room(
    state: &RoomContext,
    space_room_id: &str,
) -> Result<synapse_services::room::space::Space, ApiError> {
    let space: Option<synapse_services::room::space::Space> = state
        .space_service
        .get_space_by_room(space_room_id)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to get space by room", e))?;

    space.ok_or_else(|| ApiError::not_found("Space not found"))
}

/// See [`resolve_space`].
pub(super) async fn resolve_space(
    state: &RoomContext,
    space_identifier: &str,
) -> Result<synapse_services::room::space::Space, ApiError> {
    let space: Option<synapse_services::room::space::Space> = state
        .space_service
        .get_space(space_identifier)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to get space", e))?;

    if let Some(space) = space {
        return Ok(space);
    }

    resolve_space_by_room(state, space_identifier).await
}

/// See [`with_resolved_space`].
pub(super) async fn with_resolved_space<T, F, Fut>(
    state: RoomContext,
    space_room_id: String,
    operation: F,
) -> Result<T, ApiError>
where
    F: FnOnce(RoomContext, synapse_services::room::space::Space) -> Fut,
    Fut: Future<Output = Result<T, ApiError>>,
{
    let space = resolve_space(&state, &space_room_id).await?;
    operation(state, space).await
}

/// See [`can_user_view_space`].
pub(super) async fn can_user_view_space(
    state: &RoomContext,
    space: &synapse_services::room::space::Space,
    auth_user: &OptionalAuthenticatedUser,
) -> Result<bool, ApiError> {
    if space.is_public {
        return Ok(true);
    }

    match auth_user.user_id.as_deref() {
        Some(user_id) => state.space_service.check_user_can_see_space(&space.space_id, user_id).await,
        None => Ok(false),
    }
}

/// See [`ensure_space_visible`].
pub(super) async fn ensure_space_visible(
    state: &RoomContext,
    space: &synapse_services::room::space::Space,
    auth_user: &OptionalAuthenticatedUser,
) -> Result<(), ApiError> {
    if can_user_view_space(state, space, auth_user).await? {
        return Ok(());
    }

    if auth_user.user_id.is_some() {
        Err(ApiError::forbidden("User cannot access this space"))
    } else {
        Err(ApiError::unauthorized("Authentication required for private spaces"))
    }
}

/// See [`with_visible_space`].
pub(super) async fn with_visible_space<T, F, Fut>(
    state: RoomContext,
    space_room_id: String,
    auth_user: OptionalAuthenticatedUser,
    operation: F,
) -> Result<T, ApiError>
where
    F: FnOnce(RoomContext, synapse_services::room::space::Space, OptionalAuthenticatedUser) -> Fut,
    Fut: Future<Output = Result<T, ApiError>>,
{
    let space = resolve_space(&state, &space_room_id).await?;
    ensure_space_visible(&state, &space, &auth_user).await?;
    operation(state, space, auth_user).await
}

/// See [`validate_request`].
pub(super) fn validate_request<T>(request: &T) -> Result<(), ApiError>
where
    T: Validate,
{
    request.validate().map_err(|e| ApiError::bad_request(format!("Validation error: {e}")))
}

/// See [`encode_space_member_cursor`].
pub(super) fn encode_space_member_cursor(joined_ts: i64, user_id: &str) -> String {
    BASE64.encode(format!("{}:{}", joined_ts, user_id))
}

/// See [`decode_space_member_cursor`].
pub(super) fn decode_space_member_cursor(cursor: &str) -> Option<(i64, String)> {
    let decoded = BASE64.decode(cursor).ok()?;
    let s = String::from_utf8(decoded).ok()?;
    let mut parts = s.splitn(2, ':');
    let ts = parts.next()?.parse().ok()?;
    let user_id = parts.next()?.to_string();
    Some((ts, user_id))
}

/// See [`encode_space_child_cursor`].
pub(super) fn encode_space_child_cursor(added_ts: i64, id: i64) -> String {
    BASE64.encode(format!("{}:{}", added_ts, id))
}

/// See [`decode_space_child_cursor`].
pub(super) fn decode_space_child_cursor(cursor: &str) -> Option<(i64, i64)> {
    let decoded = BASE64.decode(cursor).ok()?;
    let s = String::from_utf8(decoded).ok()?;
    let mut parts = s.splitn(2, ':');
    let ts = parts.next()?.parse().ok()?;
    let id = parts.next()?.parse().ok()?;
    Some((ts, id))
}

/// See [`create_space_router`].
///
/// ISSUE-13：空间模块此前把**整个** router 同时 nest 到 `/_matrix/client/v1` 与 `/_matrix/client/v3`，
/// 于是 22 条路径膨胀成 44 条注册条目。实际上只有 MSC2946 的 `hierarchy`（含早期形状 `hierarchy/v1`）
/// 是规范端点，其余都是项目私有扩展 —— 因此拆成两个 router：
/// * spec  → `nest` 到 `/_matrix/client/{v1,v3}`（保持不变）
/// * private → `nest` 到唯一规范位置 `/_matrix/vendor/v1`
///
/// 结果：44 条 client 条目 → 22 条 vendor 条目 + 4 条 keep。见
/// `docs/前缀命名空间治理方案-2026-10-08.md` §10.2/§10.3。
pub fn create_space_router(state: AppState) -> Router<AppState> {
    // MSC2946 spaces hierarchy：规范端点，留在 client 前缀（v1 + v3 同一份 router）。
    let spec = Router::new().merge(create_space_hierarchy_spec_routes());

    // 项目私有面：lifecycle_query / children(非 hierarchy) / membership_state / summary。
    let private = Router::new()
        .merge(create_space_lifecycle_query_routes())
        .merge(create_space_children_private_routes())
        .merge(create_space_membership_state_routes())
        .merge(create_space_summary_routes());

    Router::new()
        .nest("/_matrix/client/v1", spec.clone())
        .nest("/_matrix/client/v3", spec)
        .nest("/_matrix/vendor/v1", private)
        .with_state(state)
}

// 原 `test_space_routes_structure` 已删除：它只是断言一份**硬编码字符串列表**以某前缀开头
// （跑不到 router，永远不会红）—— 铁律 8 意义上它不是门禁。真实断言已移到
// `route_module.rs::space_manifest_splits_spec_from_private`，那里读的是派生表（真实路由面）。
