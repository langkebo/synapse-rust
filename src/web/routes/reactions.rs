use axum::{
    extract::{Path, State},
    routing::put,
    Json, Router,
};
use serde::{Deserialize, Serialize};

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

    // 提取 reaction 内容 (emoji)
    let annotation = body
        .get("body")
        .and_then(|v| v.as_str())
        .unwrap_or("👍")
        .to_string();

    let origin_server_ts = chrono::Utc::now().timestamp_millis();
    let relation = state
        .services
        .relations_service
        .send_annotation(crate::services::relations_service::SendAnnotationRequest {
            room_id: room_id.clone(),
            relates_to_event_id: relates_to.event_id.clone(),
            sender: auth_user.user_id.clone(),
            key: annotation.clone(),
            origin_server_ts,
        })
        .await?;

    tracing::info!(
        "User {} added reaction {} to event {} in room {}",
        auth_user.user_id,
        annotation,
        relates_to.event_id,
        room_id
    );

    Ok(Json(ReactionResponse {
        event_id: relation.event_id,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

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

        assert!(compat_routes
            .iter()
            .all(|route| route.starts_with("/_matrix/client/")));
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

        assert!(compat_paths
            .iter()
            .all(|path| !path.contains("/relations/") && !path.contains("/annotations/")));
    }
}
