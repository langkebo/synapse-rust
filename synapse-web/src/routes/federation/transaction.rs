use crate::middleware::FederationRequestAuth;
use crate::routes::context::FederationContext;
use crate::routes::extractors::TransactionId;
use crate::utils::auth::resolve_request_id;
use axum::{
    extract::{Extension, Json, Path, State},
    http::HeaderMap,
};
use serde_json::{json, Value};
use std::sync::Arc;
use synapse_common::current_timestamp_millis;
use synapse_common::*;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

mod edus;
use crate::routes::federation::transaction::edus::log_edu_summary;
use crate::routes::federation::transaction::edus::process_inbound_edus as process_edus;

/// See [`send_transaction`].
pub(super) async fn send_transaction(
    State(ctx): State<FederationContext>,
    Extension(auth): Extension<FederationRequestAuth>,
    headers: HeaderMap,
    Path(txn_id): Path<TransactionId>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    super::increment_counter(&ctx, "federation_inbound_txn_total");
    let request_id = resolve_request_id(&headers);

    let origin = body
        .get("origin")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Origin required".to_string()))?;
    super::validate_federation_origin(&auth.origin, Some(origin))?;

    {
        let dedup_key = format!("federation_txn:{origin}:{txn_id}");
        let already_processed: Option<bool> = match ctx.cache.get(&dedup_key).await {
            Ok(val) => val,
            Err(e) => {
                ::tracing::warn!(
                    request_id = %request_id,
                    txn_id = %txn_id,
                    origin = %origin,
                    error = %e,
                    "Failed to read transaction dedup cache, proceeding as not processed"
                );
                None
            }
        };
        if already_processed.unwrap_or(false) {
            ::tracing::debug!(
                request_id = %request_id,
                txn_id = %txn_id,
                origin = %origin,
                "Dedup: transaction already processed, returning empty result"
            );
            super::increment_counter(&ctx, "federation_inbound_txn_dedup_total");
            return Ok(Json(json!({ "results": [] })));
        }
    }
    let pdus = body
        .get("pdus")
        .or_else(|| body.get("pdu"))
        .and_then(|v| v.as_array())
        .ok_or_else(|| ApiError::bad_request("PDUs required".to_string()))?;
    let edus = body.get("edus").and_then(|v| v.as_array());
    let process_inbound_edus = ctx.config.federation.process_inbound_edus;
    let process_inbound_presence_edus = ctx.config.federation.process_inbound_presence_edus;
    let inbound_edus_max_per_txn = ctx.config.federation.inbound_edus_max_per_txn;
    let inbound_presence_updates_max_per_txn = ctx.config.federation.inbound_presence_updates_max_per_txn;

    let edus_array_ref = if process_inbound_edus { edus } else { None };
    if let Some(edus) = edus_array_ref {
        let stats = process_edus(
            &ctx,
            origin,
            &txn_id,
            &request_id,
            edus,
            process_inbound_presence_edus,
            inbound_edus_max_per_txn,
            inbound_presence_updates_max_per_txn,
        )
        .await
        .unwrap_or_default();
        log_edu_summary(&request_id, &txn_id, origin, pdus.len(), edus.len(), &stats);
    }
    let mut results = Vec::new();

    // F-02: PDU count cap is now a config-driven value (default 50, Matrix
    // spec §4.1). Hard-coded 100 was generous but could be exploited for
    // CPU exhaustion on large txns — now bounded by configuration.
    let max_pdus = ctx.config.federation.inbound_max_pdus_per_txn.max(1);
    if pdus.len() > max_pdus {
        ::tracing::warn!(
            target: "security_audit",
            event = "federation_pdu_count_exceeded",
            origin = origin,
            pdu_count = pdus.len(),
            max = max_pdus,
            "Transaction contains too many PDUs - truncating"
        );
    }
    let pdus_to_process = &pdus[..pdus.len().min(max_pdus)];

    for pdu in pdus_to_process {
        // Identity and signature material both depend on the room version: a
        // v3+ PDU carries no `event_id` (the receiver derives the reference
        // hash) and redaction — hence the signed bytes — differs per version.
        // The version is never guessed: an unresolvable one rejects this PDU
        // only, and the rest of the transaction still processes.
        let room_version = match inbound_pdu_room_version(&ctx, pdu).await {
            Ok(version) => version,
            Err(error) => {
                super::increment_counter(&ctx, "federation_inbound_txn_pdu_error_total");
                ::tracing::warn!(
                    target: "security_audit",
                    event = "federation_pdu_room_version_unresolvable",
                    origin = origin,
                    error = %error,
                    "Inbound PDU room version could not be resolved — rejecting"
                );
                results.push(json!({
                    "event_id": carried_event_id_label(pdu),
                    "error": error
                }));
                continue;
            }
        };

        // The receiver must arrive at the same ID the sender computed.  For v3+
        // that is the reference hash of *this* PDU; inventing an ID (the
        // previous `format!("${}", generate_event_id(..))` produced `$$…`) made
        // every later `prev_events` reference to the event dangle.
        let event_id = match synapse_common::event_id::resolve_received_event_id(&room_version, pdu) {
            Ok(event_id) => event_id,
            Err(error) => {
                super::increment_counter(&ctx, "federation_inbound_txn_pdu_error_total");
                ::tracing::warn!(
                    target: "security_audit",
                    event = "federation_pdu_event_id_unresolvable",
                    origin = origin,
                    room_version = %room_version,
                    error = %error,
                    "Inbound PDU has no derivable event ID — rejecting"
                );
                results.push(json!({
                    "event_id": carried_event_id_label(pdu),
                    "error": error.to_string()
                }));
                continue;
            }
        };

        if let Err(e) = crate::federation::signing::check_pdu_size_limits(pdu) {
            super::increment_counter(&ctx, "federation_inbound_txn_pdu_error_total");
            ::tracing::warn!(
                target: "security_audit",
                event = "federation_pdu_size_limit_exceeded",
                event_id = event_id,
                origin = origin,
                error = %e,
                "Inbound PDU exceeded size limits"
            );
            results.push(json!({
                "event_id": event_id,
                "error": e
            }));
            continue;
        }

        if let Err(e) = crate::federation::signing::verify_event_content_hash(pdu) {
            super::increment_counter(&ctx, "federation_inbound_txn_pdu_error_total");
            ::tracing::warn!(
                target: "security_audit",
                event = "federation_pdu_hash_mismatch",
                event_id = event_id,
                origin = origin,
                error = %e,
                "Inbound PDU content hash verification failed"
            );
            results.push(json!({
                "event_id": event_id,
                "error": e
            }));
            continue;
        }

        if let Err(e) = verify_pdu_sender_signature(&ctx, &room_version, pdu).await {
            super::increment_counter(&ctx, "federation_inbound_txn_pdu_error_total");
            ::tracing::warn!(
                target: "security_audit",
                event = "federation_pdu_signature_invalid",
                event_id = event_id,
                origin = origin,
                error = %e,
                "Inbound PDU sender-server signature verification failed - rejecting potential impersonation"
            );
            results.push(json!({
                "event_id": event_id,
                "error": format!("Invalid PDU signature: {}", e)
            }));
            continue;
        }

        let (room_id, user_id, event_type, state_key) = match validate_inbound_transaction_pdu(&auth.origin, pdu) {
            Ok(validated) => validated,
            Err(error) => {
                super::increment_counter(&ctx, "federation_inbound_txn_pdu_error_total");
                results.push(json!({
                    "event_id": event_id,
                    "error": error.to_string()
                }));
                continue;
            }
        };
        let content = pdu.get("content").cloned().unwrap_or(json!({}));
        let origin_server_ts = pdu.get("origin_server_ts").and_then(|v| v.as_i64()).unwrap_or(0);

        if origin != ctx.config.server.name {
            if let Ok(create_events) =
                ctx.room_service.messaging().get_state_events_by_type(room_id, "m.room.create").await
            {
                if let Some(create_event) = create_events.first() {
                    if !crate::federation::signing::check_event_federate(
                        create_event.get("content").unwrap_or(&serde_json::Value::Null),
                    ) {
                        super::increment_counter(&ctx, "federation_inbound_txn_pdu_error_total");
                        ::tracing::warn!(
                            target: "security_audit",
                            event = "federation_non_federated_room_rejected",
                            room_id = room_id,
                            origin = origin,
                            event_id = event_id,
                            "Rejected inbound PDU for non-federated room"
                        );
                        results.push(json!({
                            "event_id": event_id,
                            "error": "This room is not federated"
                        }));
                        continue;
                    }
                }
            }
        }

        if event_type != "m.room.create" {
            if let Err(e) = super::validate_federation_origin_in_room(&ctx, room_id, origin).await {
                super::increment_counter(&ctx, "federation_inbound_txn_pdu_error_total");
                ::tracing::warn!(
                    target: "security_audit",
                    event = "federation_origin_not_in_room",
                    room_id = room_id,
                    origin = origin,
                    event_id = event_id,
                    error = %e,
                    "Rejected inbound PDU from origin with no members in room"
                );
                results.push(json!({
                    "event_id": event_id,
                    "error": "Origin server has no joined members in this room"
                }));
                continue;
            }
        }

        if state_key.is_some() && event_type != "m.room.member" {
            if let Err(error) = ctx.room_auth.verify_state_event_write(room_id, user_id, event_type).await {
                super::increment_counter(&ctx, "federation_inbound_txn_pdu_error_total");
                results.push(json!({
                    "event_id": event_id,
                    "error": error.to_string()
                }));
                continue;
            }
        }

        // S5 gap 2: authorize inbound `m.room.member` transitions against the
        // membership state machine (banned re-join, invite-of-banned, self-ban,
        // knock into a non-knock room). Power is validated via the auth-event
        // chain; this fails closed only on illegal state transitions.
        if event_type == "m.room.member" {
            let to_membership = content
                .get("membership")
                .and_then(|v| v.as_str())
                .and_then(|s| {
                    match s.parse::<synapse_common::Membership>() {
                        Ok(m) => Some(m),
                        Err(_) => {
                            ::tracing::warn!(
                                event_id = event_id,
                                membership = %s,
                                "Inbound m.room.member event with unknown membership value; will rely on auth-event chain for validation"
                            );
                            None
                        }
                    }
                });
            if let Some(to) = to_membership {
                let target = state_key.unwrap_or(user_id);
                if let Err(error) = ctx
                    .room_service
                    .membership()
                    .authorize_inbound_member_transition(room_id, user_id, target, to)
                    .await
                {
                    super::increment_counter(&ctx, "federation_inbound_txn_pdu_error_total");
                    ::tracing::warn!(
                        target: "security_audit",
                        event = "federation_illegal_member_transition",
                        room_id = room_id,
                        sender = user_id,
                        target = target,
                        membership = ?to,
                        event_id = event_id,
                        error = %error,
                        "Rejected inbound m.room.member with illegal state transition"
                    );
                    results.push(json!({
                        "event_id": event_id,
                        "error": error.to_string()
                    }));
                    continue;
                }
            }
        }

        let content_for_as = content.clone();

        // P0-05/P0-08: extract `redacts` target from redaction PDUs.  For
        // v1-v10 this is a top-level field; for v11+ it lives in
        // `content.redacts` (MSC2174/MSC3820).  The shared helper checks both
        // locations.
        let redacts_target = if event_type == "m.room.redaction" {
            synapse_common::redaction::extract_redacts(pdu).map(|s| s.to_string())
        } else {
            None
        };

        // Extract DAG metadata from the PDU so we can persist it and detect
        // gaps in the event graph.  `prev_events` and `auth_events` are arrays
        // of event ID strings; `depth` is a monotonically increasing integer.
        let prev_events: Vec<String> = pdu
            .get("prev_events")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();
        let auth_events: Vec<String> = pdu
            .get("auth_events")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();
        let depth = pdu.get("depth").and_then(|v| v.as_i64()).unwrap_or(0);

        // fill_in_prev_events: if the PDU references prev_events that we don't
        // have locally, ask the origin server to fill the gap via
        // `/get_missing_events`.  The origin is guaranteed to have these events
        // because it just sent us their child.  This is the highest-ROI
        // backfill trigger and covers the common case of out-of-order delivery
        // or missed transactions.  Errors are logged but do not block PDU
        // persistence — the event graph will have a gap, but the PDU itself is
        // still stored.
        if !prev_events.is_empty() {
            if let Ok(missing) = ctx.room_service.messaging().find_missing_event_ids(&prev_events).await {
                if !missing.is_empty() {
                    ::tracing::debug!(
                        request_id = %request_id,
                        txn_id = %txn_id,
                        origin = origin,
                        event_id = %event_id,
                        room_id = room_id,
                        missing_count = missing.len(),
                        "PDU references prev_events not in local DB; requesting gap fill from origin"
                    );
                    match ctx
                        .federation_client
                        .get_missing_events(origin, room_id, &prev_events, std::slice::from_ref(&event_id), 20, None)
                        .await
                    {
                        Ok(response) => {
                            if let Some(events) = response.get("events").and_then(|v| v.as_array()) {
                                ::tracing::info!(
                                    request_id = %request_id,
                                    txn_id = %txn_id,
                                    origin = origin,
                                    room_id = room_id,
                                    fetched_count = events.len(),
                                    "Received missing events from origin"
                                );
                                for missing_pdu in events {
                                    // Best-effort persist: extract fields and
                                    // store via create_event_with_graph so the
                                    // fetched events also populate event_edges.
                                    //
                                    // U-13: gap-fill PDUs have the same identity
                                    // rule as transaction PDUs.  Previously this
                                    // required a top-level `event_id`, so every
                                    // v3+ gap-fill event was silently skipped.
                                    let missing_event_id = match synapse_common::event_id::resolve_received_event_id(
                                        &room_version,
                                        missing_pdu,
                                    ) {
                                        Ok(event_id) => event_id,
                                        Err(error) => {
                                            ::tracing::warn!(
                                                target: "security_audit",
                                                event = "federation_missing_event_id_unresolvable",
                                                request_id = %request_id,
                                                txn_id = %txn_id,
                                                origin = origin,
                                                room_version = %room_version,
                                                error = %error,
                                                "Gap-fill PDU has no derivable event ID — skipping"
                                            );
                                            continue;
                                        }
                                    };
                                    {
                                        // Skip if already exists (race or duplicate).
                                        if gap_fill_already_persisted(
                                            ctx.room_service.messaging().get_event_record(&missing_event_id).await,
                                        )? {
                                            continue;
                                        }

                                        // N4: Verify PDU integrity before
                                        // persisting.  Missing events come from
                                        // a remote server and must be validated
                                        // the same way as transaction PDUs — a
                                        // compromised peer could otherwise inject
                                        // forged events into the DAG.
                                        if let Err(e) = crate::federation::signing::check_pdu_size_limits(missing_pdu) {
                                            ::tracing::warn!(
                                                target: "security_audit",
                                                event = "federation_missing_event_size_exceeded",
                                                request_id = %request_id,
                                                txn_id = %txn_id,
                                                origin = origin,
                                                event_id = %missing_event_id,
                                                error = %e,
                                                "Missing event PDU exceeded size limits — skipping"
                                            );
                                            continue;
                                        }

                                        if let Err(e) =
                                            crate::federation::signing::verify_event_content_hash(missing_pdu)
                                        {
                                            ::tracing::warn!(
                                                target: "security_audit",
                                                event = "federation_missing_event_hash_mismatch",
                                                request_id = %request_id,
                                                txn_id = %txn_id,
                                                origin = origin,
                                                event_id = %missing_event_id,
                                                error = %e,
                                                "Missing event PDU content hash verification failed — skipping"
                                            );
                                            continue;
                                        }

                                        if let Err(e) =
                                            verify_pdu_sender_signature(&ctx, &room_version, missing_pdu).await
                                        {
                                            ::tracing::warn!(
                                                target: "security_audit",
                                                event = "federation_missing_event_signature_invalid",
                                                request_id = %request_id,
                                                txn_id = %txn_id,
                                                origin = origin,
                                                event_id = %missing_event_id,
                                                error = %e,
                                                "Missing event PDU sender signature verification failed — skipping"
                                            );
                                            continue;
                                        }

                                        let missing_room_id =
                                            missing_pdu.get("room_id").and_then(|v| v.as_str()).unwrap_or(room_id);
                                        let missing_user_id =
                                            missing_pdu.get("sender").and_then(|v| v.as_str()).unwrap_or("");
                                        let missing_event_type = missing_pdu
                                            .get("type")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("m.room.message");
                                        let missing_content = missing_pdu.get("content").cloned().unwrap_or(json!({}));
                                        let missing_state_key =
                                            missing_pdu.get("state_key").and_then(|v| v.as_str()).map(String::from);
                                        let missing_ost =
                                            missing_pdu.get("origin_server_ts").and_then(|v| v.as_i64()).unwrap_or(0);
                                        let missing_prev: Vec<String> = missing_pdu
                                            .get("prev_events")
                                            .and_then(|v| v.as_array())
                                            .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                                            .unwrap_or_default();
                                        let missing_auth: Vec<String> = missing_pdu
                                            .get("auth_events")
                                            .and_then(|v| v.as_array())
                                            .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                                            .unwrap_or_default();
                                        let missing_depth =
                                            missing_pdu.get("depth").and_then(|v| v.as_i64()).unwrap_or(0);

                                        let missing_params = synapse_services::event::CreateEventParams {
                                            event_id: missing_event_id.clone(),
                                            room_id: missing_room_id.to_string(),
                                            user_id: missing_user_id.to_string(),
                                            event_type: missing_event_type.to_string(),
                                            content: missing_content,
                                            state_key: missing_state_key,
                                            origin_server_ts: missing_ost,
                                            redacts: None,
                                        };
                                        if let Err(e) = ctx
                                            .room_service
                                            .messaging()
                                            .create_event_with_graph(
                                                missing_params,
                                                &missing_prev,
                                                &missing_auth,
                                                missing_depth,
                                                None,
                                            )
                                            .await
                                        {
                                            ::tracing::warn!(
                                                request_id = %request_id,
                                                txn_id = %txn_id,
                                                origin = origin,
                                                event_id = %missing_event_id,
                                                error = %e,
                                                "Failed to persist gap-filled event"
                                            );
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            ::tracing::warn!(
                                request_id = %request_id,
                                txn_id = %txn_id,
                                origin = origin,
                                room_id = room_id,
                                error = %e,
                                "Failed to fetch missing events from origin; PDU will be persisted with a graph gap"
                            );
                        }
                    }
                }
            }
        }

        let params = synapse_services::event::CreateEventParams {
            event_id: event_id.clone(),
            room_id: room_id.to_string(),
            user_id: user_id.to_string(),
            event_type: event_type.to_string(),
            content,
            state_key: state_key.map(|s| s.to_string()),
            origin_server_ts,
            redacts: redacts_target.clone(),
        };

        match ctx
            .room_service
            .messaging()
            .create_event_with_graph(params, &prev_events, &auth_events, depth, None)
            .await
        {
            Ok(_) => {
                ctx.room_service
                    .dispatch_appservice_event(&event_id, room_id, event_type, user_id, &content_for_as, state_key)
                    .await;

                // Persist the **origin server's** signature/hash pair. Without it
                // a re-emitted PDU would carry only our signature, and the peer
                // that requires the sender's signature would reject it. The same
                // post-insert mechanism the local signing path and the inbound
                // membership path use — one mechanism, one predicate
                // (`synapse_common::event_utils::signature_material`).
                if let Some((hashes, signatures)) =
                    synapse_common::event_utils::signature_material(pdu.get("hashes"), pdu.get("signatures"))
                {
                    if let Err(e) = ctx
                        .room_service
                        .messaging()
                        .update_event_signatures_and_hashes(&event_id, &signatures, &hashes)
                        .await
                    {
                        ::tracing::warn!(
                            request_id = %request_id,
                            txn_id = %txn_id,
                            origin = origin,
                            event_id = %event_id,
                            error = %e,
                            "failed to persist the origin server's signature material"
                        );
                    }
                }

                // P0-08: if this was a redaction PDU, apply the content
                // stripping to the target event.  This is what makes
                // redactions from remote servers actually take effect on
                // events stored locally.  We do this after the redaction
                // event itself is persisted so that the redaction is
                // recorded even if the target is missing.
                //
                // `events.redacted_by` is a self-referential FK to
                // `events.event_id` (`fk_events_redacted_by`), so it records the
                // redaction EVENT's id — the PDU we just persisted, whose id is
                // `event_id` (the value the log line below already calls
                // `redaction_event_id`) — not the sending user's id. Passing the
                // user id violated the constraint, so the target's content was
                // never stripped even though the sender got `success` back.
                if let Some(target_event_id) = &redacts_target {
                    if let Err(e) =
                        ctx.room_service.messaging().redact_event_content(target_event_id, Some(&event_id)).await
                    {
                        ::tracing::warn!(
                            target: "security_audit",
                            request_id = %request_id,
                            txn_id = %txn_id,
                            origin = %origin,
                            redaction_event_id = %event_id,
                            target_event_id = %target_event_id,
                            error = %e,
                            "Federation redaction PDU persisted but target content redaction failed"
                        );
                    }
                }

                super::increment_counter(&ctx, "federation_inbound_txn_pdu_success_total");
                results.push(json!({
                    "event_id": event_id,
                    "success": true
                }));
            }
            Err(e) => {
                super::increment_counter(&ctx, "federation_inbound_txn_pdu_error_total");
                ::tracing::error!(
                    request_id = %request_id,
                    txn_id = %txn_id,
                    origin = %origin,
                    event_id = %event_id,
                    error = %e,
                    "Failed to persist PDU"
                );
                results.push(json!({
                    "event_id": event_id,
                    "error": e.to_string()
                }));
            }
        }
    }

    ::tracing::info!(
        request_id = %request_id,
        txn_id = %txn_id,
        origin = %origin,
        pdu_count = pdus.len(),
        "Processed federation transaction"
    );

    {
        let dedup_key = format!("federation_txn:{origin}:{txn_id}");
        let dedup_ttl = ctx.config.federation.txn_dedup_ttl_secs;
        if dedup_ttl > 0 {
            if let Err(e) = ctx.cache.set(&dedup_key, true, dedup_ttl).await {
                ::tracing::warn!(
                    request_id = %request_id,
                    txn_id = %txn_id,
                    origin = %origin,
                    error = %e,
                    "Failed to set transaction dedup cache"
                );
            }
        }
    }

    Ok(Json(json!({
        "results": results
    })))
}

type PduValidationResult<'a> = Result<(&'a str, &'a str, &'a str, Option<&'a str>), ApiError>;

fn validate_inbound_transaction_pdu<'a>(authenticated_origin: &str, pdu: &'a Value) -> PduValidationResult<'a> {
    let room_id = pdu
        .get("room_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing room_id in inbound PDU".to_string()))?;
    let sender = pdu
        .get("sender")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing sender in inbound PDU".to_string()))?;
    let event_type = pdu
        .get("type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing type in inbound PDU".to_string()))?;
    let state_key = pdu.get("state_key").and_then(|v| v.as_str());

    if super::sender_server_name(sender) != Some(authenticated_origin) {
        return Err(ApiError::forbidden("Federation PDU sender does not match authenticated origin".to_string()));
    }

    if let Some(event_origin) = pdu.get("origin").and_then(|v| v.as_str()) {
        super::validate_federation_origin(authenticated_origin, Some(event_origin))?;
    }

    Ok((room_id, sender, event_type, state_key))
}

/// Best-effort label for an inbound PDU in a per-PDU error entry.
///
/// v3+ PDUs carry no `event_id`, so an entry that has to report an error before
/// the ID is derivable cannot name the event; the response shape keeps the key
/// present with an explicit marker rather than inventing an ID.
fn carried_event_id_label(pdu: &Value) -> String {
    pdu.get("event_id").and_then(Value::as_str).unwrap_or("<underivable>").to_string()
}

/// Resolve the room version an inbound PDU must be processed under.
///
/// There is deliberately **no default**: the version selects the redaction rules
/// used for the reference hash (v3+) and for the signature material, so guessing
/// one would either reject valid events or accept invalid ones.
///
/// Sources, in order:
/// 1. an `m.room.create` PDU states its own version in `content.room_version`
///    (current spec) or the legacy top-level `room_version` — needed because the
///    room row does not exist yet when the create event arrives;
/// 2. otherwise the locally recorded version for the PDU's `room_id`.
async fn inbound_pdu_room_version(ctx: &FederationContext, pdu: &Value) -> Result<String, String> {
    if pdu.get("type").and_then(Value::as_str) == Some("m.room.create") {
        if let Some(version) = pdu.get("content").and_then(|c| c.get("room_version")).and_then(Value::as_str) {
            return Ok(version.to_string());
        }
        if let Some(version) = pdu.get("room_version").and_then(Value::as_str) {
            return Ok(version.to_string());
        }
    }

    let room_id = pdu.get("room_id").and_then(Value::as_str).ok_or_else(|| "PDU has no room_id".to_string())?;
    match ctx.room_service.state().get_room_version(room_id).await {
        Ok(Some(version)) => Ok(version),
        Ok(None) => Err(format!("no room version recorded for {room_id}")),
        Err(error) => Err(format!("failed to read room version for {room_id}: {error}")),
    }
}

async fn verify_pdu_sender_signature(ctx: &FederationContext, room_version: &str, pdu: &Value) -> Result<(), String> {
    let sender = pdu.get("sender").and_then(|v| v.as_str()).ok_or_else(|| "Missing sender on PDU".to_string())?;
    let sender_server =
        super::sender_server_name(sender).ok_or_else(|| format!("Unparseable sender mxid: {sender}"))?;

    let signatures =
        pdu.get("signatures").and_then(|v| v.as_object()).ok_or_else(|| "PDU missing signatures field".to_string())?;
    let server_sigs = signatures
        .get(sender_server)
        .and_then(|v| v.as_object())
        .ok_or_else(|| format!("PDU has no signatures from sender server {sender_server}"))?;
    if server_sigs.is_empty() {
        return Err(format!("PDU signatures.{sender_server} is empty"));
    }

    // The signed bytes come from the same definition the signer uses
    // (`signature_material_bytes`): redaction per room version, minus
    // `age_ts`/`unsigned`, minus `event_id` for v3+.  Canonicalising the raw PDU
    // here (the previous behaviour) meant our own valid signatures failed our
    // own verification.
    let signed_bytes = synapse_federation::signing::signature_material_bytes(room_version, pdu)?;

    let mut last_error: Option<String> = None;
    for (key_id, sig_value) in server_sigs {
        let Some(signature) = sig_value.as_str() else {
            continue;
        };
        match crate::middleware::verify_federation_signature_with_cache(
            ctx,
            sender_server,
            key_id,
            signature,
            &signed_bytes,
            false,
        )
        .await
        {
            Ok(()) => return Ok(()),
            Err(e) => last_error = Some(e.message().to_string()),
        }
    }

    Err(last_error.unwrap_or_else(|| "No verifiable PDU signature".to_string()))
}

async fn acquire_origin_edu_permit(
    ctx: &FederationContext,
    origin: &str,
) -> Result<(OwnedSemaphorePermit, u64), ApiError> {
    let per_origin_limit = ctx.config.federation.inbound_edu_per_origin_max_concurrency.max(1);
    let semaphore = {
        let mut guard = ctx.federation_inbound_edu_origin_semaphores.lock().await;
        guard.entry(origin.to_string()).or_insert_with(|| Arc::new(Semaphore::new(per_origin_limit))).clone()
    };

    super::acquire_with_timeout(semaphore, ctx.config.federation.inbound_edu_acquire_timeout_ms).await
}

async fn get_presence_backoff_remaining_ms(ctx: &FederationContext, origin: &str) -> Option<u64> {
    let now = current_timestamp_millis();
    let guard = ctx.federation_presence_backoff_until.read().await;
    let until = guard.get(origin).copied()?;
    (until > now).then_some((until - now) as u64)
}

/// Returns whether a gap-fill PDU is already stored locally.
///
/// A database read failure must propagate instead of being treated as
/// "not present" (CLAUDE.md §踩过的坑: never swallow DB errors): the caller
/// would otherwise re-validate and re-insert an event that may already exist,
/// masking the real failure.
fn gap_fill_already_persisted<T>(lookup: Result<Option<T>, ApiError>) -> Result<bool, ApiError> {
    lookup.map(|existing| existing.is_some())
}

#[cfg(test)]
mod gap_fill_tests {
    use super::gap_fill_already_persisted;
    use synapse_common::ApiError;

    #[test]
    fn gap_fill_propagates_lookup_error() {
        let result = gap_fill_already_persisted::<()>(Err(ApiError::internal("db down".to_string())));
        assert!(result.is_err(), "DB 读取失败必须传播，不得被当作'事件不存在'");
    }

    #[test]
    fn gap_fill_detects_known_event() {
        assert!(gap_fill_already_persisted(Ok(Some(()))).expect("ok"));
    }

    #[test]
    fn gap_fill_detects_unknown_event() {
        assert!(!gap_fill_already_persisted::<()>(Ok(None)).expect("ok"));
    }
}
