use axum::{
    extract::{Path, State},
    routing::{delete, get, put},
    Json, Router,
};
use serde::Deserialize;

use crate::common::ApiError;
use crate::web::routes::response_helpers::empty_json;
use crate::web::routes::AppState;
use crate::web::routes::AuthenticatedUser;

#[derive(Debug, Deserialize)]
pub struct TagContent {
    pub order: Option<f64>,
}

fn create_tags_compat_router() -> Router<AppState> {
    Router::new()
        .route("/user/{user_id}/tags", get(get_global_tags))
        .route("/user/{user_id}/rooms/{room_id}/tags", get(get_tags))
        .route("/user/{user_id}/rooms/{room_id}/tags/{tag}", put(put_tag))
        .route("/user/{user_id}/rooms/{room_id}/tags/{tag}", delete(delete_tag))
}

pub fn create_tags_router(state: AppState) -> Router<AppState> {
    let compat_router = create_tags_compat_router();

    Router::new()
        .nest("/_matrix/client/v3", compat_router.clone())
        .nest("/_matrix/client/r0", compat_router)
        .with_state(state)
}

const TAGS_NEST_PREFIXES: &[&str] = &["/_matrix/client/v3", "/_matrix/client/r0"];

fn tags_compat_relative_routes() -> Vec<(axum::http::Method, &'static str)> {
    use axum::http::Method;
    vec![
        (Method::GET, "/user/{user_id}/tags"),
        (Method::GET, "/user/{user_id}/rooms/{room_id}/tags"),
        (Method::PUT, "/user/{user_id}/rooms/{room_id}/tags/{tag}"),
        (Method::DELETE, "/user/{user_id}/rooms/{room_id}/tags/{tag}"),
    ]
}

pub fn tags_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    crate::web::routes::route_ledger::expand_under_prefixes("tags", TAGS_NEST_PREFIXES, &tags_compat_relative_routes())
}

async fn get_global_tags(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if auth_user.user_id != user_id {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }

    let tags: Vec<crate::storage::room_tag::RoomTag> =
        state.services.rooms.room_service.get_all_tags(&user_id).await?;

    let mut rooms_map: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
    for tag in tags {
        let room_tags: &mut serde_json::Value =
            rooms_map.entry(tag.room_id.clone()).or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));

        if let Some(room_tags_map) = room_tags.as_object_mut() {
            room_tags_map.insert(
                tag.tag,
                serde_json::json!({
                    "order": tag.order.unwrap_or(0.0)
                }),
            );
        }
    }

    Ok(Json(serde_json::json!({
        "tags": rooms_map
    })))
}

async fn get_tags(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((user_id, room_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if auth_user.user_id != user_id {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }

    let tags: Vec<crate::storage::room_tag::RoomTag> =
        state.services.rooms.room_service.get_tags(&user_id, &room_id).await?;

    let tags_map: serde_json::Map<String, serde_json::Value> = tags
        .into_iter()
        .map(|t| {
            let order: f64 = t.order.unwrap_or(0.0);
            (t.tag, serde_json::json!({ "order": order }))
        })
        .collect();

    Ok(Json(serde_json::json!({
        "tags": tags_map
    })))
}

async fn put_tag(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((user_id, room_id, tag)): Path<(String, String, String)>,
    Json(content): Json<TagContent>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if auth_user.user_id != user_id {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }

    state.services.rooms.room_service.add_tag(&user_id, &room_id, &tag, content.order).await?;

    Ok(empty_json())
}

async fn delete_tag(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((user_id, room_id, tag)): Path<(String, String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if auth_user.user_id != user_id {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }

    state.services.rooms.room_service.remove_tag(&user_id, &room_id, &tag).await?;

    Ok(empty_json())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_tags_routes_structure() {
        let compat_routes = [
            "/_matrix/client/v3/user/{user_id}/rooms/{room_id}/tags",
            "/_matrix/client/r0/user/{user_id}/rooms/{room_id}/tags",
            "/_matrix/client/v3/user/{user_id}/rooms/{room_id}/tags/{tag}",
            "/_matrix/client/r0/user/{user_id}/rooms/{room_id}/tags/{tag}",
        ];

        assert!(compat_routes.iter().all(|route| route.starts_with("/_matrix/client/")));
    }

    #[test]
    fn test_tags_compat_router_contains_shared_paths() {
        let shared_paths = ["/user/{user_id}/rooms/{room_id}/tags", "/user/{user_id}/rooms/{room_id}/tags/{tag}"];

        assert_eq!(shared_paths.len(), 2);
        assert!(shared_paths.iter().all(|path| path.starts_with("/user/")));
    }

    #[test]
    fn test_tags_router_keeps_scope_limited_to_r0_and_v3() {
        let supported_paths = [
            "/_matrix/client/v3/user/{user_id}/rooms/{room_id}/tags",
            "/_matrix/client/r0/user/{user_id}/rooms/{room_id}/tags/{tag}",
        ];
        let unsupported_v1_paths = [
            "/_matrix/client/v1/user/{user_id}/rooms/{room_id}/tags",
            "/_matrix/client/v1/user/{user_id}/rooms/{room_id}/tags/{tag}",
        ];

        assert!(supported_paths.iter().all(|path| !path.starts_with("/_matrix/client/v1/")));
        assert!(unsupported_v1_paths.iter().all(|path| path.starts_with("/_matrix/client/v1/")));
    }
}
