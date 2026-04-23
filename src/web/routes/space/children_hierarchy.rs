use super::*;

#[derive(Debug, Deserialize)]
pub(super) struct HierarchyV1Query {
    pub max_depth: Option<i32>,
    pub suggested_only: Option<bool>,
    pub limit: Option<i32>,
    pub from: Option<String>,
}

pub(super) async fn get_space_children(
    State(state): State<AppState>,
    Path(space_id): Path<String>,
    auth_user: OptionalAuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    with_visible_space(
        state,
        space_id,
        auth_user,
        |state, space, _auth_user| async move {
            let children = state
                .services
                .space_service
                .get_space_children(&space.space_id)
                .await?;

            Ok(json_vec_from::<_, SpaceChildResponse>(children))
        },
    )
    .await
}

pub(super) async fn add_child(
    State(state): State<AppState>,
    Path(space_id): Path<String>,
    auth_user: AuthenticatedUser,
    Json(body): Json<AddChildBody>,
) -> Result<impl IntoResponse, ApiError> {
    validate_request(&body)?;

    with_resolved_space(state, space_id, |state, space| async move {
        let request = body.into_request(space.space_id, auth_user.user_id.clone());

        let child = state.services.space_service.add_child(request).await?;

        Ok(created_json_from::<_, SpaceChildResponse>(child))
    })
    .await
}

pub(super) async fn remove_child(
    State(state): State<AppState>,
    Path((space_id, room_id)): Path<(String, String)>,
    auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    with_resolved_space(state, space_id, |state, space| async move {
        state
            .services
            .space_service
            .remove_child(&space.space_id, &room_id, &auth_user.user_id)
            .await?;

        Ok(StatusCode::NO_CONTENT)
    })
    .await
}

pub(super) async fn get_space_hierarchy(
    State(state): State<AppState>,
    Path(space_id): Path<String>,
    Query(query): Query<HierarchyQuery>,
    auth_user: OptionalAuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    let max_depth = query.max_depth.unwrap_or(1);

    with_visible_space(
        state,
        space_id,
        auth_user,
        |state, space, _auth_user| async move {
            let hierarchy = state
                .services
                .space_service
                .get_space_hierarchy(&space.space_id, max_depth)
                .await?;

            let response = SpaceHierarchyResponse {
                space: SpaceResponse::from(hierarchy.space),
                children: json_vec_from::<_, SpaceChildResponse>(hierarchy.children).0,
                members: json_vec_from::<_, SpaceMemberResponse>(hierarchy.members).0,
            };

            Ok(Json(response))
        },
    )
    .await
}

pub(super) async fn get_space_hierarchy_v1(
    State(state): State<AppState>,
    Path(space_id): Path<String>,
    Query(query): Query<HierarchyV1Query>,
    auth_user: OptionalAuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    let max_depth = query.max_depth.unwrap_or(1);
    let suggested_only = query.suggested_only.unwrap_or(false);

    with_visible_space(
        state,
        space_id,
        auth_user,
        |state, space, auth_user| async move {
            let response = state
                .services
                .space_service
                .get_space_hierarchy_v1(
                    &space.space_id,
                    max_depth,
                    suggested_only,
                    query.limit,
                    query.from.as_deref(),
                    auth_user.user_id.as_deref(),
                )
                .await?;

            Ok(Json(response))
        },
    )
    .await
}

pub(super) async fn get_parent_spaces(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    auth_user: OptionalAuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    let spaces = state
        .services
        .space_service
        .get_parent_spaces(&room_id)
        .await?;

    let mut visible_spaces = Vec::new();
    for space in spaces {
        if can_user_view_space(&state, &space, &auth_user).await? {
            visible_spaces.push(space);
        }
    }

    Ok(json_vec_from::<_, SpaceResponse>(visible_spaces))
}

pub(super) async fn get_space_tree_path(
    State(state): State<AppState>,
    Path(space_id): Path<String>,
    auth_user: OptionalAuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    with_visible_space(
        state,
        space_id,
        auth_user,
        |state, space, auth_user| async move {
            let path = state
                .services
                .space_service
                .get_space_tree_path(&space.space_id)
                .await?;

            let mut visible_path = Vec::new();
            for ancestor in path {
                if can_user_view_space(&state, &ancestor, &auth_user).await? {
                    visible_path.push(ancestor);
                }
            }

            Ok(json_vec_from::<_, SpaceResponse>(visible_path))
        },
    )
    .await
}

pub(super) fn create_space_children_hierarchy_routes() -> Router<AppState> {
    Router::new()
        .route("/spaces/{space_id}/children", get(get_space_children))
        .route("/spaces/{space_id}/children", post(add_child))
        .route(
            "/spaces/{space_id}/children/{room_id}",
            delete(remove_child),
        )
        .route("/spaces/{space_id}/hierarchy", get(get_space_hierarchy))
        .route(
            "/spaces/{space_id}/hierarchy/v1",
            get(get_space_hierarchy_v1),
        )
        .route("/spaces/{space_id}/tree_path", get(get_space_tree_path))
        .route("/spaces/room/{room_id}/parents", get(get_parent_spaces))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_children_hierarchy_route_paths_are_scoped_under_space_domain() {
        let paths = [
            "/spaces/{space_id}/children",
            "/spaces/{space_id}/children/{room_id}",
            "/spaces/{space_id}/hierarchy",
            "/spaces/{space_id}/hierarchy/v1",
            "/spaces/{space_id}/tree_path",
            "/spaces/room/{room_id}/parents",
        ];

        assert!(paths.iter().all(|path| path.starts_with("/spaces/")));
    }

    #[test]
    fn test_hierarchy_v1_query_supports_pagination_fields() {
        let query = HierarchyV1Query {
            max_depth: Some(3),
            suggested_only: Some(true),
            limit: Some(20),
            from: Some("!room:example.com".to_string()),
        };

        assert_eq!(query.max_depth, Some(3));
        assert_eq!(query.limit, Some(20));
        assert!(query.suggested_only.unwrap());
    }
}
