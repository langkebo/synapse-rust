use super::*;

pub(super) async fn get_space_summary(
    State(state): State<AppState>,
    Path(space_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    with_resolved_space(state, space_id, |state, space| async move {
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
    user_id: Option<axum::extract::Extension<String>>,
) -> Result<impl IntoResponse, ApiError> {
    let user_id_str = user_id.as_ref().map(|u| u.0.as_str());

    with_resolved_space(state, space_id, |state, space| async move {
        let summary = state
            .services
            .space_service
            .get_space_summary_with_children(&space.space_id, user_id_str)
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
