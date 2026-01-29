use super::AppState;
use axum::{
    extract::{Path, State},
    routing::{delete, get, post, put},
    Json, Router,
};
use serde_json::Value;
use std::sync::Arc;

pub fn create_admin_router(_state: Arc<AppState>) -> Router {
    Router::new()
        .route("/_synapse/admin/v1/server_version", get(get_server_version))
        .route("/_synapse/admin/v1/users", get(get_users))
        .route("/_synapse/admin/v1/users/:user_id", get(get_user))
        .route("/_synapse/admin/v1/users/:user_id/admin", put(set_admin))
        .route(
            "/_synapse/admin/v1/users/:user_id/deactivate",
            post(deactivate_user),
        )
        .route("/_synapse/admin/v1/rooms", get(get_rooms))
        .route("/_synapse/admin/v1/rooms/:room_id", get(get_room))
        .route(
            "/_synapse/admin/v1/rooms/:room_id/delete",
            post(delete_room),
        )
        .route("/_synapse/admin/v1/purge_history", post(purge_history))
        .route("/_synapse/admin/v1/shutdown_room", post(shutdown_room))
}

#[axum::debug_handler]
async fn get_server_version() -> Json<Value> {
    Json(serde_json::json!({
        "version": "1.0.0",
        "python_version": "3.9.0"
    }))
}

#[axum::debug_handler]
async fn get_users(State(_state): State<Arc<AppState>>) -> Json<Value> {
    Json(serde_json::json!({
        "users": [],
        "total": 0
    }))
}

#[axum::debug_handler]
async fn get_user(
    State(_state): State<Arc<AppState>>,
    Path(_user_id): Path<String>,
) -> Json<Value> {
    Json(serde_json::json!({
        "name": "user:test",
        "is_guest": false,
        "admin": false,
        "deactivated": false
    }))
}

#[axum::debug_handler]
async fn set_admin(
    State(_state): State<Arc<AppState>>,
    Path(_user_id): Path<String>,
    Json(body): Json<Value>,
) -> Json<Value> {
    Json(serde_json::json!({
        "success": true
    }))
}

#[axum::debug_handler]
async fn deactivate_user(
    State(_state): State<Arc<AppState>>,
    Path(_user_id): Path<String>,
) -> Json<Value> {
    Json(serde_json::json!({
        "id_server_unbind_result": "success"
    }))
}

#[axum::debug_handler]
async fn get_rooms(State(_state): State<Arc<AppState>>) -> Json<Value> {
    Json(serde_json::json!({
        "rooms": [],
        "total": 0
    }))
}

#[axum::debug_handler]
async fn get_room(
    State(_state): State<Arc<AppState>>,
    Path(_room_id): Path<String>,
) -> Json<Value> {
    Json(serde_json::json!({
        "room_id": "!test:localhost",
        "name": "Test Room",
        "topic": "",
        "creator": "@test:localhost",
        "joined_members": 1,
        "joined_local_members": 1,
        "state_events": 0
    }))
}

#[axum::debug_handler]
async fn delete_room(
    State(_state): State<Arc<AppState>>,
    Path(_room_id): Path<String>,
) -> Json<Value> {
    Json(serde_json::json!({
        "delete_id": "!test:localhost"
    }))
}

#[axum::debug_handler]
async fn purge_history(
    State(_state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Json<Value> {
    Json(serde_json::json!({
        "success": true
    }))
}

#[axum::debug_handler]
async fn shutdown_room(
    State(_state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Json<Value> {
    Json(serde_json::json!({
        "kicked_users": [],
        "failed_to_kick_users": [],
        "closed_room": true
    }))
}
