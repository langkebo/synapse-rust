use crate::common::*;
use crate::web::routes::AppState;
use axum::{
    extract::{Json, Path, State},
    middleware,
    routing::{get, post, put},
    Router,
};
use base64::Engine;
use serde_json::{json, Value};

pub fn create_federation_router(state: AppState) -> Router<AppState> {
    let public = Router::new()
        .route("/_matrix/federation/v2/server", get(server_key))
        .route("/_matrix/key/v2/server", get(server_key))
        .route(
            "/_matrix/federation/v2/query/{server_name}/{key_id}",
            get(key_query),
        )
        .route(
            "/_matrix/key/v2/query/{server_name}/{key_id}",
            get(key_query),
        )
        .route("/_matrix/federation/v1/version", get(federation_version))
        .route("/_matrix/federation/v1", get(federation_discovery))
        .route("/_matrix/federation/v1/publicRooms", get(get_public_rooms))
        .route(
            "/_matrix/federation/v1/query/destination",
            get(query_destination),
        )
        .route(
            "/_matrix/federation/v1/room/{room_id}/{event_id}",
            get(get_room_event),
        );

    let protected = Router::new()
        .route(
            "/_matrix/federation/v1/members/{room_id}",
            get(get_room_members),
        )
        .route(
            "/_matrix/federation/v1/members/{room_id}/joined",
            get(get_joined_room_members),
        )
        .route(
            "/_matrix/federation/v1/user/devices/{user_id}",
            get(get_user_devices),
        )
        .route(
            "/_matrix/federation/v1/room_auth/{room_id}",
            get(get_room_auth),
        )
        .route(
            "/_matrix/federation/v1/knock/{room_id}/{user_id}",
            get(knock_room),
        )
        .route(
            "/_matrix/federation/v1/thirdparty/invite",
            post(thirdparty_invite),
        )
        .route(
            "/_matrix/federation/v1/get_joining_rules/{room_id}",
            get(get_joining_rules),
        )
        .route(
            "/_matrix/federation/v2/invite/{room_id}/{event_id}",
            put(invite_v2),
        )
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
        .route("/_matrix/federation/v2/key/clone", post(key_clone))
        .route(
            "/_matrix/federation/v2/user/keys/query",
            post(user_keys_query),
        );

    let protected = protected.layer(middleware::from_fn_with_state(
        state,
        crate::web::middleware::federation_auth_middleware,
    ));

    public.merge(protected)
}

async fn federation_version(State(state): State<AppState>) -> Json<Value> {
    Json(json!({
        "version": state.services.config.server.expire_access_token_lifetime.to_string(), // Just an example, maybe use a real version
        "server": {
            "name": "Synapse Rust",
            "version": "0.1.0"
        }
    }))
}

async fn federation_discovery(State(state): State<AppState>) -> Json<Value> {
    Json(json!({
        "version": "0.1.0",
        "server_name": state.services.server_name,
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

async fn get_room_members(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let members = state
        .services
        .member_storage
        .get_room_members(&room_id, "join")
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get room members: {}", e)))?;

    let members_json: Vec<Value> = members
        .into_iter()
        .map(|m| {
            json!({
                "room_id": m.room_id,
                "user_id": m.user_id,
                "membership": m.membership,
                "display_name": m.display_name,
                "avatar_url": m.avatar_url
            })
        })
        .collect();

    Ok(Json(json!({
        "members": members_json,
        "room_id": room_id,
        "offset": 0,
        "total": members_json.len()
    })))
}

async fn knock_room(
    State(state): State<AppState>,
    Path((room_id, user_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let event_id = format!(
        "${}",
        crate::common::crypto::generate_event_id(&state.services.server_name)
    );

    let params = crate::storage::event::CreateEventParams {
        event_id: event_id.clone(),
        room_id: room_id.clone(),
        user_id: user_id.clone(),
        event_type: "m.room.member".to_string(),
        content: json!({"membership": "knock"}),
        state_key: Some(user_id.clone()),
        origin_server_ts: chrono::Utc::now().timestamp_millis(),
    };

    state
        .services
        .event_storage
        .create_event(params)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create knock event: {}", e)))?;

    Ok(Json(json!({
        "event_id": event_id,
        "room_id": room_id,
        "state": "knocking"
    })))
}

async fn thirdparty_invite(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let room_id = body
        .get("room_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("room_id required".to_string()))?;
    let invitee = body
        .get("invitee")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("invitee required".to_string()))?;
    let sender = body
        .get("sender")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("sender required".to_string()))?;

    let event_id = format!(
        "${}",
        crate::common::crypto::generate_event_id(&state.services.server_name)
    );

    let params = crate::storage::event::CreateEventParams {
        event_id: event_id.clone(),
        room_id: room_id.to_string(),
        user_id: sender.to_string(),
        event_type: "m.room.member".to_string(),
        content: json!({
            "membership": "invite",
            "third_party_invite": {
                "signed": {
                    "mxid": invitee,
                    "token": format!("third_party_token_{}", event_id)
                }
            }
        }),
        state_key: Some(invitee.to_string()),
        origin_server_ts: chrono::Utc::now().timestamp_millis(),
    };

    state
        .services
        .event_storage
        .create_event(params)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create invite event: {}", e)))?;

    Ok(Json(json!({
        "event_id": event_id,
        "room_id": room_id,
        "state": "invited"
    })))
}

async fn get_joining_rules(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let room = state
        .services
        .room_storage
        .get_room(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get room: {}", e)))?;

    match room {
        Some(r) => {
            let join_rule = if r.is_public { "public" } else { "invite" };
            Ok(Json(json!({
                "room_id": room_id,
                "join_rule": join_rule,
                "allow": []
            })))
        }
        None => Err(ApiError::not_found("Room not found".to_string())),
    }
}

async fn get_joined_room_members(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let members = state
        .services
        .member_storage
        .get_room_members(&room_id, "join")
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get room members: {}", e)))?;

    let members_json: Vec<Value> = members
        .into_iter()
        .map(|m| {
            json!({
                "room_id": m.room_id,
                "user_id": m.user_id,
                "membership": m.membership,
                "display_name": m.display_name,
                "avatar_url": m.avatar_url
            })
        })
        .collect();

    Ok(Json(json!({
        "joined": members_json,
        "room_id": room_id
    })))
}

async fn get_user_devices(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let devices = state
        .services
        .device_storage
        .get_user_devices(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get user devices: {}", e)))?;

    let devices_json: Vec<Value> = devices
        .into_iter()
        .map(|d| {
            let keys = d.device_key.unwrap_or_else(|| json!({}));
            json!({
                "device_id": d.device_id,
                "user_id": d.user_id,
                "keys": keys,
                "device_display_name": d.display_name,
                "last_seen_ts": d.last_seen_ts,
                "last_seen_ip": d.last_seen_ip
            })
        })
        .collect();

    Ok(Json(json!({
        "user_id": user_id,
        "devices": devices_json
    })))
}

async fn get_room_auth(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let auth_events = state
        .services
        .event_storage
        .get_state_events(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get auth events: {}", e)))?;

    let auth_chain: Vec<Value> = auth_events
        .into_iter()
        .filter(|e| {
            e.event_type == "m.room.create"
                || e.event_type == "m.room.member"
                || e.event_type == "m.room.power_levels"
                || e.event_type == "m.room.join_rules"
                || e.event_type == "m.room.history_visibility"
        })
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
        "room_id": room_id,
        "auth_chain": auth_chain
    })))
}

async fn invite_v2(
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

    let sender = event.get("sender").and_then(|v| v.as_str()).unwrap_or("");
    let state_key = event
        .get("state_key")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let content = event.get("content").cloned().unwrap_or(json!({}));

    let params = crate::storage::event::CreateEventParams {
        event_id: event_id.clone(),
        room_id: room_id.clone(),
        user_id: sender.to_string(),
        event_type: "m.room.member".to_string(),
        content,
        state_key: Some(state_key.to_string()),
        origin_server_ts: event
            .get("origin_server_ts")
            .and_then(|v| v.as_i64())
            .unwrap_or(chrono::Utc::now().timestamp_millis()),
    };

    state
        .services
        .event_storage
        .create_event(params)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create invite event: {}", e)))?;

    ::tracing::info!(
        "Processed v2 invite for room {} event {} from {}",
        room_id,
        event_id,
        origin
    );

    Ok(Json(json!({
        "event_id": event_id
    })))
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

async fn get_room_event(
    State(state): State<AppState>,
    Path((room_id, event_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let event = state
        .services
        .event_storage
        .get_event(&event_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get event: {}", e)))?;

    match event {
        Some(e) => {
            if e.room_id != room_id {
                return Err(ApiError::bad_request(
                    "Event does not belong to this room".to_string(),
                ));
            }
            Ok(Json(json!({
                "event_id": e.event_id,
                "type": e.event_type,
                "sender": e.user_id,
                "content": e.content,
                "state_key": e.state_key,
                "origin_server_ts": e.origin_server_ts,
                "room_id": e.room_id
            })))
        }
        None => Err(ApiError::not_found("Event not found".to_string())),
    }
}

async fn query_destination(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({
        "destination": state.services.server_name,
        "host": "localhost",
        "port": 8008,
        "tls": false,
        "ts": chrono::Utc::now().timestamp_millis()
    })))
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

#[axum::debug_handler]
async fn room_directory_query(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    // 1. Try rooms
    let room = state
        .services
        .room_storage
        .get_room(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    if let Some(room) = room {
        return Ok(Json(json!({
            "room_id": room.room_id,
            "servers": [state.services.server_name],
            "name": room.name,
            "topic": room.topic,
            "guest_can_join": true,
            "world_readable": room.is_public
        })));
    }

    // 2. Try private sessions (Federation might ask for DM room info)
    if room_id.starts_with("ps_") {
        let session = state
            .services
            .private_chat_storage
            .get_session_info(&room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        if let Some(session) = session {
            return Ok(Json(json!({
                "room_id": session.session_id,
                "servers": [state.services.server_name],
                "name": "Private Chat",
                "topic": "Encrypted direct message session",
                "guest_can_join": false,
                "world_readable": false
            })));
        }
    }

    Err(ApiError::not_found("Room not found".to_string()))
}

#[axum::debug_handler]
async fn profile_query(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let profile = state
        .services
        .registration_service
        .get_profile(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get profile: {}", e)))?;

    Ok(Json(profile))
}

async fn get_public_rooms(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let limit = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);
    let _since = params.get("since").cloned();

    let rooms = state
        .services
        .room_storage
        .get_public_rooms(limit)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(json!({
        "chunk": rooms,
        "next_batch": null
    })))
}

fn topological_sort(pdus: &mut Vec<Value>) {
    use std::collections::{HashMap, VecDeque};

    let mut graph: HashMap<String, Vec<usize>> = HashMap::new();
    let mut in_degree: Vec<usize> = vec![0; pdus.len()];
    let mut event_id_to_idx: HashMap<String, usize> = HashMap::new();

    for (i, pdu) in pdus.iter().enumerate() {
        if let Some(event_id) = pdu.get("event_id").and_then(|v| v.as_str()) {
            event_id_to_idx.insert(event_id.to_string(), i);
        }
    }

    for (i, pdu) in pdus.iter().enumerate() {
        if let Some(prev_events) = pdu.get("prev_events").and_then(|v| v.as_array()) {
            for prev in prev_events {
                if let Some(prev_id) = prev.as_str() {
                    if let Some(&_prev_idx) = event_id_to_idx.get(prev_id) {
                        graph.entry(prev_id.to_string()).or_default().push(i);
                        in_degree[i] += 1;
                    }
                }
            }
        }
    }

    let mut queue = VecDeque::new();
    for (i, &degree) in in_degree.iter().enumerate() {
        if degree == 0 {
            queue.push_back(i);
        }
    }

    let mut sorted_indices = Vec::new();
    while let Some(u) = queue.pop_front() {
        sorted_indices.push(u);
        if let Some(event_id) = pdus[u].get("event_id").and_then(|v| v.as_str()) {
            if let Some(neighbors) = graph.get(event_id) {
                for &v in neighbors {
                    in_degree[v] -= 1;
                    if in_degree[v] == 0 {
                        queue.push_back(v);
                    }
                }
            }
        }
    }

    // If there's a cycle or missing nodes, sorted_indices.len() != pdus.len()
    // In that case, we just keep the original order for the remaining nodes
    if sorted_indices.len() == pdus.len() {
        let mut sorted_pdus = Vec::with_capacity(pdus.len());
        for idx in sorted_indices {
            sorted_pdus.push(pdus[idx].clone());
        }
        *pdus = sorted_pdus;
    }
}

async fn backfill(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let limit = body.get("limit").and_then(|v| v.as_i64()).unwrap_or(10);
    let v = body
        .get("v")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ApiError::bad_request("v required".to_string()))?;

    ::tracing::info!(
        "Backfilling room {} from event(s) {:?} with limit {}",
        room_id,
        v,
        limit
    );

    // 1. Fetch events from local storage (Simulating backfill)
    let events = state
        .services
        .event_storage
        .get_room_events(&room_id, limit)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let mut pdus: Vec<Value> = events
        .into_iter()
        .map(|e| {
            json!({
                "event_id": e.event_id,
                "type": e.event_type,
                "sender": e.user_id,
                "content": e.content,
                "room_id": e.room_id,
                "origin_server_ts": e.origin_server_ts,
                "prev_events": [] // In a real implementation, this would come from the event's DAG
            })
        })
        .collect();

    // 2. Adaptive Topological Sorting
    topological_sort(&mut pdus);

    ::tracing::debug!("Backfill returning {} sorted PDUs", pdus.len());

    Ok(Json(json!({
        "origin": state.services.server_name,
        "pdus": pdus,
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

async fn server_key(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let config = &state.services.config.federation;
    if !config.enabled {
        return Err(ApiError::not_found("Federation disabled".to_string()));
    }

    let key_id = config
        .key_id
        .clone()
        .unwrap_or_else(|| "ed25519:1".to_string());

    let verify_key = match config.signing_key.as_deref().and_then(|k| {
        let res = derive_ed25519_verify_key_base64(k);
        if res.is_none() {
            ::tracing::error!("Failed to derive verify key from signing_key: {}", k);
        }
        res
    }) {
        Some(k) => k,
        None => {
            ::tracing::error!("Federation signing key missing or invalid in config");
            return Err(ApiError::internal(
                "Missing or invalid federation signing key".to_string(),
            ));
        }
    };

    Ok(Json(json!({
        "server_name": config.server_name,
        "verify_keys": {
            key_id: { "key": verify_key }
        },
        "old_verify_keys": {},
        "valid_until_ts": chrono::Utc::now().timestamp_millis() + 3600 * 1000
    })))
}

async fn key_query(
    State(state): State<AppState>,
    Path((server_name, key_id)): Path<(String, String)>,
) -> Json<Value> {
    // If it's us, return our key, otherwise we would normally proxy/cache
    if server_name == state.services.server_name {
        return server_key(State(state))
            .await
            .unwrap_or_else(|e| Json(json!({ "errcode": e.code(), "error": e.message() })));
    }

    Json(json!({
        "server_name": server_name,
        "key_id": key_id,
        "verify_keys": {
            "ed25519": "remote_key_placeholder"
        }
    }))
}

fn derive_ed25519_verify_key_base64(signing_key: &str) -> Option<String> {
    let signing_key = decode_base64_32(signing_key)?;
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key);
    let verifying_key = signing_key.verifying_key();
    Some(base64::engine::general_purpose::STANDARD_NO_PAD.encode(verifying_key.as_bytes()))
}

fn decode_base64_32(value: &str) -> Option<[u8; 32]> {
    let value = value.trim();
    let engines = [
        base64::engine::general_purpose::STANDARD,
        base64::engine::general_purpose::STANDARD_NO_PAD,
        base64::engine::general_purpose::URL_SAFE,
        base64::engine::general_purpose::URL_SAFE_NO_PAD,
    ];

    for engine in engines {
        if let Ok(bytes) = engine.decode(value) {
            if bytes.len() == 32 {
                let mut out = [0u8; 32];
                out.copy_from_slice(&bytes);
                return Some(out);
            }
        }
    }
    None
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
