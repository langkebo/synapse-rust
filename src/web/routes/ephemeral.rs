// Ephemeral Events Routes - 临时事件路由
// Matrix spec: https://matrix.org/docs/spec/client_server/latest#get-matrix-client-v3-rooms-room-id-ephemeral

use crate::web::routes::{ensure_room_member, ApiError, AppState, AuthenticatedUser};
use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

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

    let events =
        state.services.rooms.room_service.messaging.get_ephemeral_events_for_client(&room_id, params.limit).await?;

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
