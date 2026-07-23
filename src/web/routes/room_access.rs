use crate::web::routes::context::{AdminContext, RoomContext};
use crate::web::routes::{ApiError, AppState, AuthenticatedUser};
use std::sync::Arc;

// =============================================================================
// Core helpers — take room_service directly to avoid depending on a specific
// state type. Both AppState and RoomContext wrappers delegate here.
// =============================================================================

async fn is_member_via(
    room_service: &Arc<dyn synapse_services::RoomServiceApi>,
    user_id: &str,
    room_id: &str,
) -> Result<bool, ApiError> {
    let membership = room_service.membership().get_room_membership(room_id, user_id).await?;
    Ok(membership.is_some_and(|m| m == "join"))
}

// =============================================================================
// AppState-based helpers — retained for backward compatibility with handlers
// that still use State<AppState>.
// =============================================================================

pub(crate) async fn is_joined_room_member(state: &AppState, user_id: &str, room_id: &str) -> Result<bool, ApiError> {
    is_member_via(&state.services.rooms.room_service, user_id, room_id).await
}

#[allow(dead_code)]
pub(crate) async fn is_joined_room_member_or_creator(
    state: &AppState,
    user_id: &str,
    room_id: &str,
    creator_user_id: Option<&str>,
) -> Result<bool, ApiError> {
    if creator_user_id == Some(user_id) {
        return Ok(true);
    }

    is_joined_room_member(state, user_id, room_id).await
}

// =============================================================================
// RoomContext-based helpers — used by handlers migrated to State<RoomContext>.
// =============================================================================

pub(crate) async fn is_member_ctx(ctx: &RoomContext, user_id: &str, room_id: &str) -> Result<bool, ApiError> {
    is_member_via(&ctx.room_service, user_id, room_id).await
}

pub(crate) async fn is_member_or_creator_ctx(
    ctx: &RoomContext,
    user_id: &str,
    room_id: &str,
    creator_user_id: Option<&str>,
) -> Result<bool, ApiError> {
    if creator_user_id == Some(user_id) {
        return Ok(true);
    }
    is_member_ctx(ctx, user_id, room_id).await
}

pub(crate) async fn ensure_room_member_ctx(
    ctx: &RoomContext,
    auth_user: &AuthenticatedUser,
    room_id: &str,
    error_message: &str,
) -> Result<(), ApiError> {
    if auth_user.is_admin {
        return Ok(());
    }
    let is_member = is_member_via(&ctx.room_service, &auth_user.user_id, room_id).await?;
    if !is_member {
        return Err(ApiError::forbidden(error_message.to_string()));
    }
    Ok(())
}

pub(crate) async fn ensure_room_member_strict_ctx(
    ctx: &RoomContext,
    auth_user: &AuthenticatedUser,
    room_id: &str,
    error_message: &str,
) -> Result<(), ApiError> {
    let is_member = is_member_via(&ctx.room_service, &auth_user.user_id, room_id).await?;
    if !is_member {
        return Err(ApiError::forbidden(error_message.to_string()));
    }
    Ok(())
}

// =============================================================================
// AdminContext-based helpers — used by handlers migrated to State<AdminContext>.
// =============================================================================

#[allow(dead_code)]
pub(crate) async fn is_joined_room_member_admin(
    ctx: &AdminContext,
    user_id: &str,
    room_id: &str,
) -> Result<bool, ApiError> {
    is_member_via(&ctx.room_service, user_id, room_id).await
}

pub(crate) async fn ensure_room_member_admin(
    ctx: &AdminContext,
    auth_user: &AuthenticatedUser,
    room_id: &str,
    error_message: &str,
) -> Result<(), ApiError> {
    if auth_user.is_admin {
        return Ok(());
    }
    let is_member = is_member_via(&ctx.room_service, &auth_user.user_id, room_id).await?;
    if !is_member {
        return Err(ApiError::forbidden(error_message.to_string()));
    }
    Ok(())
}

pub(crate) async fn ensure_room_member_strict_admin(
    ctx: &AdminContext,
    auth_user: &AuthenticatedUser,
    room_id: &str,
    error_message: &str,
) -> Result<(), ApiError> {
    let is_member = is_member_via(&ctx.room_service, &auth_user.user_id, room_id).await?;
    if !is_member {
        return Err(ApiError::forbidden(error_message.to_string()));
    }
    Ok(())
}

// =============================================================================
// RoomService-based helpers — for callers that have room_service directly.
// =============================================================================

#[allow(dead_code)]
pub(crate) async fn is_joined_room_member_svc(
    room_service: &Arc<dyn synapse_services::RoomServiceApi>,
    user_id: &str,
    room_id: &str,
) -> Result<bool, ApiError> {
    is_member_via(room_service, user_id, room_id).await
}
