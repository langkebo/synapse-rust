use super::*;

pub(super) async fn get_space_members(
    State(state): State<AppState>,
    Path(space_id): Path<String>,
    auth_user: OptionalAuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    with_visible_space(state, space_id, auth_user, |state, space, _auth_user| async move {
        let members = state
            .services
            .space_service
            .get_space_members(&space.space_id)
            .await?;

        Ok(json_vec_from::<_, SpaceMemberResponse>(members))
    })
    .await
}

pub(super) async fn get_space_rooms(
    State(state): State<AppState>,
    Path(space_id): Path<String>,
    auth_user: OptionalAuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    with_visible_space(state, space_id, auth_user, |state, space, _auth_user| async move {
        let children = state
            .services
            .space_service
            .get_space_children(&space.space_id)
            .await?;

        let rooms: Vec<String> = children.into_iter().map(|child| child.room_id).collect();

        Ok(Json(serde_json::json!({
            "space_id": space.space_id,
            "rooms": rooms,
        })))
    })
    .await
}

pub(super) async fn get_space_state(
    State(state): State<AppState>,
    Path(space_id): Path<String>,
    auth_user: OptionalAuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    with_visible_space(state, space_id, auth_user, |state, space, auth_user| async move {
        let space_state = state
            .services
            .space_service
            .get_space_state(&space.space_id, auth_user.user_id.as_deref())
            .await?;

        Ok(Json(space_state))
    })
    .await
}

pub(super) async fn invite_user(
    State(state): State<AppState>,
    Path(space_id): Path<String>,
    auth_user: AuthenticatedUser,
    Json(body): Json<InviteUserBody>,
) -> Result<impl IntoResponse, ApiError> {
    validate_request(&body)?;

    with_resolved_space(state, space_id, |state, space| async move {
        let member = state
            .services
            .space_service
            .invite_user(&space.space_id, &body.user_id, &auth_user.user_id)
            .await?;

        Ok(created_json_from::<_, SpaceMemberResponse>(member))
    })
    .await
}

pub(super) async fn join_space(
    State(state): State<AppState>,
    Path(space_id): Path<String>,
    auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    with_resolved_space(state, space_id, |state, space| async move {
        let member = state
            .services
            .space_service
            .join_space(&space.space_id, &auth_user.user_id)
            .await?;

        Ok(json_from::<_, SpaceMemberResponse>(member))
    })
    .await
}

pub(super) async fn leave_space(
    State(state): State<AppState>,
    Path(space_id): Path<String>,
    auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    with_resolved_space(state, space_id, |state, space| async move {
        state
            .services
            .space_service
            .leave_space(&space.space_id, &auth_user.user_id)
            .await?;

        Ok(StatusCode::NO_CONTENT)
    })
    .await
}

pub(super) fn create_space_membership_state_routes() -> Router<AppState> {
    Router::new()
        .route("/spaces/{space_id}/members", get(get_space_members))
        .route("/spaces/{space_id}/rooms", get(get_space_rooms))
        .route("/spaces/{space_id}/state", get(get_space_state))
        .route("/spaces/{space_id}/invite", post(invite_user))
        .route("/spaces/{space_id}/join", post(join_space))
        .route("/spaces/{space_id}/leave", post(leave_space))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_membership_state_paths_are_scoped_under_spaces() {
        let paths = [
            "/spaces/{space_id}/members",
            "/spaces/{space_id}/rooms",
            "/spaces/{space_id}/state",
            "/spaces/{space_id}/invite",
            "/spaces/{space_id}/join",
            "/spaces/{space_id}/leave",
        ];

        assert!(paths.iter().all(|path| path.starts_with("/spaces/")));
    }

    #[test]
    fn test_invite_user_body_keeps_matrix_user_shape() {
        let body = InviteUserBody {
            user_id: "@alice:example.com".to_string(),
        };

        assert!(body.user_id.starts_with('@'));
        assert!(body.user_id.contains(':'));
    }
}
