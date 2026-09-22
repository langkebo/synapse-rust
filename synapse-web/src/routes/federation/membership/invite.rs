use crate::middleware::FederationRequestAuth;
use crate::routes::context::FederationContext;
use crate::utils::auth::resolve_request_id;
use axum::{
    extract::{Extension, Json, Path, State},
    http::HeaderMap,
};
use serde_json::{json, Value};
use synapse_common::current_timestamp_millis;
use synapse_common::*;

use super::{dispatch_federation_member_event_to_appservice, federatable_room_version, re_sign_pdu_locally};
use crate::routes::extractors::RoomId;

/// See [`thirdparty_invite`].
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
    // OPT-017: Check room access BEFORE room version to prevent existence leaking.
    super::validate_federation_origin_can_observe_room(&ctx, room_id, &auth.origin).await?;
    let _room_version = federatable_room_version(&ctx, room_id).await?;

    // Same gate as the client invite path: a remote inviter must not be able
    // to place an invite that the room's lists or the invitee's own account
    // policy would have refused locally.
    ctx.room_service.membership().authorize_invite_policy(room_id, sender, invitee).await?;

    let event_id = format!("${}", synapse_common::crypto::generate_event_id(&ctx.server_name));

    let content = json!({
        "membership": "invite",
        "third_party_invite": {
            "signed": {
                "mxid": invitee,
                "token": format!("third_party_token_{}", event_id)
            }
        }
    });
    let params = synapse_services::event::CreateEventParams {
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
        .map_err(|e| ApiError::internal_with_cause("Failed to create invite event", e))?;
    // F-03: add local server signature so other origins can verify the
    // invite. We build a minimal PDU from the persisted fields.
    let mut pdu = json!({
        "event_id": event_id,
        "room_id": room_id,
        "sender": sender,
        "type": "m.room.member",
        "state_key": invitee,
        "content": content,
        "origin_server_ts": current_timestamp_millis(),
        "origin": auth.origin,
    });
    re_sign_pdu_locally(&ctx, &event_id, &mut pdu).await;
    dispatch_federation_member_event_to_appservice(&ctx, &event_id, room_id, sender, &content, Some(invitee)).await;

    Ok(Json(json!({
        "event_id": event_id,
        "room_id": room_id,
        "state": "invited"
    })))
}

/// See [`invite_v2`].
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
    // OPT-017: Check room access BEFORE room version to prevent existence leaking.
    super::validate_federation_origin_can_observe_room(&ctx, &room_id, &auth.origin).await?;
    let _room_version = federatable_room_version(&ctx, &room_id).await?;
    let content = body.get("content").cloned().unwrap_or(json!({}));

    // Same gate as the client invite path — see `thirdparty_invite`.
    ctx.room_service.membership().authorize_invite_policy(&room_id, sender, state_key).await?;

    let content_for_as = content.clone();

    let params = synapse_services::event::CreateEventParams {
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
        .map_err(|e| ApiError::internal_with_cause("Failed to create invite event", e))?;

    // F-03: add local server signature so other origins can verify the
    // invite. We reconstruct the minimal PDU that was persisted.
    let mut pdu = json!({
        "event_id": event_id,
        "room_id": room_id,
        "sender": sender,
        "type": "m.room.member",
        "state_key": state_key,
        "content": content_for_as,
        "origin_server_ts": body.get("origin_server_ts").and_then(|v| v.as_i64()).unwrap_or(current_timestamp_millis()),
        "origin": auth.origin,
    });
    re_sign_pdu_locally(&ctx, &event_id, &mut pdu).await;

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

/// See [`invite`].
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
    // OPT-017: Check room access BEFORE room version to prevent existence leaking.
    super::validate_federation_origin_can_observe_room(&ctx, &room_id, &auth.origin).await?;
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

/// See [`exchange_third_party_invite`].
pub(crate) async fn exchange_third_party_invite(
    State(ctx): State<FederationContext>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(room_id): Path<RoomId>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Test third-party invite event validation
    #[test]
    fn test_validate_federation_exchange_third_party_invite_event_valid() {
        let event = json!({
            "origin": "example.com",
            "sender": "@user:example.com",
            "room_id": "!room123:example.com",
            "type": "m.room.member",
            "state_key": "@invited:example.com",
            "content": {
                "membership": "invite"
            }
        });

        let result = validate_federation_exchange_third_party_invite_event(
            "example.com",
            "!room123:example.com",
            &event
        );

        assert!(result.is_ok());
        let (sender, state_key) = result.unwrap();
        assert_eq!(sender, "@user:example.com");
        assert_eq!(state_key, "@invited:example.com");
    }

    #[test]
    fn test_validate_missing_sender() {
        let event = json!({
            "origin": "example.com",
            "room_id": "!room123:example.com",
            "type": "m.room.member",
            "state_key": "@invited:example.com",
            "content": {
                "membership": "invite"
            }
        });

        let result = validate_federation_exchange_third_party_invite_event(
            "example.com",
            "!room123:example.com",
            &event
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_validate_wrong_membership() {
        let event = json!({
            "origin": "example.com",
            "sender": "@user:example.com",
            "room_id": "!room123:example.com",
            "type": "m.room.member",
            "state_key": "@invited:example.com",
            "content": {
                "membership": "join"
            }
        });

        let result = validate_federation_exchange_third_party_invite_event(
            "example.com",
            "!room123:example.com",
            &event
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_validate_empty_state_key() {
        let event = json!({
            "origin": "example.com",
            "sender": "@user:example.com",
            "room_id": "!room123:example.com",
            "type": "m.room.member",
            "state_key": "",
            "content": {
                "membership": "invite"
            }
        });

        let result = validate_federation_exchange_third_party_invite_event(
            "example.com",
            "!room123:example.com",
            &event
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_validate_wrong_event_type() {
        let event = json!({
            "origin": "example.com",
            "sender": "@user:example.com",
            "room_id": "!room123:example.com",
            "type": "m.room.create",
            "state_key": "@invited:example.com",
            "content": {
                "membership": "invite"
            }
        });

        let result = validate_federation_exchange_third_party_invite_event(
            "example.com",
            "!room123:example.com",
            &event
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_validate_room_id_mismatch() {
        let event = json!({
            "origin": "example.com",
            "sender": "@user:example.com",
            "room_id": "!wrong-room:example.com",
            "type": "m.room.member",
            "state_key": "@invited:example.com",
            "content": {
                "membership": "invite"
            }
        });

        let result = validate_federation_exchange_third_party_invite_event(
            "example.com",
            "!room123:example.com",
            &event
        );

        assert!(result.is_err());
    }
}
