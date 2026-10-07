/// The `invite` module.
pub(crate) mod invite;
/// The `join` module.
pub(crate) mod join;
/// The `knock` module.
pub(crate) mod knock;
/// The `leave` module.
pub(crate) mod leave;
/// The `query` module.
pub(crate) mod query;

use crate::routes::context::FederationContext;
use crate::routes::federation::pdu::{
    apply_stored_signature_material, signature_action, state_pdu, PduCompleteness, SignatureAction,
};
use crate::routes::AppState;
use axum::{
    routing::{get, post, put},
    Router,
};
use serde_json::Value;
use synapse_common::*;

// ---------------------------------------------------------------------------
// Re-export federation-level helpers so submodules can access them via
// `super::` (instead of the more verbose `super::super::`).
// ---------------------------------------------------------------------------
use super::{
    acquire_with_timeout, decrement_gauge, increment_counter, increment_gauge, observe_histogram, sender_server_name,
    user_matches_origin, validate_federation_invite_origin_can_observe_room, validate_federation_origin,
    validate_federation_origin_can_observe_room, validate_federation_origin_shares_user_room,
};

// ---------------------------------------------------------------------------
// Shared helper functions used across multiple submodules
// ---------------------------------------------------------------------------

/// See [`federatable_room_version`].
pub(crate) async fn federatable_room_version(ctx: &FederationContext, room_id: &str) -> Result<String, ApiError> {
    let room = ctx
        .room_service
        .state()
        .get_room_record(room_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Room not found"))?;

    if !can_federate_room_version(&room.room_version) {
        return Err(ApiError::incompatible_room_version(format!(
            "Room version {} is not supported for federation",
            room.room_version
        )));
    }

    Ok(room.room_version)
}

/// See [`dispatch_federation_member_event_to_appservice`].
pub(crate) async fn dispatch_federation_member_event_to_appservice(
    ctx: &FederationContext,
    event_id: &str,
    room_id: &str,
    sender: &str,
    content: &Value,
    state_key: Option<&str>,
) {
    ctx.room_service.dispatch_appservice_event(event_id, room_id, "m.room.member", sender, content, state_key).await;
}

/// See [`validate_federation_user_origin`].
pub(crate) fn validate_federation_user_origin(authenticated_origin: &str, user_id: &str) -> Result<(), ApiError> {
    if sender_server_name(user_id) != Some(authenticated_origin) {
        return Err(ApiError::forbidden("Federation user_id does not match authenticated origin".to_string()));
    }

    Ok(())
}

/// See [`validate_federation_member_event`].
pub(crate) fn validate_federation_member_event<'a>(
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

    if sender_server_name(sender) != Some(authenticated_origin) {
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
        validate_federation_origin(authenticated_origin, Some(event_origin))?;
    }

    Ok(sender)
}

/// FED-02: verify an inbound `/send_join` event's PDU integrity **when it
/// carries the evidence**. Per the spec the request body is an *event template*
/// (`origin` / `origin_server_ts` / `type` / `state_key` / `content`) which the
/// resident server completes itself, so a conformant template has no `hashes` /
/// `signatures` — requiring them would reject every spec-compliant peer. When a
/// sender does volunteer those fields (our own `make_join` template invites this,
/// since it supplies `prev_events` / `auth_events` / `depth` to be signed), they
/// must check out before we persist. This closes the gap where the event was
/// previously shape-validated but never PDU-verified.
pub(crate) async fn verify_inbound_join_pdu_integrity(
    ctx: &FederationContext,
    room_version: &str,
    event: &Value,
) -> Result<(), ApiError> {
    if event.get("hashes").is_some() {
        crate::federation::signing::verify_event_content_hash(event)
            .map_err(|e| ApiError::bad_request(format!("Invalid join event content hash: {e}")))?;
    }

    // Verify only a signature block that actually covers the sender's own
    // server; a template (no `signatures`) or an unrelated block passes through.
    let sender_server = event.get("sender").and_then(Value::as_str).and_then(sender_server_name);
    let has_sender_signature = sender_server
        .and_then(|server| event.get("signatures").and_then(Value::as_object).and_then(|sigs| sigs.get(server)))
        .and_then(Value::as_object)
        .is_some_and(|entries| !entries.is_empty());
    if has_sender_signature {
        crate::routes::federation::transaction::verify_pdu_sender_signature(ctx, room_version, event)
            .await
            .map_err(|e| ApiError::forbidden(format!("Invalid join event signature: {e}")))?;
    }

    Ok(())
}

/// See [`get_effective_room_join_rule_content`].
pub(crate) async fn get_effective_room_join_rule_content(
    ctx: &FederationContext,
    room_id: &str,
) -> ApiResult<Option<Value>> {
    Ok(ctx
        .room_service
        .messaging()
        .get_state_events_by_type(room_id, "m.room.join_rules")
        .await?
        .into_iter()
        .find(|event| event.get("state_key").and_then(Value::as_str).unwrap_or_default().is_empty())
        .and_then(|event| event.get("content").cloned()))
}

/// See [`get_effective_room_join_rule`].
pub(crate) async fn get_effective_room_join_rule(ctx: &FederationContext, room_id: &str) -> ApiResult<String> {
    let effective_join_rule = if let Some(content) = get_effective_room_join_rule_content(ctx, room_id).await? {
        content.get("join_rule").and_then(|value| value.as_str()).map(|value| value.to_string())
    } else {
        None
    };

    let room = ctx
        .room_service
        .state()
        .get_room_record(room_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Room not found"))?;

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

// ---------------------------------------------------------------------------
// F-03: Local re-sign helper for federation-derived events
// ---------------------------------------------------------------------------
//
// Used by `invite`, `join`, and `leave` route handlers after the event row has
// been persisted, to add the local server's ed25519 signature to the PDU.
// Without this, third-party origins in `verify_pdu_sender_signature` would
// reject the event because only the remote sender's signature is present.

/// Sign the **projected** persisted event with the local server's current
/// signing key and persist `signatures` + `hashes` back into the events row.
///
/// The signature must cover the exact PDU a peer will receive: the projection
/// of the stored row ([`state_pdu`], including the row's `depth` /
/// `prev_events` / `auth_events`), **not** a hand-assembled subset of the
/// event's fields. Signing a partial dict yields a `hashes.sha256` and a
/// signature that no verifier can reproduce from the full PDU, so every peer
/// rejects the event.
///
/// Fail-closed, exactly like the other federation projectors: if the room
/// version cannot be resolved or the projection is incomplete (the row carries
/// no graph metadata — e.g. an event written by `create_event`), the event is
/// left unsigned rather than fabricating a DAG position or signing bytes
/// nobody can reproduce. See `crate::routes::federation::pdu` for the rationale.
///
/// Best-effort towards the caller: the inbound federation request has already
/// been accepted, so failures are logged and swallowed. Callers that need the
/// signed PDU itself (to put it in a federation *response*, e.g. the invite
/// endpoints) use [`project_and_sign_pdu_locally`].
pub(crate) async fn re_sign_pdu_locally(ctx: &FederationContext, event_id: &str) {
    let _ = project_and_sign_pdu_locally(ctx, event_id).await;
}

/// [`re_sign_pdu_locally`], but handing back the projected and locally-signed
/// PDU so a federation response can echo it (the invite endpoints must answer
/// with `{"event": <PDU>}`, and the signed bytes have to be exactly the ones
/// persisted above).
///
/// Returns `None` — logging why — whenever the event cannot be signed; the
/// caller then answers without a PDU rather than emitting one nobody can verify.
pub(crate) async fn project_and_sign_pdu_locally(ctx: &FederationContext, event_id: &str) -> Option<Value> {
    let local_server = &ctx.server_name;

    // 1. The persisted row is the only authority on what a peer will receive.
    let record = match ctx.room_service.messaging().get_event_record(event_id).await {
        Ok(Some(record)) => record,
        Ok(None) => {
            ::tracing::warn!(
                event_id = %event_id,
                server_name = %local_server,
                "F-03: no persisted row for event — nothing to sign"
            );
            return None;
        }
        Err(error) => {
            ::tracing::warn!(
                event_id = %event_id,
                server_name = %local_server,
                %error,
                "F-03: failed to read the persisted event — refusing to sign"
            );
            return None;
        }
    };

    // 2. Signature material is room-version dependent (the redaction applied
    //    before signing differs per version), so the version must be resolved,
    //    never guessed.
    let room_version = match ctx.room_service.state().get_room_version(&record.room_id).await {
        Ok(Some(room_version)) => room_version,
        Ok(None) => {
            ::tracing::warn!(
                event_id = %event_id,
                server_name = %local_server,
                room_id = %record.room_id,
                "F-03: no room version recorded for room — refusing to sign; event will lack local signature"
            );
            return None;
        }
        Err(error) => {
            ::tracing::warn!(
                event_id = %event_id,
                server_name = %local_server,
                room_id = %record.room_id,
                %error,
                "F-03: failed to resolve room version — refusing to sign; event will lack local signature"
            );
            return None;
        }
    };

    // 3. Project the row with the very projector the federation emitters use,
    //    so the signed bytes and the emitted bytes cannot diverge.
    let state_records = match ctx.room_service.messaging().get_state_event_records(&record.room_id).await {
        Ok(records) => records,
        Err(error) => {
            ::tracing::warn!(
                event_id = %event_id,
                server_name = %local_server,
                room_id = %record.room_id,
                %error,
                "F-03: failed to read the room state — refusing to sign; event will lack local signature"
            );
            return None;
        }
    };
    let Some(persisted) = state_records.iter().find(|candidate| candidate.event_id == event_id) else {
        ::tracing::warn!(
            event_id = %event_id,
            server_name = %local_server,
            room_id = %record.room_id,
            "F-03: persisted event is not part of the room's current state — refusing to sign; event will lack local signature"
        );
        return None;
    };

    let (mut pdu, completeness) = state_pdu(local_server, persisted, Some(room_version.as_str()));
    if completeness != PduCompleteness::Complete {
        ::tracing::warn!(
            event_id = %event_id,
            server_name = %local_server,
            room_id = %record.room_id,
            "F-03: projected PDU is missing graph metadata (depth/prev_events/auth_events); \
             refusing to sign — signing a PDU whose bytes we cannot reproduce is worse than \
             leaving it unsigned (see federation::pdu module docs)"
        );
        return None;
    }

    match signature_action(persisted, completeness) {
        SignatureAction::KeepStored => {
            // The row already carries the hash/signature pair the origin server
            // signed; reuse it verbatim so the projection stays byte-identical.
            if !apply_stored_signature_material(persisted, &mut pdu) {
                ::tracing::warn!(
                    event_id = %event_id,
                    server_name = %local_server,
                    "F-03: stored hashes/signatures are incomplete — refusing to sign"
                );
                return None;
            }
        }
        SignatureAction::RefuseIncomplete => {
            // Unreachable: completeness was checked above. Kept so a future
            // change to `signature_action` cannot start signing an incomplete PDU.
            ::tracing::warn!(
                event_id = %event_id,
                server_name = %local_server,
                "F-03: projected PDU incomplete — refusing to sign"
            );
            return None;
        }
        SignatureAction::SignLocally => {
            let key = match ctx.key_rotation_manager.get_current_key().await {
                Ok(Some(key)) => key,
                Ok(None) => {
                    ::tracing::warn!(
                        event_id = %event_id,
                        server_name = %local_server,
                        "F-03: no signing key available — federation event will lack local signature"
                    );
                    return None;
                }
                Err(error) => {
                    ::tracing::warn!(
                        event_id = %event_id,
                        server_name = %local_server,
                        %error,
                        "F-03: failed to fetch signing key — federation event will lack local signature"
                    );
                    return None;
                }
            };

            if let Err(error) = synapse_federation::signing::sign_and_hash_event(
                &room_version,
                local_server,
                &key.key_id,
                &key.secret_key,
                &mut pdu,
            ) {
                ::tracing::warn!(
                    event_id = %event_id,
                    server_name = %local_server,
                    %error,
                    "F-03: sign_and_hash_event failed — federation event will lack local signature"
                );
                return None;
            }
        }
    }

    let signatures = pdu.get("signatures").cloned().unwrap_or(Value::Null);
    let hashes = pdu.get("hashes").cloned().unwrap_or(Value::Null);
    if let Err(error) =
        ctx.room_service.messaging().update_event_signatures_and_hashes(event_id, &signatures, &hashes).await
    {
        ::tracing::warn!(
            event_id = %event_id,
            server_name = %local_server,
            %error,
            "F-03: failed to persist local signatures/hashes — event will be missing signatures in subsequent federation"
        );
    }

    // The in-memory PDU is signed even if persisting the pair failed; the peer
    // can still verify it, so hand it back rather than answering without a PDU.
    Some(pdu)
}

// ---------------------------------------------------------------------------
// Router assembly
// ---------------------------------------------------------------------------

/// Build the membership sub-router with all federation membership routes.
pub(crate) fn create_router() -> Router<AppState> {
    Router::new()
        .route("/_matrix/federation/v1/members/{room_id}", get(query::get_room_members))
        .route("/_matrix/federation/v1/members/{room_id}/joined", get(query::get_joined_room_members))
        .route("/_matrix/federation/v1/user/devices/{user_id}", get(query::get_user_devices))
        .route("/_matrix/federation/v1/knock/{room_id}/{user_id}", post(knock::knock_room))
        .route("/_matrix/federation/v1/thirdparty/invite", post(invite::thirdparty_invite))
        .route("/_matrix/federation/v2/invite/{room_id}/{event_id}", put(invite::invite_v2))
        .route("/_matrix/federation/v1/make_join/{room_id}/{user_id}", get(join::make_join))
        .route("/_matrix/federation/v1/make_leave/{room_id}/{user_id}", get(leave::make_leave))
        .route("/_matrix/federation/v1/send_join/{room_id}/{event_id}", put(join::send_join))
        .route("/_matrix/federation/v1/send_leave/{room_id}/{event_id}", put(leave::send_leave))
        .route("/_matrix/federation/v1/invite/{room_id}/{event_id}", put(invite::invite))
        .route("/_matrix/federation/v2/send_join/{room_id}/{event_id}", put(join::send_join_v2))
        .route("/_matrix/federation/v2/send_leave/{room_id}/{event_id}", put(leave::send_leave_v2))
        .route("/_matrix/federation/v1/exchange_third_party_invite/{room_id}", put(invite::exchange_third_party_invite))
        .route("/_synapse/federation/v1/get_joining_rules/{room_id}", get(query::get_joining_rules))
}

// ---------------------------------------------------------------------------
// Route manifest – keeps the route ledger aligned with the router
// ---------------------------------------------------------------------------
