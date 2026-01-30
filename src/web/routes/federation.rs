use crate::common::*;
use crate::web::routes::AppState;
use axum::{
    extract::{Json, Path, State},
    routing::{get, post, put},
    Router,
};
use serde_json::{json, Value};

pub fn create_federation_router(state: AppState) -> Router {
    Router::new()
        .route("/_matrix/federation/v1/version", get(federation_version))
        .route("/_matrix/federation/v1", get(federation_discovery))
        .route(
            "/_matrix/federation/v1/send/{txn_id}",
            put(send_transaction),
        )
        .route(
            "/_matrix/federation/v1/make_join/{room_id}/{user_id}",
            get(make_join),
        )
        .route(
            "/_matrix/federation/v1/make_leave/{room_id}/{user_id}",
            get(make_leave),
        )
        .route(
            "/_matrix/federation/v1/send_join/{room_id}/{event_id}",
            put(send_join),
        )
        .route(
            "/_matrix/federation/v1/send_leave/{room_id}/{event_id}",
            put(send_leave),
        )
        .route(
            "/_matrix/federation/v1/invite/{room_id}/{event_id}",
            put(invite),
        )
        .route(
            "/_matrix/federation/v1/get_missing_events/{room_id}",
            post(get_missing_events),
        )
        .route(
            "/_matrix/federation/v1/get_event_auth/{room_id}/{event_id}",
            get(get_event_auth),
        )
        .route("/_matrix/federation/v1/state/{room_id}", get(get_state))
        .route("/_matrix/federation/v1/event/{event_id}", get(get_event))
        .route(
            "/_matrix/federation/v1/state_ids/{room_id}",
            get(get_state_ids),
        )
        .route(
            "/_matrix/federation/v1/query/directory/room/{room_id}",
            get(room_directory_query),
        )
        .route(
            "/_matrix/federation/v1/query/profile/{user_id}",
            get(profile_query),
        )
        .route("/_matrix/federation/v1/backfill/{room_id}", get(backfill))
        .route("/_matrix/federation/v1/keys/claim", post(keys_claim))
        .route("/_matrix/federation/v1/keys/upload", post(keys_upload))
        .route("/_matrix/federation/v2/server", get(server_key))
        .route(
            "/_matrix/federation/v2/query/{server_name}/{key_id}",
            get(key_query),
        )
        .route("/_matrix/federation/v2/key/clone", post(key_clone))
        .route(
            "/_matrix/federation/v2/user/keys/query",
            post(user_keys_query),
        )
        .with_state(state)
}

async fn federation_version() -> Json<Value> {
    Json(json!({
        "version": "0.1.0"
    }))
}

async fn federation_discovery() -> Json<Value> {
    Json(json!({
        "version": "0.1.0",
        "name": "Synapse Rust",
        "capabilities": {
            "m.change_password": true,
            "m.room_versions": {
                "1": {
                    "status": "stable"
                }
            }
        }
    }))
}

async fn send_transaction(
    State(_state): State<AppState>,
    Path(txn_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let _origin = body
        .get("origin")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Origin required".to_string()))?;
    let pdus = body
        .get("pdu")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ApiError::bad_request("PDU required".to_string()))?;

    let mut results = Vec::new();

    for pdu in pdus {
        let event_id = pdu
            .get("event_id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        results.push(json!({
            "event_id": event_id,
            "success": true
        }));
    }

    ::tracing::info!(
        "Received transaction {} from {} with {} PDUs",
        txn_id,
        _origin,
        pdus.len()
    );

    Ok(Json(json!({
        "txn_id": txn_id,
        "results": results
    })))
}

async fn make_join(
    State(state): State<AppState>,
    Path((room_id, user_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let auth_events = state
        .services
        .event_storage
        .get_state_events(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get auth events: {}", e)))?;

    let auth_events_json: Vec<Value> = auth_events
        .iter()
        .map(|e| {
            json!({
                "event_id": e.event_id,
                "type": e.event_type,
                "state_key": e.state_key
            })
        })
        .collect();

    Ok(Json(json!({
        "room_version": "1",
        "auth_events": auth_events_json,
        "event": {
            "type": "m.room.member",
            "content": {
                "membership": "join"
            },
            "sender": user_id,
            "state_key": user_id
        }
    })))
}

async fn make_leave(
    State(state): State<AppState>,
    Path((room_id, user_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let auth_events = state
        .services
        .event_storage
        .get_state_events(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get auth events: {}", e)))?;

    let auth_events_json: Vec<Value> = auth_events
        .iter()
        .map(|e| {
            json!({
                "event_id": e.event_id,
                "type": e.event_type,
                "state_key": e.state_key
            })
        })
        .collect();

    Ok(Json(json!({
        "room_version": "1",
        "auth_events": auth_events_json,
        "event": {
            "type": "m.room.member",
            "content": {
                "membership": "leave"
            },
            "sender": user_id,
            "state_key": user_id
        }
    })))
}

async fn send_join(
    State(_state): State<AppState>,
    Path((room_id, event_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let origin = body
        .get("origin")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Origin required".to_string()))?;

    ::tracing::info!(
        "Processing join for room {} event {} from {}",
        room_id,
        event_id,
        origin
    );

    Ok(Json(json!({
        "event_id": event_id
    })))
}

async fn send_leave(
    State(_state): State<AppState>,
    Path((room_id, event_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let origin = body
        .get("origin")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Origin required".to_string()))?;

    ::tracing::info!(
        "Processing leave for room {} event {} from {}",
        room_id,
        event_id,
        origin
    );

    Ok(Json(json!({
        "event_id": event_id
    })))
}

async fn invite(
    State(_state): State<AppState>,
    Path((room_id, event_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let _origin = body
        .get("origin")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Origin required".to_string()))?;

    ::tracing::info!("Processing invite for room {} event {}", room_id, event_id);

    Ok(Json(json!({
        "event_id": event_id
    })))
}

async fn get_missing_events(
    State(_state): State<AppState>,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let _earliest_events = body
        .get("earliest_events")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ApiError::bad_request("earliest_events required".to_string()))?;
    let _latest_events = body
        .get("latest_events")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ApiError::bad_request("latest_events required".to_string()))?;

    ::tracing::info!("Getting missing events for room {}", room_id);

    Ok(Json(json!({
        "events": [],
        "limit": 10
    })))
}

async fn get_event_auth(
    State(_state): State<AppState>,
    Path((room_id, event_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    ::tracing::info!("Getting event auth for room {} event {}", room_id, event_id);

    Ok(Json(json!({
        "auth_chain": []
    })))
}

async fn get_event(
    State(state): State<AppState>,
    Path(event_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let event = state
        .services
        .event_storage
        .get_event(&event_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get event: {}", e)))?;

    match event {
        Some(e) => Ok(Json(json!({
            "event_id": e.event_id,
            "type": e.event_type,
            "sender": e.user_id,
            "content": serde_json::from_str(&e.content).unwrap_or(json!({})),
            "state_key": e.state_key,
            "origin_server_ts": e.origin_server_ts,
            "room_id": e.room_id
        }))),
        None => Err(ApiError::not_found("Event not found".to_string())),
    }
}

async fn get_state(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let events = state
        .services
        .event_storage
        .get_state_events(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get state: {}", e)))?;

    let state_events: Vec<Value> = events
        .iter()
        .map(|e| {
            json!({
                "event_id": e.event_id,
                "type": e.event_type,
                "sender": e.user_id,
                "content": serde_json::from_str(&e.content).unwrap_or(json!({})),
                "state_key": e.state_key
            })
        })
        .collect();

    Ok(Json(json!({
        "state": state_events
    })))
}

async fn get_state_ids(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let events = state
        .services
        .event_storage
        .get_state_events(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get state: {}", e)))?;

    let state_ids: Vec<String> = events.iter().map(|e| e.event_id.clone()).collect();

    Ok(Json(json!({
        "state_ids": state_ids
    })))
}

async fn room_directory_query(
    State(_state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({
        "room_id": room_id,
        "servers": []
    })))
}

async fn profile_query(
    State(_state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({
        "user_id": user_id,
        "displayname": None::<String>,
        "avatar_url": None::<String>
    })))
}

async fn backfill(
    State(_state): State<AppState>,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let limit = body.get("limit").and_then(|v| v.as_u64()).unwrap_or(10);
    let events = body
        .get("v")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ApiError::bad_request("v required".to_string()))?;

    ::tracing::info!("Backfilling room {} with {} events", room_id, events.len());

    Ok(Json(json!({
        "origin": "localhost",
        "pdus": [],
        "limit": limit
    })))
}

async fn keys_claim(
    State(_state): State<AppState>,
    Json(_body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    ::tracing::info!("Processing keys claim request");

    Ok(Json(json!({
        "one_time_keys": {}
    })))
}

async fn keys_upload(
    State(_state): State<AppState>,
    Json(_body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    ::tracing::info!("Processing keys upload request");

    Ok(Json(json!({
        "one_time_key_counts": {
            "curve25519": 0,
            "signed_curve25519": 0
        }
    })))
}

async fn server_key() -> Json<Value> {
    Json(json!({
        "server_name": "localhost",
        "verify_keys": {
            "ed25519": "base64_key",
            "curve25519": "base64_key"
        }
    }))
}

async fn key_query(Path((server_name, key_id)): Path<(String, String)>) -> Json<Value> {
    Json(json!({
        "server_name": server_name,
        "key_id": key_id,
        "verify_keys": {
            "ed25519": "base64_key",
            "curve25519": "base64_key"
        }
    }))
}

async fn key_clone(Json(_body): Json<Value>) -> Json<Value> {
    Json(json!({
        "success": true
    }))
}

async fn user_keys_query(
    State(_state): State<AppState>,
    Json(_body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    ::tracing::info!("Processing user keys query");

    Ok(Json(json!({
        "device_keys": {}
    })))
}
