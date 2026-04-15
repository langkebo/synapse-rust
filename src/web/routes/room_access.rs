use crate::web::routes::{ApiError, AppState, AuthenticatedUser};

pub(crate) async fn ensure_room_member(
    state: &AppState,
    auth_user: &AuthenticatedUser,
    room_id: &str,
    error_message: &str,
) -> Result<(), ApiError> {
    let is_member = state
        .services
        .member_storage
        .is_member(room_id, &auth_user.user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check membership: {}", e)))?;

    if !is_member {
        return Err(ApiError::forbidden(error_message.to_string()));
    }

    Ok(())
}

pub(crate) async fn ensure_room_member_or_admin(
    state: &AppState,
    auth_user: &AuthenticatedUser,
    room_id: &str,
    error_message: &str,
) -> Result<(), ApiError> {
    if auth_user.is_admin {
        return Ok(());
    }

    ensure_room_member(state, auth_user, room_id, error_message).await
}
