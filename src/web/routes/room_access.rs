use crate::web::routes::{ApiError, AppState, AuthenticatedUser};

pub(crate) async fn is_joined_room_member(
    state: &AppState,
    user_id: &str,
    room_id: &str,
) -> Result<bool, ApiError> {
    state
        .services
        .member_storage
        .is_member(room_id, user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check membership: {}", e)))
}

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

pub(crate) async fn ensure_room_member(
    state: &AppState,
    auth_user: &AuthenticatedUser,
    room_id: &str,
    error_message: &str,
) -> Result<(), ApiError> {
    if auth_user.is_admin {
        return Ok(());
    }

    let is_member = is_joined_room_member(state, &auth_user.user_id, room_id).await?;

    if !is_member {
        return Err(ApiError::forbidden(error_message.to_string()));
    }

    Ok(())
}
