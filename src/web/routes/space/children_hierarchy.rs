use super::*;
use crate::web::routes::context::RoomContext;
use crate::web::routes::extractors::RoomId;

/// The `HierarchyV1Query` struct.
#[derive(Debug, Deserialize)]
pub(crate) struct HierarchyV1Query {
    /// The `max_depth` field.
    pub max_depth: Option<i32>,
    /// The `suggested_only` field.
    pub suggested_only: Option<bool>,
    /// The `limit` field.
    pub limit: Option<i32>,
    /// The `from` field.
    pub from: Option<String>,
}

/// See [`get_space_children`].
pub(super) async fn get_space_children(
    State(ctx): State<RoomContext>,
    Path(space_id): Path<RoomId>,
    auth_user: OptionalAuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    with_visible_space(ctx, space_id.to_string(), auth_user, |ctx, space, _auth_user| async move {
        let children = ctx.space_service.get_space_children(&space.space_id).await?;

        Ok(json_vec_from::<_, SpaceChildResponse>(children))
    })
    .await
}

/// See [`add_child`].
pub(super) async fn add_child(
    State(ctx): State<RoomContext>,
    Path(space_id): Path<RoomId>,
    auth_user: AuthenticatedUser,
    Json(body): Json<AddChildBody>,
) -> Result<impl IntoResponse, ApiError> {
    validate_request(&body)?;

    with_resolved_space(ctx, space_id.to_string(), |ctx, space| async move {
        let request = body.into_request(space.space_id, auth_user.user_id.clone());

        let child = ctx.space_service.add_child(request).await?;

        Ok(created_json_from::<_, SpaceChildResponse>(child))
    })
    .await
}

/// See [`remove_child`].
pub(super) async fn remove_child(
    State(ctx): State<RoomContext>,
    Path((space_id, room_id)): Path<(String, String)>,
    auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    with_resolved_space(ctx, space_id, |ctx, space| async move {
        ctx.space_service.remove_child(&space.space_id, &room_id, &auth_user.user_id).await?;

        Ok(StatusCode::NO_CONTENT)
    })
    .await
}

/// See [`get_space_hierarchy`].
pub(super) async fn get_space_hierarchy(
    State(ctx): State<RoomContext>,
    Path(space_id): Path<RoomId>,
    Query(query): Query<HierarchyQuery>,
    auth_user: OptionalAuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    let max_depth = query.max_depth.unwrap_or(1).clamp(1, 10);

    with_visible_space(ctx, space_id.to_string(), auth_user, |ctx, space, _auth_user| async move {
        let hierarchy = ctx.space_service.get_space_hierarchy(&space.space_id, max_depth).await?;

        let rooms = ctx.space_service.build_hierarchy_rooms(&hierarchy.children).await;

        let response = SpaceHierarchyResponse {
            space: SpaceResponse::from(hierarchy.space),
            children: json_vec_from::<_, SpaceChildResponse>(hierarchy.children).0,
            members: json_vec_from::<_, SpaceMemberResponse>(hierarchy.members).0,
            rooms,
        };

        Ok(Json(response))
    })
    .await
}

/// See [`get_space_hierarchy_v1`].
pub(super) async fn get_space_hierarchy_v1(
    State(ctx): State<RoomContext>,
    Path(space_id): Path<RoomId>,
    Query(query): Query<HierarchyV1Query>,
    auth_user: OptionalAuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    let max_depth = query.max_depth.unwrap_or(1);
    let suggested_only = query.suggested_only.unwrap_or(false);

    with_visible_space(ctx, space_id.to_string(), auth_user, |ctx, space, auth_user| async move {
        let response = ctx
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
    })
    .await
}

/// See [`get_parent_spaces`].
pub(super) async fn get_parent_spaces(
    State(ctx): State<RoomContext>,
    Path(room_id): Path<RoomId>,
    auth_user: OptionalAuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    let spaces = ctx.space_service.get_parent_spaces(&room_id).await?;

    let mut visible_spaces = Vec::new();
    for space in spaces {
        if can_user_view_space(&ctx, &space, &auth_user).await? {
            visible_spaces.push(space);
        }
    }

    Ok(json_vec_from::<_, SpaceResponse>(visible_spaces))
}

/// See [`get_space_tree_path`].
pub(super) async fn get_space_tree_path(
    State(ctx): State<RoomContext>,
    Path(space_id): Path<RoomId>,
    auth_user: OptionalAuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    with_visible_space(ctx, space_id.to_string(), auth_user, |ctx, space, auth_user| async move {
        let path = ctx.space_service.get_space_tree_path(&space.space_id).await?;

        let mut visible_path = Vec::new();
        for ancestor in path {
            if can_user_view_space(&ctx, &ancestor, &auth_user).await? {
                visible_path.push(ancestor);
            }
        }

        Ok(json_vec_from::<_, SpaceResponse>(visible_path))
    })
    .await
}

/// See [`create_space_children_hierarchy_routes`].
pub(super) fn create_space_children_hierarchy_routes() -> Router<AppState> {
    Router::new()
        .route("/spaces/{space_id}/children", get(get_space_children))
        .route("/spaces/{space_id}/children", post(add_child))
        .route("/spaces/{space_id}/children/{room_id}", delete(remove_child))
        .route("/spaces/{space_id}/hierarchy", get(get_space_hierarchy))
        .route("/spaces/{space_id}/hierarchy/v1", get(get_space_hierarchy_v1))
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
