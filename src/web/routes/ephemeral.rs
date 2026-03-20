// Ephemeral Events Routes - 临时事件路由
// Matrix spec: https://matrix.org/docs/spec/client_server/latest#get-matrix-client-v3-rooms-room-id-ephemeral

use crate::web::routes::{ApiError, AppState, AuthenticatedUser};
use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Deserialize)]
pub struct EphemeralParams {
    #[serde(default = "default_limit")]
    limit: i64,
}

fn default_limit() -> i64 {
    100
}

#[derive(Debug, Serialize)]
pub struct EphemeralResponse {
    #[serde(rename = "chunk")]
    pub events: Vec<Value>,
    pub start: Option<String>,
    pub end: Option<String>,
}

/// Get ephemeral events for a room
/// GET /_matrix/client/v3/rooms/{room_id}/ephemeral
pub async fn get_ephemeral_events(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Query(params): Query<EphemeralParams>,
) -> Result<Json<EphemeralResponse>, ApiError> {
    // Verify user is in the room
    let user_id = &auth_user.user_id;
    
    // Check if user is in the room
    let membership: Option<String> = sqlx::query_scalar(
        "SELECT membership FROM room_memberships WHERE room_id = $1 AND user_id = $2"
    )
    .bind(&room_id)
    .bind(user_id)
    .fetch_optional(&*state.services.event_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to check membership: {}", e)))?;

    if membership.is_none() || membership.as_deref() != Some("join") {
        return Err(ApiError::forbidden("User is not in the room".to_string()));
    }

    let now = chrono::Utc::now().timestamp_millis();
    
    // Get ephemeral events (typing, receipts, etc.)
    let rows = sqlx::query(
        r#"
        SELECT event_type, user_id, content, stream_id
        FROM room_ephemeral
        WHERE room_id = $1
        AND (expires_at IS NULL OR expires_at > $2)
        ORDER BY stream_id DESC
        LIMIT $3
        "#,
    )
    .bind(&room_id)
    .bind(now)
    .bind(params.limit)
    .fetch_all(&*state.services.event_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to get ephemeral events: {}", e)))?;

    let mut events: Vec<Value> = Vec::new();
    
    for row in rows {
        use sqlx::Row;
        let event_type: String = row.get("event_type");
        let sender: String = row.get("user_id");
        let content: Value = row.get("content");
        
        let event = json!({
            "type": event_type,
            "sender": sender,
            "content": content
        });
        events.push(event);
    }

    Ok(Json(EphemeralResponse {
        events,
        start: None,
        end: None,
    }))
}

pub fn create_ephemeral_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/_matrix/client/v3/rooms/{room_id}/ephemeral",
            get(get_ephemeral_events),
        )
        .with_state(state)
}
