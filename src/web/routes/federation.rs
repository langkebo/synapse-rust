use crate::common::*;
use crate::web::routes::AppState;
use axum::{
    extract::{Json, Path, State},
    routing::{get, post, put},
    Router,
};
use serde_json::{json, Value};

pub fn create_federation_router(_state: AppState) -> Router<AppState> {
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
    State(state): State<AppState>,
    Path(txn_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let _origin = body
        .get("origin")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Origin required".to_string()))?;
    let pdus = body
        .get("pdus") // Matrix spec uses 'pdus'
        .or_else(|| body.get("pdu")) // Fallback to 'pdu'
        .and_then(|v| v.as_array())
        .ok_or_else(|| ApiError::bad_request("PDUs required".to_string()))?;

    let mut results = Vec::new();

    for pdu in pdus {
        let event_id = pdu
            .get("event_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("${}", crate::common::crypto::generate_event_id(_origin)));

        let room_id = pdu.get("room_id").and_then(|v| v.as_str()).unwrap_or("");
        let user_id = pdu.get("sender").and_then(|v| v.as_str()).unwrap_or("");
        let event_type = pdu.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let content = pdu.get("content").cloned().unwrap_or(json!({}));
        let state_key = pdu
            .get("state_key")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let origin_server_ts = pdu
            .get("origin_server_ts")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        let params = crate::storage::event::CreateEventParams {
            event_id: event_id.clone(),
            room_id: room_id.to_string(),
            user_id: user_id.to_string(),
            event_type: event_type.to_string(),
            content,
            state_key,
            origin_server_ts,
        };

        match state.services.event_storage.create_event(params).await {
            Ok(_) => {
                results.push(json!({
                    "event_id": event_id,
                    "success": true
                }));
            }
            Err(e) => {
                ::tracing::error!("Failed to persist PDU {}: {}", event_id, e);
                results.push(json!({
                    "event_id": event_id,
                    "error": e.to_string()
                }));
            }
        }
    }

    ::tracing::info!(
        "Processed transaction {} from {} with {} PDUs",
        txn_id,
        _origin,
        pdus.len()
    );

    Ok(Json(json!({
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
    State(state): State<AppState>,
    Path((room_id, event_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let origin = body
        .get("origin")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Origin required".to_string()))?;

    let event = body
        .get("event")
        .ok_or_else(|| ApiError::bad_request("Event required".to_string()))?;
    let user_id = event.get("sender").and_then(|v| v.as_str()).unwrap_or("");
    let content = event.get("content").cloned().unwrap_or(json!({}));
    let display_name = content.get("displayname").and_then(|v| v.as_str());

    // 1. Persist the event
    let params = crate::storage::event::CreateEventParams {
        event_id: event_id.clone(),
        room_id: room_id.clone(),
        user_id: user_id.to_string(),
        event_type: "m.room.member".to_string(),
        content: content.clone(),
        state_key: Some(user_id.to_string()),
        origin_server_ts: event
            .get("origin_server_ts")
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
    };
    state
        .services
        .event_storage
        .create_event(params)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to persist join event: {}", e)))?;

    // 2. Update membership
    state
        .services
        .member_storage
        .add_member(&room_id, user_id, "join", display_name, None)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to update membership: {}", e)))?;

    ::tracing::info!(
        "Processed join for room {} event {} from {}",
        room_id,
        event_id,
        origin
    );

    Ok(Json(json!({
        "event_id": event_id
    })))
}

async fn send_leave(
    State(state): State<AppState>,
    Path((room_id, event_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let origin = body
        .get("origin")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Origin required".to_string()))?;

    let event = body
        .get("event")
        .ok_or_else(|| ApiError::bad_request("Event required".to_string()))?;
    let user_id = event.get("sender").and_then(|v| v.as_str()).unwrap_or("");

    // 1. Persist the event
    let params = crate::storage::event::CreateEventParams {
        event_id: event_id.clone(),
        room_id: room_id.clone(),
        user_id: user_id.to_string(),
        event_type: "m.room.member".to_string(),
        content: event.get("content").cloned().unwrap_or(json!({})),
        state_key: Some(user_id.to_string()),
        origin_server_ts: event
            .get("origin_server_ts")
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
    };
    state
        .services
        .event_storage
        .create_event(params)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to persist leave event: {}", e)))?;

    // 2. Update membership
    state
        .services
        .member_storage
        .add_member(&room_id, user_id, "leave", None, None)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to update membership: {}", e)))?;

    ::tracing::info!(
        "Processed leave for room {} event {} from {}",
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
    State(state): State<AppState>,
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
    let limit = body.get("limit").and_then(|v| v.as_i64()).unwrap_or(10);

    let events = state
        .services
        .event_storage
        .get_room_events(&room_id, limit)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get missing events: {}", e)))?;

    let events_json: Vec<Value> = events
        .into_iter()
        .map(|e| {
            json!({
                "event_id": e.event_id,
                "type": e.event_type,
                "sender": e.user_id,
                "content": e.content,
                "room_id": e.room_id,
                "origin_server_ts": e.origin_server_ts
            })
        })
        .collect();

    Ok(Json(json!({
        "events": events_json
    })))
}

async fn get_event_auth(
    State(state): State<AppState>,
    Path((room_id, _event_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let auth_events = state
        .services
        .event_storage
        .get_state_events(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get auth events: {}", e)))?;

    let auth_chain: Vec<Value> = auth_events
        .into_iter()
        .map(|e| {
            json!({
                "event_id": e.event_id,
                "type": e.event_type,
                "sender": e.user_id,
                "content": e.content,
                "state_key": e.state_key,
                "origin_server_ts": e.origin_server_ts
            })
        })
        .collect();

    Ok(Json(json!({
        "auth_chain": auth_chain
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
            "content": e.content,
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
                "content": e.content,
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
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let request: crate::e2ee::device_keys::KeyClaimRequest = serde_json::from_value(body)
        .map_err(|e| ApiError::bad_request(format!("Invalid claim request: {}", e)))?;

    let response = state
        .services
        .device_keys_service
        .claim_keys(request)
        .await?;

    Ok(Json(json!({
        "one_time_keys": response.one_time_keys,
        "failures": response.failures
    })))
}

async fn keys_upload(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let request: crate::e2ee::device_keys::KeyUploadRequest = serde_json::from_value(body)
        .map_err(|e| ApiError::bad_request(format!("Invalid upload request: {}", e)))?;

    let response = state
        .services
        .device_keys_service
        .upload_keys(request)
        .await?;

    Ok(Json(json!({
        "one_time_key_counts": response.one_time_key_counts
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
