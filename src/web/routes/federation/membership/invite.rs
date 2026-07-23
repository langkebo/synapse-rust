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

use super::{dispatch_federation_member_event_to_appservice, federatable_room_version};

pub(crate) async fn thirdparty_invite(
    State(ctx): State<FederationContext>,
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
    super::validate_federation_user_origin(&auth.origin, sender)?;
    let _room_version = federatable_room_version(&ctx, room_id).await?;

    let event_id = format!("${}", crate::common::crypto::generate_event_id(&ctx.server_name));

    let content = json!({
        "membership": "invite",
        "third_party_invite": {
            "signed": {
                "mxid": invitee,
                "token": format!("third_party_token_{}", event_id)
            }
        }
    });
    let params = synapse_storage::event::CreateEventParams {
        event_id: event_id.clone(),
        room_id: room_id.to_string(),
        user_id: sender.to_string(),
        event_type: "m.room.member".to_string(),
        content: content.clone(),
        state_key: Some(invitee.to_string()),
        origin_server_ts: current_timestamp_millis(),
        redacts: None,
    };

    ctx.room_service
        .messaging()
        .create_event(params, None)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to create invite event", &e))?;
    dispatch_federation_member_event_to_appservice(&ctx, &event_id, room_id, sender, &content, Some(invitee)).await;

    Ok(Json(json!({
        "event_id": event_id,
        "room_id": room_id,
        "state": "invited"
    })))
}

pub(crate) async fn invite_v2(
    State(ctx): State<FederationContext>,
    Extension(auth): Extension<FederationRequestAuth>,
    headers: HeaderMap,
    Path((room_id, event_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);
    if let Some(origin) = body.get("origin").and_then(|v| v.as_str()) {
        super::validate_federation_origin(&auth.origin, Some(origin))?;
    }
    let (sender, state_key) = validate_federation_invite_event(&auth.origin, &room_id, &event_id, &body)?;
    let _room_version = federatable_room_version(&ctx, &room_id).await?;
    let content = body.get("content").cloned().unwrap_or(json!({}));

    let content_for_as = content.clone();

    let params = synapse_storage::event::CreateEventParams {
        event_id: event_id.clone(),
        room_id: room_id.clone(),
        user_id: sender.to_string(),
        event_type: "m.room.member".to_string(),
        content,
        state_key: Some(state_key.to_string()),
        origin_server_ts: body.get("origin_server_ts").and_then(|v| v.as_i64()).unwrap_or(current_timestamp_millis()),
        redacts: None,
    };

    ctx.room_service
        .messaging()
        .create_event(params, None)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to create invite event", &e))?;
    dispatch_federation_member_event_to_appservice(&ctx, &event_id, &room_id, sender, &content_for_as, Some(state_key))
        .await;

    ::tracing::info!(
        request_id = %request_id,
        origin = %auth.origin,
        room_id = %room_id,
        event_id = %event_id,
        "Processed v2 invite"
    );

    Ok(Json(json!({
        "event_id": event_id
    })))
}

pub(crate) async fn invite(
    State(ctx): State<FederationContext>,
    Extension(auth): Extension<FederationRequestAuth>,
    headers: HeaderMap,
    Path((room_id, event_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);
    if let Some(origin) = body.get("origin").and_then(|v| v.as_str()) {
        super::validate_federation_origin(&auth.origin, Some(origin))?;
    }
    validate_federation_invite_event(&auth.origin, &room_id, &event_id, &body)?;
    let _room_version = federatable_room_version(&ctx, &room_id).await?;

    ::tracing::info!(
        request_id = %request_id,
        origin = %auth.origin,
        room_id = %room_id,
        event_id = %event_id,
        "Processing invite"
    );

    Ok(Json(json!({
        "event_id": event_id
    })))
}

pub(crate) async fn exchange_third_party_invite(
    State(ctx): State<FederationContext>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    if !room_id.starts_with('!') || !room_id.contains(':') {
        return Err(ApiError::bad_request("Invalid room_id format"));
    }

    let room_version = federatable_room_version(&ctx, &room_id).await?;

    let default_event_id = format!("${}:{}", uuid::Uuid::new_v4(), room_id.split(':').next_back().unwrap_or("server"));
    let event_id = body.get("event_id").and_then(|v| v.as_str()).unwrap_or(&default_event_id).to_string();

    let origin_server_ts =
        body.get("origin_server_ts").and_then(|v| v.as_i64()).unwrap_or_else(current_timestamp_millis);

    let (sender, state_key) = validate_federation_exchange_third_party_invite_event(&auth.origin, &room_id, &body)?;
    let content = body.get("content").cloned().unwrap_or_else(|| json!({}));

    // Build the event JSON that will be signed and returned to the requesting
    // (invitee's) homeserver.  The requesting server persists the event; we
    // only sign it because we are the room's home server and hold the
    // `m.room.third_party_invite` state that backs this token.
    let mut signed_event = json!({
        "event_id": event_id,
        "room_id": room_id,
        "type": "m.room.member",
        "sender": sender,
        "origin": auth.origin,
        "origin_server_ts": origin_server_ts,
        "room_version": room_version,
        "state_key": state_key,
        "content": content,
    });

    // Sign the event with the local server's key.
    let local_server = &ctx.server_name;
    if let Ok(Some(key)) = ctx.key_rotation_manager.get_current_key().await {
        if let Err(e) = synapse_federation::signing::sign_and_hash_event(
            local_server,
            &key.key_id,
            &key.secret_key,
            &mut signed_event,
        ) {
            ::tracing::warn!(
                room_id = %room_id,
                event_id = %event_id,
                error = %e,
                "Failed to sign third-party invite event"
            );
        }
    }

    Ok(Json(signed_event))
}

// ---------------------------------------------------------------------------
// Invite-specific event validation helpers
// ---------------------------------------------------------------------------

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
