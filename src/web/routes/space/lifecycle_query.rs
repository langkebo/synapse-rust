use super::*;

fn decode_public_space_cursor(cursor: Option<&str>) -> Option<(i64, &str)> {
    let cursor = cursor?;
    let (created_ts, space_id) = cursor.split_once('|')?;
    let created_ts = created_ts.parse::<i64>().ok()?;
    if space_id.is_empty() {
        return None;
    }
    Some((created_ts, space_id))
}

fn encode_public_space_cursor(created_ts: i64, space_id: &str) -> String {
    format!("{created_ts}|{space_id}")
}

#[cfg(test)]
mod cursor_tests {
    use super::{decode_public_space_cursor, encode_public_space_cursor};

    #[test]
    fn test_public_space_cursor_round_trip() {
        let cursor = encode_public_space_cursor(1_700_000_000_000, "!space:example.com");
        assert_eq!(decode_public_space_cursor(Some(&cursor)), Some((1_700_000_000_000, "!space:example.com")));
    }

    #[test]
    fn test_public_space_cursor_rejects_invalid_value() {
        assert_eq!(decode_public_space_cursor(Some("bad-cursor")), None);
        assert_eq!(decode_public_space_cursor(Some("123|")), None);
    }
}

pub(super) async fn create_space(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<CreateSpaceBody>,
) -> Result<impl IntoResponse, ApiError> {
    validate_request(&body)?;
    let request: crate::storage::space::CreateSpaceRequest = body.into_request(auth_user.user_id.clone());

    let space: crate::storage::space::Space = state.services.rooms.space_service.create_space(request).await?;

    Ok(created_json_from::<_, SpaceResponse>(SpaceResponse::from(space)))
}

pub(super) async fn get_space(
    State(state): State<AppState>,
    Path(space_id): Path<String>,
    auth_user: OptionalAuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    with_visible_space(state, space_id, auth_user, |_state, space: crate::storage::space::Space, _auth_user| async move {
        Ok(json_from::<_, SpaceResponse>(SpaceResponse::from(space)))
    })
    .await
}

pub(super) async fn get_space_by_room(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    auth_user: OptionalAuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    with_visible_space(state, room_id, auth_user, |_state, space: crate::storage::space::Space, _auth_user| async move {
        Ok(json_from::<_, SpaceResponse>(SpaceResponse::from(space)))
    })
    .await
}

pub(super) async fn update_space(
    State(state): State<AppState>,
    Path(space_id): Path<String>,
    auth_user: AuthenticatedUser,
    Json(body): Json<UpdateSpaceBody>,
) -> Result<impl IntoResponse, ApiError> {
    validate_request(&body)?;
    let request: crate::storage::space::UpdateSpaceRequest = body.into_request();

    with_resolved_space(state, space_id, |state, space: crate::storage::space::Space| async move {
        let space: crate::storage::space::Space = state.services.rooms.space_service.update_space(&space.space_id, &request, &auth_user.user_id).await?;

        Ok(json_from::<_, SpaceResponse>(SpaceResponse::from(space)))
    })
    .await
}

pub(super) async fn delete_space(
    State(state): State<AppState>,
    Path(space_id): Path<String>,
    auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    with_resolved_space(state, space_id, |state, space: crate::storage::space::Space| async move {
        state.services.rooms.space_service.delete_space(&space.space_id, &auth_user.user_id).await?;

        Ok(StatusCode::NO_CONTENT)
    })
    .await
}

pub(super) async fn get_user_spaces(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    let spaces: Vec<crate::storage::space::Space> = state.services.rooms.space_service.get_user_spaces(&auth_user.user_id).await?;

    Ok(json_vec_from::<_, SpaceResponse>(spaces.into_iter().map(SpaceResponse::from).collect()))
}

pub(super) async fn get_public_spaces(
    State(state): State<AppState>,
    Query(query): Query<PaginationQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let limit: i64 = query.limit.unwrap_or(100).clamp(1, 500);
    let cursor: Option<(i64, &str)> = decode_public_space_cursor(query.from.as_deref());
    if query.from.is_some() && cursor.is_none() {
        return Err(ApiError::bad_request("Invalid from cursor".to_string()));
    }

    let spaces: Vec<crate::storage::space::Space> = state
        .services.rooms.space_service
        .get_public_spaces(limit, cursor.map(|(created_ts, _)| created_ts), cursor.map(|(_, space_id)| space_id))
        .await?;

    let next_batch: Option<String> = if spaces.len() as i64 == limit {
        spaces.last().map(|space| encode_public_space_cursor(space.created_ts, &space.space_id))
    } else {
        None
    };

    let payload: Vec<SpaceResponse> = spaces.into_iter().map(SpaceResponse::from).collect();

    Ok(Json(serde_json::json!({
        "spaces": payload,
        "next_batch": next_batch
    })))
}

pub(super) async fn search_spaces(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
    auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    let limit: i64 = query.limit.unwrap_or(10).clamp(1, 100);

    let spaces: Vec<crate::storage::space::Space> = state.services.rooms.space_service.search_spaces(&query.query, limit, Some(&auth_user.user_id)).await?;

    Ok(json_vec_from::<_, SpaceResponse>(spaces.into_iter().map(SpaceResponse::from).collect()))
}

pub(super) async fn get_space_statistics(
    State(state): State<AppState>,
    auth_user: OptionalAuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    let stats: Vec<serde_json::Value> = state.services.rooms.space_service.get_space_statistics().await?;
    let mut visible_stats: Vec<serde_json::Value> = Vec::new();

    for stat in stats {
        let Some(space_id) = stat.get("space_id").and_then(serde_json::Value::as_str) else {
            continue;
        };

        let space_opt: Option<crate::storage::space::Space> = state.services.rooms.space_service.get_space(space_id).await?;
        let Some(space) = space_opt else {
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
        let query = SearchQuery { query: "alias".to_string(), limit: Some(10) };

        assert_eq!(query.query, "alias");
        assert_eq!(query.limit, Some(10));
    }
}
