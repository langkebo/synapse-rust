use super::AppState;
use axum::{
    extract::{Path, Query, State},
    routing::{get, post, put},
    Json, Router,
};
use serde_json::Value;
use std::sync::Arc;

pub fn create_e2ee_router(_state: Arc<AppState>) -> Router {
    Router::new()
        .route(
            "/_matrix/client/r0/keys/upload/:device_id",
            post(upload_keys),
        )
        .route("/_matrix/client/r0/keys/query", post(query_keys))
        .route("/_matrix/client/r0/keys/claim", post(claim_keys))
        .route("/_matrix/client/r0/keys/changes", get(key_changes))
        .route(
            "/_matrix/client/r0/directory/list/room/:room_id",
            get(room_key_distribution),
        )
        .route(
            "/_matrix/client/r0/sendToDevice/:transaction_id",
            post(send_to_device),
        )
        .route(
            "/_matrix/client/r0/room_keys/version",
            post(create_room_keys_version),
        )
        .route(
            "/_matrix/client/r0/room_keys/:version",
            get(get_room_keys_version),
        )
        .route(
            "/_matrix/client/r0/room_keys/:version",
            put(put_room_keys_backup),
        )
        .route(
            "/_matrix/client/r0/room_keys/:version/keys",
            post(put_room_keys),
        )
}

#[axum::debug_handler]
async fn upload_keys(
    State(_state): State<Arc<AppState>>,
    Path(_device_id): Path<String>,
    Json(body): Json<Value>,
) -> Json<Value> {
    Json(serde_json::json!({
        "one_time_key_counts": {
            "curve25519": 0,
            "signed_curve25519": 0
        }
    }))
}

#[axum::debug_handler]
async fn query_keys(State(_state): State<Arc<AppState>>, Json(body): Json<Value>) -> Json<Value> {
    Json(serde_json::json!({
        "device_keys": {}
    }))
}

#[axum::debug_handler]
async fn claim_keys(State(_state): State<Arc<AppState>>, Json(body): Json<Value>) -> Json<Value> {
    Json(serde_json::json!({
        "one_time_keys": {}
    }))
}

#[axum::debug_handler]
async fn key_changes(
    State(_state): State<Arc<AppState>>,
    Query(params): Query<Value>,
) -> Json<Value> {
    let from = params.get("from").map(|v| v.as_str()).unwrap_or("");
    let to = params.get("to").map(|v| v.as_str()).unwrap_or("");
    Json(serde_json::json!({
        "changed": [],
        "left": [],
        "from": from,
        "to": to
    }))
}

#[axum::debug_handler]
async fn room_key_distribution(
    State(_state): State<Arc<AppState>>,
    Path(_room_id): Path<String>,
) -> Json<Value> {
    Json(serde_json::json!({
        "room_id": "!test:localhost",
        "algorithm": "m.megolm.v1.aes-sha2",
        "session_id": "test_session_id",
        "session_key": "test_session_key"
    }))
}

#[axum::debug_handler]
async fn send_to_device(
    State(_state): State<Arc<AppState>>,
    Path(_transaction_id): Path<String>,
    Json(body): Json<Value>,
) -> Json<Value> {
    Json(serde_json::json!({
        "txn_id": _transaction_id
    }))
}

#[axum::debug_handler]
async fn create_room_keys_version(
    State(_state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Json<Value> {
    Json(serde_json::json!({
        "version": "1"
    }))
}

#[axum::debug_handler]
async fn get_room_keys_version(
    State(_state): State<Arc<AppState>>,
    Path(_version): Path<String>,
) -> Json<Value> {
    Json(serde_json::json!({
        "algorithm": "m.megolm.v1.aes-sha2",
        "auth_data": {},
        "count": 0,
        "etag": "test_etag",
        "version": _version
    }))
}

#[axum::debug_handler]
async fn put_room_keys_backup(
    State(_state): State<Arc<AppState>>,
    Path(version): Path<String>,
    Json(body): Json<Value>,
) -> Json<Value> {
    Json(serde_json::json!({
        "success": true
    }))
}

#[axum::debug_handler]
async fn put_room_keys(
    State(_state): State<Arc<AppState>>,
    Path(version): Path<String>,
    Json(body): Json<Value>,
) -> Json<Value> {
    Json(serde_json::json!({
        "count": 0,
        "etag": "test_etag"
    }))
}
