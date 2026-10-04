//! Outbound federation membership flows: make_join/send_join, make_leave/
//! send_leave, and invite.
//!
//! These methods handle the case where a local user needs to interact with a
//! room or user on a remote homeserver.  The flows follow the Matrix
//! federation specification:
//!
//! - **Join**: `GET /_matrix/federation/v1/make_join` → sign locally →
//!   `PUT /_matrix/federation/v2/send_join` → persist returned state.
//! - **Leave**: `GET /_matrix/federation/v1/make_leave` → sign locally →
//!   `PUT /_matrix/federation/v2/send_leave`.
//! - **Invite**: build invite event → sign locally →
//!   `PUT /_matrix/federation/v2/invite` → persist returned event.
//!
//! Reference: element-hq/synapse
//! `synapse/handlers/federation.py::FederationHandler.do_invite_join` and
//! `synapse/handlers/federation.py::FederationHandler.do_remotely_reject_invite`

use crate::common::error::{ApiError, ApiResult};
use crate::room::state::auth_events::{select_auth_events, AuthStateSnapshot};
use serde_json::{json, Value};
use synapse_common::current_timestamp_millis;
use synapse_common::generate_event_id;
use synapse_federation::signing::sign_and_hash_event;
use synapse_storage::CreateEventParams;

use super::service::MembershipService;

/// Set the PDU `origin` field to the signing (local) server name.
///
/// The `origin` of a PDU is the server that created and signed it — for the
/// outbound `make_join`/`make_leave` flow that is **this** server, not the
/// remote resident homeserver. [`sign_and_hash_event`] signs with
/// `self.server_name` and does not inject `origin`, so the two must be kept
/// consistent here. Filling `origin` with the destination (resident) server
/// would make the remote verifier reject the event: it checks
/// `origin == authenticated_origin` (the X-Matrix `origin` header, i.e. us).
fn ensure_template_origin(template: &mut Value, origin_server: &str, flow: &str) {
    let Some(obj) = template.as_object_mut() else {
        ::tracing::warn!(flow = %flow, "make_* template is not a JSON object; cannot set origin");
        return;
    };
    if obj.get("origin").and_then(|v| v.as_str()) == Some(origin_server) {
        return;
    }
    ::tracing::warn!(
        origin_server = %origin_server,
        flow = %flow,
        "make_* template had a missing/mismatched `origin`; setting it to the signing server"
    );
    obj.insert("origin".to_string(), Value::String(origin_server.to_string()));
}

/// The stripped state an outbound invite must carry (MSC4311).
///
/// The invitee renders the invite from this list, and MSC4311 makes
/// `m.room.create` mandatory: without it every conforming receiver logs
/// `Stripped state must include m.room.create event` and the invitee cannot
/// determine the room version. Entries are projected to the stripped shape
/// (`type` / `state_key` / `content` / `sender`) — the invitee's server stores
/// exactly this on the membership event's `unsigned` and serves it as
/// `rooms.invite[*].invite_state`.
///
/// Only pre-join state is included, plus the inviter's own membership event so
/// the invitee can show who invited them (MSC4319). `m.room.create` is ordered
/// first, matching upstream's `_room_prejoin_state_types`.
pub(crate) fn invite_room_state_for(state_events: &[synapse_storage::event::StateEvent]) -> Vec<Value> {
    const PREJOIN_TYPES: [&str; 7] = [
        "m.room.create",
        "m.room.join_rules",
        "m.room.name",
        "m.room.avatar",
        "m.room.topic",
        "m.room.encryption",
        "m.room.canonical_alias",
    ];

    let mut ordered: Vec<&synapse_storage::event::StateEvent> = Vec::new();
    for event_type in PREJOIN_TYPES {
        ordered.extend(
            state_events.iter().filter(|event| {
                event.event_type.as_deref() == Some(event_type) && event.state_key.as_deref() == Some("")
            }),
        );
    }
    // MSC4319: the inviter's membership, so the invitee's client can render the
    // inviter's profile even when the room has no name/avatar (it must be the
    // *inviter*'s entry, which the caller cannot know here — every member entry
    // whose state_key is not the invitee is therefore included; the invitee's own
    // membership does not exist yet).
    ordered.extend(state_events.iter().filter(|event| event.event_type.as_deref() == Some("m.room.member")).filter(
        |event| {
            event
                .content
                .get("membership")
                .and_then(Value::as_str)
                .is_some_and(|membership| matches!(membership, "join" | "invite"))
        },
    ));

    ordered
        .into_iter()
        .map(|event| {
            let mut stripped = serde_json::Map::new();
            stripped.insert("type".to_string(), json!(event.event_type));
            stripped.insert("state_key".to_string(), json!(event.state_key));
            stripped.insert("content".to_string(), event.content.clone());
            stripped.insert("sender".to_string(), json!(event.sender));
            Value::Object(stripped)
        })
        .collect()
}

impl MembershipService {
    // =========================================================================
    // Outbound federation join
    // =========================================================================

    /// Join a room on a remote homeserver via the make_join / send_join flow.
    ///
    /// 1. Call `make_join` on `destination` to get a template PDU.
    /// 2. Sign the template PDU locally.
    /// 3. Call `send_join` on `destination` with the signed PDU.
    /// 4. Create the room locally if it doesn't exist.
    /// 5. Persist the returned state events and auth chain.
    /// 6. Add the user as a joined member.
    pub async fn join_room_via_federation(&self, destination: &str, room_id: &str, user_id: &str) -> ApiResult<()> {
        ::tracing::info!(
            destination = %destination,
            room_id = %room_id,
            user_id = %user_id,
            "Joining room via federation"
        );

        // Check room ACL before contacting the remote server
        self.check_outbound_server_acl(room_id, destination).await?;

        let federation_client = self.require_federation_client().await?;

        // 1. make_join: get the template event from the remote server.
        let make_join_response = federation_client.make_join(destination, room_id, user_id).await.map_err(|e| {
            ::tracing::warn!(error = %e, destination = %destination, "make_join failed");
            ApiError::bad_request(format!("Remote server rejected make_join: {e}"))
        })?;

        // The signature material is room-version dependent (redaction differs
        // per version), so a response that does not state the room version
        // cannot be signed. Never guess one — the former hard-coded "10"
        // fallback silently mis-signed every non-v10 room.
        let Some(room_version) = make_join_response.room_version else {
            ::tracing::warn!(
                room_id = %room_id,
                destination = %destination,
                "make_join response did not state a room version; refusing to sign the template"
            );
            return Err(ApiError::bad_request("Remote make_join response did not state a room version".to_string()));
        };
        let mut event_template = make_join_response.event;

        // Spec PR #2284 / Synapse #20189: never sign an unvalidated template.
        // A malicious resident server could otherwise have us sign a
        // `membership: ban` (or a different sender/state_key).
        synapse_federation::make_response_validation::validate_make_membership_template(
            &event_template,
            room_id,
            user_id,
            "join",
        )
        .map_err(|e| {
            ::tracing::warn!(error = %e, room_id = %room_id, destination = %destination, "make_join template rejected");
            ApiError::bad_request(format!("Remote make_join response is malformed: {e}"))
        })?;

        // `origin` is part of the signed bytes and must be this (signing)
        // server, not the remote resident server — see `ensure_template_origin`.
        ensure_template_origin(&mut event_template, &self.server_name, "make_join");

        // The template must carry `room_id` (part of the signed bytes) before
        // signing. Some resident servers omit it and expect the joining server
        // to add it (see `validate_make_membership_template`); fill `room_id`
        // and `origin_server_ts` defensively here so the signed PDU is complete.
        if let Some(obj) = event_template.as_object_mut() {
            obj.entry("room_id").or_insert_with(|| Value::String(room_id.to_string()));
            obj.entry("origin_server_ts").or_insert_with(|| json!(current_timestamp_millis()));
        }

        // 2. Sign the template event locally.
        let signing_key = self.require_signing_key().await?;
        sign_and_hash_event(
            &room_version,
            &self.server_name,
            &signing_key.key_id,
            &signing_key.secret_key,
            &mut event_template,
        )
        .map_err(|e| ApiError::internal(format!("Failed to sign join event: {e}")))?;

        // Derive the event identity **after** signing. For v3+ rooms the ID is
        // the reference hash (which must not include `event_id`, so it is
        // computed before the field is inserted); v1/v2 keep a server-assigned
        // or template-provided ID. The send_join body must carry `event_id`
        // matching the request path, so it is written back into the event here.
        let event_id = if synapse_common::event_id::uses_reference_hash_event_id(&room_version) {
            synapse_common::event_id::compute_event_id(&room_version, &event_template)
                .map_err(|e| ApiError::internal(format!("Failed to compute join event id: {e}")))?
        } else {
            event_template
                .get("event_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| generate_event_id(&self.server_name))
        };
        if let Some(obj) = event_template.as_object_mut() {
            obj.insert("event_id".to_string(), Value::String(event_id.clone()));
        }

        // 3. send_join: send the signed event to the remote server.
        let mut send_join_response =
            federation_client.send_join(destination, room_id, &event_id, &event_template).await.map_err(|e| {
                ::tracing::warn!(error = %e, destination = %destination, "send_join failed");
                ApiError::bad_request(format!("Remote server rejected send_join: {e}"))
            })?;

        // 4. Create the room locally if it doesn't exist.
        let room_exists = self
            .room_storage
            .room_exists(room_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to check room existence", e))?;

        if !room_exists {
            // Derive join_rule and visibility from the returned state events.
            let join_rule = send_join_response
                .state
                .iter()
                .find(|e| {
                    e.get("type").and_then(|v| v.as_str()) == Some("m.room.join_rules")
                        && e.get("state_key").and_then(|v| v.as_str()) == Some("")
                })
                .and_then(|e| e.get("content"))
                .and_then(|c| c.get("join_rule"))
                .and_then(|v| v.as_str())
                .unwrap_or("invite")
                .to_string();

            let is_public = join_rule == "public";

            self.room_storage
                .create_room(room_id, user_id, &join_rule, &room_version, is_public)
                .await
                .map_err(|e| ApiError::internal_with_cause("Failed to create federated room", e))?;

            ::tracing::info!(
                room_id = %room_id,
                room_version = %room_version,
                join_rule = %join_rule,
                "Created local record for federated room"
            );
        }

        // 5. Persist the returned state events and auth chain.
        //    We use create_event_with_graph so that event_edges is populated.
        //    P1b: Wrap all state-event persistence in a single transaction
        //    to avoid N+1 round-trips and ensure atomicity.
        //
        //    Persist in topological order (parents before children): each
        //    event's `prev_events` must already exist in `events` for the
        //    `event_edges.prev_event_id` FK, and `depth` is strictly greater
        //    than every parent's, so an ascending `depth` sort is a valid
        //    topological order.
        send_join_response.state.sort_by_key(|event| event.get("depth").and_then(Value::as_i64).unwrap_or(0));

        let mut persisted_event_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
        // The state events this batch commits, in write order: their resolved-state
        // record is maintained after the transaction commits, because the DAG walk
        // must see committed rows.
        let mut committed_state_events: Vec<(String, String, String)> = Vec::new();

        let mut _tx = if let Some(ref pool) = self.db_pool {
            Some(
                pool.begin()
                    .await
                    .map_err(|e| ApiError::internal_with_cause("Failed to begin transaction for federation join", e))?,
            )
        } else {
            None
        };

        for state_event in &send_join_response.state {
            // v3+ PDUs do not carry `event_id` (spec room v3 "Event format");
            // derive it from the reference hash. v1/v2 (and peers that still
            // emit the legacy field) carry it directly.
            let event_id = match state_event.get("event_id").and_then(Value::as_str) {
                Some(id) => id.to_string(),
                None => match synapse_common::event_id::compute_event_id(&room_version, state_event) {
                    Ok(id) => id,
                    Err(error) => {
                        ::tracing::warn!(error = %error, "Failed to derive federated state event id during join");
                        return Err(ApiError::internal_with_cause(
                            "Failed to derive federated state event id during join",
                            error,
                        ));
                    }
                },
            };
            if persisted_event_ids.contains(&event_id) {
                continue;
            }
            persisted_event_ids.insert(event_id.clone());

            let prev_events: Vec<String> = state_event
                .get("prev_events")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|e| {
                            e.as_array()
                                .and_then(|inner| inner.first())
                                .and_then(|id| id.as_str())
                                .map(|s| s.to_string())
                                .or_else(|| e.as_str().map(|s| s.to_string()))
                        })
                        .collect()
                })
                .unwrap_or_default();

            let auth_events: Vec<String> = state_event
                .get("auth_events")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|e| {
                            e.as_array()
                                .and_then(|inner| inner.first())
                                .and_then(|id| id.as_str())
                                .map(|s| s.to_string())
                                .or_else(|| e.as_str().map(|s| s.to_string()))
                        })
                        .collect()
                })
                .unwrap_or_default();

            let depth = state_event.get("depth").and_then(|v| v.as_i64()).unwrap_or(0);

            let event_type = state_event.get("type").and_then(|v| v.as_str()).unwrap_or("m.unknown").to_string();

            let sender = state_event.get("sender").and_then(|v| v.as_str()).unwrap_or("").to_string();

            let content = state_event.get("content").cloned().unwrap_or(Value::Object(serde_json::Map::new()));

            let state_key = state_event.get("state_key").and_then(|v| v.as_str()).map(|s| s.to_string());

            let origin_server_ts =
                state_event.get("origin_server_ts").and_then(|v| v.as_i64()).unwrap_or_else(current_timestamp_millis);

            let redacts = state_event.get("redacts").and_then(|v| v.as_str()).map(|s| s.to_string());

            // Fail closed: dropping `_tx` without committing rolls back the
            // state events written so far, and membership is never claimed,
            // so a persistence failure cannot leave the local event graph
            // out of sync with the membership tables (B10c).
            if let Some(state_key) = state_key.as_deref() {
                committed_state_events.push((event_id.clone(), event_type.clone(), state_key.to_string()));
            }

            let mut state_params = CreateEventParams {
                event_id: event_id.clone(),
                room_id: room_id.to_string(),
                user_id: sender,
                event_type,
                content,
                state_key,
                origin_server_ts,
                redacts,
            };

            // Third-party event admission (Synapse `check_event_allowed`) for the
            // inbound federated state. The PDU is origin-signed, so a rule may
            // refuse it but must not rewrite it. A refusal returns before the
            // commit below, rolling the shared transaction back (fail closed).
            crate::module_service::consult_event_admission(
                self.event_admission_gate.as_ref(),
                self.event_reader.as_ref(),
                &mut state_params,
                false,
            )
            .await?;

            if let Err(e) = self
                .event_writer
                .create_event_with_graph(
                    state_params,
                    &prev_events,
                    &auth_events,
                    depth,
                    _tx.as_mut(), // P1b: share the transaction across all state events
                )
                .await
            {
                ::tracing::warn!(
                    event_id = %event_id,
                    error = %e,
                    "Failed to persist federated state event during join"
                );
                return Err(ApiError::internal_with_cause("Failed to persist federated state event during join", e));
            }
        }

        // P1b: Commit the single transaction after all state events are persisted.
        if let Some(tx) = _tx {
            tx.commit()
                .await
                .map_err(|e| ApiError::internal_with_cause("Failed to commit federation join transaction", e))?;
        }

        // Maintain the room's resolved-state record for the state events this batch
        // just committed (MSC4297 v2.1, F-1). This is the second state-write seam:
        // unlike `MessagingService::create_event_with_graph` these events share one
        // transaction, so the record is maintained here, after the commit, rather
        // than per event. Best-effort — the events are already durable, so a failure
        // degrades to the event-log derivation.
        if !committed_state_events.is_empty() {
            let state_groups = synapse_storage::state_groups::StateGroupStorage::new(self.event_writer.pool());
            let record = crate::room::state_record::StateRecord {
                event_reader: self.event_reader.as_ref(),
                room_storage: self.room_storage.as_ref(),
                state_groups: &state_groups,
                resolution_cache: &self.resolution_cache,
            };
            for (committed_event_id, event_type, state_key) in &committed_state_events {
                if let Err(error) = record.after_state_event(room_id, committed_event_id, event_type, state_key).await {
                    ::tracing::warn!(
                        error = %error,
                        room_id = %room_id,
                        event_id = %committed_event_id,
                        "Failed to maintain the resolved-state record for a federated-join state event"
                    );
                }
            }
        }

        // Invalidate room-state cache after persisting federated state events.
        let _ = self.cache.delete(&format!("room_state:{room_id}")).await;

        // Persist the join event itself **before** claiming membership, so that a
        // persistence failure cannot leave membership tables (or the member
        // count) claiming a join the event graph does not contain (B10c).
        // The join event is normally already part of the resident server's
        // returned `state` (persisted above), so only persist it here when that
        // state did not include it — persisting it twice would violate the
        // `events` primary key.
        let join_event_id = event_template.get("event_id").and_then(|v| v.as_str()).unwrap_or(&event_id).to_string();

        if !persisted_event_ids.contains(&join_event_id) {
            let join_sender = event_template.get("sender").and_then(|v| v.as_str()).unwrap_or(user_id).to_string();

            let join_content = event_template.get("content").cloned().unwrap_or(json!({ "membership": "join" }));

            let join_ts = event_template
                .get("origin_server_ts")
                .and_then(|v| v.as_i64())
                .unwrap_or_else(current_timestamp_millis);

            let mut join_params = CreateEventParams {
                event_id: join_event_id.clone(),
                room_id: room_id.to_string(),
                user_id: join_sender,
                event_type: "m.room.member".to_string(),
                content: join_content,
                state_key: Some(user_id.to_string()),
                origin_server_ts: join_ts,
                redacts: None,
            };

            // Third-party event admission for the join event persisted from the
            // resident server's template. Remote-sourced, so a rule may refuse
            // but must not rewrite it. A refusal returns before `add_member`
            // below, so membership is never claimed for a refused join.
            crate::module_service::consult_event_admission(
                self.event_admission_gate.as_ref(),
                self.event_reader.as_ref(),
                &mut join_params,
                false,
            )
            .await?;

            if let Err(e) = self.event_writer.create_event(join_params, None).await {
                ::tracing::warn!(error = %e, "Failed to persist join event after federation join");
                return Err(ApiError::internal_with_cause("Failed to persist join event after federation join", e));
            }
        }

        // Invalidate room-state cache after membership state change.
        let _ = self.cache.delete(&format!("room_state:{room_id}")).await;

        // 6. Claim membership only once the event graph is durable.
        self.member_storage
            .add_member(room_id, user_id, "join", None, None, None, None)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to add member after federation join", e))?;

        // 6b. Backfill the resident members from the returned state so the
        // inbound-transaction origin check (`validate_federation_origin_in_room`)
        // accepts events from their servers. Only the joining user's membership
        // is claimed above; without these rows the resident server(s) appear to
        // have no joined members locally and every later transaction is dropped.
        for state_event in &send_join_response.state {
            let is_join_member = state_event.get("type").and_then(Value::as_str) == Some("m.room.member")
                && state_event.get("content").and_then(|c| c.get("membership")).and_then(Value::as_str) == Some("join");
            if !is_join_member {
                continue;
            }
            let Some(member_id) = state_event.get("state_key").and_then(Value::as_str) else {
                continue;
            };
            if member_id == user_id {
                continue;
            }
            if let Err(e) = self.user_storage.ensure_remote_user(member_id).await {
                ::tracing::warn!(error = %e, user_id = member_id, "Failed to ensure remote member user during join");
                continue;
            }
            // Best-effort: a duplicate (already a member) is not fatal.
            if let Err(e) = self.member_storage.add_member(room_id, member_id, "join", None, None, None, None).await {
                ::tracing::warn!(error = %e, user_id = member_id, "Failed to backfill remote member during join");
            }
        }

        self.room_storage
            .increment_member_count(room_id, None)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to update member count after federation join", e))?;

        // Materialise the room summary from the room we just joined. The state events
        // above were persisted with `should_update_summary = false` (they share one
        // transaction), so nothing has projected name/topic/heroes into `room_summaries`
        // yet. Without this the room becomes visible in the user's room list only once
        // the queued join event is processed, and even then without its name.
        // Best-effort: the membership itself is already durable.
        if let Err(error) = self.room_summary_service.sync_from_room(room_id).await {
            ::tracing::warn!(
                error = %error,
                room_id = %room_id,
                "Failed to materialize room summary after federation join"
            );
        }

        Ok(())
    }

    // =========================================================================
    // Outbound federation leave
    // =========================================================================

    /// Leave a federated room via the make_leave / send_leave flow.
    ///
    /// 1. Call `make_leave` on `destination` to get a template PDU.
    /// 2. Sign the template PDU locally.
    /// 3. Call `send_leave` on `destination` with the signed PDU.
    /// 4. Update local membership to "leave".
    pub async fn leave_room_via_federation(&self, destination: &str, room_id: &str, user_id: &str) -> ApiResult<()> {
        ::tracing::info!(
            destination = %destination,
            room_id = %room_id,
            user_id = %user_id,
            "Leaving room via federation"
        );

        // Check room ACL before contacting the remote server
        self.check_outbound_server_acl(room_id, destination).await?;

        let federation_client = self.require_federation_client().await?;

        // 1. make_leave: get the template event from the remote server.
        let make_leave_response = federation_client.make_leave(destination, room_id, user_id).await.map_err(|e| {
            ::tracing::warn!(error = %e, destination = %destination, "make_leave failed");
            ApiError::bad_request(format!("Remote server rejected make_leave: {e}"))
        })?;

        let mut event_template = make_leave_response.event;

        // Spec PR #2284 / Synapse #20189: never sign an unvalidated template.
        synapse_federation::make_response_validation::validate_make_membership_template(
            &event_template,
            room_id,
            user_id,
            "leave",
        )
        .map_err(|e| {
            ::tracing::warn!(error = %e, room_id = %room_id, destination = %destination, "make_leave template rejected");
            ApiError::bad_request(format!("Remote make_leave response is malformed: {e}"))
        })?;

        // 2. Resolve the room version that drives the signature material. Prefer
        //    the version the resident server stated (MSC1813); if it stated none,
        //    fall back to the version recorded for this room locally — this
        //    server is a member of the room it is leaving, so the stored version
        //    is authoritative. Never guess.
        let room_version = match make_leave_response.room_version {
            Some(version) => version,
            None => match self.room_storage.get_room_version_only(room_id).await {
                Ok(Some(version)) => version,
                Ok(None) => {
                    ::tracing::warn!(
                        room_id = %room_id,
                        destination = %destination,
                        "make_leave response stated no room version and the room is unknown locally; refusing to sign the template"
                    );
                    return Err(ApiError::bad_request(
                        "make_leave response did not state a room version and the room is unknown locally".to_string(),
                    ));
                }
                Err(e) => {
                    ::tracing::warn!(
                        room_id = %room_id,
                        destination = %destination,
                        error = %e,
                        "failed to read the local room version; refusing to sign the make_leave template"
                    );
                    return Err(ApiError::internal_with_cause("Failed to read room version", e));
                }
            },
        };

        // `origin` is part of the signed bytes and must be this (signing)
        // server, not the remote resident server — see `ensure_template_origin`.
        ensure_template_origin(&mut event_template, &self.server_name, "make_leave");

        // 3. Sign the template event locally.
        let signing_key = self.require_signing_key().await?;
        sign_and_hash_event(
            &room_version,
            &self.server_name,
            &signing_key.key_id,
            &signing_key.secret_key,
            &mut event_template,
        )
        .map_err(|e| ApiError::internal(format!("Failed to sign leave event: {e}")))?;

        let event_id = event_template
            .get("event_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| generate_event_id(&self.server_name));

        // Third-party event admission (Synapse `check_event_allowed`) for the
        // outbound federated leave. The template is the resident server's PDU
        // signed locally, so a rule may refuse but must not rewrite it. A
        // refusal returns before `send_leave` and before any local membership
        // mutation, so neither the remote server nor our own state learns of a
        // refused leave.
        let mut leave_params = CreateEventParams {
            event_id: event_id.clone(),
            room_id: room_id.to_string(),
            user_id: event_template.get("sender").and_then(|v| v.as_str()).unwrap_or(user_id).to_string(),
            event_type: "m.room.member".to_string(),
            content: event_template.get("content").cloned().unwrap_or(json!({ "membership": "leave" })),
            state_key: Some(user_id.to_string()),
            origin_server_ts: event_template
                .get("origin_server_ts")
                .and_then(|v| v.as_i64())
                .unwrap_or_else(current_timestamp_millis),
            redacts: None,
        };
        crate::module_service::consult_event_admission(
            self.event_admission_gate.as_ref(),
            self.event_reader.as_ref(),
            &mut leave_params,
            false,
        )
        .await?;

        // 3. send_leave: send the signed event to the remote server.
        federation_client.send_leave(destination, room_id, &event_id, &event_template).await.map_err(|e| {
            ::tracing::warn!(error = %e, destination = %destination, "send_leave failed");
            ApiError::bad_request(format!("Remote server rejected send_leave: {e}"))
        })?;

        // Persist the leave event locally **before** mutating membership, so a
        // persistence failure cannot leave membership tables claiming a leave
        // the event graph does not contain (B10c).
        let leave_sender = event_template.get("sender").and_then(|v| v.as_str()).unwrap_or(user_id).to_string();

        let leave_content = event_template.get("content").cloned().unwrap_or(json!({ "membership": "leave" }));

        let leave_ts =
            event_template.get("origin_server_ts").and_then(|v| v.as_i64()).unwrap_or_else(current_timestamp_millis);

        if let Err(e) = self
            .event_writer
            .create_event(
                CreateEventParams {
                    event_id,
                    room_id: room_id.to_string(),
                    user_id: leave_sender,
                    event_type: "m.room.member".to_string(),
                    content: leave_content,
                    state_key: Some(user_id.to_string()),
                    origin_server_ts: leave_ts,
                    redacts: None,
                },
                None,
            )
            .await
        {
            ::tracing::warn!(error = %e, "Failed to persist leave event after federation leave");
            return Err(ApiError::internal_with_cause("Failed to persist leave event after federation leave", e));
        }

        // Invalidate room-state cache after membership state change.
        let _ = self.cache.delete(&format!("room_state:{room_id}")).await;

        // 4. Update local membership.
        let existing_member = self
            .member_storage
            .get_room_member(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to check membership before federation leave", e))?;

        self.member_storage
            .remove_member(room_id, user_id, None)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to leave federated room", e))?;

        if existing_member.as_ref().is_some_and(|member| member.membership == "join") {
            self.room_storage.decrement_member_count(room_id, None).await.map_err(|e| {
                ApiError::internal_with_cause("Failed to update member count after federation leave", e)
            })?;
        }

        Ok(())
    }

    // =========================================================================
    // Outbound federation invite
    // =========================================================================

    /// Invite a remote user to a room via the federation invite flow.
    ///
    /// 1. Build an `m.room.member` invite event.
    /// 2. Sign the event locally.
    /// 3. Call `invite` on the invitee's home server.
    /// 4. The remote server signs the event and returns it.
    /// 5. Persist the signed event locally.
    pub async fn invite_user_via_federation(&self, room_id: &str, inviter_id: &str, invitee_id: &str) -> ApiResult<()> {
        let destination = Self::user_server_name(invitee_id)
            .ok_or_else(|| ApiError::bad_request("Invalid invitee ID: missing server name".to_string()))?
            .to_string();

        ::tracing::info!(
            destination = %destination,
            room_id = %room_id,
            inviter_id = %inviter_id,
            invitee_id = %invitee_id,
            "Inviting remote user via federation"
        );

        // Check room ACL before contacting the remote server
        self.check_outbound_server_acl(room_id, &destination).await?;

        let federation_client = self.require_federation_client().await?;

        // Build the invite event with complete PDU graph fields (MSC4311).
        //
        // U-13: the PDU is assembled by the single assembler and its identity is
        // derived from it — a hand-written `event_id` is wrong for v3+ (the
        // reference hash is the identity) and, because the signer no longer
        // covers `event_id` for v3+, the field would make the receiver recompute
        // different bytes than we signed.
        let placeholder_event_id = generate_event_id(&self.server_name);
        let now = current_timestamp_millis();

        // Resolve the room version first: it decides whether the PDU carries an
        // `event_id` at all, and it drives the signature material. Never guess.
        let room_version = match self.room_storage.get_room_version_only(room_id).await {
            Ok(Some(version)) => version,
            Ok(None) => {
                ::tracing::warn!(
                    room_id = %room_id,
                    destination = %destination,
                    "room version unknown; refusing to sign the federation invite"
                );
                return Err(ApiError::bad_request("Room version unknown for federated invite".to_string()));
            }
            Err(e) => {
                ::tracing::warn!(
                    room_id = %room_id,
                    destination = %destination,
                    error = %e,
                    "failed to read room version; refusing to sign the federation invite"
                );
                return Err(ApiError::internal_with_cause("Failed to read room version", e));
            }
        };

        // Fail-closed: if the room's graph cannot be read, refuse to send the
        // invite — fabricating `prev_events: []` / `depth: 1` would make the
        // event look like a DAG root to a spec-compliant peer (the same rule
        // the inbound projector applies: `RefuseIncomplete`).
        let extremities = self.event_reader.get_forward_extremities_in_room(room_id, 10).await.map_err(|e| {
            ::tracing::warn!(room_id = %room_id, error = %e, "failed to read room graph for invite");
            ApiError::internal_with_cause("Failed to read room graph for federated invite", e)
        })?;
        let depth =
            self.event_reader.calculate_event_depth(room_id, &extremities).await.map_err(|e| {
                ApiError::internal_with_cause("Failed to calculate event depth for federated invite", e)
            })?;
        let invite_content = json!({
            "membership": "invite",
            "displayname": invitee_id
                .trim_start_matches('@')
                .split(':')
                .next()
                .unwrap_or(invitee_id),
        });

        // The auth chain carried by the outbound PDU must be the *canonical*
        // selection for this event — the same one the local write path derives
        // (`select_auth_events`) — because the reference hash that becomes the
        // event ID is computed over it. Signing one chain and letting the writer
        // resolve another makes this server store an invite under an ID no other
        // participant in the room has ever seen, and the two rows disagree about
        // `auth_events` for the same event.
        //
        // Referencing `m.room.create` alone was the former shape: it is not a
        // complete chain for v11+ (power levels, the sender's own membership and
        // the join rules are the required entries), and from v12 the create event
        // is implicit (MSC4291) so it must not be listed at all.
        let state_events = self
            .event_reader
            .get_state_events(room_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to fetch room state for invite auth chain", e))?;
        let auth_state = AuthStateSnapshot::from_state_events(&state_events);
        let auth_events = select_auth_events(
            &room_version,
            &auth_state,
            "m.room.member",
            Some(invitee_id),
            inviter_id,
            &invite_content,
        );

        let prev_events = extremities;

        let invite_parts = synapse_common::pdu::PduParts {
            room_version: &room_version,
            // v1/v2 only: `build_pdu` drops it for v3+, where the reference
            // hash is the identity.
            event_id: Some(placeholder_event_id.as_str()),
            room_id,
            sender: inviter_id,
            event_type: "m.room.member",
            content: &invite_content,
            state_key: Some(invitee_id),
            origin_server_ts: now,
            origin: &self.server_name,
            depth,
            prev_events: &prev_events,
            auth_events: &auth_events,
            redacts: None,
        };

        let finalized = synapse_federation::event_finalize::finalize_local_pdu(&invite_parts)
            .map_err(|e| ApiError::internal(format!("Failed to finalize invite event: {e}")))?;
        let event_id = finalized.event_id.clone();

        let mut invite_event = synapse_common::pdu::build_pdu(&invite_parts);
        if let Some(object) = invite_event.as_object_mut() {
            object.insert("hashes".to_string(), finalized.hashes.clone());
        }

        // Sign the event locally (the material is derived from the same PDU the
        // peer will receive).
        let signing_key = self.require_signing_key().await?;
        sign_and_hash_event(
            &room_version,
            &self.server_name,
            &signing_key.key_id,
            &signing_key.secret_key,
            &mut invite_event,
        )
        .map_err(|e| ApiError::internal(format!("Failed to sign invite event: {e}")))?;

        // Third-party event admission (Synapse `check_event_allowed`) for the
        // outbound federated invite. Gated *before* the remote `invite` call so
        // a refusal leaves no invite on the resident server; the PDU is signed
        // for transport, so a rule may refuse but must not rewrite it.
        let mut invite_params = CreateEventParams {
            event_id: event_id.clone(),
            room_id: room_id.to_string(),
            user_id: inviter_id.to_string(),
            event_type: "m.room.member".to_string(),
            content: invite_content.clone(),
            state_key: Some(invitee_id.to_string()),
            origin_server_ts: now,
            redacts: None,
        };
        crate::module_service::consult_event_admission(
            self.event_admission_gate.as_ref(),
            self.event_reader.as_ref(),
            &mut invite_params,
            false,
        )
        .await?;

        // Call invite on the remote server (the body carries the PDU plus the
        // room version — see `FederationClient::invite`).
        let invite_room_state = invite_room_state_for(&state_events);
        let invite_response = federation_client
            .invite(&destination, room_id, &event_id, &room_version, &invite_event, &invite_room_state)
            .await
            .map_err(|e| {
                ::tracing::warn!(error = %e, destination = %destination, "federation invite failed");
                ApiError::bad_request(format!("Remote server rejected invite: {e}"))
            })?;

        // 4. Add the invitee as an invited member locally.
        self.member_storage
            .add_member(room_id, invitee_id, "invite", None, None, Some(inviter_id), None)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to record invite after federation invite", e))?;

        // 5. Persist the signed event returned by the remote server.
        let final_event = invite_response.event;

        let persisted_event_id = final_event.get("event_id").and_then(|v| v.as_str()).unwrap_or(&event_id).to_string();

        let persisted_sender = final_event.get("sender").and_then(|v| v.as_str()).unwrap_or(inviter_id).to_string();

        let persisted_content = final_event.get("content").cloned().unwrap_or(json!({ "membership": "invite" }));

        let persisted_ts = final_event.get("origin_server_ts").and_then(|v| v.as_i64()).unwrap_or(now);

        if let Err(e) = self
            .event_writer
            .create_event(
                CreateEventParams {
                    event_id: persisted_event_id,
                    room_id: room_id.to_string(),
                    user_id: persisted_sender,
                    event_type: "m.room.member".to_string(),
                    content: persisted_content,
                    state_key: Some(invitee_id.to_string()),
                    origin_server_ts: persisted_ts,
                    redacts: None,
                },
                None,
            )
            .await
        {
            ::tracing::warn!(error = %e, "Failed to persist invite event after federation invite");
        } else {
            // Invalidate room-state cache after membership state change.
            let _ = self.cache.delete(&format!("room_state:{room_id}")).await;
        }

        Ok(())
    }

    // =========================================================================
    // Outbound exchange_third_party_invite
    // =========================================================================

    /// Exchange a third-party invite on a remote homeserver.
    ///
    /// When a local user wants to join a room via a third-party invite (e.g.
    /// email invite) on a remote server, we send the signed token to the
    /// room's home server.  The home server verifies the token against the
    /// `m.room.third_party_invite` state event, signs the `m.room.member`
    /// invite event, and returns it.  We then persist the signed event
    /// locally.
    ///
    /// Reference: element-hq/synapse
    /// `synapse/handlers/federation.py::FederationHandler.exchange_third_party_invite`
    pub async fn exchange_third_party_invite_via_federation(
        &self,
        destination: &str,
        room_id: &str,
        invite_event: &Value,
    ) -> ApiResult<Value> {
        ::tracing::info!(
            destination = %destination,
            room_id = %room_id,
            "Exchanging third-party invite via federation"
        );

        let federation_client = self.require_federation_client().await?;

        let signed_event =
            federation_client.exchange_third_party_invite(destination, room_id, invite_event).await.map_err(|e| {
                ::tracing::warn!(error = %e, destination = %destination, "exchange_third_party_invite failed");
                ApiError::bad_request(format!("Remote server rejected exchange_third_party_invite: {e}"))
            })?;

        // Persist the signed event locally.
        let event_id = signed_event
            .get("event_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| ApiError::internal("Remote server returned event without event_id".to_string()))?;

        let sender = signed_event.get("sender").and_then(|v| v.as_str()).unwrap_or("").to_string();

        let state_key = signed_event.get("state_key").and_then(|v| v.as_str()).map(|s| s.to_string());

        let content = signed_event.get("content").cloned().unwrap_or(json!({ "membership": "invite" }));

        let origin_server_ts =
            signed_event.get("origin_server_ts").and_then(|v| v.as_i64()).unwrap_or_else(current_timestamp_millis);

        // Third-party event admission (Synapse `check_event_allowed`) for the
        // exchanged invite. The event is signed by the resident server, so a
        // rule may refuse but must not rewrite it. A refusal returns before the
        // `add_member` below, so no membership row is claimed for a refused
        // invite.
        let mut exchange_params = CreateEventParams {
            event_id: event_id.clone(),
            room_id: room_id.to_string(),
            user_id: sender.clone(),
            event_type: "m.room.member".to_string(),
            content: content.clone(),
            state_key: state_key.clone(),
            origin_server_ts,
            redacts: None,
        };
        crate::module_service::consult_event_admission(
            self.event_admission_gate.as_ref(),
            self.event_reader.as_ref(),
            &mut exchange_params,
            false,
        )
        .await?;

        // Add the invitee as an invited member.
        if let Some(ref invitee_id) = state_key {
            self.member_storage
                .add_member(room_id, invitee_id, "invite", None, None, Some(&sender), None)
                .await
                .map_err(|e| ApiError::internal_with_cause("Failed to record invite after third-party exchange", e))?;
        }

        // Persist the event.
        if let Err(e) = self
            .event_writer
            .create_event(
                CreateEventParams {
                    event_id,
                    room_id: room_id.to_string(),
                    user_id: sender,
                    event_type: "m.room.member".to_string(),
                    content,
                    state_key,
                    origin_server_ts,
                    redacts: None,
                },
                None,
            )
            .await
        {
            ::tracing::warn!(error = %e, "Failed to persist third-party invite event");
        } else {
            // Invalidate room-state cache after membership state change.
            let _ = self.cache.delete(&format!("room_state:{room_id}")).await;
        }

        Ok(signed_event)
    }
}

#[cfg(test)]
mod join_persistence_failure_tests {
    use super::*;
    use crate::room::membership::service::MembershipServiceConfig;
    use crate::room::summary::RoomSummaryService;
    use crate::test_mocks::{FakeInvitePolicyGate, FakeRoomAuth};
    use std::sync::Arc as StdArc;
    use synapse_cache::{CacheConfig, CacheManager};
    use synapse_federation::client::{MakeJoinResponse, SendJoinResponse};
    use synapse_federation::key_rotation::KeyRotationManager;
    use synapse_federation::test_mocks::MockFederationClient;
    use synapse_storage::event::{EventReader, EventStorage, EventWriter};
    use synapse_storage::room::RoomStorage;
    use synapse_storage::test_mocks::room_summary::InMemoryRoomSummaryStore;
    use synapse_storage::test_mocks::{FakeUserStore, InMemoryMemberStore};
    use synapse_storage::{MemberStoreApi, RoomStoreApi, UserStore};

    /// B10c：`join_room_via_federation` 的事件持久化失败当前只 `warn!` 后继续，
    /// 于是成员表会被写成 join、而本地事件图里没有 `m.room.member` 事件（本地/远端分叉）。
    /// 修复后：任一步持久化失败都必须 fail-closed，且**不得**先宣称成员关系。
    #[tokio::test]
    async fn join_fails_closed_without_claiming_membership_when_event_persistence_fails() {
        let pool = match crate::test_utils::prepare_isolated_test_pool().await {
            Ok(pool) => pool,
            Err(error) => {
                eprintln!("Skipping federation join persistence test, test database unavailable: {error}");
                return;
            }
        };

        let server = "test.example.com";
        let destination = "remote.example.com";
        let room_id = "!fedjoin:remote.example.com";
        let user_id = "@joiner:test.example.com";
        let now = current_timestamp_millis();

        let event_storage = StdArc::new(EventStorage::new(&pool, server.to_string()));
        let event_reader: StdArc<dyn EventReader> = event_storage.clone();
        let event_writer: StdArc<dyn EventWriter> = event_storage;
        // 用内存成员存储：本用例只让**事件持久化**失败，成员关系是否被宣称才是观测点。
        // 真实成员表有 users 外键，会把无关的夹具缺失变成"错误"，使测试因错误原因通过。
        let member_store = InMemoryMemberStore::new();
        let member_storage: StdArc<dyn MemberStoreApi> = StdArc::new(member_store);
        let room_storage: StdArc<dyn RoomStoreApi> = StdArc::new(RoomStorage::new(&pool));

        // 测试环境显式允许明文签名密钥（生产默认拒绝，除非配置 master key）。
        let key_manager = StdArc::new(KeyRotationManager::new(&pool, server).with_allow_plaintext_signing_keys(true));
        key_manager.load_or_create_key().await.expect("create signing key");

        let federation_client = StdArc::new(MockFederationClient::new(server));
        federation_client
            .seed_make_join(
                room_id,
                MakeJoinResponse {
                    room_id: Some(room_id.to_string()),
                    room_version: Some("10".to_string()),
                    event: serde_json::json!({
                        "event_id": "$join:test.example.com",
                        "room_id": room_id,
                        "sender": user_id,
                        "type": "m.room.member",
                        "state_key": user_id,
                        "origin_server_ts": now,
                        "content": {"membership": "join"},
                        "prev_events": [],
                        "auth_events": [],
                    }),
                },
            )
            .await;
        federation_client
            .seed_send_join(
                room_id,
                SendJoinResponse {
                    room_id: room_id.to_string(),
                    origin: destination.to_string(),
                    state: vec![serde_json::json!({
                        "event_id": "$create:remote.example.com",
                        "room_id": room_id,
                        "sender": "@creator:remote.example.com",
                        "type": "m.room.create",
                        "state_key": "",
                        "origin_server_ts": now,
                        "content": {"creator": "@creator:remote.example.com", "room_version": "10"},
                        "prev_events": [],
                        "auth_events": [],
                    })],
                    auth_chain: vec![],
                    event: None,
                },
            )
            .await;

        let user_storage: StdArc<dyn UserStore> = StdArc::new(FakeUserStore::new());
        let room_summary_service = StdArc::new(RoomSummaryService::new(
            StdArc::new(InMemoryRoomSummaryStore::new()),
            event_reader.clone(),
            Some(member_storage.clone()),
        ));

        let svc = MembershipService::new(MembershipServiceConfig {
            member_storage: member_storage.clone(),
            room_storage,
            event_reader,
            event_writer,
            user_storage,
            room_auth: StdArc::new(FakeRoomAuth::new()),
            server_name: server.to_string(),
            federation_client: Some(federation_client),
            key_rotation_manager: Some(key_manager),
            event_broadcaster: None,
            room_summary_service,
            cache: StdArc::new(CacheManager::new(&CacheConfig::default())),
            resolution_cache: crate::room::state_record::ResolutionCache::default(),
            key_rotation_storage: None,
            app_service_manager: None,
            db_pool: Some(pool.as_ref().clone()),
            policy_service: None,
            invite_policy_gate: StdArc::new(FakeInvitePolicyGate::new()),
            event_admission_gate: StdArc::new(crate::test_mocks::FakeEventAdmissionGate::new()),
        });

        // 注入：events 写入必失败。CHECK (false) NOT VALID 只校验**新插入行**。
        sqlx::query("ALTER TABLE events ADD CONSTRAINT injected_fail_event CHECK (false) NOT VALID")
            .execute(&*pool)
            .await
            .expect("inject event write failure");

        let result = svc.join_room_via_federation(destination, room_id, user_id).await;

        assert!(result.is_err(), "事件持久化失败必须 fail-closed，不得静默返回 Ok(())");
        let is_member = member_storage.is_member(room_id, user_id).await.expect("membership lookup");
        assert!(!is_member, "事件未持久化时不得宣称用户已加入，否则本地成员表与事件图分叉");
    }

    /// B2（§12.5 旧清单口径）：`make_join` 返回的模板在**签名之前**必须校验。
    ///
    /// 恶意常驻服务器返回 `content.membership = "ban"` 时，加入方不得签名、
    /// 更不得调用 `send_join`——否则本服务器的签名会落在攻击者选定的成员事件上
    /// （上游 synapse #20189 / 规范 PR #2284）。
    #[tokio::test]
    async fn join_rejects_make_join_template_with_wrong_membership_before_signing() {
        let pool = match crate::test_utils::prepare_isolated_test_pool().await {
            Ok(pool) => pool,
            Err(error) => {
                eprintln!("Skipping make_join validation test, test database unavailable: {error}");
                return;
            }
        };

        let server = "test.example.com";
        let destination = "remote.example.com";
        let room_id = "!fedjoinbad:remote.example.com";
        let user_id = "@joiner:test.example.com";

        let event_storage = StdArc::new(EventStorage::new(&pool, server.to_string()));
        let event_reader: StdArc<dyn EventReader> = event_storage.clone();
        let event_writer: StdArc<dyn EventWriter> = event_storage;
        let member_storage: StdArc<dyn MemberStoreApi> = StdArc::new(InMemoryMemberStore::new());
        let room_storage: StdArc<dyn RoomStoreApi> = StdArc::new(RoomStorage::new(&pool));

        let federation_client = StdArc::new(MockFederationClient::new(server));
        federation_client
            .seed_make_join(
                room_id,
                MakeJoinResponse {
                    room_id: Some(room_id.to_string()),
                    room_version: Some("10".to_string()),
                    // The attack: a template that would have us sign a ban.
                    event: serde_json::json!({
                        "room_id": room_id,
                        "sender": user_id,
                        "type": "m.room.member",
                        "state_key": user_id,
                        "content": {"membership": "ban"},
                    }),
                },
            )
            .await;

        let user_storage: StdArc<dyn UserStore> = StdArc::new(FakeUserStore::new());
        let room_summary_service = StdArc::new(RoomSummaryService::new(
            StdArc::new(InMemoryRoomSummaryStore::new()),
            event_reader.clone(),
            Some(member_storage.clone()),
        ));

        let svc = MembershipService::new(MembershipServiceConfig {
            member_storage: member_storage.clone(),
            room_storage,
            event_reader,
            event_writer,
            user_storage,
            room_auth: StdArc::new(FakeRoomAuth::new()),
            server_name: server.to_string(),
            federation_client: Some(federation_client.clone()),
            // The check must run *before* the signing key is fetched.  Deliberately
            // providing no key manager means a regression that signs first fails
            // here with "no signing key" instead of silently signing a ban.
            key_rotation_manager: None,
            event_broadcaster: None,
            room_summary_service,
            cache: StdArc::new(CacheManager::new(&CacheConfig::default())),
            resolution_cache: crate::room::state_record::ResolutionCache::default(),
            key_rotation_storage: None,
            app_service_manager: None,
            db_pool: None,
            policy_service: None,
            invite_policy_gate: StdArc::new(FakeInvitePolicyGate::new()),
            event_admission_gate: StdArc::new(crate::test_mocks::FakeEventAdmissionGate::new()),
        });

        let result = svc.join_room_via_federation(destination, room_id, user_id).await;
        let err = result.expect_err("a make_join template with membership=ban must be rejected");
        assert!(err.to_string().contains("malformed"), "unexpected error: {err}");
        assert_eq!(
            federation_client.send_join_call_count(),
            0,
            "a rejected template must never reach send_join (that is where our signature would leak)"
        );
    }
}

#[cfg(test)]
mod invite_room_state_tests {
    use super::invite_room_state_for;
    use serde_json::{json, Value};
    use synapse_storage::event::StateEvent;

    fn state_event(event_type: &str, state_key: &str, content: Value, sender: &str) -> StateEvent {
        StateEvent {
            event_id: format!("${event_type}${state_key}"),
            room_id: "!r:remote.example".to_string(),
            sender: sender.to_string(),
            event_type: Some(event_type.to_string()),
            content,
            state_key: Some(state_key.to_string()),
            unsigned: None,
            is_redacted: Some(false),
            origin_server_ts: 1,
            depth: Some(1),
            processed_ts: None,
            not_before: None,
            status: None,
            origin: Some("remote.example".to_string()),
            user_id: Some(sender.to_string()),
            stream_ordering: None,
            prev_events: None,
            auth_events: None,
            signatures: None,
            hashes: None,
        }
    }

    /// P-18 (outbound half): the invitee renders its invite from this list, so
    /// MSC4311 makes `m.room.create` mandatory — an empty list (which this used
    /// to send unconditionally) makes every conforming receiver log
    /// `Stripped state must include m.room.create event`.
    #[test]
    fn create_comes_first_and_every_entry_is_stripped() {
        let inviter = "@inviter:remote.example";
        let state = vec![
            state_event("m.room.name", "", json!({"name": "Room"}), inviter),
            state_event("m.room.create", "", json!({"creator": inviter}), inviter),
            state_event("m.room.member", inviter, json!({"membership": "join"}), inviter),
            // Not pre-join state: must not leak into the invitee's invite_state.
            state_event("m.room.power_levels", "", json!({"users": {}}), inviter),
            state_event("m.room.message", "", json!({"body": "hi"}), inviter),
        ];

        let stripped = invite_room_state_for(&state);
        let types: Vec<&str> = stripped.iter().filter_map(|entry| entry["type"].as_str()).collect();
        assert_eq!(types, vec!["m.room.create", "m.room.name", "m.room.member"], "got {stripped:?}");

        // Exactly the four keys a stripped-state event may carry — a full PDU
        // echoed verbatim would leak `hashes`/`signatures`/graph fields to a
        // client that was never in the room.
        for entry in &stripped {
            let mut keys: Vec<&str> = entry.as_object().expect("an object").keys().map(String::as_str).collect();
            keys.sort_unstable();
            assert_eq!(keys, vec!["content", "sender", "state_key", "type"], "got {entry}");
        }
    }

    /// A room with nothing but a create event still yields a usable payload.
    #[test]
    fn create_only_still_produces_the_mandatory_entry() {
        let stripped = invite_room_state_for(&[state_event(
            "m.room.create",
            "",
            json!({"creator": "@a:remote.example"}),
            "@a:remote.example",
        )]);
        assert_eq!(stripped.len(), 1);
        assert_eq!(stripped[0]["type"], json!("m.room.create"));
    }
}
