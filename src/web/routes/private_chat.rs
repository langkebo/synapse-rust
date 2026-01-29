use super::AppState;
use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use serde_json::Value;
use std::sync::Arc;

pub fn create_private_chat_router(_state: Arc<AppState>) -> Router {
    Router::new()
        .route("/_matrix/client/r0/dm", get(get_dm_rooms))
        .route("/_matrix/client/r0/createDM", post(create_dm_room))
        .route(
            "/_matrix/client/r0/rooms/:room_id/dm",
            get(get_dm_room_details),
        )
        .route(
            "/_matrix/client/r0/rooms/:room_id/unread",
            get(get_unread_notifications),
        )
}

#[axum::debug_handler]
async fn get_dm_rooms(State(_state): State<Arc<AppState>>) -> Json<Value> {
    Json(serde_json::json!({
        "rooms": []
    }))
}

#[axum::debug_handler]
async fn create_dm_room(
    State(_state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Json<Value> {
    Json(serde_json::json!({
        "room_id": "!dm:localhost"
    }))
}

#[axum::debug_handler]
async fn get_dm_room_details(
    State(_state): State<Arc<AppState>>,
    Path(_room_id): Path<String>,
) -> Json<Value> {
    Json(serde_json::json!({
        "room_id": _room_id,
        "members": [],
        "is_dm": true
    }))
}

#[axum::debug_handler]
async fn get_unread_notifications(
    State(_state): State<Arc<AppState>>,
    Path(_room_id): Path<String>,
) -> Json<Value> {
    Json(serde_json::json!({
        "notification_count": 0,
        "highlight_count": 0
    }))
}
