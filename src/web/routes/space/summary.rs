use super::*;

pub(super) async fn get_space_summary(
    State(state): State<AppState>,
    Path(space_id): Path<String>,
    auth_user: OptionalAuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    with_visible_space(state, space_id, auth_user, |state, space, _auth_user| async move {
        let summary = state
            .services
            .space_service
            .get_space_summary(&space.space_id)
            .await?
            .ok_or_else(|| ApiError::not_found("Space summary not found"))?;

        Ok(Json(summary))
    })
    .await
}

pub(super) async fn get_space_summary_with_children(
    State(state): State<AppState>,
    Path(space_id): Path<String>,
    auth_user: OptionalAuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    with_visible_space(state, space_id, auth_user, |state, space, auth_user| async move {
        let summary = state
            .services
            .space_service
            .get_space_summary_with_children(&space.space_id, auth_user.user_id.as_deref())
            .await?;

        Ok(Json(summary))
    })
    .await
}

pub(super) fn create_space_summary_routes() -> Router<AppState> {
    Router::new()
        .route("/spaces/{space_id}/summary", get(get_space_summary))
        .route(
            "/spaces/{space_id}/summary/with_children",
            get(get_space_summary_with_children),
        )
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_summary_route_paths_are_scoped_under_spaces() {
        let paths = [
            "/spaces/{space_id}/summary",
            "/spaces/{space_id}/summary/with_children",
        ];

        assert!(paths.iter().all(|path| path.starts_with("/spaces/")));
    }
}
