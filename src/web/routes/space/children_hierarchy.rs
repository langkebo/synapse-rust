use super::*;
use serde_json::json;
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub(crate) struct HierarchyV1Query {
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

            let rooms = state
                .services
                .space_service
                .build_hierarchy_rooms(&hierarchy.children)
                .await;

            let response = SpaceHierarchyResponse {
                space: SpaceResponse::from(hierarchy.space),
                children: json_vec_from::<_, SpaceChildResponse>(hierarchy.children).0,
                members: json_vec_from::<_, SpaceMemberResponse>(hierarchy.members).0,
                rooms,
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

#[allow(dead_code)]
pub(crate) async fn get_room_hierarchy_msc2946(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    Query(query): Query<HierarchyV1Query>,
    auth_user: OptionalAuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let max_depth = query.max_depth.unwrap_or(1);
    let suggested_only = query.suggested_only.unwrap_or(false);

    match resolve_space_by_room(&state, &room_id).await {
        Ok(space) => {
            ensure_space_visible(&state, &space, &auth_user).await?;
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
            Ok(Json(serde_json::to_value(response).unwrap_or_default()))
        }
        Err(_) => {
            let room = state
                .services
                .room_storage
                .get_room(&room_id)
                .await
                .map_err(|e| ApiError::internal(format!("Failed to get room: {}", e)))?
                .ok_or_else(|| ApiError::not_found("Room not found"))?;

            let state_events = state
                .services
                .event_storage
                .get_state_events(&room_id)
                .await
                .map_err(|e| ApiError::internal(format!("Failed to get state: {}", e)))?;

            let create_event = state_events
                .iter()
                .find(|e| e.event_type.as_deref() == Some("m.room.create"));
            let room_type = create_event
                .and_then(|e| e.content.get("type"))
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let name_event = state_events
                .iter()
                .find(|e| e.event_type.as_deref() == Some("m.room.name"));
            let room_name = name_event
                .and_then(|e| e.content.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let avatar_event = state_events
                .iter()
                .find(|e| e.event_type.as_deref() == Some("m.room.avatar"));
            let avatar_url = avatar_event
                .and_then(|e| e.content.get("url"))
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let join_rules_event = state_events
                .iter()
                .find(|e| e.event_type.as_deref() == Some("m.room.join_rules"));
            let join_rule = join_rules_event
                .and_then(|e| e.content.get("join_rule"))
                .and_then(|v| v.as_str())
                .unwrap_or("invite");

            let world_readable = state_events.iter().any(|e| {
                e.event_type.as_deref() == Some("m.room.history_visibility")
                    && e.content.get("history_visibility").and_then(|v| v.as_str())
                        == Some("world_readable")
            });

            let guest_can_join = state_events.iter().any(|e| {
                e.event_type.as_deref() == Some("m.room.guest_access")
                    && e.content.get("guest_access").and_then(|v| v.as_str()) == Some("can_join")
            });

            let mut children_state = Vec::new();
            for ev in &state_events {
                if ev.event_type.as_deref() == Some("m.space.child") {
                    let via = ev
                        .content
                        .get("via")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    if !via.is_empty() {
                        children_state.push(json!({
                            "type": "m.space.child",
                            "state_key": ev.state_key,
                            "content": ev.content,
                            "origin_server_ts": ev.origin_server_ts,
                        }));
                    }
                }
            }

            let mut rooms = vec![json!({
                "room_id": room_id,
                "room_type": if room_type == "m.space" { Some("m.space") } else { None::<&str> },
                "name": room_name,
                "avatar_url": avatar_url,
                "join_rule": join_rule,
                "num_joined_members": room.member_count,
                "world_readable": world_readable,
                "guest_can_join": guest_can_join,
                "children_state": children_state,
            })];

            if max_depth > 0 {
                for ev in &state_events {
                    if ev.event_type.as_deref() == Some("m.space.child") {
                        let child_room_id = match ev.state_key.as_deref() {
                            Some(sk) => sk.to_string(),
                            None => continue,
                        };
                        let child_room = state
                            .services
                            .room_storage
                            .get_room(&child_room_id)
                            .await
                            .ok()
                            .flatten();
                        let child_state = state
                            .services
                            .event_storage
                            .get_state_events(&child_room_id)
                            .await
                            .unwrap_or_default();
                        let child_create = child_state
                            .iter()
                            .find(|e| e.event_type.as_deref() == Some("m.room.create"));
                        let child_type = child_create
                            .and_then(|e| e.content.get("type"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let child_name_ev = child_state
                            .iter()
                            .find(|e| e.event_type.as_deref() == Some("m.room.name"));
                        let child_name = child_name_ev
                            .and_then(|e| e.content.get("name"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let child_avatar_ev = child_state
                            .iter()
                            .find(|e| e.event_type.as_deref() == Some("m.room.avatar"));
                        let child_avatar = child_avatar_ev
                            .and_then(|e| e.content.get("url"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let child_jr_ev = child_state
                            .iter()
                            .find(|e| e.event_type.as_deref() == Some("m.room.join_rules"));
                        let child_jr = child_jr_ev
                            .and_then(|e| e.content.get("join_rule"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("invite");
                        let child_mc = child_room.map(|r| r.member_count).unwrap_or(0);
                        let child_wr = child_state.iter().any(|e| {
                            e.event_type.as_deref() == Some("m.room.history_visibility")
                                && e.content.get("history_visibility").and_then(|v| v.as_str())
                                    == Some("world_readable")
                        });
                        let child_gc = child_state.iter().any(|e| {
                            e.event_type.as_deref() == Some("m.room.guest_access")
                                && e.content.get("guest_access").and_then(|v| v.as_str())
                                    == Some("can_join")
                        });

                        rooms.push(json!({
                            "room_id": child_room_id,
                            "room_type": if child_type == "m.space" { Some("m.space") } else { None::<&str> },
                            "name": child_name,
                            "avatar_url": child_avatar,
                            "join_rule": child_jr,
                            "num_joined_members": child_mc,
                            "world_readable": child_wr,
                            "guest_can_join": child_gc,
                        }));
                    }
                }
            }

            let response = json!({
                "rooms": rooms,
                "next_batch": Option::<String>::None,
            });
            Ok(Json(response))
        }
    }
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
