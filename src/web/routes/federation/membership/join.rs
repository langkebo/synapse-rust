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
    validate_federation_user_origin,
};

/// Build a join event template (pure, testable).
///
/// Constructs the event object that the requesting server must sign before
/// calling `send_join`.  The caller is responsible for merging in
/// `auth_events` and `room_version` fields.
pub(crate) fn build_join_event_template(user_id: &str) -> serde_json::Value {
    json!({
        "type": "m.room.member",
        "content": {
            "membership": "join"
        },
        "sender": user_id,
        "state_key": user_id
    })
}

pub(crate) async fn make_join(
    State(ctx): State<FederationContext>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path((room_id, user_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    super::increment_gauge(&ctx, "federation_join_in_flight");
    let (permit, wait_ms) = match super::acquire_with_timeout(
        ctx.federation_join_semaphore.clone(),
        ctx.config.federation.join_acquire_timeout_ms,
    )
    .await
    {
        Ok(value) => value,
        Err(error) => {
            super::decrement_gauge(&ctx, "federation_join_in_flight");
            super::increment_counter(&ctx, "federation_join_429_total");
            return Err(error);
        }
    };
    super::observe_histogram(&ctx, "federation_join_wait_ms", wait_ms as f64);

    let result: Result<Json<Value>, ApiError> = async {
        validate_federation_user_origin(&auth.origin, &user_id)?;

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

        let room_version = federatable_room_version(&ctx, &room_id).await?;

        Ok(Json(json!({
            "room_version": room_version,
            "auth_events": auth_events_json,
            "event": build_join_event_template(&user_id)
        })))
    }
    .await;

    drop(permit);
    super::decrement_gauge(&ctx, "federation_join_in_flight");
    match &result {
        Ok(_) => super::increment_counter(&ctx, "federation_join_ok_total"),
        Err(e) if e.is_rate_limited() => super::increment_counter(&ctx, "federation_join_429_total"),
        Err(_) => super::increment_counter(&ctx, "federation_join_error_total"),
    }

    result
}

pub(crate) async fn send_join(
    State(ctx): State<FederationContext>,
    Extension(auth): Extension<FederationRequestAuth>,
    headers: HeaderMap,
    Path((room_id, event_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);
    super::increment_gauge(&ctx, "federation_join_in_flight");
    let (permit, wait_ms) = match super::acquire_with_timeout(
        ctx.federation_join_semaphore.clone(),
        ctx.config.federation.join_acquire_timeout_ms,
    )
    .await
    {
        Ok(value) => value,
        Err(error) => {
            super::decrement_gauge(&ctx, "federation_join_in_flight");
            super::increment_counter(&ctx, "federation_join_429_total");
            return Err(error);
        }
    };
    super::observe_histogram(&ctx, "federation_join_wait_ms", wait_ms as f64);

    let result: Result<Json<Value>, ApiError> = async {
        super::validate_federation_origin(&auth.origin, body.get("origin").and_then(|v| v.as_str()))?;

        let event = body.get("event").ok_or_else(|| ApiError::bad_request("Event required".to_string()))?;
        let user_id = validate_federation_member_event(&auth.origin, &room_id, &event_id, event, "join")?;
        // OPT-017: Check join access BEFORE room version to prevent existence leaking.
        // A non-existent room now gets 404 from inside validate_federation_join_access;
        // a private room now also returns 404 (same error, no leak).
        validate_federation_join_access(&ctx, &room_id, user_id).await.map_err(|e| {
            if e.is_forbidden() {
                ApiError::not_found("Room not found")
            } else {
                e
            }
        })?;
        let _room_version = federatable_room_version(&ctx, &room_id).await?;
        let content = event.get("content").cloned().unwrap_or(json!({}));
        let display_name = content.get("displayname").and_then(|v| v.as_str());

        let params = synapse_storage::event::CreateEventParams {
            event_id: event_id.clone(),
            room_id: room_id.clone(),
            user_id: user_id.to_string(),
            event_type: "m.room.member".to_string(),
            content: content.clone(),
            state_key: Some(user_id.to_string()),
            origin_server_ts: event.get("origin_server_ts").and_then(|v| v.as_i64()).unwrap_or(0),
            redacts: None,
        };
        ctx.room_service
            .messaging()
            .create_event(params, None)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to persist join event", &e))?;
        dispatch_federation_member_event_to_appservice(&ctx, &event_id, &room_id, user_id, &content, Some(user_id))
            .await;

        ctx.room_service
            .membership()
            .add_member(&room_id, user_id, "join", display_name, None, None)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to update membership", &e))?;

        ::tracing::info!(
            request_id = %request_id,
            origin = %auth.origin,
            room_id = %room_id,
            event_id = %event_id,
            "Processed join"
        );

        Ok(Json(json!({
            "event_id": event_id
        })))
    }
    .await;

    drop(permit);
    super::decrement_gauge(&ctx, "federation_join_in_flight");
    match &result {
        Ok(_) => super::increment_counter(&ctx, "federation_join_ok_total"),
        Err(e) if e.is_rate_limited() => super::increment_counter(&ctx, "federation_join_429_total"),
        Err(_) => super::increment_counter(&ctx, "federation_join_error_total"),
    }

    result
}

pub(crate) async fn send_join_v2(
    State(ctx): State<FederationContext>,
    Extension(auth): Extension<FederationRequestAuth>,
    headers: HeaderMap,
    Path((room_id, event_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);
    super::increment_gauge(&ctx, "federation_join_in_flight");
    let (permit, wait_ms) = match super::acquire_with_timeout(
        ctx.federation_join_semaphore.clone(),
        ctx.config.federation.join_acquire_timeout_ms,
    )
    .await
    {
        Ok(value) => value,
        Err(error) => {
            super::decrement_gauge(&ctx, "federation_join_in_flight");
            super::increment_counter(&ctx, "federation_join_429_total");
            return Err(error);
        }
    };
    super::observe_histogram(&ctx, "federation_join_wait_ms", wait_ms as f64);

    let result = async {
        if !room_id.starts_with('!') || !room_id.contains(':') {
            return Err(ApiError::bad_request("Invalid room_id format"));
        }

        if let Some(origin) = body.get("origin").and_then(|v| v.as_str()) {
            super::validate_federation_origin(&auth.origin, Some(origin))?;
        }
        let sender = validate_federation_member_event(&auth.origin, &room_id, &event_id, &body, "join")?;
        let _room_version = federatable_room_version(&ctx, &room_id).await?;
        validate_federation_join_access(&ctx, &room_id, sender).await?;
        let content = body.get("content").cloned().unwrap_or(json!({}));
        let display_name = content.get("displayname").and_then(|v| v.as_str());

        let params = synapse_storage::event::CreateEventParams {
            event_id: event_id.clone(),
            room_id: room_id.clone(),
            user_id: sender.to_string(),
            event_type: "m.room.member".to_string(),
            content: content.clone(),
            state_key: Some(sender.to_string()),
            origin_server_ts: body
                .get("origin_server_ts")
                .and_then(|v| v.as_i64())
                .unwrap_or_else(current_timestamp_millis),
            redacts: None,
        };
        ctx.room_service
            .messaging()
            .create_event(params, None)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to persist join event", &e))?;
        dispatch_federation_member_event_to_appservice(&ctx, &event_id, &room_id, sender, &content, Some(sender)).await;

        ctx.room_service
            .membership()
            .add_member(&room_id, sender, "join", display_name, None, None)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to update membership", &e))?;

        ::tracing::info!(
            target: "federation",
            request_id = %request_id,
            event = "federation_send_join_v2",
            origin = %auth.origin,
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
    super::decrement_gauge(&ctx, "federation_join_in_flight");
    match &result {
        Ok(_) => super::increment_counter(&ctx, "federation_join_ok_total"),
        Err(e) if e.is_rate_limited() => super::increment_counter(&ctx, "federation_join_429_total"),
        Err(_) => super::increment_counter(&ctx, "federation_join_error_total"),
    }

    result
}

async fn validate_federation_join_access(ctx: &FederationContext, room_id: &str, user_id: &str) -> ApiResult<()> {
    let join_rule = super::get_effective_room_join_rule(ctx, room_id).await?;
    let existing_member = ctx.room_service.membership().get_room_member_record(room_id, user_id).await?;

    if let Some(member) = existing_member.as_ref() {
        if member.membership == "join" {
            return Ok(());
        }

        if member.membership == "ban"
            || member.is_banned.unwrap_or_else(|| {
                ::tracing::warn!(
                    room_id = room_id,
                    user_id = user_id,
                    "is_banned field is NULL for non-ban member; assuming not banned"
                );
                false
            })
        {
            return Err(ApiError::forbidden("User is not allowed to join this room"));
        }
    }

    if join_rule != "public" && existing_member.as_ref().is_none_or(|member| member.membership != "invite") {
        return Err(ApiError::forbidden("User is not allowed to join this room"));
    }

    Ok(())
}
