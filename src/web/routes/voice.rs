use super::AppState;
use axum::{
    extract::{Path, Query, State},
    routing::{delete, get, post},
    Json, Router,
};
use serde_json::Value;
use std::sync::Arc;

pub fn create_voice_router(_state: Arc<AppState>, _voice_path: std::path::PathBuf) -> Router {
    Router::new()
        .route(
            "/_matrix/client/r0/voice/upload",
            post(upload_voice_message),
        )
        .route(
            "/_matrix/client/r0/voice/:message_id",
            get(get_voice_message),
        )
        .route(
            "/_matrix/client/r0/voice/:message_id",
            delete(delete_voice_message),
        )
        .route(
            "/_matrix/client/r0/voice/user/:user_id",
            get(get_user_voice_messages),
        )
        .route(
            "/_matrix/client/r0/voice/room/:room_id",
            get(get_room_voice_messages),
        )
        .route(
            "/_matrix/client/r0/voice/user/:user_id/stats",
            get(get_user_voice_stats),
        )
}

#[axum::debug_handler]
async fn upload_voice_message(
    State(_state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Json<Value> {
    Json(serde_json::json!({
        "message_id": "voice_test",
        "duration": 0,
        "size": 0
    }))
}

#[axum::debug_handler]
async fn get_voice_message(
    State(_state): State<Arc<AppState>>,
    Path(message_id): Path<String>,
) -> Json<Value> {
    Json(serde_json::json!({
        "message_id": message_id,
        "user_id": "@test:localhost",
        "duration": 0,
        "size": 0,
        "url": "mxc://localhost/voice_test"
    }))
}

#[axum::debug_handler]
async fn delete_voice_message(
    State(_state): State<Arc<AppState>>,
    Path(message_id): Path<String>,
) -> Json<Value> {
    Json(serde_json::json!({
        "deleted": true,
        "message_id": message_id
    }))
}

#[axum::debug_handler]
async fn get_user_voice_messages(
    State(_state): State<Arc<AppState>>,
    Path(_user_id): Path<String>,
) -> Json<Value> {
    Json(serde_json::json!({
        "messages": [],
        "total": 0
    }))
}

#[axum::debug_handler]
async fn get_room_voice_messages(
    State(_state): State<Arc<AppState>>,
    Path(_room_id): Path<String>,
) -> Json<Value> {
    Json(serde_json::json!({
        "messages": [],
        "total": 0
    }))
}

#[axum::debug_handler]
async fn get_user_voice_stats(
    State(_state): State<Arc<AppState>>,
    Path(_user_id): Path<String>,
) -> Json<Value> {
    Json(serde_json::json!({
        "total_messages": 0,
        "total_duration": 0,
        "total_size": 0
    }))
}
