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

use super::{
    dispatch_federation_member_event_to_appservice, federatable_room_version, project_and_sign_pdu_locally,
    re_sign_pdu_locally,
};
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
    if !synapse_common::room_id::is_well_formed_room_id(room_id) {
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

    // A placeholder for the third-party token below only: the write entry below
    // replaces it with the final ID (reference hash for v3+), which is the value
    // consumed further down. `generate_event_id` already carries its own `$`.
    let event_id = synapse_common::crypto::generate_event_id(&ctx.server_name);

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

    let stored = ctx
        .room_service
        .messaging()
        .create_event(params, None)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to create invite event", e))?;

    // The write entry point owns event identity (§4.1): consume the ID it
    // returns rather than the locally generated placeholder, otherwise the row
    // lookup below finds nothing and the invite stays unsigned.
    let event_id = stored.event_id;

    // F-03: sign the PDU the persisted row projects to, so other origins can
    // verify the invite. The helper reads the row back itself — a
    // hand-assembled partial dict would hash bytes no peer can reproduce.
    re_sign_pdu_locally(&ctx, &event_id).await;
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
    Path((room_id, path_event_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);

    // The v2 request body is `{"event": …, "room_version": …, "invite_room_state": …}`
    // (spec `PUT /_matrix/federation/v2/invite`; upstream
    // `FederationV2InviteServlet`), **not** the bare event.  `room_version` is
    // the only source of the version for a v3+ PDU, which carries neither a
    // version nor an `event_id` of its own.
    let event = body.get("event").ok_or_else(|| ApiError::bad_request("Missing event in invite body".to_string()))?;
    let room_version = body
        .get("room_version")
        .and_then(Value::as_str)
        .ok_or_else(|| ApiError::bad_request("Missing room_version in invite body".to_string()))?
        .to_string();

    if let Some(origin) = event.get("origin").and_then(|v| v.as_str()) {
        super::validate_federation_origin(&auth.origin, Some(origin))?;
    }
    let (sender, state_key) = validate_federation_invite_event(&auth.origin, &room_id, event)?;

    // OPT-017 still applies, but only for rooms we host: the invitee's server
    // legitimately learns of a room *through* this request, so an unknown room
    // must be allowed through. See the helper for the full rationale.
    super::validate_federation_invite_origin_can_observe_room(&ctx, &room_id, &auth.origin).await?;

    // If we know the room, the version we were told must be the one we have on
    // record — otherwise our own ID/signature computation would diverge from the
    // sender's.
    match ctx.room_service.state().get_room_version(&room_id).await {
        Ok(Some(local_version)) if local_version != room_version => {
            return Err(ApiError::bad_request(format!(
                "invite room_version {room_version} does not match the local room version {local_version}"
            )));
        }
        Ok(_) => {}
        Err(e) => {
            return Err(ApiError::internal_with_cause("Failed to read room version", e));
        }
    }

    // The event ID is the sender's: v3+ derive it from the received PDU, v1/v2
    // carry it.  Never invent one, and never let the write path re-derive a
    // different one.
    let persisted_event_id = synapse_common::event_id::resolve_received_event_id(&room_version, event)
        .map_err(|e| ApiError::bad_request(format!("Cannot derive the invited event's ID: {e}")))?;

    // v1/v2 keep the sender-assigned ID, and the request path must agree with it.
    if !synapse_common::event_id::uses_reference_hash_event_id(&room_version) && persisted_event_id != path_event_id {
        return Err(ApiError::bad_request("Invite event_id does not match the request path".to_string()));
    }

    let content = event.get("content").cloned().unwrap_or(json!({}));

    // Same gate as the client invite path — see `thirdparty_invite`.
    ctx.room_service.membership().authorize_invite_policy(&room_id, sender, state_key).await?;

    let content_for_as = content.clone();

    // The received PDU's graph position is authoritative: persisting through the
    // explicit-graph path keeps the sender's ID and DAG fields byte-faithful
    // (`create_event` would re-derive an ID from *our* state and disagree).
    let depth = event.get("depth").and_then(Value::as_i64);
    let prev_events = event_id_array(event, "prev_events");
    let auth_events = event_id_array(event, "auth_events");
    let (Some(depth), Some(prev_events), Some(auth_events)) = (depth, prev_events, auth_events) else {
        return Err(ApiError::bad_request(
            "Invite PDU is missing depth/prev_events/auth_events; refusing to persist it under a fabricated DAG position"
                .to_string(),
        ));
    };

    // A federated invite is how this server first learns the room exists, so it
    // may hold no `rooms` row yet — and `events.room_id` /
    // `room_memberships.room_id` are both foreign keys onto `rooms(room_id)`.
    // Materialise the row now, *after* the DAG-field check, so a malformed
    // invite is rejected before it can leave an orphan room behind.
    let invite_join_rule = invite_room_state_join_rule(body.get("invite_room_state"));
    ctx.room_service
        .membership()
        .ensure_room_record_for_remote_invite(&room_id, &room_version, sender, &invite_join_rule)
        .await?;

    // Whether we already hold this room's `m.room.create`. If we do, the invite's
    // `prev_events` name parents we hold and the graph path applies; if we do
    // not, this request is how we first learn the room exists, so every
    // `prev_events` entry is a parent we have never seen and writing
    // `event_edges` for it would fail the foreign key onto `events`.
    let room_is_hosted = ctx.room_service.messaging().get_room_create_event_id(&room_id).await?.is_some();

    let params = synapse_services::event::CreateEventParams {
        event_id: persisted_event_id.clone(),
        room_id: room_id.clone(),
        user_id: sender.to_string(),
        event_type: "m.room.member".to_string(),
        content,
        state_key: Some(state_key.to_string()),
        origin_server_ts: event
            .get("origin_server_ts")
            .and_then(|v| v.as_i64())
            .unwrap_or_else(current_timestamp_millis),
        redacts: None,
    };

    // Both paths keep the sender's ID and DAG fields byte-faithful; they differ
    // only in whether the `event_edges` rows are written, i.e. whether the
    // parents named above are this server's own DAG or foreign events it can only
    // record as an outlier (see `create_outlier_event`).
    let stored = if room_is_hosted {
        ctx.room_service.messaging().create_event_with_graph(params, &prev_events, &auth_events, depth, None).await
    } else {
        ctx.room_service.messaging().create_outlier_event(params, &prev_events, &auth_events, depth, None).await
    }
    .map_err(|e| ApiError::internal_with_cause("Failed to create invite event", e))?;

    // The invitee is one of our users — that is why the remote server sent the
    // invite here — so ensure a local `users` row exists before the membership
    // write below, whose `user_id` is a foreign key onto it. Idempotent for an
    // account we already host.
    ctx.user_service.ensure_remote_user(state_key).await?;

    // Nothing else records the invitee's membership: the persisted event is the
    // remote PDU, and the write above goes through the raw event path. Without
    // this row `/sync` and `/rooms/{roomId}/state` would not show the invite.
    ctx.room_service.membership().record_inbound_federation_invite(&room_id, state_key, sender).await?;

    // F-03: sign the PDU the persisted row projects to (see `thirdparty_invite`);
    // the graph fields the sender supplied above are part of the signed bytes.
    // The spec answer is `{"event": …}` — the caller must be able to verify the
    // event it just handed us, so a missing signature is a hard failure.
    let Some(signed_pdu) = project_and_sign_pdu_locally(&ctx, &stored.event_id).await else {
        return Err(ApiError::internal(
            "Failed to sign the persisted invite event for the federation response".to_string(),
        ));
    };

    dispatch_federation_member_event_to_appservice(
        &ctx,
        &stored.event_id,
        &room_id,
        sender,
        &content_for_as,
        Some(state_key),
    )
    .await;

    ::tracing::info!(
        request_id = %request_id,
        origin = %auth.origin,
        room_id = %room_id,
        event_id = %stored.event_id,
        "Processed v2 invite"
    );

    Ok(Json(json!({
        "event": signed_pdu
    })))
}

/// Derive the room's join rule from the `invite_room_state` stripped state the
/// inviting server supplies, defaulting to `invite`.
///
/// `invite_room_state` is a list of stripped state events (`type`, `state_key`,
/// `content`, `sender`); the `m.room.join_rules` entry carries the rule in
/// `content.join_rule`. Anything absent, malformed, or outside the
/// `rooms.join_rules` check-constraint whitelist falls back to `invite` — the
/// most restrictive choice, and the value the local invite path records.
fn invite_room_state_join_rule(invite_room_state: Option<&Value>) -> String {
    let raw = invite_room_state.and_then(Value::as_array).and_then(|events| {
        events.iter().find_map(|ev| {
            if ev.get("type").and_then(Value::as_str) != Some("m.room.join_rules") {
                return None;
            }
            ev.get("content")?.get("join_rule")?.as_str()
        })
    });

    match raw {
        Some(rule @ ("invite" | "public" | "knock" | "restricted")) => rule.to_string(),
        _ => "invite".to_string(),
    }
}

/// Collect a PDU's `field` array of event IDs, or `None` when the key is absent.
///
/// `Some(vec![])` (an explicitly empty list) is preserved: for a create event an
/// empty `prev_events` is legitimate, whereas a missing key is not.
fn event_id_array(event: &Value, field: &str) -> Option<Vec<String>> {
    event
        .get(field)
        .and_then(|v| v.as_array())
        .map(|array| array.iter().filter_map(|v| v.as_str().map(String::from)).collect())
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
    validate_federation_invite_event(&auth.origin, &room_id, &body)?;
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
    if !synapse_common::room_id::is_well_formed_room_id(&room_id) {
        return Err(ApiError::bad_request("Invalid room_id format"));
    }

    let room_version = federatable_room_version(&ctx, &room_id).await?;

    // Generate a default event_id if not provided in the body.
    // For v3+ (including v12), event IDs are derived from the reference hash.
    // For v1/v2, use the opaque server-based form `$<timestamp>:<server>`.
    let default_event_id = if room_id.contains(':') {
        // Legacy room IDs (v1-v11) contain a server sigil (`!<room_id>:server`).
        // Extract the server portion for the event ID fallback.
        format!("${}:{}", uuid::Uuid::new_v4(), room_id.split(':').next_back().unwrap_or("server"))
    } else {
        // v12+ domainless room IDs have no colon; use a format compatible with
        // the opaque server name that will appear in the room ID itself.
        format!("${}", uuid::Uuid::new_v4())
    };
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
        "state_key": state_key,
        "content": content,
    });

    // Sign the event with the local server's key.
    let local_server = &ctx.server_name;
    if let Ok(Some(key)) = ctx.key_rotation_manager.get_current_key().await {
        if let Err(e) = synapse_federation::signing::sign_and_hash_event(
            &room_version,
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

    // There is deliberately **no** top-level `event_id` check here.
    //
    // Room version 3 removed `event_id` from federated PDUs ("A server receiving
    // an event should compute the relevant event ID for itself"), so a compliant
    // v3+ invite carries none — requiring it rejected every upstream invite.
    // Upstream's `FederationV2InviteServlet` does not compare the path segment
    // with the body either (`# TODO(paul): assert that event_id parsed from path
    // actually match those given in content`).  The v1/v2 case is checked by the
    // caller with the derived ID instead (see `invite_v2`).

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

        let result =
            validate_federation_exchange_third_party_invite_event("example.com", "!room123:example.com", &event);

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

        let result =
            validate_federation_exchange_third_party_invite_event("example.com", "!room123:example.com", &event);

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

        let result =
            validate_federation_exchange_third_party_invite_event("example.com", "!room123:example.com", &event);

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

        let result =
            validate_federation_exchange_third_party_invite_event("example.com", "!room123:example.com", &event);

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

        let result =
            validate_federation_exchange_third_party_invite_event("example.com", "!room123:example.com", &event);

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

        let result =
            validate_federation_exchange_third_party_invite_event("example.com", "!room123:example.com", &event);

        assert!(result.is_err());
    }
}
