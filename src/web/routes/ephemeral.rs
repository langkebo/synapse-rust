// Ephemeral Events Routes - 临时事件路由
// Matrix spec: https://matrix.org/docs/spec/client_server/latest#get-matrix-client-v3-rooms-room-id-ephemeral

use crate::web::routes::{ensure_room_member, ApiError, AppState, AuthenticatedUser};
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
    ensure_room_member(&state, &auth_user, &room_id, "User is not in the room").await?;

    let now = chrono::Utc::now().timestamp_millis();

    // Get ephemeral events (typing, receipts, etc.)
    let rows = sqlx::query!(
        r#"
        SELECT event_type, user_id, content, stream_id, created_ts
        FROM room_ephemeral
        WHERE room_id = $1
        AND (expires_at IS NULL OR expires_at > $2)
        ORDER BY stream_id DESC
        LIMIT $3
        "#,
        &room_id,
        now,
        params.limit
    )
    .fetch_all(&*state.services.rooms.event_storage.pool)
    .await
    .map_err(|e| ApiError::internal_with_log("Failed to get ephemeral events", &e))?;

    let mut events: Vec<Value> = Vec::new();

    for row in rows {
        let event_type = row.event_type;
        let sender = row.user_id;
        let content = row.content;
        let stream_id = row.stream_id;
        let origin_server_ts = row.created_ts;
        // room_ephemeral 没有原生 event_id；按 Matrix 约定合成 `$ephemeral_{stream_id}`，
        // 保证客户端可以按 id 做幂等去重。
        let event_id = format!("$ephemeral_{stream_id}");

        let event = json!({
            "type": event_type,
            "sender": sender,
            "content": content,
            "origin_server_ts": origin_server_ts,
            "stream_id": stream_id,
            "event_id": event_id,
        });
        events.push(event);
    }

    Ok(Json(EphemeralResponse { events, start: None, end: None }))
}

pub fn create_ephemeral_router(state: AppState) -> Router<AppState> {
    Router::new().route("/_matrix/client/v3/rooms/{room_id}/ephemeral", get(get_ephemeral_events)).with_state(state)
}

pub fn ephemeral_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::RouteEntry;
    use axum::http::Method;
    vec![RouteEntry::new(Method::GET, "/_matrix/client/v3/rooms/{room_id}/ephemeral", "ephemeral")]
}
