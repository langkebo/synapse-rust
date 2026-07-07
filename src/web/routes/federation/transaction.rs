use crate::common::*;
use crate::federation::EduDispatcher;
use crate::web::middleware::FederationRequestAuth;
use crate::web::routes::context::FederationContext;
use crate::web::utils::auth::resolve_request_id;
use axum::{
    extract::{Extension, Json, Path, State},
    http::HeaderMap,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

const TXN_DEDUP_TTL_SECS: u64 = 86400;

pub(super) async fn send_transaction(
    State(ctx): State<FederationContext>,
    Extension(auth): Extension<FederationRequestAuth>,
    headers: HeaderMap,
    Path(txn_id): Path<String>,
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

    if process_inbound_edus {
        if let Some(edus) = edus {
            let mut processed_edus = 0usize;
            let mut total_processed = 0usize;
            let mut total_dropped = 0usize;
            let mut total_errored = 0usize;

            super::increment_gauge(&ctx, "federation_inbound_edu_in_flight");

            let edu_processing = async {
                let (_global_permit, wait_ms) = super::acquire_with_timeout(
                    ctx.federation_inbound_edu_semaphore.clone(),
                    ctx.config.federation.inbound_edu_acquire_timeout_ms,
                )
                .await?;
                super::observe_histogram(&ctx, "federation_inbound_edu_wait_ms", wait_ms as f64);

                let _origin_permit = acquire_origin_edu_permit(&ctx, origin).await?.0;

                if let Some(backoff_ms) = get_presence_backoff_remaining_ms(&ctx, origin).await {
                    super::increment_counter(&ctx, "federation_inbound_presence_backoff_total");
                    ::tracing::debug!(
                        "Skipping presence EDU processing for origin {} due to backoff {}ms",
                        origin,
                        backoff_ms
                    );
                    // Skip only presence EDUs; other types can still be processed.
                }

                for edu in edus.iter().take(inbound_edus_max_per_txn) {
                    processed_edus += 1;
                    let edu_type_str = edu.get("edu_type").and_then(|v| v.as_str()).unwrap_or("");

                    // Skip presence EDUs when disabled or backoff is active.
                    if edu_type_str == "m.presence" && !process_inbound_presence_edus {
                        continue;
                    }
                    if edu_type_str == "m.presence" && get_presence_backoff_remaining_ms(&ctx, origin).await.is_some() {
                        continue;
                    }

                    // Per-type rate limiting for presence.
                    let remaining = if edu_type_str == "m.presence" {
                        inbound_presence_updates_max_per_txn.saturating_sub(total_processed)
                    } else {
                        inbound_edus_max_per_txn
                    };

                    if remaining == 0 {
                        continue;
                    }

                    match EduDispatcher::dispatch(&ctx, origin, edu, remaining).await {
                        Some(result) => {
                            total_processed += result.processed;
                            total_dropped += result.dropped;
                            total_errored += result.errored;
                            if result.errored > 0 {
                                break;
                            }
                        }
                        None => {
                            // Unknown/unsupported EDU type — silently skip.
                            ::tracing::trace!(
                                request_id = %request_id,
                                txn_id = %txn_id,
                                origin = %origin,
                                edu_type = edu_type_str,
                                "Skipping unknown EDU type"
                            );
                        }
                    }
                }
                Ok::<(), ApiError>(())
            }
            .await;

            if let Err(error) = edu_processing {
                if error.is_rate_limited() {
                    super::increment_counter(&ctx, "federation_inbound_edu_limited_total");
                } else {
                    super::increment_counter(&ctx, "federation_inbound_edu_error_total");
                    ::tracing::warn!(
                        request_id = %request_id,
                        txn_id = %txn_id,
                        origin = %origin,
                        error = %error,
                        "Failed to process inbound EDUs"
                    );
                }
            }

            super::decrement_gauge(&ctx, "federation_inbound_edu_in_flight");

            ::tracing::debug!(
                request_id = %request_id,
                txn_id = %txn_id,
                origin = %origin,
                pdu_count = pdus.len(),
                edu_count = edus.len(),
                edus_processed = processed_edus,
                edu_updates_processed = total_processed,
                edu_updates_dropped = total_dropped,
                edu_updates_errored = total_errored,
                "Inbound federation EDU processing summary"
            );
        }
    }

    let mut results = Vec::new();

    const MAX_PDUS_PER_TRANSACTION: usize = 100;
    if pdus.len() > MAX_PDUS_PER_TRANSACTION {
        ::tracing::warn!(
            target: "security_audit",
            event = "federation_pdu_count_exceeded",
            origin = origin,
            pdu_count = pdus.len(),
            max = MAX_PDUS_PER_TRANSACTION,
            "Transaction contains too many PDUs - truncating"
        );
    }
    let pdus_to_process = &pdus[..pdus.len().min(MAX_PDUS_PER_TRANSACTION)];

    for pdu in pdus_to_process {
        let event_id = pdu
            .get("event_id")
            .and_then(|v| v.as_str())
            .map_or_else(|| format!("${}", crate::common::crypto::generate_event_id(origin)), |s| s.to_string());

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

        if let Err(e) = verify_pdu_sender_signature(&ctx, pdu).await {
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
                ctx.room_service.messaging.get_state_events_by_type(room_id, "m.room.create").await
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
            if let Err(error) = ctx.auth_service.verify_state_event_write(room_id, user_id, event_type).await {
                super::increment_counter(&ctx, "federation_inbound_txn_pdu_error_total");
                results.push(json!({
                    "event_id": event_id,
                    "error": error.to_string()
                }));
                continue;
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
            if let Ok(missing) = ctx.room_service.messaging.find_missing_event_ids(&prev_events).await {
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
                                    if let Some(missing_event_id) = missing_pdu.get("event_id").and_then(|v| v.as_str())
                                    {
                                        // Skip if already exists (race or duplicate).
                                        if ctx
                                            .room_service
                                            .messaging
                                            .get_event_record(missing_event_id)
                                            .await
                                            .ok()
                                            .flatten()
                                            .is_some()
                                        {
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

                                        let missing_params = synapse_storage::event::CreateEventParams {
                                            event_id: missing_event_id.to_string(),
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
                                            .messaging
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
                                                event_id = missing_event_id,
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

        let params = synapse_storage::event::CreateEventParams {
            event_id: event_id.clone(),
            room_id: room_id.to_string(),
            user_id: user_id.to_string(),
            event_type: event_type.to_string(),
            content,
            state_key: state_key.map(|s| s.to_string()),
            origin_server_ts,
            redacts: redacts_target.clone(),
        };

        match ctx.room_service.messaging.create_event_with_graph(params, &prev_events, &auth_events, depth, None).await
        {
            Ok(_) => {
                ctx.room_service
                    .dispatch_appservice_event(&event_id, room_id, event_type, user_id, &content_for_as, state_key)
                    .await;

                // P0-08: if this was a redaction PDU, apply the content
                // stripping to the target event.  This is what makes
                // redactions from remote servers actually take effect on
                // events stored locally.  We do this after the redaction
                // event itself is persisted so that the redaction is
                // recorded even if the target is missing.
                if let Some(target_event_id) = &redacts_target {
                    if let Err(e) =
                        ctx.room_service.messaging.redact_event_content(target_event_id, Some(user_id)).await
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
        if let Err(e) = ctx.cache.set(&dedup_key, true, TXN_DEDUP_TTL_SECS).await {
            ::tracing::warn!(
                request_id = %request_id,
                txn_id = %txn_id,
                origin = %origin,
                error = %e,
                "Failed to set transaction dedup cache"
            );
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

async fn verify_pdu_sender_signature(ctx: &FederationContext, pdu: &Value) -> Result<(), String> {
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

    let mut signing_payload = pdu.clone();
    if let Some(obj) = signing_payload.as_object_mut() {
        obj.remove("signatures");
        obj.remove("unsigned");
    }
    let signed_bytes = match synapse_common::canonical_json_bytes(&signing_payload) {
        Ok(b) => b,
        Err(e) => {
            return Err(format!("Canonical JSON error for PDU signature verification: {e}"));
        }
    };

    let mut last_error: Option<String> = None;
    for (key_id, sig_value) in server_sigs {
        let Some(signature) = sig_value.as_str() else {
            continue;
        };
        match crate::web::middleware::verify_federation_signature_with_cache(
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
    let now = chrono::Utc::now().timestamp_millis();
    let guard = ctx.federation_presence_backoff_until.read().await;
    let until = guard.get(origin).copied()?;
    (until > now).then_some((until - now) as u64)
}
