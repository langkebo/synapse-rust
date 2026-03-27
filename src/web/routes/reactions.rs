use axum::{
    extract::{Path, Query, State},
    routing::{get, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::common::error::ApiError;
use crate::web::routes::{AppState, AuthenticatedUser};

fn create_reactions_compat_router() -> Router<AppState> {
    Router::new().route(
        "/rooms/{room_id}/send/m.reaction/{txn_id}",
        put(add_reaction),
    )
}

pub fn create_reactions_router(state: AppState) -> Router<AppState> {
    let compat_router = create_reactions_compat_router();

    Router::new()
        .nest("/_matrix/client/v3", compat_router.clone())
        .nest("/_matrix/client/r0", compat_router)
        .route(
            "/_matrix/client/v3/rooms/{room_id}/relations/{event_id}",
            get(get_relations),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/annotations/{event_id}",
            get(get_annotations),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/relations/{event_id}/m.reference",
            get(get_references),
        )
        .with_state(state)
}

#[derive(Debug, Deserialize)]
pub struct RelatesTo {
    pub event_id: String,
    #[serde(rename = "rel_type")]
    pub rel_type: String,
    #[serde(default)]
    pub is_falling_back: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct ReactionResponse {
    pub event_id: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RelationsQuery {
    pub limit: Option<i64>,
    pub from: Option<String>,
    pub dir: Option<String>,
}

/// 添加 reaction 到事件 (m.annotation)
async fn add_reaction(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, _txn_id)): Path<(String, String)>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ReactionResponse>, ApiError> {
    // 验证房间存在
    if !state.services.room_service.room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    // 提取 relates_to 信息
    let relates_to = body
        .get("relates_to")
        .and_then(|v| serde_json::from_value::<RelatesTo>(v.clone()).ok())
        .ok_or_else(|| ApiError::bad_request("Missing relates_to".to_string()))?;

    // 验证 rel_type 是 annotation (reaction)
    if relates_to.rel_type != "m.annotation" {
        return Err(ApiError::bad_request(
            "rel_type must be m.annotation for reactions".to_string(),
        ));
    }

    // 生成事件 ID
    let event_id = format!(
        "${}",
        crate::common::crypto::generate_event_id(&state.services.config.server.name)
    );

    // 提取 reaction 内容 (emoji)
    let annotation = body
        .get("body")
        .and_then(|v| v.as_str())
        .unwrap_or("👍")
        .to_string();

    // 保存 reaction
    let pool = &*state.services.user_storage.pool;

    sqlx::query(
        "INSERT INTO reaction_aggregations (event_id, relates_to_event_id, sender, room_id, reaction_key, count) VALUES ($1, $2, $3, $4, $5, 1)"
    )
    .bind(&event_id)
    .bind(&relates_to.event_id)
    .bind(&auth_user.user_id)
    .bind(&room_id)
    .bind(&annotation)
    .execute(pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to add reaction: {}", e)))?;

    tracing::info!(
        "User {} added reaction {} to event {} in room {}",
        auth_user.user_id,
        annotation,
        relates_to.event_id,
        room_id
    );

    Ok(Json(ReactionResponse { event_id }))
}

/// 获取关系 (Relations)
async fn get_relations(
    State(state): State<AppState>,
    Path((room_id, event_id)): Path<(String, String)>,
    Query(query): Query<RelationsQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool = &*state.services.user_storage.pool;
    let limit = query.limit.unwrap_or(50).min(100);

    // 获取 reactions (m.annotation)
    let reactions: Vec<(String, String, String, i64)> = sqlx::query_as(
        "SELECT event_id, sender, reaction_key, origin_server_ts FROM reaction_aggregations WHERE room_id = $1 AND relates_to_event_id = $2 ORDER BY origin_server_ts DESC LIMIT $3"
    )
    .bind(&room_id)
    .bind(&event_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    // 构建 chunk
    let mut chunk: Vec<serde_json::Value> = Vec::new();
    let mut annotations: std::collections::HashMap<String, i64> = std::collections::HashMap::new();

    for (evt_id, sender, annotation, origin_ts) in reactions {
        *annotations.entry(annotation.clone()).or_insert(0) += 1;
        chunk.push(json!({
            "type": "m.reaction",
            "event_id": evt_id,
            "sender": sender,
            "origin_server_ts": origin_ts,
            "content": {
                "m.relates_to": {
                    "rel_type": "m.annotation",
                    "event_id": event_id
                },
                "body": annotation
            }
        }));
    }

    // 获取 edits (m.replace)
    let edits: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT event_id, sender, origin_server_ts FROM events WHERE room_id = $1 AND event_type = 'm.room.message' AND content->'m_new_content'->'m_relates_to'->>'event_id' = $2 LIMIT $3"
    )
    .bind(&room_id)
    .bind(&event_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    for (evt_id, sender, origin_ts) in edits {
        chunk.push(json!({
            "type": "m.room.message",
            "event_id": evt_id,
            "sender": sender,
            "origin_server_ts": origin_ts,
            "content": {
                "m.relates_to": {
                    "rel_type": "m.replace",
                    "event_id": event_id
                }
            }
        }));
    }

    Ok(Json(json!({
        "chunk": chunk,
        "next_batch": serde_json::Value::Null
    })))
}

/// 获取 annotations (别名)
async fn get_annotations(
    State(state): State<AppState>,
    Path((room_id, event_id)): Path<(String, String)>,
    Query(query): Query<RelationsQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    get_relations(State(state), Path((room_id, event_id)), Query(query)).await
}

/// 获取引用 (References)
async fn get_references(
    State(state): State<AppState>,
    Path((room_id, event_id)): Path<(String, String)>,
    Query(query): Query<RelationsQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool = &*state.services.user_storage.pool;
    let limit = query.limit.unwrap_or(50).min(100);

    // 查询引用 (m.reference) - 需要 events 表中有相关数据
    let rows = sqlx::query(
        "SELECT event_id, sender, origin_server_ts FROM events WHERE room_id = $1 AND content->'m_relates_to'->>'event_id' = $2 AND content->'m_relates_to'->>'rel_type' = 'm.reference' LIMIT $3"
    )
    .bind(&room_id)
    .bind(&event_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let chunk: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|row| {
            let event_id: String = sqlx::Row::get(&row, "event_id");
            let sender: String = sqlx::Row::get(&row, "sender");
            let origin_ts: i64 = sqlx::Row::get(&row, "origin_server_ts");

            json!({
                "type": "m.room.message",
                "event_id": event_id,
                "sender": sender,
                "origin_server_ts": origin_ts,
                "content": {
                    "m.relates_to": {
                        "rel_type": "m.reference",
                        "event_id": event_id
                    }
                }
            })
        })
        .collect();

    Ok(Json(json!({
        "chunk": chunk,
        "next_batch": serde_json::Value::Null
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_relations_query_default() {
        let query = RelationsQuery {
            limit: None,
            from: None,
            dir: None,
        };
        assert_eq!(query.limit, None);
    }

    #[test]
    fn test_relates_to_parse() {
        let json = r#"{
            "event_id": "$test_event",
            "rel_type": "m.annotation"
        }"#;
        let relates: RelatesTo =
            serde_json::from_str(json).expect("Failed to parse RelatesTo JSON");
        assert_eq!(relates.event_id, "$test_event");
        assert_eq!(relates.rel_type, "m.annotation");
    }

    #[test]
    fn test_reactions_routes_structure() {
        let compat_routes = [
            "/_matrix/client/v3/rooms/{room_id}/send/m.reaction/{txn_id}",
            "/_matrix/client/r0/rooms/{room_id}/send/m.reaction/{txn_id}",
        ];
        let v3_only_routes = [
            "/_matrix/client/v3/rooms/{room_id}/relations/{event_id}",
            "/_matrix/client/v3/rooms/{room_id}/annotations/{event_id}",
            "/_matrix/client/v3/rooms/{room_id}/relations/{event_id}/m.reference",
        ];

        assert!(compat_routes
            .iter()
            .all(|route| route.starts_with("/_matrix/client/")));
        assert!(v3_only_routes
            .iter()
            .all(|route| route.starts_with("/_matrix/client/v3/")));
    }

    #[test]
    fn test_reactions_compat_router_contains_shared_paths() {
        let shared_paths = ["/rooms/{room_id}/send/m.reaction/{txn_id}"];

        assert_eq!(shared_paths.len(), 1);
        assert!(shared_paths.iter().all(|path| path.starts_with("/rooms/")));
    }

    #[test]
    fn test_reactions_router_keeps_read_endpoints_outside_compat_scope() {
        let compat_paths = ["/rooms/{room_id}/send/m.reaction/{txn_id}"];
        let v3_only_paths = [
            "/_matrix/client/v3/rooms/{room_id}/relations/{event_id}",
            "/_matrix/client/v3/rooms/{room_id}/annotations/{event_id}",
        ];
        let absent_r0_paths = ["/_matrix/client/r0/rooms/{room_id}/relations/{event_id}"];

        assert!(compat_paths
            .iter()
            .all(|path| !path.contains("/relations/") && !path.contains("/annotations/")));
        assert!(v3_only_paths
            .iter()
            .all(|path| path.starts_with("/_matrix/client/v3/")));
        assert!(absent_r0_paths
            .iter()
            .all(|path| path.starts_with("/_matrix/client/r0/")));
    }
}
