use crate::common::*;
use crate::web::middleware::FederationRequestAuth;
use crate::web::routes::context::FederationContext;
use axum::extract::{Extension, Json, Path, State};
use serde_json::{json, Value};

use super::{
    dispatch_federation_member_event_to_appservice, federatable_room_version, validate_federation_user_origin,
};

pub(crate) async fn knock_room(
    State(ctx): State<FederationContext>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path((room_id, user_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_federation_user_origin(&auth.origin, &user_id)?;
    validate_federation_knock_event(&auth.origin, &room_id, &user_id, &body)?;
    let _room_version = federatable_room_version(&ctx, &room_id).await?;

    let event_id = format!("${}", crate::common::crypto::generate_event_id(&ctx.server_name));
    let origin_server_ts = chrono::Utc::now().timestamp_millis();

    let content = json!({"membership": "knock"});
    let params = synapse_storage::event::CreateEventParams {
        event_id: event_id.clone(),
        room_id: room_id.clone(),
        user_id: user_id.clone(),
        event_type: "m.room.member".to_string(),
        content: content.clone(),
        state_key: Some(user_id.clone()),
        origin_server_ts,
        redacts: None,
    };

    ctx.room_service
        .messaging()
        .create_event(params, None)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to create knock event", &e))?;
    dispatch_federation_member_event_to_appservice(&ctx, &event_id, &room_id, &user_id, &content, Some(&user_id)).await;

    // P1-14: Spec-compliant response — return full event object under "event" key,
    // and use "knock" (not "knocking") for the state field.
    Ok(Json(json!({
        "event": {
            "event_id": event_id,
            "room_id": room_id,
            "sender": user_id,
            "type": "m.room.member",
            "state_key": user_id,
            "content": content,
            "origin_server_ts": origin_server_ts,
            "origin": auth.origin,
        },
        "state": "knock"
    })))
}

// ---------------------------------------------------------------------------
// Knock-specific event validation helpers
// ---------------------------------------------------------------------------

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
