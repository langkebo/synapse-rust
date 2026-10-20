use crate::middleware::FederationRequestAuth;
use crate::routes::context::FederationContext;
use crate::routes::directory_reporting::{decode_public_rooms_cursor, encode_public_rooms_cursor};
use crate::routes::extractors::UserId;
use crate::routes::public_rooms_filter::parse_public_rooms_filter;
use crate::routes::validate_room_alias;
use axum::extract::{Extension, Json, Path, Query, State};
use serde::Deserialize;
use serde_json::{json, Value};
use synapse_common::current_timestamp_millis;
use synapse_common::*;

use crate::routes::extractors::EventId;
use crate::routes::extractors::RoomId;
use crate::routes::MatrixJson;
/// See [`get_room_auth`].
pub(super) async fn get_room_auth(
    State(ctx): State<FederationContext>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(room_id): Path<RoomId>,
) -> Result<Json<Value>, ApiError> {
    super::validate_federation_origin_can_observe_room(&ctx, &room_id, &auth.origin).await?;

    let auth_events = ctx.room_service.messaging().get_state_event_records(&room_id).await.map_err(ApiError::from)?;

    let room_version = super::pdu::room_version_from_state(&auth_events);
    let auth_chain_records: Vec<&synapse_services::event::StateEvent> =
        auth_events.iter().filter(|record| super::pdu::is_auth_chain_member(record, &room_version)).collect();
    let auth_chain = super::pdu::build_pdus(&ctx, &auth_chain_records).await;

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
    MatrixJson(body): MatrixJson<Value>,
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

    // Selection rule unchanged (every state event at or before the target);
    // only the serialisation moved to the shared PDU projection.
    let auth_chain_records: Vec<&synapse_services::event::StateEvent> = auth_events.iter().collect();
    let auth_chain = super::pdu::build_pdus(&ctx, &auth_chain_records).await;

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
    let (pdus, auth_chain) = build_federation_state_payload(&ctx, &mut events).await;

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

/// Query parameters accepted by [`get_public_rooms`].
///
/// Deliberately **not** `deny_unknown_fields`: the handler used to take a bare
/// `HashMap<String, String>` and ignored unknown parameters.
#[derive(Debug, Default, Deserialize)]
pub(super) struct PublicRoomsQuery {
    limit: Option<String>,
    since: Option<String>,
}

/// See [`get_public_rooms`].
pub(super) async fn get_public_rooms(
    State(ctx): State<FederationContext>,
    Query(params): Query<PublicRoomsQuery>,
) -> Result<Json<Value>, ApiError> {
    let limit = params.limit.as_deref().and_then(|v| v.parse::<u64>().ok()).unwrap_or(20).clamp(1, 1000) as i64;
    // `since` 与客户端 `/publicRooms` 共用同一个 keyset 游标（`<created_ts>|<room_id>`）；
    // 以前的实现把它读进 `_since` 后丢弃，于是对端永远从第一页重新开始。
    let cursor = decode_public_rooms_cursor(params.since.as_deref());

    // GET 变体按 spec 只接受 `limit` / `since` / `server`：`filter`（含
    // `generic_search_term`）是 **POST 变体**的字段（MSC2197），故这里按 `None` 取值。
    // POST 变体的接线见 [`post_public_rooms`]。
    let (rooms, total) = tokio::try_join!(
        async {
            ctx.room_service
                .state()
                .get_public_rooms_paginated(
                    limit,
                    cursor.map(|(ts, _)| ts),
                    cursor.map(|(_, room_id)| room_id),
                    None,
                    false,
                )
                .await
        },
        async { ctx.room_service.state().count_public_rooms(None, false).await }
    )?;

    // 只有返回满页时才有下一页，语义与客户端侧一致。
    let next_batch = if rooms.len() as i64 == limit {
        rooms.last().map(|room| encode_public_rooms_cursor(room.created_ts, &room.room_id))
    } else {
        None
    };

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
        "next_batch": next_batch
    })))
}

/// See [`post_public_rooms`].
pub(super) async fn post_public_rooms(
    State(ctx): State<FederationContext>,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, ApiError> {
    let limit = body.get("limit").and_then(|v| v.as_i64()).unwrap_or(20).clamp(1, 1000);
    // `since` 与 GET 变体 / 客户端侧共用同一个 keyset 游标（`<created_ts>|<room_id>`）。
    let cursor = decode_public_rooms_cursor(body.get("since").and_then(Value::as_str));
    // D-110（2026-09-30 裁定 ①）：federation 侧按 MSC2197 接线 `filter.generic_search_term`，
    // 与 client 侧**共用同一个解析器**（[`parse_public_rooms_filter`]）—— 解析规则、错误语义
    // （形状非法一律 400 `M_INVALID_PARAM` 而不是静默忽略）与 `room_types` 行为都保持一致。
    //
    // 搜索路径是 `WHERE … LIKE` + `ORDER BY name`，与 keyset 游标（`created_ts, room_id`）
    // **不同构**：该路径仍不发游标（否则对端续传会跳进未过滤的列表）。
    let filter = parse_public_rooms_filter(&body)?;
    let room_types = filter.room_types.as_deref();

    let (rooms, total) = if let Some(term) = filter.search_term.as_deref() {
        ctx.room_service.state().search_public_rooms(term, limit, room_types, filter.include_room_type_null).await?
    } else {
        tokio::try_join!(
            async {
                ctx.room_service
                    .state()
                    .get_public_rooms_paginated(
                        limit,
                        cursor.map(|(ts, _)| ts),
                        cursor.map(|(_, room_id)| room_id),
                        room_types,
                        filter.include_room_type_null,
                    )
                    .await
            },
            async { ctx.room_service.state().count_public_rooms(room_types, filter.include_room_type_null).await }
        )?
    };

    let next_batch = if filter.search_term.is_none() && rooms.len() as i64 == limit {
        rooms.last().map(|room| encode_public_rooms_cursor(room.created_ts, &room.room_id))
    } else {
        None
    };

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
        "next_batch": next_batch
    })))
}

/// Query parameters accepted by [`query_directory`].
///
/// Deliberately **not** `deny_unknown_fields`: the handler used to take a bare
/// `Query<Value>` and ignored unknown parameters. `room_alias` stays optional
/// at the serde layer so the missing-parameter answer keeps its Matrix
/// `M_BAD_REQUEST` body.
#[derive(Debug, Deserialize)]
pub(super) struct DirectoryQueryParams {
    room_alias: Option<String>,
}

/// See [`query_directory`].
pub(super) async fn query_directory(
    State(ctx): State<FederationContext>,
    Extension(auth): Extension<FederationRequestAuth>,
    Query(params): Query<DirectoryQueryParams>,
) -> Result<Json<Value>, ApiError> {
    let room_alias =
        params.room_alias.as_deref().ok_or_else(|| ApiError::bad_request("Missing room_alias parameter"))?;
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

/// Query parameters accepted by [`query_destination`].
///
/// `destination` selects the remote server whose capabilities are reported;
/// it defaults to this server. Unknown keys are ignored (the endpoint has
/// always been tolerant), but the key set is typed so the contract can see it.
#[derive(Debug, Default, Deserialize)]
pub(super) struct QueryDestinationQuery {
    #[serde(default)]
    destination: Option<String>,
}

/// See [`query_destination`].
pub(super) async fn query_destination(
    State(ctx): State<FederationContext>,
    Query(params): Query<QueryDestinationQuery>,
) -> Result<Json<Value>, ApiError> {
    // Spec shape `{default, available}` — the builder already carries `default`.
    let room_versions = federation_room_versions_capability();
    let destination = params.destination.filter(|d| !d.is_empty()).unwrap_or_else(|| ctx.server_name.clone());
    Ok(Json(json!({
        "server_name": ctx.server_name,
        "destination": destination,
        "retry_last_ts": 0,
        "retry_interval_ms": 0,
        "capabilities": {
            "m.change_password": crate::routes::handlers::versions::change_password_capability_enabled(&ctx.config),
            "m.room_versions": room_versions
        }
    })))
}

/// Query parameters accepted by [`timestamp_to_event`].
///
/// Deliberately **not** `deny_unknown_fields`: the handler used to take a bare
/// `Query<Value>` and ignored unknown parameters. `ts` stays optional at the
/// serde layer so the missing-parameter answer keeps its `M_BAD_REQUEST` body.
#[derive(Debug, Default, Deserialize)]
pub(super) struct TimestampToEventQuery {
    ts: Option<String>,
}

/// See [`timestamp_to_event`].
pub(super) async fn timestamp_to_event(
    State(ctx): State<FederationContext>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(room_id): Path<RoomId>,
    Query(params): Query<TimestampToEventQuery>,
) -> Result<Json<Value>, ApiError> {
    if !synapse_common::room_id::is_well_formed_room_id(&room_id) {
        return Err(ApiError::bad_request("Invalid room_id format"));
    }

    super::validate_federation_origin_can_observe_room(&ctx, &room_id, &auth.origin).await?;

    let timestamp = match params.ts.as_deref() {
        Some(raw) => raw.parse::<i64>().map_err(|_| ApiError::bad_request("Invalid 'ts' parameter"))?,
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
    if !synapse_common::room_id::is_well_formed_room_id(&room_id) {
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

/// Query parameters for [`backfill`], per the Matrix S2S spec.
///
/// `from` is a *single* event id (the pagination cursor), not a list, and `v`
/// is the room version. Before D-9 the handler ignored `from` altogether and
/// read `v` as a list of event ids, so a spec-compliant caller silently got
/// the newest events back instead of the ones preceding its cursor.
#[derive(Debug, Deserialize)]
pub(super) struct BackfillQuery {
    /// Room version the caller expects (required by the spec).
    pub(super) v: String,
    /// Return events immediately preceding (and excluding) this event id.
    pub(super) from: String,
    pub(super) limit: Option<i64>,
}

/// See [`backfill`].
pub(super) async fn backfill(
    State(ctx): State<FederationContext>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(room_id): Path<RoomId>,
    Query(query): Query<BackfillQuery>,
) -> Result<Json<Value>, ApiError> {
    super::validate_federation_origin_can_observe_room(&ctx, &room_id, &auth.origin).await?;

    let room_version = query.v.trim();
    if room_version.is_empty() {
        return Err(ApiError::bad_request("Missing `v` (room version) query parameter".to_string()));
    }
    let limit = backfill_limit(query.limit);

    ::tracing::info!("Backfilling room {} (v{}) from {} with limit {}", room_id, room_version, query.from, limit);

    // `from` is the pagination cursor: the response starts immediately before
    // it and never includes it. An unknown cursor is a client error — silently
    // falling back to the newest events (the pre-D-9 behaviour) fabricated a
    // pagination window the caller never asked for.
    let from_ts = get_room_event_in_room(&ctx, &room_id, &query.from)
        .await
        .map_err(|_| {
            ApiError::not_found(format!("Backfill cursor event {} not found in room {}", query.from, room_id))
        })?
        .origin_server_ts;

    let mut events = ctx
        .room_service
        .messaging()
        .get_room_events_paginated_admin(&room_id, Some(from_ts), limit, "b")
        .await
        .map_err(ApiError::from)?;
    sort_room_events_stably(&mut events);

    let mut auth_events =
        ctx.room_service.messaging().get_state_events_at_or_before(&room_id, from_ts).await.map_err(ApiError::from)?;
    let (_, auth_chain) = build_federation_state_payload(&ctx, &mut auth_events).await;

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

/// Which state events belong in the `/state` auth chain.
///
/// Extracted so the selection rule stays unit-testable now that the
/// serialisation itself lives in [`super::pdu`] and needs a `FederationContext`
/// to sign.
fn is_state_auth_chain_member(event: &synapse_services::event::StateEvent) -> bool {
    event.event_type.as_deref().is_some_and(crate::federation::event_auth::EventAuthChain::is_auth_event)
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

async fn build_federation_state_payload(
    ctx: &FederationContext,
    events: &mut [synapse_services::event::StateEvent],
) -> (Vec<Value>, Vec<Value>) {
    sort_state_events_stably(events);

    let all: Vec<&synapse_services::event::StateEvent> = events.iter().collect();
    let auth_members: Vec<&synapse_services::event::StateEvent> =
        events.iter().filter(|event| is_state_auth_chain_member(event)).collect();

    let pdus = super::pdu::build_pdus(ctx, &all).await;
    let auth_chain = super::pdu::build_pdus(ctx, &auth_members).await;

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

/// The spec leaves `limit` optional; default to 10 and clamp to `1..=100`.
fn backfill_limit(limit: Option<i64>) -> i64 {
    limit.unwrap_or(10).clamp(1, 100)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backfill_query_requires_v_and_from() {
        let query: BackfillQuery = serde_json::from_value(json!({"v": "10", "from": "$abc"})).unwrap();
        assert_eq!(query.v, "10");
        assert_eq!(query.from, "$abc");
        assert_eq!(query.limit, None);

        assert!(
            serde_json::from_value::<BackfillQuery>(json!({"from": "$abc"})).is_err(),
            "`v` (room version) is required"
        );
        assert!(
            serde_json::from_value::<BackfillQuery>(json!({"v": "10"})).is_err(),
            "`from` (pagination cursor) is required"
        );
    }

    #[test]
    fn backfill_limit_defaults_and_clamps() {
        assert_eq!(backfill_limit(None), 10);
        assert_eq!(backfill_limit(Some(50)), 50);
        assert_eq!(backfill_limit(Some(999)), 100);
        assert_eq!(backfill_limit(Some(0)), 1);
        assert_eq!(backfill_limit(Some(-5)), 1);
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
            unsigned: None,
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
            prev_events: None,
            auth_events: None,
            signatures: None,
            hashes: None,
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
    fn state_auth_chain_membership_follows_is_auth_event() {
        // 序列化搬到 `super::pdu` 后，这里只锁住**选择规则**（授权链成员的判定），
        // 它仍然留在本模块内、且必须保持与原实现一致。
        assert!(is_state_auth_chain_member(&make_state_event("create", "m.room.create", 100, None)));
        assert!(is_state_auth_chain_member(&make_state_event("member", "m.room.member", 100, None)));
        assert!(!is_state_auth_chain_member(&make_state_event("message", "m.room.message", 100, None)));
        // 类型缺失（NULL event_type）必须判为“不属于授权链”，而不是 panic。
        let mut untyped = make_state_event("x", "m.room.create", 100, None);
        untyped.event_type = None;
        assert!(!is_state_auth_chain_member(&untyped));
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
    fn sort_state_events_stably_orders_before_pdu_projection() {
        // `build_federation_state_payload` 现在是 async 且需要 `FederationContext`
        // （要签名），所以这里只覆盖它仍然自有的那一半：排序。PDU 形状与
        // 签名规则由 `super::pdu` 的自测覆盖。
        let mut events = vec![
            make_state_event("create", "m.room.create", 100, None),
            make_state_event("message", "m.room.message", 200, None),
        ];
        sort_state_events_stably(&mut events);
        let order: Vec<&str> = events.iter().map(|event| event.event_id.as_str()).collect();
        assert_eq!(order, vec!["message", "create"]);
    }
}
