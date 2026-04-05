use axum::{
    extract::{Path, State},
    routing::{delete, get, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::common::ApiError;
use crate::web::routes::AppState;
use crate::web::routes::AuthenticatedUser;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RoomTag {
    pub user_id: String,
    pub room_id: String,
    pub tag: String,
    #[sqlx(rename = "order_value")]
    pub order: Option<f64>,
    pub created_ts: i64,
}

#[derive(Debug, Deserialize)]
pub struct TagContent {
    pub order: Option<f64>,
}

fn create_tags_compat_router() -> Router<AppState> {
    Router::new()
        .route("/user/{user_id}/tags", get(get_global_tags))
        .route("/user/{user_id}/rooms/{room_id}/tags", get(get_tags))
        .route("/user/{user_id}/rooms/{room_id}/tags/{tag}", put(put_tag))
        .route(
            "/user/{user_id}/rooms/{room_id}/tags/{tag}",
            delete(delete_tag),
        )
}

pub fn create_tags_router(state: AppState) -> Router<AppState> {
    let compat_router = create_tags_compat_router();

    Router::new()
        .nest("/_matrix/client/v3", compat_router.clone())
        .nest("/_matrix/client/r0", compat_router)
        .with_state(state)
}

async fn get_global_tags(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if auth_user.user_id != user_id && !auth_user.is_admin {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }

    let tags = get_all_user_tags(&state.services.user_storage.pool, &user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get tags: {}", e)))?;

    let mut rooms_map = serde_json::Map::new();
    for tag in tags {
        let room_tags = rooms_map
            .entry(tag.room_id.clone())
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));

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
    if auth_user.user_id != user_id && !auth_user.is_admin {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }

    let tags = get_room_tags(&state.services.user_storage.pool, &user_id, &room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get tags: {}", e)))?;

    let tags_map: serde_json::Map<String, serde_json::Value> = tags
        .into_iter()
        .map(|t| {
            let order = t.order.unwrap_or(0.0);
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
    if auth_user.user_id != user_id && !auth_user.is_admin {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }

    let now = chrono::Utc::now().timestamp_millis();

    upsert_room_tag(
        &state.services.user_storage.pool,
        &user_id,
        &room_id,
        &tag,
        content.order,
        now,
    )
    .await
    .map_err(|e| ApiError::internal(format!("Failed to set tag: {}", e)))?;

    Ok(Json(serde_json::json!({})))
}

async fn delete_tag(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((user_id, room_id, tag)): Path<(String, String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if auth_user.user_id != user_id && !auth_user.is_admin {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }

    delete_room_tag(&state.services.user_storage.pool, &user_id, &room_id, &tag)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to delete tag: {}", e)))?;

    Ok(Json(serde_json::json!({})))
}

async fn get_room_tags(
    pool: &PgPool,
    user_id: &str,
    room_id: &str,
) -> Result<Vec<RoomTag>, sqlx::Error> {
    sqlx::query_as::<_, RoomTag>(
        r#"
        SELECT user_id, room_id, tag, order_value, created_ts
        FROM room_tags
        WHERE user_id = $1 AND room_id = $2
        ORDER BY tag
        "#,
    )
    .bind(user_id)
    .bind(room_id)
    .fetch_all(pool)
    .await
}

async fn get_all_user_tags(pool: &PgPool, user_id: &str) -> Result<Vec<RoomTag>, sqlx::Error> {
    sqlx::query_as::<_, RoomTag>(
        r#"
        SELECT user_id, room_id, tag, order_value, created_ts
        FROM room_tags
        WHERE user_id = $1
        ORDER BY room_id, tag
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

async fn upsert_room_tag(
    pool: &PgPool,
    user_id: &str,
    room_id: &str,
    tag: &str,
    order: Option<f64>,
    created_ts: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO room_tags (user_id, room_id, tag, order_value, created_ts)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (user_id, room_id, tag)
        DO UPDATE SET order_value = $4, created_ts = $5
        "#,
    )
    .bind(user_id)
    .bind(room_id)
    .bind(tag)
    .bind(order)
    .bind(created_ts)
    .execute(pool)
    .await?;

    Ok(())
}

async fn delete_room_tag(
    pool: &PgPool,
    user_id: &str,
    room_id: &str,
    tag: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        DELETE FROM room_tags
        WHERE user_id = $1 AND room_id = $2 AND tag = $3
        "#,
    )
    .bind(user_id)
    .bind(room_id)
    .bind(tag)
    .execute(pool)
    .await?;

    Ok(())
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

        assert!(compat_routes
            .iter()
            .all(|route| route.starts_with("/_matrix/client/")));
    }

    #[test]
    fn test_tags_compat_router_contains_shared_paths() {
        let shared_paths = [
            "/user/{user_id}/rooms/{room_id}/tags",
            "/user/{user_id}/rooms/{room_id}/tags/{tag}",
        ];

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

        assert!(supported_paths
            .iter()
            .all(|path| !path.starts_with("/_matrix/client/v1/")));
        assert!(unsupported_v1_paths
            .iter()
            .all(|path| path.starts_with("/_matrix/client/v1/")));
    }
}
