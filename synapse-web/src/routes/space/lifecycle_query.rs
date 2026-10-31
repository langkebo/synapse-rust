use super::*;
use crate::routes::context::RoomContext;
use crate::routes::extractors::MatrixPath;
use crate::routes::extractors::MatrixQuery;
use crate::routes::extractors::RoomId;
use crate::routes::MatrixJson;

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

/// See [`create_space`].
pub(super) async fn create_space(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    MatrixJson(body): MatrixJson<CreateSpaceBody>,
) -> Result<impl IntoResponse, ApiError> {
    validate_request(&body)?;
    let request: synapse_services::room::space::CreateSpaceRequest = body.into_request(auth_user.user_id.clone());

    let space: synapse_services::room::space::Space = ctx.space_service.create_space(request).await?;

    Ok(created_json_from::<_, SpaceResponse>(SpaceResponse::from(space)))
}

/// See [`get_space`].
pub(super) async fn get_space(
    State(ctx): State<RoomContext>,
    MatrixPath(space_id): MatrixPath<RoomId>,
    auth_user: OptionalAuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    with_visible_space(
        ctx,
        space_id.to_string(),
        auth_user,
        |_state, space: synapse_services::room::space::Space, _auth_user| async move {
            Ok(json_from::<_, SpaceResponse>(SpaceResponse::from(space)))
        },
    )
    .await
}

/// See [`get_space_by_room`].
pub(super) async fn get_space_by_room(
    State(ctx): State<RoomContext>,
    MatrixPath(room_id): MatrixPath<RoomId>,
    auth_user: OptionalAuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    with_visible_space(
        ctx,
        room_id.to_string(),
        auth_user,
        |_state, space: synapse_services::room::space::Space, _auth_user| async move {
            Ok(json_from::<_, SpaceResponse>(SpaceResponse::from(space)))
        },
    )
    .await
}

/// See [`update_space`].
pub(super) async fn update_space(
    State(ctx): State<RoomContext>,
    MatrixPath(space_id): MatrixPath<RoomId>,
    auth_user: AuthenticatedUser,
    MatrixJson(body): MatrixJson<UpdateSpaceBody>,
) -> Result<impl IntoResponse, ApiError> {
    validate_request(&body)?;
    let request: synapse_services::room::space::UpdateSpaceRequest = body.into_request();

    with_resolved_space(ctx, space_id.to_string(), |ctx, space: synapse_services::room::space::Space| async move {
        let space: synapse_services::room::space::Space =
            ctx.space_service.update_space(&space.space_id, &request, &auth_user.user_id).await?;

        Ok(json_from::<_, SpaceResponse>(SpaceResponse::from(space)))
    })
    .await
}

/// See [`delete_space`].
pub(super) async fn delete_space(
    State(ctx): State<RoomContext>,
    MatrixPath(space_id): MatrixPath<RoomId>,
    auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    with_resolved_space(ctx, space_id.to_string(), |ctx, space: synapse_services::room::space::Space| async move {
        ctx.space_service.delete_space(&space.space_id, &auth_user.user_id).await?;

        Ok(StatusCode::NO_CONTENT)
    })
    .await
}

/// See [`get_user_spaces`].
pub(super) async fn get_user_spaces(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    let spaces: Vec<synapse_services::room::space::Space> =
        ctx.space_service.get_user_spaces(&auth_user.user_id).await?;

    Ok(json_vec_from::<_, SpaceResponse>(spaces.into_iter().map(SpaceResponse::from).collect()))
}

/// See [`get_public_spaces`].
pub(super) async fn get_public_spaces(
    State(ctx): State<RoomContext>,
    MatrixQuery(query): MatrixQuery<PaginationQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let limit: i64 = query.limit.unwrap_or(100).clamp(1, 500);
    let cursor: Option<(i64, &str)> = decode_public_space_cursor(query.from.as_deref());
    if query.from.is_some() && cursor.is_none() {
        return Err(ApiError::bad_request("Invalid from cursor".to_string()));
    }

    let spaces: Vec<synapse_services::room::space::Space> = ctx
        .space_service
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

/// See [`search_spaces`].
pub(super) async fn search_spaces(
    State(ctx): State<RoomContext>,
    MatrixQuery(query): MatrixQuery<SearchQuery>,
    auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    let limit: i64 = query.limit.unwrap_or(10).clamp(1, 100);

    let spaces: Vec<synapse_services::room::space::Space> =
        ctx.space_service.search_spaces(&query.query, limit, Some(&auth_user.user_id)).await?;

    Ok(json_vec_from::<_, SpaceResponse>(spaces.into_iter().map(SpaceResponse::from).collect()))
}

/// See [`get_space_statistics`].
pub(super) async fn get_space_statistics(
    State(ctx): State<RoomContext>,
    MatrixQuery(query): MatrixQuery<StatisticsQuery>,
    auth_user: OptionalAuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    let limit: i64 = query.limit.unwrap_or(100).clamp(1, 500);
    let stats: Vec<serde_json::Value> = ctx.space_service.get_space_statistics(limit).await?;

    // One visibility query for the whole page (public, or joined by the caller)
    // replaces the previous per-row `get_space` + `check_user_can_see_space`.
    let space_ids: Vec<String> = stats
        .iter()
        .filter_map(|stat| stat.get("space_id").and_then(serde_json::Value::as_str))
        .map(ToOwned::to_owned)
        .collect();
    let visible = ctx.space_service.get_space_visibility_batch(&space_ids, auth_user.user_id.as_deref()).await?;

    let visible_stats: Vec<serde_json::Value> = stats
        .into_iter()
        .filter(|stat| {
            stat.get("space_id")
                .and_then(serde_json::Value::as_str)
                .and_then(|space_id| visible.get(space_id))
                .copied()
                .unwrap_or(false)
        })
        .collect();

    Ok(Json(visible_stats))
}

/// See [`create_space_lifecycle_query_routes`].
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
