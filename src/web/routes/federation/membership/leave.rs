use crate::common::*;
use crate::web::middleware::FederationRequestAuth;
use crate::web::routes::context::FederationContext;
use crate::web::utils::auth::resolve_request_id;
use axum::{
    extract::{Extension, Json, Path, State},
    http::HeaderMap,
};
use serde_json::{json, Value};
use synapse_common::current_timestamp_millis;

use super::{
    dispatch_federation_member_event_to_appservice, federatable_room_version, validate_federation_member_event,
};

pub(crate) async fn make_leave(
    State(ctx): State<FederationContext>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path((room_id, user_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    super::validate_federation_user_origin(&auth.origin, &user_id)?;

    // OPT-017: Check room access BEFORE room version to prevent existence leaking.
    // Access denied and non-existent rooms both return 404.
    super::validate_federation_origin_can_observe_room(&ctx, &room_id, &auth.origin).await?;
    let room_version = federatable_room_version(&ctx, &room_id).await?;

    let auth_events = ctx.room_service.messaging().get_state_event_records(&room_id).await?;

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

pub(crate) async fn send_leave(
    State(ctx): State<FederationContext>,
    Extension(auth): Extension<FederationRequestAuth>,
    headers: HeaderMap,
    Path((room_id, event_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);
    super::validate_federation_origin(&auth.origin, body.get("origin").and_then(|v| v.as_str()))?;

    let event = body.get("event").ok_or_else(|| ApiError::bad_request("Event required".to_string()))?;
    let user_id = validate_federation_member_event(&auth.origin, &room_id, &event_id, event, "leave")?;
    // OPT-017: Check room access BEFORE room version to prevent existence leaking.
    // Access denied and non-existent rooms both return 404.
    super::validate_federation_origin_can_observe_room(&ctx, &room_id, &auth.origin).await?;
    let _room_version = federatable_room_version(&ctx, &room_id).await?;

    let params = synapse_storage::event::CreateEventParams {
        event_id: event_id.clone(),
        room_id: room_id.clone(),
        user_id: user_id.to_string(),
        event_type: "m.room.member".to_string(),
        content: event.get("content").cloned().unwrap_or(json!({})),
        state_key: Some(user_id.to_string()),
        origin_server_ts: event.get("origin_server_ts").and_then(|v| v.as_i64()).unwrap_or(0),
        redacts: None,
    };
    ctx.room_service
        .messaging()
        .create_event(params, None)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to persist leave event", &e))?;
    let content = event.get("content").cloned().unwrap_or(json!({}));
    dispatch_federation_member_event_to_appservice(&ctx, &event_id, &room_id, user_id, &content, Some(user_id)).await;

    ctx.room_service
        .membership()
        .add_member(&room_id, user_id, "leave", None, None, None)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to update membership", &e))?;

    ::tracing::info!(
        request_id = %request_id,
        origin = %auth.origin,
        room_id = %room_id,
        event_id = %event_id,
        "Processed leave"
    );

    Ok(Json(json!({
        "event_id": event_id
    })))
}

pub(crate) async fn send_leave_v2(
    State(ctx): State<FederationContext>,
    Extension(auth): Extension<FederationRequestAuth>,
    headers: HeaderMap,
    Path((room_id, event_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);
    if !room_id.starts_with('!') || !room_id.contains(':') {
        return Err(ApiError::bad_request("Invalid room_id format"));
    }

    if let Some(origin) = body.get("origin").and_then(|v| v.as_str()) {
        super::validate_federation_origin(&auth.origin, Some(origin))?;
    }
    let sender = validate_federation_member_event(&auth.origin, &room_id, &event_id, &body, "leave")?;
    let _room_version = federatable_room_version(&ctx, &room_id).await?;
    let membership_content = serde_json::json!({
        "membership": "leave"
    });

    let membership_content_for_as = membership_content.clone();

    let params = synapse_storage::event::CreateEventParams {
        event_id: event_id.clone(),
        room_id: room_id.clone(),
        user_id: sender.to_string(),
        event_type: "m.room.member".to_string(),
        content: membership_content,
        state_key: Some(sender.to_string()),
        origin_server_ts: current_timestamp_millis(),
        redacts: None,
    };

    ctx.room_service
        .messaging()
        .create_event(params, None)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to persist leave event", &e))?;
    dispatch_federation_member_event_to_appservice(
        &ctx,
        &event_id,
        &room_id,
        sender,
        &membership_content_for_as,
        Some(sender),
    )
    .await;

    ctx.room_service
        .membership()
        .remove_member_record(&room_id, sender)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to update membership", &e))?;

    ::tracing::info!(
        target: "federation",
        request_id = %request_id,
        origin = %auth.origin,
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
