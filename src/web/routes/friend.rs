use super::AppState;
use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use serde_json::Value;
use std::sync::Arc;

pub fn create_friend_router(_state: Arc<AppState>) -> Router {
    Router::new()
        .route("/_matrix/client/r0/user/:user_id/rooms", get(get_rooms))
        .route(
            "/_matrix/client/r0/rooms/:room_id/reply",
            post(reply_to_room),
        )
        .route("/_matrix/client/r0/rooms/:room_id/kick", post(kick_user))
        .route("/_matrix/client/r0/rooms/:room_id/ban", post(ban_user))
        .route("/_matrix/client/r0/rooms/:room_id/unban", post(unban_user))
}

#[axum::debug_handler]
async fn get_rooms(
    State(_state): State<Arc<AppState>>,
    Path(_user_id): Path<String>,
) -> Json<Value> {
    Json(serde_json::json!({
        "joined_rooms": []
    }))
}

#[axum::debug_handler]
async fn reply_to_room(
    State(_state): State<Arc<AppState>>,
    Path(_room_id): Path<String>,
    Json(body): Json<Value>,
) -> Json<Value> {
    Json(serde_json::json!({
        "event_id": "$test:localhost"
    }))
}

#[axum::debug_handler]
async fn kick_user(
    State(_state): State<Arc<AppState>>,
    Path(_room_id): Path<String>,
    Json(body): Json<Value>,
) -> Json<Value> {
    Json(serde_json::json!({
        "event_id": "$test:localhost"
    }))
}

#[axum::debug_handler]
async fn ban_user(
    State(_state): State<Arc<AppState>>,
    Path(_room_id): Path<String>,
    Json(body): Json<Value>,
) -> Json<Value> {
    Json(serde_json::json!({
        "event_id": "$test:localhost"
    }))
}

#[axum::debug_handler]
async fn unban_user(
    State(_state): State<Arc<AppState>>,
    Path(_room_id): Path<String>,
    Json(body): Json<Value>,
) -> Json<Value> {
    Json(serde_json::json!({
        "event_id": "$test:localhost"
    }))
}
