use crate::middleware::FederationRequestAuth;
use crate::routes::context::FederationContext;
use crate::routes::extractors::UserId;
use crate::routes::validate_room_alias;
use axum::extract::{Extension, Json, Path, Query, RawQuery, State};
use serde::Deserialize;
use serde_json::{json, Value};
use synapse_common::current_timestamp_millis;
use synapse_common::*;

use crate::routes::extractors::EventId;
use crate::routes::extractors::RoomId;
/// See [`get_room_auth`].
pub(super) async fn get_room_auth(
    State(ctx): State<FederationContext>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(room_id): Path<RoomId>,
) -> Result<Json<Value>, ApiError> {
    super::validate_federation_origin_can_observe_room(&ctx, &room_id, &auth.origin).await?;

    let auth_events = ctx.room_service.messaging().get_state_event_records(&room_id).await.map_err(ApiError::from)?;

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

/// See [`get_missing_events`].
pub(super) async fn get_missing_events(
    State(ctx): State<FederationContext>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(room_id): Path<RoomId>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    super::validate_federation_origin_can_observe_room(&ctx, &room_id, &auth.origin).await?;

    let earliest_events: Vec<String> = body
        .get("earliest_events")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ApiError::bad_request("earliest_events required".to_string()))?
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();
    let latest_events: Vec<String> = body
        .get("latest_events")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ApiError::bad_request("latest_events required".to_string()))?
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();
    let limit = body.get("limit").and_then(|v| v.as_i64()).unwrap_or(10).clamp(1, 100);

    // Walk the event DAG backwards from `latest_events` via `event_edges`
    // until we hit `earliest_events`, collecting the events in between.
    // This is the spec-compliant response to `/get_missing_events`: the
    // requester already has `earliest_events` and `latest_events`, and wants
    // the events that connect them.
    let events = ctx
        .room_service
        .messaging()
        .get_missing_events_between(&room_id, &earliest_events, &latest_events, limit)
        .await?;

    Ok(Json(json!({
        "events": events
    })))
}

/// See [`get_event_auth`].
pub(super) async fn get_event_auth(
    State(ctx): State<FederationContext>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path((room_id, event_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    super::validate_federation_origin_can_observe_room(&ctx, &room_id, &auth.origin).await?;

    let event = get_room_event_in_room(&ctx, &room_id, &event_id).await?;
    let auth_events = ctx
        .room_service
        .messaging()
        .get_state_events_at_or_before(&room_id, event.origin_server_ts)
        .await
        .map_err(ApiError::from)?;

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

/// See [`get_event`].
pub(super) async fn get_event(
    State(ctx): State<FederationContext>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(event_id): Path<EventId>,
) -> Result<Json<Value>, ApiError> {
    let event = ctx.room_service.messaging().get_event_record(&event_id).await?;

    match event {
        Some(e) => {
            super::validate_federation_origin_can_observe_room(&ctx, &e.room_id, &auth.origin).await?;
            Ok(Json(build_federation_event_response(&ctx.server_name, &e)))
        }
        None => Err(ApiError::not_found("Event not found".to_string())),
    }
}

/// See [`get_room_event`].
pub(super) async fn get_room_event(
    State(ctx): State<FederationContext>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path((room_id, event_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    super::validate_federation_origin_can_observe_room(&ctx, &room_id, &auth.origin).await?;

    let event = ctx.room_service.messaging().get_event_record(&event_id).await?;

    match event {
        Some(e) => {
            if e.room_id != room_id {
                return Err(ApiError::bad_request("Event does not belong to this room".to_string()));
            }
            Ok(Json(build_federation_event_response(&ctx.server_name, &e)))
        }
        None => Err(ApiError::not_found("Event not found".to_string())),
    }
}

/// See [`get_state`].
pub(super) async fn get_state(
    State(ctx): State<FederationContext>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(room_id): Path<RoomId>,
    Query(query): Query<FederationStateAtEventQuery>,
) -> Result<Json<Value>, ApiError> {
    super::validate_federation_origin_can_observe_room(&ctx, &room_id, &auth.origin).await?;

    let mut events = load_federation_state_events(&ctx, &room_id, query.event_id.as_deref()).await?;
    let (pdus, auth_chain) = build_federation_state_payload(&ctx.server_name, &mut events);

    Ok(Json(json!({
        "room_id": room_id,
        "origin": ctx.server_name,
        "pdus": pdus,
        "auth_chain": auth_chain
    })))
}

/// See [`get_state_ids`].
pub(super) async fn get_state_ids(
    State(ctx): State<FederationContext>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(room_id): Path<RoomId>,
    Query(query): Query<FederationStateAtEventQuery>,
) -> Result<Json<Value>, ApiError> {
    super::validate_federation_origin_can_observe_room(&ctx, &room_id, &auth.origin).await?;

    let mut events = load_federation_state_events(&ctx, &room_id, query.event_id.as_deref()).await?;
    sort_state_events_stably(&mut events);

    let pdu_ids: Vec<String> = events.iter().map(|event| event.event_id.clone()).collect();
    let auth_chain_ids: Vec<String> = events
        .iter()
        .filter(|event| {
            event.event_type.as_deref().is_some_and(crate::federation::event_auth::EventAuthChain::is_auth_event)
        })
        .map(|event| event.event_id.clone())
        .collect();

    Ok(Json(json!({
        "room_id": room_id,
        "origin": ctx.server_name,
        "pdu_ids": pdu_ids,
        "auth_chain_ids": auth_chain_ids
    })))
}

/// See [`room_directory_query`].
#[axum::debug_handler]
pub(super) async fn room_directory_query(
    State(ctx): State<FederationContext>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(room_id): Path<RoomId>,
) -> Result<Json<Value>, ApiError> {
    let room = ctx.room_service.state().get_room_record(&room_id).await.map_err(ApiError::from)?;

    if let Some(room) = room {
        if !room.is_public {
            super::validate_federation_origin_can_observe_room(&ctx, &room_id, &auth.origin).await?;
        }

        return Ok(Json(json!({
            "room_id": room.room_id,
            "servers": [ctx.server_name]
        })));
    }

    if room_id.starts_with("ps_") {
        return Err(ApiError::not_found("Private session not supported".to_string()));
    }

    Err(ApiError::not_found("Room not found".to_string()))
}

/// The `FederationProfileQueryParams` struct.
#[derive(Deserialize)]
pub(super) struct FederationProfileQueryParams {
    user_id: Option<String>,
    field: Option<String>,
}

/// The `FederationProfileFieldQuery` struct.
#[derive(Deserialize)]
pub(super) struct FederationProfileFieldQuery {
    field: Option<String>,
}

/// The `FederationHierarchyQueryParams` struct.
#[derive(Deserialize)]
pub(super) struct FederationHierarchyQueryParams {
    max_depth: Option<i32>,
    suggested_only: Option<bool>,
    limit: Option<i32>,
    from: Option<String>,
}

/// See [`profile_query`].
#[axum::debug_handler]
pub(super) async fn profile_query(
    State(ctx): State<FederationContext>,
    Extension(auth): Extension<FederationRequestAuth>,
    Query(params): Query<FederationProfileQueryParams>,
) -> Result<Json<Value>, ApiError> {
    let user_id = params.user_id.ok_or_else(|| ApiError::bad_request("Missing user_id query parameter".to_string()))?;

    build_profile_query_response(&ctx, &auth.origin, &user_id, params.field.as_deref()).await
}

/// See [`profile_query_legacy`].
#[axum::debug_handler]
pub(super) async fn profile_query_legacy(
    State(ctx): State<FederationContext>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(user_id): Path<UserId>,
    Query(params): Query<FederationProfileFieldQuery>,
) -> Result<Json<Value>, ApiError> {
    build_profile_query_response(&ctx, &auth.origin, &user_id, params.field.as_deref()).await
}

async fn build_profile_query_response(
    ctx: &FederationContext,
    origin: &str,
    user_id: &str,
    field: Option<&str>,
) -> Result<Json<Value>, ApiError> {
    if matches!(field, Some(value) if value != "displayname" && value != "avatar_url") {
        return Err(ApiError::bad_request(
            "Invalid field parameter. Allowed values are 'displayname' or 'avatar_url'".to_string(),
        ));
    }

    if !super::user_matches_origin(user_id, &ctx.server_name) {
        return Err(ApiError::not_found("User is not hosted on this server".to_string()));
    }

    let profile = ctx.registration_service.get_profile(user_id).await?;

    super::validate_federation_origin_shares_user_room(ctx, user_id, origin).await?;

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
        // Defensive: validation above should reject unknown fields, but under
        // `panic = "abort"` we must not crash if the invariant ever breaks.
        Some(other) => return Err(ApiError::bad_request(format!("Invalid field parameter: {other}"))),
    };

    Ok(Json(response))
}

/// See [`get_public_rooms`].
pub(super) async fn get_public_rooms(
    State(ctx): State<FederationContext>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let limit = params.get("limit").and_then(|v| v.parse().ok()).unwrap_or(10).min(1000);
    let _since = params.get("since").cloned();

    let rooms = ctx.room_service.state().get_public_rooms_paginated(limit, None, None).await?;

    let total = ctx.room_service.state().count_public_rooms().await?;

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
        "total_room_count_estimate": total,
        "next_batch": null
    })))
}

/// See [`post_public_rooms`].
pub(super) async fn post_public_rooms(
    State(ctx): State<FederationContext>,
    Query(_params): Query<Value>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let limit = body.get("limit").and_then(|v| v.as_i64()).unwrap_or(20).min(1000);
    let rooms = ctx.room_service.state().get_public_rooms_paginated(limit, None, None).await?;

    let total = ctx.room_service.state().count_public_rooms().await?;

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
        "total_room_count_estimate": total
    })))
}

/// See [`query_directory`].
pub(super) async fn query_directory(
    State(ctx): State<FederationContext>,
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
    if alias_server_name != ctx.server_name {
        return Err(ApiError::not_found("Room alias is not hosted on this server".to_string()));
    }

    let room_id = ctx.room_service.state().get_room_by_alias(room_alias).await?;
    let room_id = room_id.ok_or_else(|| {
        ApiError::not_found(format!(
            "Room alias not found: {room_alias}. Create the alias before querying the federation directory."
        ))
    })?;
    let room = ctx
        .room_service
        .state()
        .get_room_record(&room_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

    if !room.is_public {
        super::validate_federation_origin_can_observe_room(&ctx, &room_id, &auth.origin).await?;
    }

    Ok(Json(json!({
        "room_id": room_id,
        "servers": [ctx.server_name.clone()]
    })))
}

/// See [`query_destination`].
pub(super) async fn query_destination(State(ctx): State<FederationContext>) -> Result<Json<Value>, ApiError> {
    let mut room_versions = federation_room_versions_capability();
    if let Some(obj) = room_versions.as_object_mut() {
        obj.insert("default".to_string(), json!(DEFAULT_ROOM_VERSION));
    }
    Ok(Json(json!({
        "server_name": ctx.server_name,
        "destination": ctx.server_name,
        "retry_last_ts": 0,
        "retry_interval_ms": 0,
        "capabilities": {
            "m.change_password": crate::routes::handlers::versions::change_password_capability_enabled(&ctx.config),
            "m.room_versions": room_versions
        }
    })))
}

/// See [`timestamp_to_event`].
pub(super) async fn timestamp_to_event(
    State(ctx): State<FederationContext>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(room_id): Path<RoomId>,
    Query(params): Query<Value>,
) -> Result<Json<Value>, ApiError> {
    if !room_id.starts_with('!') || !room_id.contains(':') {
        return Err(ApiError::bad_request("Invalid room_id format"));
    }

    super::validate_federation_origin_can_observe_room(&ctx, &room_id, &auth.origin).await?;

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

    let _room = ctx
        .room_service
        .state()
        .get_room_record(&room_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::not_found("Room not found"))?;

    let event = ctx.room_service.messaging().find_event_by_timestamp(&room_id, timestamp, true).await?;

    if let Some(evt) = event {
        let (event_id, ts) = evt;
        return Ok(Json(json!({
            "event_id": event_id,
            "origin_server_ts": ts
        })));
    }

    Ok(Json(json!({
        "event_id": null,
        "origin_server_ts": timestamp
    })))
}

/// See [`get_room_hierarchy`].
pub(super) async fn get_room_hierarchy(
    State(ctx): State<FederationContext>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(room_id): Path<RoomId>,
    Query(params): Query<FederationHierarchyQueryParams>,
) -> Result<Json<Value>, ApiError> {
    if !room_id.starts_with('!') || !room_id.contains(':') {
        return Err(ApiError::bad_request("Invalid room_id format"));
    }

    let room = ctx
        .room_service
        .state()
        .get_room_record(&room_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::not_found("Room not found"))?;

    if !room.is_public {
        super::validate_federation_origin_can_observe_room(&ctx, &room_id, &auth.origin).await?;
    }

    let space =
        ctx.space_service.get_space_by_room(&room_id).await?.ok_or_else(|| ApiError::not_found("Space not found"))?;

    let hierarchy = ctx
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
        .map_err(|e| ApiError::internal_with_cause("Failed to serialize hierarchy response", e))?;

    Ok(Json(response))
}

/// See [`backfill`].
pub(super) async fn backfill(
    State(ctx): State<FederationContext>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(room_id): Path<RoomId>,
    RawQuery(raw_query): RawQuery,
) -> Result<Json<Value>, ApiError> {
    super::validate_federation_origin_can_observe_room(&ctx, &room_id, &auth.origin).await?;

    let (v, limit) = parse_backfill_query(raw_query)?;

    ::tracing::info!("Backfilling room {} from event(s) {:?} with limit {}", room_id, v, limit);

    let mut backfill_before_ts = i64::MAX;
    for event_id in &v {
        match get_room_event_in_room(&ctx, &room_id, event_id).await {
            Ok(event) => {
                backfill_before_ts = backfill_before_ts.min(event.origin_server_ts);
            }
            Err(_) => {
                ::tracing::warn!("Backfill: event {} not found in room {}, skipping", event_id, room_id);
            }
        }
    }

    if backfill_before_ts == i64::MAX {
        let recent_events = ctx
            .room_service
            .messaging()
            .get_room_events_paginated_admin(&room_id, None, 1, "b")
            .await
            .map_err(ApiError::from)?;
        if let Some(latest) = recent_events.first() {
            backfill_before_ts = latest.origin_server_ts;
        } else {
            backfill_before_ts = current_timestamp_millis();
        }
    }

    let mut events = ctx
        .room_service
        .messaging()
        .get_room_events_paginated_admin(&room_id, Some(backfill_before_ts), limit, "b")
        .await
        .map_err(ApiError::from)?;
    sort_room_events_stably(&mut events);

    let mut auth_events = ctx
        .room_service
        .messaging()
        .get_state_events_at_or_before(&room_id, backfill_before_ts)
        .await
        .map_err(ApiError::from)?;
    let (_, auth_chain) = build_federation_state_payload(&ctx.server_name, &mut auth_events);

    let mut pdus: Vec<Value> =
        events.into_iter().map(|event| serialize_room_event_minimal(&ctx.server_name, &event)).collect();

    topological_sort(&mut pdus);

    ::tracing::debug!("Backfill returning {} sorted PDUs", pdus.len());

    Ok(Json(json!({
        "origin": ctx.server_name,
        "origin_server_ts": current_timestamp_millis(),
        "pdus": pdus,
        "auth_chain": auth_chain
    })))
}

fn build_federation_event_response(server_name: &str, event: &synapse_services::event::RoomEvent) -> Value {
    let event_origin = match event.origin.trim() {
        "" | "self" | "undefined" => server_name.to_string(),
        value => value.to_string(),
    };

    json!({
        "origin": server_name,
        "origin_server_ts": current_timestamp_millis(),
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

fn serialize_state_event_minimal(server_name: &str, event: &synapse_services::event::StateEvent) -> Value {
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

fn serialize_room_event_minimal(server_name: &str, event: &synapse_services::event::RoomEvent) -> Value {
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

fn sort_state_events_stably(events: &mut [synapse_services::event::StateEvent]) {
    events.sort_by(|left, right| {
        right.origin_server_ts.cmp(&left.origin_server_ts).then_with(|| left.event_id.cmp(&right.event_id))
    });
}

fn sort_room_events_stably(events: &mut [synapse_services::event::RoomEvent]) {
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
    events: &mut [synapse_services::event::StateEvent],
) -> (Vec<Value>, Vec<Value>) {
    sort_state_events_stably(events);

    let pdus = events.iter().map(|event| serialize_state_event_minimal(server_name, event)).collect();
    let auth_chain = events
        .iter()
        .filter(|event| {
            event.event_type.as_deref().is_some_and(crate::federation::event_auth::EventAuthChain::is_auth_event)
        })
        .map(|event| serialize_state_event_minimal(server_name, event))
        .collect();

    (pdus, auth_chain)
}

/// The `FederationStateAtEventQuery` struct.
#[derive(Deserialize, Default)]
pub(super) struct FederationStateAtEventQuery {
    event_id: Option<String>,
}

async fn get_room_event_in_room(
    ctx: &FederationContext,
    room_id: &str,
    event_id: &str,
) -> Result<synapse_services::event::RoomEvent, ApiError> {
    let event = ctx.room_service.messaging().get_event_record_in_room(room_id, event_id).await?;

    Ok(event)
}

async fn load_federation_state_events(
    ctx: &FederationContext,
    room_id: &str,
    event_id: Option<&str>,
) -> Result<Vec<synapse_services::event::StateEvent>, ApiError> {
    match event_id {
        Some(event_id) => {
            let event = get_room_event_in_room(ctx, room_id, event_id).await?;
            ctx.room_service
                .messaging()
                .get_state_events_at_or_before(room_id, event.origin_server_ts)
                .await
                .map_err(ApiError::from)
        }
        None => ctx.room_service.messaging().get_state_event_records(room_id).await.map_err(ApiError::from),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_backfill_query_none_returns_defaults() {
        let (event_ids, limit) = parse_backfill_query(None).unwrap();
        assert!(event_ids.is_empty());
        assert_eq!(limit, 10);
    }

    #[test]
    fn parse_backfill_query_extracts_event_ids_and_limit() {
        let (event_ids, limit) = parse_backfill_query(Some("v=event1&v=event2&limit=50".to_string())).unwrap();
        assert_eq!(event_ids, vec!["event1".to_string(), "event2".to_string()]);
        assert_eq!(limit, 50);
    }

    #[test]
    fn parse_backfill_query_clamps_limit_to_range() {
        let (_, high) = parse_backfill_query(Some("limit=999".to_string())).unwrap();
        assert_eq!(high, 100);
        let (_, low) = parse_backfill_query(Some("limit=0".to_string())).unwrap();
        assert_eq!(low, 1);
        let (_, neg) = parse_backfill_query(Some("limit=-5".to_string())).unwrap();
        assert_eq!(neg, 1);
    }

    #[test]
    fn parse_backfill_query_invalid_limit_returns_error() {
        assert!(parse_backfill_query(Some("limit=abc".to_string())).is_err());
    }

    #[test]
    fn parse_backfill_query_skips_empty_v() {
        let (event_ids, _) = parse_backfill_query(Some("v=&v=event1".to_string())).unwrap();
        assert_eq!(event_ids, vec!["event1".to_string()]);
    }

    #[test]
    fn normalized_event_origin_falls_back_to_server_name() {
        assert_eq!(normalized_event_origin("server.example", None), "server.example");
        assert_eq!(normalized_event_origin("server.example", Some("")), "server.example");
        assert_eq!(normalized_event_origin("server.example", Some("self")), "server.example");
        assert_eq!(normalized_event_origin("server.example", Some("undefined")), "server.example");
    }

    #[test]
    fn normalized_event_origin_keeps_remote_origin() {
        assert_eq!(normalized_event_origin("server.example", Some("remote.example")), "remote.example");
        assert_eq!(normalized_event_origin("server.example", Some("  remote.example  ")), "remote.example");
    }

    #[test]
    fn topological_sort_orders_by_prev_events() {
        let mut pdus = vec![
            json!({"event_id": "C", "prev_events": ["B"]}),
            json!({"event_id": "B", "prev_events": ["A"]}),
            json!({"event_id": "A"}),
        ];
        topological_sort(&mut pdus);
        let order: Vec<&str> = pdus.iter().map(|p| p["event_id"].as_str().unwrap()).collect();
        assert_eq!(order, vec!["A", "B", "C"]);
    }

    #[test]
    fn topological_sort_independent_events_keep_order() {
        let mut pdus = vec![json!({"event_id": "A"}), json!({"event_id": "B"})];
        topological_sort(&mut pdus);
        let order: Vec<&str> = pdus.iter().map(|p| p["event_id"].as_str().unwrap()).collect();
        assert_eq!(order, vec!["A", "B"]);
    }

    #[test]
    fn topological_sort_cycle_keeps_original_order() {
        let mut pdus =
            vec![json!({"event_id": "A", "prev_events": ["B"]}), json!({"event_id": "B", "prev_events": ["A"]})];
        topological_sort(&mut pdus);
        // 有环无法完成拓扑排序，保持原序。
        let order: Vec<&str> = pdus.iter().map(|p| p["event_id"].as_str().unwrap()).collect();
        assert_eq!(order, vec!["A", "B"]);
    }

    fn make_room_event(
        event_id: &str,
        depth: i64,
        origin_server_ts: i64,
        origin: &str,
    ) -> synapse_services::event::RoomEvent {
        synapse_services::event::RoomEvent {
            event_id: event_id.to_string(),
            room_id: "!r:server.example".to_string(),
            user_id: "@alice:server.example".to_string(),
            event_type: "m.room.message".to_string(),
            content: json!({"body": "hi"}),
            state_key: None,
            depth,
            origin_server_ts,
            processed_ts: 0,
            not_before: 0,
            status: None,
            origin: origin.to_string(),
            stream_ordering: None,
            redacts: None,
        }
    }

    fn make_state_event(
        event_id: &str,
        event_type: &str,
        origin_server_ts: i64,
        origin: Option<&str>,
    ) -> synapse_services::event::StateEvent {
        synapse_services::event::StateEvent {
            event_id: event_id.to_string(),
            room_id: "!r:server.example".to_string(),
            sender: "@alice:server.example".to_string(),
            event_type: Some(event_type.to_string()),
            content: json!({}),
            state_key: Some(String::new()),
            unsigned: None,
            is_redacted: None,
            origin_server_ts,
            depth: None,
            processed_ts: None,
            not_before: None,
            status: None,
            origin: origin.map(str::to_string),
            user_id: None,
            stream_ordering: None,
        }
    }

    #[test]
    fn sort_state_events_stably_orders_by_ts_desc_then_event_id_asc() {
        let mut events = vec![
            make_state_event("e1", "m.room.create", 100, None),
            make_state_event("e2", "m.room.name", 300, None),
            make_state_event("e3", "m.room.member", 200, None),
            make_state_event("e1b", "m.room.topic", 100, None),
        ];
        sort_state_events_stably(&mut events);
        let order: Vec<&str> = events.iter().map(|e| e.event_id.as_str()).collect();
        // ts 300 最前；ts 200 其次；ts 100 的 e1/e1b 按 event_id 升序（e1 < e1b）。
        assert_eq!(order, vec!["e2", "e3", "e1", "e1b"]);
    }

    #[test]
    fn sort_room_events_stably_orders_by_depth_then_ts_then_event_id() {
        let mut events = vec![
            make_room_event("r1", 1, 100, "self"),
            make_room_event("r2", 3, 100, "self"),
            make_room_event("r3", 3, 200, "self"),
            make_room_event("r4", 3, 200, "self"),
        ];
        sort_room_events_stably(&mut events);
        let order: Vec<&str> = events.iter().map(|e| e.event_id.as_str()).collect();
        // depth 3 的三条在前（ts 200 的 r3/r4 在 ts 100 的 r2 前；r3<r4 按 id），depth 1 最后。
        assert_eq!(order, vec!["r3", "r4", "r2", "r1"]);
    }

    #[test]
    fn serialize_state_event_minimal_normalizes_origin() {
        let event = make_state_event("e1", "m.room.create", 123, None);
        let json = serialize_state_event_minimal("server.example", &event);
        assert_eq!(json["event_id"], "e1");
        assert_eq!(json["origin"], "server.example");
        assert_eq!(json["origin_server_ts"], 123);

        let remote = make_state_event("e2", "m.room.create", 123, Some("remote.example"));
        let json = serialize_state_event_minimal("server.example", &remote);
        assert_eq!(json["origin"], "remote.example");
    }

    #[test]
    fn serialize_room_event_minimal_uses_sender_user_id() {
        let event = make_room_event("e1", 1, 123, "remote.example");
        let json = serialize_room_event_minimal("server.example", &event);
        assert_eq!(json["event_id"], "e1");
        assert_eq!(json["sender"], "@alice:server.example");
        assert_eq!(json["origin"], "remote.example");
    }

    #[test]
    fn build_federation_event_response_normalizes_self_origin() {
        for origin in ["", "self", "undefined"] {
            let event = make_room_event("e1", 1, 123, origin);
            let json = build_federation_event_response("server.example", &event);
            let pdus = json["pdus"].as_array().unwrap();
            assert_eq!(pdus.len(), 1);
            assert_eq!(pdus[0]["origin"], "server.example");
            assert_eq!(pdus[0]["event_id"], "e1");
        }
    }

    #[test]
    fn build_federation_event_response_keeps_remote_origin() {
        let event = make_room_event("e1", 1, 123, "remote.example");
        let json = build_federation_event_response("server.example", &event);
        assert_eq!(json["pdus"][0]["origin"], "remote.example");
    }

    #[test]
    fn build_federation_state_payload_splits_auth_chain() {
        // m.room.create 是 auth event；m.room.message 不是。
        let mut events = vec![
            make_state_event("create", "m.room.create", 100, None),
            make_state_event("message", "m.room.message", 200, None),
        ];
        let (pdus, auth_chain) = build_federation_state_payload("server.example", &mut events);
        assert_eq!(pdus.len(), 2);
        // 排序后 ts 200 的 message 在前。
        assert_eq!(pdus[0]["event_id"], "message");
        // auth_chain 只包含 is_auth_event 判定为 true 的事件（m.room.create）。
        let auth_ids: Vec<&str> = auth_chain.iter().map(|e| e["event_id"].as_str().unwrap()).collect();
        assert_eq!(auth_ids, vec!["create"]);
    }
}
