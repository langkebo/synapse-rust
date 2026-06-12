use crate::middleware::FederationRequestAuth;
use crate::routes::validate_room_alias;
use crate::routes::AppState;
use axum::extract::{Extension, Json, Path, Query, RawQuery, State};
use serde::Deserialize;
use serde_json::{json, Value};
use synapse_common::*;

pub(super) async fn get_room_auth(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    super::validate_federation_origin_in_room(&state, &room_id, &auth.origin).await?;

    let auth_events = state
        .services
        .rooms
        .event_storage
        .get_state_events(&room_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get auth events", &e))?;

    let auth_chain: Vec<Value> = auth_events
        .into_iter()
        .filter(|e| {
            e.event_type.as_deref() == Some("m.room.create")
                || e.event_type.as_deref() == Some("m.room.member")
                || e.event_type.as_deref() == Some("m.room.power_levels")
                || e.event_type.as_deref() == Some("m.room.join_rules")
                || e.event_type.as_deref() == Some("m.room.history_visibility")
        })
        .map(|e| {
            json!({
                "event_id": e.event_id,
                "type": e.event_type.clone().unwrap_or_default(),
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

pub(super) async fn get_missing_events(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    super::validate_federation_origin_in_room(&state, &room_id, &auth.origin).await?;

    let _earliest_events = body
        .get("earliest_events")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ApiError::bad_request("earliest_events required".to_string()))?;
    let _latest_events = body
        .get("latest_events")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ApiError::bad_request("latest_events required".to_string()))?;
    let limit = body.get("limit").and_then(|v| v.as_i64()).unwrap_or(10).clamp(1, 100);

    let events = state
        .services
        .rooms
        .event_storage
        .get_room_events(&room_id, limit)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get missing events", &e))?;

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

pub(super) async fn get_event_auth(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path((room_id, event_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    super::validate_federation_origin_in_room(&state, &room_id, &auth.origin).await?;

    let event = get_room_event_in_room(&state, &room_id, &event_id).await?;
    let auth_events = state
        .services
        .rooms
        .event_storage
        .get_state_events_at_or_before(&room_id, event.origin_server_ts)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get auth events", &e))?;

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

pub(super) async fn get_event(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(event_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let event = state
        .services
        .rooms
        .event_storage
        .get_event(&event_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get event", &e))?;

    match event {
        Some(e) => {
            super::validate_federation_origin_in_room(&state, &e.room_id, &auth.origin).await?;
            Ok(Json(build_federation_event_response(&state.services.core.server_name, &e)))
        }
        None => Err(ApiError::not_found("Event not found".to_string())),
    }
}

pub(super) async fn get_room_event(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path((room_id, event_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    super::validate_federation_origin_in_room(&state, &room_id, &auth.origin).await?;

    let event = state
        .services
        .rooms
        .event_storage
        .get_event(&event_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get event", &e))?;

    match event {
        Some(e) => {
            if e.room_id != room_id {
                return Err(ApiError::bad_request("Event does not belong to this room".to_string()));
            }
            Ok(Json(build_federation_event_response(&state.services.core.server_name, &e)))
        }
        None => Err(ApiError::not_found("Event not found".to_string())),
    }
}

pub(super) async fn get_state(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(room_id): Path<String>,
    Query(query): Query<FederationStateAtEventQuery>,
) -> Result<Json<Value>, ApiError> {
    super::validate_federation_origin_can_observe_room(&state, &room_id, &auth.origin).await?;

    let mut events = load_federation_state_events(&state, &room_id, query.event_id.as_deref()).await?;
    let (pdus, auth_chain) = build_federation_state_payload(&state.services.core.server_name, &mut events);

    Ok(Json(json!({
        "room_id": room_id,
        "origin": state.services.core.server_name,
        "pdus": pdus,
        "auth_chain": auth_chain
    })))
}

pub(super) async fn get_state_ids(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(room_id): Path<String>,
    Query(query): Query<FederationStateAtEventQuery>,
) -> Result<Json<Value>, ApiError> {
    super::validate_federation_origin_can_observe_room(&state, &room_id, &auth.origin).await?;

    let mut events = load_federation_state_events(&state, &room_id, query.event_id.as_deref()).await?;
    sort_state_events_stably(&mut events);

    let pdu_ids: Vec<String> = events.iter().map(|event| event.event_id.clone()).collect();
    let auth_chain_ids: Vec<String> = events
        .iter()
        .filter(|event| {
            event.event_type.as_deref().is_some_and(synapse_federation::event_auth::EventAuthChain::is_auth_event)
        })
        .map(|event| event.event_id.clone())
        .collect();

    Ok(Json(json!({
        "room_id": room_id,
        "origin": state.services.core.server_name,
        "pdu_ids": pdu_ids,
        "auth_chain_ids": auth_chain_ids
    })))
}

#[axum::debug_handler]
pub(super) async fn room_directory_query(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let room = state
        .services
        .rooms
        .room_storage
        .get_room(&room_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

    if let Some(room) = room {
        if !room.is_public {
            super::validate_federation_origin_in_room(&state, &room_id, &auth.origin).await?;
        }

        return Ok(Json(json!({
            "room_id": room.room_id,
            "servers": [state.services.core.server_name]
        })));
    }

    if room_id.starts_with("ps_") {
        return Err(ApiError::not_found("Private session not supported".to_string()));
    }

    Err(ApiError::not_found("Room not found".to_string()))
}

#[derive(Deserialize)]
pub(super) struct FederationProfileQueryParams {
    user_id: Option<String>,
    field: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct FederationProfileFieldQuery {
    field: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct FederationHierarchyQueryParams {
    max_depth: Option<i32>,
    suggested_only: Option<bool>,
    limit: Option<i32>,
    from: Option<String>,
}

#[axum::debug_handler]
pub(super) async fn profile_query(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Query(params): Query<FederationProfileQueryParams>,
) -> Result<Json<Value>, ApiError> {
    let user_id = params.user_id.ok_or_else(|| ApiError::bad_request("Missing user_id query parameter".to_string()))?;

    build_profile_query_response(&state, &auth.origin, &user_id, params.field.as_deref()).await
}

#[axum::debug_handler]
pub(super) async fn profile_query_legacy(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(user_id): Path<String>,
    Query(params): Query<FederationProfileFieldQuery>,
) -> Result<Json<Value>, ApiError> {
    build_profile_query_response(&state, &auth.origin, &user_id, params.field.as_deref()).await
}

async fn build_profile_query_response(
    state: &AppState,
    origin: &str,
    user_id: &str,
    field: Option<&str>,
) -> Result<Json<Value>, ApiError> {
    if matches!(field, Some(value) if value != "displayname" && value != "avatar_url") {
        return Err(ApiError::bad_request(
            "Invalid field parameter. Allowed values are 'displayname' or 'avatar_url'".to_string(),
        ));
    }

    if !super::user_matches_origin(user_id, &state.services.core.server_name) {
        return Err(ApiError::not_found("User is not hosted on this server".to_string()));
    }

    let profile = state.services.core.registration_service.get_profile(user_id).await?;

    super::validate_federation_origin_shares_user_room(state, user_id, origin).await?;

    let displayname = profile.get("displayname").cloned().unwrap_or(Value::Null);
    let avatar_url = profile.get("avatar_url").cloned().unwrap_or(Value::Null);

    let response = match field {
        None => json!({
            "displayname": displayname,
            "avatar_url": avatar_url
        }),
        Some("displayname") => json!({
            "displayname": displayname
        }),
        Some("avatar_url") => json!({
            "avatar_url": avatar_url
        }),
        Some(_) => unreachable!("field was validated above"),
    };

    Ok(Json(response))
}

pub(super) async fn get_public_rooms(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let limit = params.get("limit").and_then(|v| v.parse().ok()).unwrap_or(10).min(1000);
    let _since = params.get("since").cloned();

    let rooms = state
        .services
        .rooms
        .room_storage
        .get_public_rooms(limit)
        .await
        .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

    let mut room_list = Vec::new();
    for room in rooms {
        room_list.push(json!({
            "room_id": room.room_id,
            "name": room.name,
            "topic": room.topic,
            "avatar_url": room.avatar_url,
            "num_joined_members": room.member_count,
            "world_readable": room.is_public,
            "guest_can_join": false
        }));
    }

    Ok(Json(json!({
        "chunk": room_list,
        "next_batch": null
    })))
}

pub(super) async fn post_public_rooms(
    State(state): State<AppState>,
    Query(_params): Query<Value>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let limit = body.get("limit").and_then(|v| v.as_i64()).unwrap_or(20).min(1000);
    let rooms = state
        .services
        .rooms
        .room_storage
        .get_public_rooms(limit)
        .await
        .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

    let mut room_list = Vec::new();
    for room in rooms {
        room_list.push(json!({
            "room_id": room.room_id,
            "name": room.name,
            "topic": room.topic,
            "avatar_url": room.avatar_url,
            "num_joined_members": room.member_count,
            "world_readable": room.is_public,
            "guest_can_join": false
        }));
    }

    Ok(Json(json!({
        "chunk": room_list,
        "total_room_count_estimate": room_list.len()
    })))
}

pub(super) async fn query_directory(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Query(params): Query<Value>,
) -> Result<Json<Value>, ApiError> {
    let room_alias = params
        .get("room_alias")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing room_alias parameter"))?;
    validate_room_alias(room_alias)?;

    let Some((_, alias_server_name)) = room_alias[1..].rsplit_once(':') else {
        return Err(ApiError::bad_request("Invalid room alias format".to_string()));
    };
    if alias_server_name != state.services.core.server_name {
        return Err(ApiError::not_found("Room alias is not hosted on this server".to_string()));
    }

    let room_id = state.services.rooms.room_service.get_room_by_alias(room_alias).await?;
    let room_id = room_id.ok_or_else(|| {
        ApiError::not_found(format!(
            "Room alias not found: {room_alias}. Create the alias before querying the federation directory."
        ))
    })?;
    let room = state
        .services
        .rooms
        .room_storage
        .get_room(&room_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get room", &e))?
        .ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

    if !room.is_public {
        super::validate_federation_origin_in_room(&state, &room_id, &auth.origin).await?;
    }

    Ok(Json(json!({
        "room_id": room_id,
        "servers": [state.services.core.server_name.clone()]
    })))
}

pub(super) async fn query_destination(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({
        "server_name": state.services.core.server_name,
        "destination": state.services.core.server_name,
        "retry_last_ts": 0,
        "retry_interval_ms": 0
    })))
}

pub(super) async fn timestamp_to_event(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(room_id): Path<String>,
    Query(params): Query<Value>,
) -> Result<Json<Value>, ApiError> {
    if !room_id.starts_with('!') || !room_id.contains(':') {
        return Err(ApiError::bad_request("Invalid room_id format"));
    }

    super::validate_federation_origin_in_room(&state, &room_id, &auth.origin).await?;

    let timestamp = match params.get("ts") {
        Some(v) => {
            if let Some(ts) = v.as_i64() {
                ts
            } else if let Some(s) = v.as_str() {
                s.parse::<i64>().map_err(|_| ApiError::bad_request("Invalid 'ts' parameter"))?
            } else {
                return Err(ApiError::bad_request("Invalid 'ts' parameter"));
            }
        }
        None => return Err(ApiError::bad_request("Missing 'ts' parameter")),
    };

    let _room = state
        .services
        .rooms
        .room_storage
        .get_room(&room_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Room not found"))?;

    let event = state.services.rooms.event_storage.find_event_by_timestamp(&room_id, timestamp).await?;

    if let Some(evt) = event {
        if let Some(arr) = evt.as_array() {
            if let (Some(event_id), Some(ts)) =
                (arr.first().and_then(|v| v.as_str()), arr.get(1).and_then(|v| v.as_i64()))
            {
                return Ok(Json(json!({
                    "event_id": event_id,
                    "origin_server_ts": ts
                })));
            }
        }
    }

    Ok(Json(json!({
        "event_id": null,
        "origin_server_ts": timestamp
    })))
}

pub(super) async fn get_room_hierarchy(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(room_id): Path<String>,
    Query(params): Query<FederationHierarchyQueryParams>,
) -> Result<Json<Value>, ApiError> {
    if !room_id.starts_with('!') || !room_id.contains(':') {
        return Err(ApiError::bad_request("Invalid room_id format"));
    }

    let room = state
        .services
        .rooms
        .room_storage
        .get_room(&room_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Room not found"))?;

    if !room.is_public {
        super::validate_federation_origin_in_room(&state, &room_id, &auth.origin).await?;
    }

    let space = state
        .services
        .rooms
        .space_service
        .get_space_by_room(&room_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Space not found"))?;

    let hierarchy = state
        .services
        .rooms
        .space_service
        .get_space_hierarchy_v1(
            &space.space_id,
            params.max_depth.unwrap_or(1),
            params.suggested_only.unwrap_or(false),
            params.limit,
            params.from.as_deref(),
            None,
        )
        .await?;

    let response = serde_json::to_value(hierarchy)
        .map_err(|e| ApiError::internal_with_log("Failed to serialize hierarchy response", &e))?;

    Ok(Json(response))
}

pub(super) async fn backfill(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(room_id): Path<String>,
    RawQuery(raw_query): RawQuery,
) -> Result<Json<Value>, ApiError> {
    super::validate_federation_origin_can_observe_room(&state, &room_id, &auth.origin).await?;

    let (v, limit) = parse_backfill_query(raw_query)?;

    ::tracing::info!("Backfilling room {} from event(s) {:?} with limit {}", room_id, v, limit);

    let mut backfill_before_ts = i64::MAX;
    for event_id in &v {
        match get_room_event_in_room(&state, &room_id, event_id).await {
            Ok(event) => {
                backfill_before_ts = backfill_before_ts.min(event.origin_server_ts);
            }
            Err(_) => {
                ::tracing::warn!("Backfill: event {} not found in room {}, skipping", event_id, room_id);
            }
        }
    }

    if backfill_before_ts == i64::MAX {
        let recent_events = state
            .services
            .rooms
            .event_storage
            .get_room_events_paginated(&room_id, None, 1, "b")
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;
        if let Some(latest) = recent_events.first() {
            backfill_before_ts = latest.origin_server_ts;
        } else {
            backfill_before_ts = chrono::Utc::now().timestamp_millis();
        }
    }

    let mut events = state
        .services
        .rooms
        .event_storage
        .get_room_events_paginated(&room_id, Some(backfill_before_ts), limit, "b")
        .await
        .map_err(|e| ApiError::internal_with_log("Database error", &e))?;
    sort_room_events_stably(&mut events);

    let mut auth_events = state
        .services
        .rooms
        .event_storage
        .get_state_events_at_or_before(&room_id, backfill_before_ts)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get auth chain", &e))?;
    let (_, auth_chain) = build_federation_state_payload(&state.services.core.server_name, &mut auth_events);

    let mut pdus: Vec<Value> = events
        .into_iter()
        .map(|event| serialize_room_event_minimal(&state.services.core.server_name, &event))
        .collect();

    topological_sort(&mut pdus);

    ::tracing::debug!("Backfill returning {} sorted PDUs", pdus.len());

    Ok(Json(json!({
        "origin": state.services.core.server_name,
        "origin_server_ts": chrono::Utc::now().timestamp_millis(),
        "pdus": pdus,
        "auth_chain": auth_chain
    })))
}

fn build_federation_event_response(server_name: &str, event: &synapse_storage::event::RoomEvent) -> Value {
    let event_origin = match event.origin.trim() {
        "" | "self" | "undefined" => server_name.to_string(),
        value => value.to_string(),
    };

    json!({
        "origin": server_name,
        "origin_server_ts": chrono::Utc::now().timestamp_millis(),
        "pdus": [{
            "event_id": event.event_id,
            "type": event.event_type,
            "sender": event.user_id,
            "content": event.content,
            "state_key": event.state_key,
            "origin_server_ts": event.origin_server_ts,
            "room_id": event.room_id,
            "origin": event_origin
        }]
    })
}

fn normalized_event_origin(server_name: &str, origin: Option<&str>) -> String {
    match origin.map(str::trim) {
        Some("") | Some("self") | Some("undefined") | None => server_name.to_string(),
        Some(value) => value.to_string(),
    }
}

fn serialize_state_event_minimal(server_name: &str, event: &synapse_storage::event::StateEvent) -> Value {
    json!({
        "event_id": event.event_id,
        "type": event.event_type,
        "sender": event.user_id.as_deref().unwrap_or(&event.sender),
        "content": event.content,
        "state_key": event.state_key,
        "origin_server_ts": event.origin_server_ts,
        "room_id": event.room_id,
        "origin": normalized_event_origin(server_name, event.origin.as_deref())
    })
}

fn serialize_room_event_minimal(server_name: &str, event: &synapse_storage::event::RoomEvent) -> Value {
    json!({
        "event_id": event.event_id,
        "type": event.event_type,
        "sender": event.user_id,
        "content": event.content,
        "state_key": event.state_key,
        "origin_server_ts": event.origin_server_ts,
        "room_id": event.room_id,
        "origin": normalized_event_origin(server_name, Some(&event.origin))
    })
}

fn sort_state_events_stably(events: &mut [synapse_storage::event::StateEvent]) {
    events.sort_by(|left, right| {
        right.origin_server_ts.cmp(&left.origin_server_ts).then_with(|| left.event_id.cmp(&right.event_id))
    });
}

fn sort_room_events_stably(events: &mut [synapse_storage::event::RoomEvent]) {
    events.sort_by(|left, right| {
        right
            .depth
            .cmp(&left.depth)
            .then_with(|| right.origin_server_ts.cmp(&left.origin_server_ts))
            .then_with(|| left.event_id.cmp(&right.event_id))
    });
}

fn build_federation_state_payload(
    server_name: &str,
    events: &mut [synapse_storage::event::StateEvent],
) -> (Vec<Value>, Vec<Value>) {
    sort_state_events_stably(events);

    let pdus = events.iter().map(|event| serialize_state_event_minimal(server_name, event)).collect();
    let auth_chain = events
        .iter()
        .filter(|event| {
            event.event_type.as_deref().is_some_and(synapse_federation::event_auth::EventAuthChain::is_auth_event)
        })
        .map(|event| serialize_state_event_minimal(server_name, event))
        .collect();

    (pdus, auth_chain)
}

#[derive(Deserialize, Default)]
pub(super) struct FederationStateAtEventQuery {
    event_id: Option<String>,
}

async fn get_room_event_in_room(
    state: &AppState,
    room_id: &str,
    event_id: &str,
) -> Result<synapse_storage::event::RoomEvent, ApiError> {
    let event = state
        .services
        .rooms
        .event_storage
        .get_event(event_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get event", &e))?
        .ok_or_else(|| ApiError::not_found("Event not found".to_string()))?;

    if event.room_id != room_id {
        return Err(ApiError::bad_request("Event does not belong to this room".to_string()));
    }

    Ok(event)
}

async fn load_federation_state_events(
    state: &AppState,
    room_id: &str,
    event_id: Option<&str>,
) -> Result<Vec<synapse_storage::event::StateEvent>, ApiError> {
    match event_id {
        Some(event_id) => {
            let event = get_room_event_in_room(state, room_id, event_id).await?;
            state
                .services
                .rooms
                .event_storage
                .get_state_events_at_or_before(room_id, event.origin_server_ts)
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to get state", &e))
        }
        None => state
            .services
            .rooms
            .event_storage
            .get_state_events(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get state", &e)),
    }
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

    if sorted_indices.len() == pdus.len() {
        let mut sorted_pdus = Vec::with_capacity(pdus.len());
        for idx in sorted_indices {
            sorted_pdus.push(pdus[idx].clone());
        }
        *pdus = sorted_pdus;
    }
}

fn parse_backfill_query(raw_query: Option<String>) -> Result<(Vec<String>, i64), ApiError> {
    let mut event_ids = Vec::new();
    let mut limit = 10_i64;

    if let Some(raw_query) = raw_query {
        for (key, value) in url::form_urlencoded::parse(raw_query.as_bytes()) {
            match key.as_ref() {
                "v" if !value.is_empty() => event_ids.push(value.into_owned()),
                "limit" => {
                    limit = value
                        .parse::<i64>()
                        .map_err(|_| ApiError::bad_request("Invalid limit query parameter".to_string()))?;
                }
                _ => {}
            }
        }
    }

    Ok((event_ids, limit.clamp(1, 100)))
}
