use super::{
    ensure_room_state_write_access, ensure_room_view_access, normalize_room_event_type, state_event_content_response,
};
use crate::common::ApiError;
use crate::map_internal;
use crate::web::routes::context::RoomContext;
use crate::web::routes::{validate_room_id, AuthenticatedUser};
use axum::extract::{Json, Path, State};
use serde_json::{json, Value};
#[cfg(feature = "beacons")]
use synapse_storage::beacon::CreateBeaconInfoParams;
use synapse_storage::event::CreateEventParams;

pub(crate) async fn get_room_state(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let room_exists = ctx.room_service.state().room_exists(&room_id).await?;

    if !room_exists {
        return Err(ApiError::not_found(format!("Room '{room_id}' not found")));
    }

    ensure_room_view_access(&ctx, &auth_user, &room_id).await?;

    let state_events = ctx.room_service.messaging().get_state_events(&room_id).await?;

    Ok(Json(json!({
        "events": state_events
    })))
}

pub(crate) async fn get_state_by_type(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_type)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let room_exists = ctx.room_service.state().room_exists(&room_id).await?;

    if !room_exists {
        return Err(ApiError::not_found(format!("Room '{room_id}' not found")));
    }

    ensure_room_view_access(&ctx, &auth_user, &room_id).await?;

    let final_event_type = normalize_room_event_type(&event_type);

    let events = ctx.room_service.messaging().get_state_events_by_type(&room_id, &final_event_type).await?;

    let event_with_empty_key =
        events.iter().find(|e| e.get("state_key").and_then(|v| v.as_str()) == Some("") || e.get("state_key").is_none());

    if let Some(event) = event_with_empty_key {
        Ok(Json(state_event_content_response(event.get("content").unwrap_or(&json!({})))))
    } else if events.len() == 1 {
        Ok(Json(state_event_content_response(events[0].get("content").unwrap_or(&json!({})))))
    } else if events.is_empty() {
        Err(ApiError::not_found("State event not found".to_string()))
    } else {
        Ok(Json(json!({ "events": events })))
    }
}

pub(crate) async fn get_state_event(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_type, state_key)): Path<(String, String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    ensure_room_view_access(&ctx, &auth_user, &room_id).await?;

    let final_event_type = normalize_room_event_type(&event_type);

    let events = ctx.room_service.messaging().get_state_events_by_type(&room_id, &final_event_type).await?;

    let event = events
        .iter()
        .find(|e| {
            e.get("state_key").and_then(|v| v.as_str()) == Some(state_key.as_str())
                || (e.get("state_key").and_then(|v| v.as_str()).map(|s| s.is_empty()) == Some(true)
                    && state_key.is_empty())
        })
        .ok_or_else(|| ApiError::not_found("State event not found".to_string()))?;

    Ok(Json(state_event_content_response(event.get("content").unwrap_or(&json!({})))))
}

pub(crate) async fn send_state_event(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_type)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let content = body;

    let new_event_id = crate::common::crypto::generate_event_id(&ctx.server_name);
    let now = chrono::Utc::now().timestamp_millis();

    let final_event_type = normalize_room_event_type(&event_type);
    ensure_room_state_write_access(&ctx, &auth_user, &room_id, &final_event_type).await?;

    // Variable used only when `beacons` feature is enabled.
    #[allow(unused_variables)]
    let beacon_info_params = if final_event_type.starts_with("m.beacon_info")
        || final_event_type.starts_with("org.matrix.msc3672.beacon_info")
        || final_event_type.starts_with("org.matrix.msc3489.beacon_info")
    {
        let beacon_obj = content
            .get("m.beacon_info")
            .and_then(|v| v.as_object())
            .ok_or_else(|| ApiError::bad_request("Missing m.beacon_info in beacon_info content".to_string()))?;

        let timeout = beacon_obj
            .get("timeout")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| ApiError::bad_request("Missing m.beacon_info.timeout".to_string()))?;

        let is_live = beacon_obj.get("live").and_then(|v| v.as_bool()).unwrap_or(true);

        let description = beacon_obj.get("description").and_then(|v| v.as_str()).map(|v| v.to_string());

        let created_ts = content
            .get("m.ts")
            .or_else(|| content.get("org.matrix.msc3488.ts"))
            .and_then(|v| v.as_i64())
            .unwrap_or(now);

        let asset_type = content
            .get("m.asset")
            .or_else(|| content.get("org.matrix.msc3488.asset"))
            .and_then(|v| v.get("type"))
            .and_then(|v| v.as_str())
            .unwrap_or("m.self")
            .to_string();

        #[cfg(feature = "beacons")]
        {
            Some(CreateBeaconInfoParams {
                room_id: room_id.clone(),
                event_id: new_event_id.clone(),
                state_key: auth_user.user_id.clone(),
                sender: auth_user.user_id.clone(),
                description,
                timeout,
                is_live,
                asset_type,
                created_ts,
            })
        }
        #[cfg(not(feature = "beacons"))]
        {
            let _ = (timeout, is_live, description, created_ts, asset_type);
            None::<()>
        }
    } else {
        None
    };

    // State events with empty state_key per Matrix spec (global room state)
    const EMPTY_STATE_KEY_TYPES: &[&str] = &[
        "m.room.encryption",
        "m.room.power_levels",
        "m.room.join_rules",
        "m.room.history_visibility",
        "m.room.guest_access",
        "m.room.name",
        "m.room.topic",
        "m.room.avatar",
        "m.room.canonical_alias",
        "m.room.server_acl",
    ];

    let state_key = if EMPTY_STATE_KEY_TYPES.contains(&final_event_type.as_str()) {
        Some("".to_string())
    } else {
        Some(auth_user.user_id.clone())
    };

    let state_event = ctx
        .room_service
        .messaging()
        .create_event(
            CreateEventParams {
                event_id: new_event_id.clone(),
                room_id: room_id.clone(),
                user_id: auth_user.user_id.clone(),
                event_type: final_event_type.clone(),
                content,
                state_key,
                origin_server_ts: now,
                redacts: None,
            },
            None,
        )
        .await
        .map_err(map_internal!("Failed to send state event"))?;

    #[cfg(feature = "beacons")]
    if let Some(params) = beacon_info_params {
        ctx.beacon_service.create_beacon(params).await.map_err(map_internal!("Failed to index beacon_info"))?;
    }

    Ok(Json(json!({
        "event_id": new_event_id,
        "type": state_event.event_type,
        "state_key": state_event.state_key
    })))
}

pub(crate) async fn put_state_event(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_type, state_key)): Path<(String, String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let new_event_id = crate::common::crypto::generate_event_id(&ctx.server_name);
    let now = chrono::Utc::now().timestamp_millis();

    let final_event_type = normalize_room_event_type(&event_type);
    ensure_room_state_write_access(&ctx, &auth_user, &room_id, &final_event_type).await?;

    if (final_event_type.starts_with("m.beacon_info")
        || final_event_type.starts_with("org.matrix.msc3672.beacon_info")
        || final_event_type.starts_with("org.matrix.msc3489.beacon_info"))
        && state_key != auth_user.user_id
    {
        return Err(ApiError::forbidden("beacon_info stateKey must match sender".to_string()));
    }

    // Variable used only when `beacons` feature is enabled.
    #[allow(unused_variables)]
    let beacon_info_params = if final_event_type.starts_with("m.beacon_info")
        || final_event_type.starts_with("org.matrix.msc3672.beacon_info")
        || final_event_type.starts_with("org.matrix.msc3489.beacon_info")
    {
        let beacon_obj = body
            .get("m.beacon_info")
            .and_then(|v| v.as_object())
            .ok_or_else(|| ApiError::bad_request("Missing m.beacon_info in beacon_info content".to_string()))?;

        let timeout = beacon_obj
            .get("timeout")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| ApiError::bad_request("Missing m.beacon_info.timeout".to_string()))?;

        let is_live = beacon_obj.get("live").and_then(|v| v.as_bool()).unwrap_or(true);

        let description = beacon_obj.get("description").and_then(|v| v.as_str()).map(|v| v.to_string());

        let created_ts =
            body.get("m.ts").or_else(|| body.get("org.matrix.msc3488.ts")).and_then(|v| v.as_i64()).unwrap_or(now);

        let asset_type = body
            .get("m.asset")
            .or_else(|| body.get("org.matrix.msc3488.asset"))
            .and_then(|v| v.get("type"))
            .and_then(|v| v.as_str())
            .unwrap_or("m.self")
            .to_string();

        #[cfg(feature = "beacons")]
        {
            Some(CreateBeaconInfoParams {
                room_id: room_id.clone(),
                event_id: new_event_id.clone(),
                state_key: state_key.clone(),
                sender: auth_user.user_id.clone(),
                description,
                timeout,
                is_live,
                asset_type,
                created_ts,
            })
        }
        #[cfg(not(feature = "beacons"))]
        {
            let _ = (timeout, is_live, description, created_ts, asset_type);
            None::<()>
        }
    } else {
        None
    };

    let event = ctx
        .room_service
        .messaging()
        .create_event(
            CreateEventParams {
                event_id: new_event_id.clone(),
                room_id: room_id.clone(),
                user_id: auth_user.user_id.clone(),
                event_type: final_event_type.clone(),
                content: body,
                state_key: Some(state_key),
                origin_server_ts: now,
                redacts: None,
            },
            None,
        )
        .await
        .map_err(map_internal!("Failed to put state event"))?;

    #[cfg(feature = "beacons")]
    if let Some(params) = beacon_info_params {
        ctx.beacon_service.create_beacon(params).await.map_err(map_internal!("Failed to index beacon_info"))?;
    }

    Ok(Json(json!({
        "event_id": new_event_id,
        "type": event.event_type,
        "state_key": event.state_key
    })))
}

pub(crate) async fn get_state_event_empty_key(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_type)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    ensure_room_view_access(&ctx, &auth_user, &room_id).await?;

    let final_event_type = normalize_room_event_type(&event_type);

    let events = ctx.room_service.messaging().get_state_events_by_type(&room_id, &final_event_type).await?;

    let event = events
        .iter()
        .find(|e| e.get("state_key").and_then(|v| v.as_str()).map(|s| s.is_empty()) == Some(true))
        .ok_or_else(|| ApiError::not_found("State event not found".to_string()))?;

    Ok(Json(state_event_content_response(event.get("content").unwrap_or(&json!({})))))
}

pub(crate) async fn get_power_levels(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    ensure_room_view_access(&ctx, &auth_user, &room_id).await?;

    let events = ctx.room_service.messaging().get_state_events_by_type(&room_id, "m.room.power_levels").await?;

    let event = events
        .iter()
        .find(|e| e.get("state_key").and_then(|v| v.as_str()).map(|s| s.is_empty()) == Some(true))
        .ok_or_else(|| ApiError::not_found("Power levels not found".to_string()))?;

    let power_levels_content = event.get("content").cloned().unwrap_or_else(|| json!({}));

    Ok(Json(power_levels_content))
}

pub(crate) async fn put_state_event_empty_key(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_type)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let new_event_id = crate::common::crypto::generate_event_id(&ctx.server_name);
    let now = chrono::Utc::now().timestamp_millis();

    let final_event_type = normalize_room_event_type(&event_type);
    ensure_room_state_write_access(&ctx, &auth_user, &room_id, &final_event_type).await?;

    let event = ctx
        .room_service
        .messaging()
        .create_event(
            CreateEventParams {
                event_id: new_event_id.clone(),
                room_id: room_id.clone(),
                user_id: auth_user.user_id.clone(),
                event_type: final_event_type.clone(),
                content: body,
                state_key: Some("".to_string()),
                origin_server_ts: now,
                redacts: None,
            },
            None,
        )
        .await
        .map_err(map_internal!("Failed to put state event"))?;

    Ok(Json(json!({
        "event_id": new_event_id,
        "type": event.event_type,
        "state_key": event.state_key
    })))
}

pub(crate) async fn put_state_event_no_key(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_type)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let new_event_id = crate::common::crypto::generate_event_id(&ctx.server_name);
    let now = chrono::Utc::now().timestamp_millis();

    let final_event_type = normalize_room_event_type(&event_type);
    ensure_room_state_write_access(&ctx, &auth_user, &room_id, &final_event_type).await?;

    let event = ctx
        .room_service
        .messaging()
        .create_event(
            CreateEventParams {
                event_id: new_event_id.clone(),
                room_id: room_id.clone(),
                user_id: auth_user.user_id.clone(),
                event_type: final_event_type,
                content: body,
                state_key: Some("".to_string()),
                origin_server_ts: now,
                redacts: None,
            },
            None,
        )
        .await
        .map_err(map_internal!("Failed to put state event"))?;

    Ok(Json(json!({
        "event_id": new_event_id,
        "type": event.event_type,
        "state_key": event.state_key
    })))
}

pub(crate) async fn get_room_permissions(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    ensure_room_view_access(&ctx, &auth_user, &room_id).await?;

    let power_levels_events =
        ctx.room_service.messaging().get_state_events_by_type(&room_id, "m.room.power_levels").await?;

    let pl_content = power_levels_events
        .iter()
        .find(|e| e.get("state_key").and_then(|v| v.as_str()).map(|s| s.is_empty()) == Some(true))
        .and_then(|e| e.get("content").cloned())
        .unwrap_or(json!({}));

    let join_rules_events =
        ctx.room_service.messaging().get_state_events_by_type(&room_id, "m.room.join_rules").await?;

    let join_rule = join_rules_events
        .iter()
        .find(|e| e.get("state_key").and_then(|v| v.as_str()).map(|s| s.is_empty()) == Some(true))
        .and_then(|e| e.get("content"))
        .and_then(|content| content.get("join_rule").cloned())
        .unwrap_or(json!("invite"));

    let user_pl = pl_content
        .get("users")
        .and_then(|u| u.get(&auth_user.user_id))
        .and_then(|v| v.as_i64())
        .unwrap_or_else(|| pl_content.get("users_default").and_then(|v| v.as_i64()).unwrap_or(0));

    Ok(Json(json!({
        "room_id": room_id,
        "user_id": auth_user.user_id,
        "user_power_level": user_pl,
        "join_rule": join_rule,
        "power_levels": pl_content
    })))
}
