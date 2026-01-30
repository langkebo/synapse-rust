use super::AppState;
use axum::{
    extract::{Path, State},
    routing::{delete, get, post, put},
    Json, Router,
};
use serde_json::Value;

pub fn create_key_backup_router(state: AppState) -> Router {
    Router::new()
        .route(
            "/_matrix/client/r0/room_keys/version",
            post(create_backup_version),
        )
        .route(
            "/_matrix/client/r0/room_keys/version/{version}",
            get(get_backup_version),
        )
        .route(
            "/_matrix/client/r0/room_keys/version/{version}",
            put(update_backup_version),
        )
        .route(
            "/_matrix/client/r0/room_keys/version/{version}",
            delete(delete_backup_version),
        )
        .route("/_matrix/client/r0/room_keys/{version}", get(get_room_keys))
        .route("/_matrix/client/r0/room_keys/{version}", put(put_room_keys))
        .route(
            "/_matrix/client/r0/room_keys/{version}/keys",
            post(put_room_keys_multi),
        )
        .route(
            "/_matrix/client/r0/room_keys/{version}/keys/{room_id}",
            get(get_room_key_by_id),
        )
        .route(
            "/_matrix/client/r0/room_keys/{version}/keys/{room_id}/{session_id}",
            get(get_room_key),
        )
        .with_state(state)
}

#[axum::debug_handler]
async fn create_backup_version(
    State(_state): State<AppState>,
    Json(_body): Json<Value>,
) -> Json<Value> {
    Json(serde_json::json!({
        "version": "1"
    }))
}

#[axum::debug_handler]
async fn get_backup_version(
    State(_state): State<AppState>,
    Path(version): Path<String>,
) -> Json<Value> {
    Json(serde_json::json!({
        "algorithm": "m.megolm.v1.aes-sha2",
        "auth_data": {},
        "count": 0,
        "etag": "test_etag",
        "version": version
    }))
}

#[axum::debug_handler]
async fn update_backup_version(
    State(_state): State<AppState>,
    Path(version): Path<String>,
    Json(_body): Json<Value>,
) -> Json<Value> {
    Json(serde_json::json!({
        "version": version
    }))
}

#[axum::debug_handler]
async fn delete_backup_version(
    State(_state): State<AppState>,
    Path(version): Path<String>,
) -> Json<Value> {
    Json(serde_json::json!({
        "deleted": true,
        "version": version
    }))
}

#[axum::debug_handler]
async fn get_room_keys(
    State(_state): State<AppState>,
    Path(_version): Path<String>,
) -> Json<Value> {
    Json(serde_json::json!({
        "rooms": {},
        "etag": "test_etag"
    }))
}

#[axum::debug_handler]
async fn put_room_keys(
    State(_state): State<AppState>,
    Path(_version): Path<String>,
    Json(_body): Json<Value>,
) -> Json<Value> {
    Json(serde_json::json!({
        "count": 0,
        "etag": "test_etag"
    }))
}

#[axum::debug_handler]
async fn put_room_keys_multi(
    State(_state): State<AppState>,
    Path(_version): Path<String>,
    Json(_body): Json<Value>,
) -> Json<Value> {
    Json(serde_json::json!({
        "count": 0,
        "etag": "test_etag"
    }))
}

#[axum::debug_handler]
async fn get_room_key_by_id(
    State(_state): State<AppState>,
    Path((_version, _room_id)): Path<(String, String)>,
) -> Json<Value> {
    Json(serde_json::json!({
        "rooms": {}
    }))
}

#[axum::debug_handler]
async fn get_room_key(
    State(_state): State<AppState>,
    Path((_version, _room_id, _session_id)): Path<(String, String, String)>,
) -> Json<Value> {
    Json(serde_json::json!({
        "room_id": _room_id,
        "session_id": _session_id,
        "session_key": "test_key",
        "algorithm": "m.megolm.v1.aes-sha2"
    }))
}
