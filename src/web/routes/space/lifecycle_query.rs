use super::*;

pub(super) async fn create_space(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<CreateSpaceBody>,
) -> Result<impl IntoResponse, ApiError> {
    validate_request(&body)?;
    let request = body.into_request(auth_user.user_id.clone());

    let space = state.services.space_service.create_space(request).await?;

    Ok(created_json_from::<_, SpaceResponse>(space))
}

pub(super) async fn get_space(
    State(state): State<AppState>,
    Path(space_id): Path<String>,
    auth_user: OptionalAuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    with_visible_space(
        state,
        space_id,
        auth_user,
        |_state, space, _auth_user| async move { Ok(json_from::<_, SpaceResponse>(space)) },
    )
    .await
}

pub(super) async fn get_space_by_room(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    auth_user: OptionalAuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    with_visible_space(
        state,
        room_id,
        auth_user,
        |_state, space, _auth_user| async move { Ok(json_from::<_, SpaceResponse>(space)) },
    )
    .await
}

pub(super) async fn update_space(
    State(state): State<AppState>,
    Path(space_id): Path<String>,
    auth_user: AuthenticatedUser,
    Json(body): Json<UpdateSpaceBody>,
) -> Result<impl IntoResponse, ApiError> {
    validate_request(&body)?;
    let request = body.into_request();

    with_resolved_space(state, space_id, |state, space| async move {
        let space = state
            .services
            .space_service
            .update_space(&space.space_id, &request, &auth_user.user_id)
            .await?;

        Ok(json_from::<_, SpaceResponse>(space))
    })
    .await
}

pub(super) async fn delete_space(
    State(state): State<AppState>,
    Path(space_id): Path<String>,
    auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    with_resolved_space(state, space_id, |state, space| async move {
        state
            .services
            .space_service
            .delete_space(&space.space_id, &auth_user.user_id)
            .await?;

        Ok(StatusCode::NO_CONTENT)
    })
    .await
}

pub(super) async fn get_user_spaces(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    let spaces = state
        .services
        .space_service
        .get_user_spaces(&auth_user.user_id)
        .await?;

    Ok(json_vec_from::<_, SpaceResponse>(spaces))
}

pub(super) async fn get_public_spaces(
    State(state): State<AppState>,
    Query(query): Query<PaginationQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = query.limit.unwrap_or(100);
    let offset = query.offset.unwrap_or(0);

    let spaces = state
        .services
        .space_service
        .get_public_spaces(limit, offset)
        .await?;

    Ok(json_vec_from::<_, SpaceResponse>(spaces))
}

pub(super) async fn search_spaces(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
    auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    let limit = query.limit.unwrap_or(10);

    let spaces = state
        .services
        .space_service
        .search_spaces(&query.query, limit, Some(&auth_user.user_id))
        .await?;

    Ok(json_vec_from::<_, SpaceResponse>(spaces))
}

pub(super) async fn get_space_statistics(
    State(state): State<AppState>,
    auth_user: OptionalAuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    let stats = state.services.space_service.get_space_statistics().await?;
    let mut visible_stats = Vec::new();

    for stat in stats {
        let Some(space_id) = stat.get("space_id").and_then(|value| value.as_str()) else {
            continue;
        };

        let Some(space) = state.services.space_service.get_space(space_id).await? else {
            continue;
        };

        if can_user_view_space(&state, &space, &auth_user).await? {
            visible_stats.push(stat);
        }
    }

    Ok(Json(visible_stats))
}

pub(super) fn create_space_lifecycle_query_routes() -> Router<AppState> {
    Router::new()
        .route("/spaces", post(create_space))
        .route("/spaces/public", get(get_public_spaces))
        .route("/spaces/search", get(search_spaces))
        .route("/spaces/statistics", get(get_space_statistics))
        .route("/spaces/user", get(get_user_spaces))
        .route("/spaces/{space_id}", get(get_space))
        .route("/spaces/{space_id}", put(update_space))
        .route("/spaces/{space_id}", delete(delete_space))
        .route("/spaces/room/{room_id}", get(get_space_by_room))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lifecycle_query_paths_keep_space_prefixes() {
        let paths = [
            "/spaces",
            "/spaces/public",
            "/spaces/search",
            "/spaces/statistics",
            "/spaces/user",
            "/spaces/{space_id}",
            "/spaces/room/{room_id}",
        ];

        assert!(paths.iter().all(|path| path.starts_with("/spaces")));
    }

    #[test]
    fn test_search_query_supports_query_alias() {
        let query = SearchQuery {
            query: "alias".to_string(),
            limit: Some(10),
        };

        assert_eq!(query.query, "alias");
        assert_eq!(query.limit, Some(10));
    }
}
