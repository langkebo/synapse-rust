use crate::common::*;
use crate::web::middleware::FederationRequestAuth;
use crate::web::routes::AppState;
use axum::extract::{Extension, Json, Path, State};
use serde_json::{json, Value};

async fn federatable_room_version(state: &AppState, room_id: &str) -> Result<String, ApiError> {
    let room = state
        .services.rooms.room_storage
        .get_room(room_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get room", &e))?
        .ok_or_else(|| ApiError::not_found("Room not found"))?;

    if !can_federate_room_version(&room.room_version) {
        return Err(ApiError::incompatible_room_version(format!(
            "Room version {} is not supported for federation",
            room.room_version
        )));
    }

    Ok(room.room_version)
}

pub(super) async fn get_room_members(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    super::validate_federation_origin_in_room(&state, &room_id, &auth.origin).await?;
    let _room_version = federatable_room_version(&state, &room_id).await?;

    let members = state
        .services.rooms.member_storage
        .get_room_members(&room_id, "join")
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get room members", &e))?;

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

pub(super) async fn knock_room(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path((room_id, user_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_federation_user_origin(&auth.origin, &user_id)?;
    validate_federation_knock_event(&auth.origin, &room_id, &user_id, &body)?;
    let _room_version = federatable_room_version(&state, &room_id).await?;

    let event_id = format!("${}", crate::common::crypto::generate_event_id(&state.services.server_name));

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
        .services.rooms.event_storage
        .create_event(params, None)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to create knock event", &e))?;

    Ok(Json(json!({
        "event_id": event_id,
        "room_id": room_id,
        "state": "knocking"
    })))
}

pub(super) async fn thirdparty_invite(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let room_id = body
        .get("room_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("room_id required".to_string()))?;
    if !room_id.starts_with('!') || !room_id.contains(':') {
        return Err(ApiError::bad_request("Invalid room_id format".to_string()));
    }

    let invitee = body
        .get("invitee")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("invitee required".to_string()))?;
    let sender = body
        .get("sender")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("sender required".to_string()))?;
    validate_federation_user_origin(&auth.origin, sender)?;
    let _room_version = federatable_room_version(&state, room_id).await?;

    let event_id = format!("${}", crate::common::crypto::generate_event_id(&state.services.server_name));

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
        .services.rooms.event_storage
        .create_event(params, None)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to create invite event", &e))?;

    Ok(Json(json!({
        "event_id": event_id,
        "room_id": room_id,
        "state": "invited"
    })))
}

pub(super) async fn get_joining_rules(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let join_rule_content = get_effective_room_join_rule_content(&state, &room_id).await?;
    let join_rule = get_effective_room_join_rule(&state, &room_id).await?;

    if join_rule != "public" {
        super::validate_federation_origin_in_room(&state, &room_id, &auth.origin).await?;
    }

    let allow = join_rule_content
        .as_ref()
        .and_then(|content| content.get("allow"))
        .filter(|value| value.is_array())
        .cloned()
        .unwrap_or_else(|| json!([]));

    Ok(Json(json!({
        "room_id": room_id,
        "join_rule": join_rule,
        "allow": allow
    })))
}

pub(super) async fn get_joined_room_members(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    super::validate_federation_origin_in_room(&state, &room_id, &auth.origin).await?;
    let _room_version = federatable_room_version(&state, &room_id).await?;

    let members = state
        .services.rooms.member_storage
        .get_room_members(&room_id, "join")
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get room members", &e))?;

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

pub(super) async fn get_user_devices(
    State(state): State<AppState>,
    Extension(_auth): Extension<FederationRequestAuth>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if !super::user_matches_origin(&user_id, &state.services.server_name) {
        return Err(ApiError::not_found("User is not hosted on this server".to_string()));
    }

    super::validate_federation_origin_shares_user_room(&state, &user_id, &_auth.origin).await?;

    let devices = state
        .services
        .device_storage
        .get_user_devices(&user_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get user devices", &e))?;

    let stream_id: i64 = sqlx::query_scalar(
        r"
        SELECT COALESCE(MAX(stream_id), 0)
        FROM device_lists_stream
        WHERE user_id = $1
        ",
    )
    .bind(&user_id)
    .fetch_one(&*state.services.device_storage.pool)
    .await
    .map_err(|e| ApiError::internal_with_log("Failed to get device stream id", &e))?;

    let master_key: Option<Value> = sqlx::query_scalar(
        r"
        SELECT key_data
        FROM cross_signing_keys
        WHERE user_id = $1 AND key_type = 'master'
        LIMIT 1
        ",
    )
    .bind(&user_id)
    .fetch_optional(&*state.services.device_storage.pool)
    .await
    .map_err(|e| ApiError::internal_with_log("Failed to get master key", &e))?
    .flatten()
    .and_then(|raw: String| serde_json::from_str(&raw).ok());

    let self_signing_key: Option<Value> = sqlx::query_scalar(
        r"
        SELECT key_data
        FROM cross_signing_keys
        WHERE user_id = $1 AND key_type = 'self_signing'
        LIMIT 1
        ",
    )
    .bind(&user_id)
    .fetch_optional(&*state.services.device_storage.pool)
    .await
    .map_err(|e| ApiError::internal_with_log("Failed to get self-signing key", &e))?
    .flatten()
    .and_then(|raw: String| serde_json::from_str(&raw).ok());

    let devices_json: Vec<Value> = devices
        .into_iter()
        .map(|d| {
            let keys = d.device_key.unwrap_or_else(|| json!({}));
            let algorithms = keys.get("algorithms").cloned().unwrap_or_else(|| json!([]));
            let signatures = keys.get("signatures").cloned().unwrap_or_else(|| json!({}));
            let keys_map = keys.get("keys").cloned().unwrap_or_else(|| json!({}));
            json!({
                "device_id": d.device_id,
                "user_id": d.user_id,
                "algorithms": algorithms,
                "keys": keys_map,
                "signatures": signatures
            })
        })
        .collect();

    Ok(Json(json!({
        "user_id": user_id,
        "stream_id": stream_id,
        "devices": devices_json,
        "master_key": master_key,
        "self_signing_key": self_signing_key
    })))
}

pub(super) async fn invite_v2(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path((room_id, event_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    if let Some(origin) = body.get("origin").and_then(|v| v.as_str()) {
        super::validate_federation_origin(&auth.origin, Some(origin))?;
    }
    let (sender, state_key) = validate_federation_invite_event(&auth.origin, &room_id, &event_id, &body)?;
    let _room_version = federatable_room_version(&state, &room_id).await?;
    let content = body.get("content").cloned().unwrap_or(json!({}));

    let params = crate::storage::event::CreateEventParams {
        event_id: event_id.clone(),
        room_id: room_id.clone(),
        user_id: sender.to_string(),
        event_type: "m.room.member".to_string(),
        content,
        state_key: Some(state_key.to_string()),
        origin_server_ts: body
            .get("origin_server_ts")
            .and_then(|v| v.as_i64())
            .unwrap_or(chrono::Utc::now().timestamp_millis()),
    };

    state
        .services.rooms.event_storage
        .create_event(params, None)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to create invite event", &e))?;

    ::tracing::info!("Processed v2 invite for room {} event {} from {}", room_id, event_id, auth.origin);

    Ok(Json(json!({
        "event_id": event_id
    })))
}

pub(super) async fn make_join(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path((room_id, user_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    super::increment_gauge(&state, "federation_join_in_flight");
    let (permit, wait_ms) = match super::acquire_with_timeout(
        state.federation_join_semaphore.clone(),
        state.services.config.federation.join_acquire_timeout_ms,
    )
    .await
    {
        Ok(value) => value,
        Err(error) => {
            super::decrement_gauge(&state, "federation_join_in_flight");
            super::increment_counter(&state, "federation_join_429_total");
            return Err(error);
        }
    };
    super::observe_histogram(&state, "federation_join_wait_ms", wait_ms as f64);

    let result = async {
        validate_federation_user_origin(&auth.origin, &user_id)?;

        let auth_events = state
            .services.rooms.event_storage
            .get_state_events(&room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get auth events", &e))?;

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

        let room_version = federatable_room_version(&state, &room_id).await?;

        Ok(Json(json!({
            "room_version": room_version,
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
    .await;

    drop(permit);
    super::decrement_gauge(&state, "federation_join_in_flight");
    match &result {
        Ok(_) => super::increment_counter(&state, "federation_join_ok_total"),
        Err(ApiError::RateLimitedWithRetry(_)) => super::increment_counter(&state, "federation_join_429_total"),
        Err(_) => super::increment_counter(&state, "federation_join_error_total"),
    }

    result
}

pub(super) async fn make_leave(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path((room_id, user_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_federation_user_origin(&auth.origin, &user_id)?;

    let auth_events = state
        .services.rooms.event_storage
        .get_state_events(&room_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get auth events", &e))?;

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

    let room_version = federatable_room_version(&state, &room_id).await?;

    Ok(Json(json!({
        "room_version": room_version,
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

pub(super) async fn send_join(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path((room_id, event_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    super::increment_gauge(&state, "federation_join_in_flight");
    let (permit, wait_ms) = match super::acquire_with_timeout(
        state.federation_join_semaphore.clone(),
        state.services.config.federation.join_acquire_timeout_ms,
    )
    .await
    {
        Ok(value) => value,
        Err(error) => {
            super::decrement_gauge(&state, "federation_join_in_flight");
            super::increment_counter(&state, "federation_join_429_total");
            return Err(error);
        }
    };
    super::observe_histogram(&state, "federation_join_wait_ms", wait_ms as f64);

    let result = async {
        super::validate_federation_origin(&auth.origin, body.get("origin").and_then(|v| v.as_str()))?;

        let event = body.get("event").ok_or_else(|| ApiError::bad_request("Event required".to_string()))?;
        let user_id = validate_federation_member_event(&auth.origin, &room_id, &event_id, event, "join")?;
        let _room_version = federatable_room_version(&state, &room_id).await?;
        validate_federation_join_access(&state, &room_id, user_id).await?;
        let content = event.get("content").cloned().unwrap_or(json!({}));
        let display_name = content.get("displayname").and_then(|v| v.as_str());

        let params = crate::storage::event::CreateEventParams {
            event_id: event_id.clone(),
            room_id: room_id.clone(),
            user_id: user_id.to_string(),
            event_type: "m.room.member".to_string(),
            content: content.clone(),
            state_key: Some(user_id.to_string()),
            origin_server_ts: event.get("origin_server_ts").and_then(|v| v.as_i64()).unwrap_or(0),
        };
        state
            .services.rooms.event_storage
            .create_event(params, None)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to persist join event", &e))?;

        state
            .services.rooms.member_storage
            .add_member(&room_id, user_id, "join", display_name, None, None, None)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to update membership", &e))?;

        ::tracing::info!("Processed join for room {} event {} from {}", room_id, event_id, auth.origin);

        Ok(Json(json!({
            "event_id": event_id
        })))
    }
    .await;

    drop(permit);
    super::decrement_gauge(&state, "federation_join_in_flight");
    match &result {
        Ok(_) => super::increment_counter(&state, "federation_join_ok_total"),
        Err(ApiError::RateLimitedWithRetry(_)) => super::increment_counter(&state, "federation_join_429_total"),
        Err(_) => super::increment_counter(&state, "federation_join_error_total"),
    }

    result
}

pub(super) async fn send_leave(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path((room_id, event_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    super::validate_federation_origin(&auth.origin, body.get("origin").and_then(|v| v.as_str()))?;

    let event = body.get("event").ok_or_else(|| ApiError::bad_request("Event required".to_string()))?;
    let user_id = validate_federation_member_event(&auth.origin, &room_id, &event_id, event, "leave")?;
    let _room_version = federatable_room_version(&state, &room_id).await?;

    let params = crate::storage::event::CreateEventParams {
        event_id: event_id.clone(),
        room_id: room_id.clone(),
        user_id: user_id.to_string(),
        event_type: "m.room.member".to_string(),
        content: event.get("content").cloned().unwrap_or(json!({})),
        state_key: Some(user_id.to_string()),
        origin_server_ts: event.get("origin_server_ts").and_then(|v| v.as_i64()).unwrap_or(0),
    };
    state
        .services.rooms.event_storage
        .create_event(params, None)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to persist leave event", &e))?;

    state
        .services.rooms.member_storage
        .add_member(&room_id, user_id, "leave", None, None, None, None)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to update membership", &e))?;

    ::tracing::info!("Processed leave for room {} event {} from {}", room_id, event_id, auth.origin);

    Ok(Json(json!({
        "event_id": event_id
    })))
}

pub(super) async fn invite(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path((room_id, event_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    if let Some(origin) = body.get("origin").and_then(|v| v.as_str()) {
        super::validate_federation_origin(&auth.origin, Some(origin))?;
    }
    validate_federation_invite_event(&auth.origin, &room_id, &event_id, &body)?;
    let _room_version = federatable_room_version(&state, &room_id).await?;

    ::tracing::info!("Processing invite for room {} event {}", room_id, event_id);

    Ok(Json(json!({
        "event_id": event_id
    })))
}

pub(super) async fn send_join_v2(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path((room_id, event_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    super::increment_gauge(&state, "federation_join_in_flight");
    let (permit, wait_ms) = match super::acquire_with_timeout(
        state.federation_join_semaphore.clone(),
        state.services.config.federation.join_acquire_timeout_ms,
    )
    .await
    {
        Ok(value) => value,
        Err(error) => {
            super::decrement_gauge(&state, "federation_join_in_flight");
            super::increment_counter(&state, "federation_join_429_total");
            return Err(error);
        }
    };
    super::observe_histogram(&state, "federation_join_wait_ms", wait_ms as f64);

    let result = async {
        if !room_id.starts_with('!') || !room_id.contains(':') {
            return Err(ApiError::bad_request("Invalid room_id format"));
        }

        if let Some(origin) = body.get("origin").and_then(|v| v.as_str()) {
            super::validate_federation_origin(&auth.origin, Some(origin))?;
        }
        let sender = validate_federation_member_event(&auth.origin, &room_id, &event_id, &body, "join")?;
        let _room_version = federatable_room_version(&state, &room_id).await?;
        validate_federation_join_access(&state, &room_id, sender).await?;
        let content = body.get("content").cloned().unwrap_or(json!({}));
        let display_name = content.get("displayname").and_then(|v| v.as_str());

        let params = crate::storage::event::CreateEventParams {
            event_id: event_id.clone(),
            room_id: room_id.clone(),
            user_id: sender.to_string(),
            event_type: "m.room.member".to_string(),
            content: content.clone(),
            state_key: Some(sender.to_string()),
            origin_server_ts: body
                .get("origin_server_ts")
                .and_then(|v| v.as_i64())
                .unwrap_or_else(|| chrono::Utc::now().timestamp_millis()),
        };
        state
            .services.rooms.event_storage
            .create_event(params, None)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to persist join event", &e))?;

        state
            .services.rooms.member_storage
            .add_member(&room_id, sender, "join", display_name, None, None, None)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to update membership", &e))?;

        ::tracing::info!(
            target: "federation",
            event = "federation_send_join_v2",
            origin = auth.origin,
            sender = sender,
            room_id = room_id,
            "Federation send_join_v2 processed"
        );

        Ok(Json(json!({
            "room_id": room_id,
            "event_id": event_id
        })))
    }
    .await;

    drop(permit);
    super::decrement_gauge(&state, "federation_join_in_flight");
    match &result {
        Ok(_) => super::increment_counter(&state, "federation_join_ok_total"),
        Err(ApiError::RateLimitedWithRetry(_)) => super::increment_counter(&state, "federation_join_429_total"),
        Err(_) => super::increment_counter(&state, "federation_join_error_total"),
    }

    result
}

pub(super) async fn send_leave_v2(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path((room_id, event_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    if !room_id.starts_with('!') || !room_id.contains(':') {
        return Err(ApiError::bad_request("Invalid room_id format"));
    }

    if let Some(origin) = body.get("origin").and_then(|v| v.as_str()) {
        super::validate_federation_origin(&auth.origin, Some(origin))?;
    }
    let sender = validate_federation_member_event(&auth.origin, &room_id, &event_id, &body, "leave")?;
    let _room_version = federatable_room_version(&state, &room_id).await?;
    let membership_content = serde_json::json!({
        "membership": "leave"
    });

    let params = crate::storage::event::CreateEventParams {
        event_id: event_id.clone(),
        room_id: room_id.clone(),
        user_id: sender.to_string(),
        event_type: "m.room.member".to_string(),
        content: membership_content,
        state_key: Some(sender.to_string()),
        origin_server_ts: chrono::Utc::now().timestamp_millis(),
    };

    state
        .services.rooms.event_storage
        .create_event(params, None)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to persist leave event", &e))?;

    state
        .services.rooms.member_storage
        .remove_member(&room_id, sender)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to update membership", &e))?;

    ::tracing::info!(
        target: "federation",
        event = "federation_send_leave",
        sender = sender,
        room_id = room_id,
        "Federation send_leave processed"
    );

    Ok(Json(json!({
        "room_id": room_id,
        "event_id": event_id
    })))
}

pub(super) async fn exchange_third_party_invite(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    if !room_id.starts_with('!') || !room_id.contains(':') {
        return Err(ApiError::bad_request("Invalid room_id format"));
    }

    let room_version = federatable_room_version(&state, &room_id).await?;

    let default_event_id = format!("${}:{}", uuid::Uuid::new_v4(), room_id.split(':').next_back().unwrap_or("server"));
    let event_id = body.get("event_id").and_then(|v| v.as_str()).unwrap_or(&default_event_id);

    let origin_server_ts =
        body.get("origin_server_ts").and_then(|v| v.as_i64()).unwrap_or_else(|| chrono::Utc::now().timestamp_millis());

    let (sender, state_key) = validate_federation_exchange_third_party_invite_event(&auth.origin, &room_id, &body)?;
    let content = body.get("content").cloned().unwrap_or_else(|| json!({}));

    Ok(Json(serde_json::json!({
        "event_id": event_id,
        "room_id": room_id,
        "type": "m.room.member",
        "sender": sender,
        "origin": auth.origin,
        "origin_server_ts": origin_server_ts,
        "room_version": room_version,
        "state_key": state_key,
        "content": content,
        "processed": true
    })))
}

fn validate_federation_user_origin(authenticated_origin: &str, user_id: &str) -> Result<(), ApiError> {
    if super::sender_server_name(user_id) != Some(authenticated_origin) {
        return Err(ApiError::forbidden("Federation user_id does not match authenticated origin".to_string()));
    }

    Ok(())
}

fn validate_federation_member_event<'a>(
    authenticated_origin: &str,
    room_id: &str,
    event_id: &str,
    event: &'a Value,
    expected_membership: &str,
) -> Result<&'a str, ApiError> {
    let sender = event
        .get("sender")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request(format!("Missing sender in {expected_membership} event")))?;

    if super::sender_server_name(sender) != Some(authenticated_origin) {
        return Err(ApiError::forbidden(
            "Federation member event sender does not match authenticated origin".to_string(),
        ));
    }

    let event_room_id = event
        .get("room_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing room_id in membership event".to_string()))?;
    if event_room_id != room_id {
        return Err(ApiError::bad_request("Membership event room_id does not match request path".to_string()));
    }

    let event_event_id = event
        .get("event_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing event_id in membership event".to_string()))?;
    if event_event_id != event_id {
        return Err(ApiError::bad_request("Membership event event_id does not match request path".to_string()));
    }

    let event_type = event
        .get("type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing event type in membership event".to_string()))?;
    if event_type != "m.room.member" {
        return Err(ApiError::bad_request(
            "Federation send_join/send_leave only accepts m.room.member events".to_string(),
        ));
    }

    let state_key = event
        .get("state_key")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing state_key in membership event".to_string()))?;
    if state_key != sender {
        return Err(ApiError::bad_request("Membership event state_key must match sender".to_string()));
    }

    let membership = event
        .get("content")
        .and_then(|v| v.get("membership"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing membership in event content".to_string()))?;
    if membership != expected_membership {
        return Err(ApiError::bad_request(format!(
            "Expected membership '{expected_membership}' but got '{membership}'"
        )));
    }

    if let Some(event_origin) = event.get("origin").and_then(|v| v.as_str()) {
        super::validate_federation_origin(authenticated_origin, Some(event_origin))?;
    }

    Ok(sender)
}

fn validate_federation_knock_event<'a>(
    authenticated_origin: &str,
    room_id: &str,
    user_id: &str,
    event: &'a Value,
) -> Result<&'a str, ApiError> {
    let sender = validate_federation_member_event_without_event_id(authenticated_origin, room_id, event, "knock")?;

    if sender != user_id {
        return Err(ApiError::bad_request("Knock event sender must match request path user_id".to_string()));
    }

    Ok(sender)
}

fn validate_federation_member_event_without_event_id<'a>(
    authenticated_origin: &str,
    room_id: &str,
    event: &'a Value,
    expected_membership: &str,
) -> Result<&'a str, ApiError> {
    let sender = event
        .get("sender")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request(format!("Missing sender in {expected_membership} event")))?;

    if super::sender_server_name(sender) != Some(authenticated_origin) {
        return Err(ApiError::forbidden(format!(
            "Federation {expected_membership} event sender does not match authenticated origin"
        )));
    }

    let event_room_id = event
        .get("room_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing room_id in membership event".to_string()))?;
    if event_room_id != room_id {
        return Err(ApiError::bad_request("Membership event room_id does not match request path".to_string()));
    }

    let event_type = event
        .get("type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing event type in membership event".to_string()))?;
    if event_type != "m.room.member" {
        return Err(ApiError::bad_request(
            "Federation membership endpoints only accept m.room.member events".to_string(),
        ));
    }

    let state_key = event
        .get("state_key")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing state_key in membership event".to_string()))?;
    if state_key != sender {
        return Err(ApiError::bad_request("Membership event state_key must match sender".to_string()));
    }

    let membership = event
        .get("content")
        .and_then(|v| v.get("membership"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing membership in event content".to_string()))?;
    if membership != expected_membership {
        return Err(ApiError::bad_request(format!(
            "Expected membership '{expected_membership}' but got '{membership}'"
        )));
    }

    if let Some(event_origin) = event.get("origin").and_then(|v| v.as_str()) {
        super::validate_federation_origin(authenticated_origin, Some(event_origin))?;
    }

    Ok(sender)
}

fn validate_federation_invite_event<'a>(
    authenticated_origin: &str,
    room_id: &str,
    event_id: &str,
    event: &'a Value,
) -> Result<(&'a str, &'a str), ApiError> {
    let sender = event
        .get("sender")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing sender in invite event".to_string()))?;

    if super::sender_server_name(sender) != Some(authenticated_origin) {
        return Err(ApiError::forbidden(
            "Federation invite event sender does not match authenticated origin".to_string(),
        ));
    }

    let event_room_id = event
        .get("room_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing room_id in invite event".to_string()))?;
    if event_room_id != room_id {
        return Err(ApiError::bad_request("Invite event room_id does not match request path".to_string()));
    }

    let event_event_id = event
        .get("event_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing event_id in invite event".to_string()))?;
    if event_event_id != event_id {
        return Err(ApiError::bad_request("Invite event event_id does not match request path".to_string()));
    }

    let event_type = event
        .get("type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing event type in invite event".to_string()))?;
    if event_type != "m.room.member" {
        return Err(ApiError::bad_request("Federation invite only accepts m.room.member events".to_string()));
    }

    let state_key = event
        .get("state_key")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing state_key in invite event".to_string()))?;

    let membership = event
        .get("content")
        .and_then(|v| v.get("membership"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing membership in invite event".to_string()))?;
    if membership != "invite" {
        return Err(ApiError::bad_request(format!("Expected membership 'invite' but got '{membership}'")));
    }

    if let Some(event_origin) = event.get("origin").and_then(|v| v.as_str()) {
        super::validate_federation_origin(authenticated_origin, Some(event_origin))?;
    }

    Ok((sender, state_key))
}

fn validate_federation_exchange_third_party_invite_event<'a>(
    authenticated_origin: &str,
    room_id: &str,
    event: &'a Value,
) -> Result<(&'a str, &'a str), ApiError> {
    if let Some(origin) = event.get("origin").and_then(|v| v.as_str()) {
        super::validate_federation_origin(authenticated_origin, Some(origin))?;
    }

    let sender = event
        .get("sender")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing sender in third-party invite event".to_string()))?;
    if super::sender_server_name(sender) != Some(authenticated_origin) {
        return Err(ApiError::forbidden(
            "Federation third-party invite sender does not match authenticated origin".to_string(),
        ));
    }

    let event_room_id = event
        .get("room_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing room_id in third-party invite event".to_string()))?;
    if event_room_id != room_id {
        return Err(ApiError::bad_request("Third-party invite room_id does not match request path".to_string()));
    }

    let event_type = event
        .get("type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing event type in third-party invite event".to_string()))?;
    if event_type != "m.room.member" {
        return Err(ApiError::bad_request(
            "Federation third-party invite only accepts m.room.member events".to_string(),
        ));
    }

    let state_key = event
        .get("state_key")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing state_key in third-party invite event".to_string()))?;
    if state_key.is_empty() {
        return Err(ApiError::bad_request("Third-party invite state_key must not be empty".to_string()));
    }

    let membership = event
        .get("content")
        .and_then(|v| v.get("membership"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing membership in third-party invite event".to_string()))?;
    if membership != "invite" {
        return Err(ApiError::bad_request(format!("Expected membership 'invite' but got '{membership}'")));
    }

    Ok((sender, state_key))
}

async fn get_effective_room_join_rule(state: &AppState, room_id: &str) -> ApiResult<String> {
    let effective_join_rule = if let Some(content) = get_effective_room_join_rule_content(state, room_id).await? {
        content.get("join_rule").and_then(|value| value.as_str()).map(|value| value.to_string())
    } else {
        None
    };

    let room =
        state.services.rooms.room_storage.get_room(room_id).await?.ok_or_else(|| ApiError::not_found("Room not found"))?;

    Ok(effective_join_rule.or_else(|| (!room.join_rule.is_empty()).then(|| room.join_rule.clone())).unwrap_or_else(
        || {
            if room.is_public {
                "public".to_string()
            } else {
                "invite".to_string()
            }
        },
    ))
}

async fn get_effective_room_join_rule_content(state: &AppState, room_id: &str) -> ApiResult<Option<Value>> {
    Ok(state
        .services.rooms.event_storage
        .get_state_events_by_type(room_id, "m.room.join_rules")
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to load room join rules", &e))?
        .into_iter()
        .find(|event| event.state_key.as_deref().unwrap_or_default().is_empty())
        .map(|event| event.content))
}

async fn validate_federation_join_access(state: &AppState, room_id: &str, user_id: &str) -> ApiResult<()> {
    let join_rule = get_effective_room_join_rule(state, room_id).await?;
    let existing_member = state
        .services.rooms.member_storage
        .get_room_member(room_id, user_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to check membership", &e))?;

    if let Some(member) = existing_member.as_ref() {
        if member.membership == "join" {
            return Ok(());
        }

        if member.membership == "ban" || member.is_banned.unwrap_or(false) {
            return Err(ApiError::forbidden("User is not allowed to join this room"));
        }
    }

    if join_rule != "public" && existing_member.as_ref().is_none_or(|member| member.membership != "invite") {
        return Err(ApiError::forbidden("User is not allowed to join this room"));
    }

    Ok(())
}
