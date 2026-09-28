//! Federation PDU projection for persisted state events.
//!
//! **Why this module exists.** The federation state routes (`/send_join` v1+v2,
//! `/state`, `/get_room_auth`, `/get_event_auth`, `backfill`'s auth chain) used
//! to hand-assemble their payloads inline as a 5–6 key JSON object
//! (`event_id`, `sender`, `type`, `content`, `state_key`). That shape is **not**
//! a Matrix PDU: `/send_join`'s `state` array carried no `origin_server_ts` at
//! all, and none of them carried `room_id` / `origin` / `depth` / `prev_events` /
//! `auth_events` / `hashes` / `signatures`. A joining server therefore could not
//! validate — or in places even parse — the state it was handed. Every emitter
//! now goes through [`state_pdu`].
//!
//! **What the projection can and cannot know.** `depth` / `prev_events` /
//! `auth_events` are persisted for every known-room-version write: the inbound
//! federation paths use `EventStorage::create_event_with_graph` /
//! `EventStorage::create_state_event_with_dag`, and locally-produced events go
//! through `MessagingService::create_event`, which resolves the graph fields and
//! persists them via `create_event_with_pdu` (see `graph_metadata`). A row can
//! still carry SQL `NULL` graph columns — it predates that write path, or its
//! room version could not be resolved — and the forward extremities such a row
//! actually had at creation time are not recoverable afterwards. This module
//! therefore **omits** those keys and reports
//! [`PduCompleteness::MissingGraphMetadata`] instead of substituting `[]` / `0`.
//! Substituting would be actively harmful: a peer that trusts a fabricated
//! `prev_events: []` files the event as a DAG root and corrupts its own room
//! graph.
//!
//! The same reasoning drives [`SignatureAction::RefuseIncomplete`]: an
//! incomplete PDU is never signed, because a valid content hash would only make
//! an invalid PDU *look* verifiable to a lenient peer.
//!
//! Residual gaps that this module does **not** close (tracked in
//! `docs/audit/PROJECT_REMAINING_ISSUES_2026-09-14.md` §21):
//!   1. inbound events do not persist the **origin server's** `signatures` /
//!      `hashes`, so a re-emitted remote PDU still lacks the sender signature a
//!      peer requires — signing here adds the local server's signature only;
//!   2. `event_id` is `$<ts>$<base64>:<server>` (`synapse_common::crypto::generate_event_id`),
//!      not the v4+ reference hash, so a v11 peer cannot accept these PDUs as canonical
//!      however complete their field set is.

use crate::routes::context::FederationContext;
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use synapse_common::event_utils::{event_id_array, signature_material};
use synapse_services::event::StateEvent;

/// Whether a projected PDU carried every field the federation format requires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PduCompleteness {
    /// `depth`, `prev_events` and `auth_events` were all present in the row.
    Complete,
    /// At least one of them was `NULL`, so the PDU omits the graph fields.
    MissingGraphMetadata,
}

/// What [`build_pdus`] must do about a projected PDU's hash/signature pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureAction {
    /// The row already carries a `hashes.sha256` **and** a `signatures` object:
    /// emit both verbatim so the projection stays byte-identical to the PDU the
    /// origin server signed.
    KeepStored,
    /// Compute the content hash and sign the emitted PDU with the local key.
    SignLocally,
    /// Emit the PDU unsigned: it is missing mandatory keys, so signing it would
    /// misrepresent an invalid PDU as verifiable.
    RefuseIncomplete,
}

/// `origin` normalisation shared with the other federation serialisers:
/// an empty / `self` / `undefined` origin means "produced by this server".
fn normalized_origin(server_name: &str, origin: Option<&str>) -> String {
    match origin.map(str::trim) {
        Some("") | Some("self") | Some("undefined") | None => server_name.to_string(),
        Some(value) => value.to_string(),
    }
}

/// Project a persisted state event into a federation PDU.
///
/// The returned [`PduCompleteness`] tells the caller whether `depth` /
/// `prev_events` / `auth_events` were emitted. Callers must surface an
/// incomplete projection (log + counter) rather than treating it as a valid PDU.
///
/// `hashes` / `signatures` are **not** attached here — see
/// [`SignatureAction`] and [`apply_stored_signature_material`].
///
/// `room_version` decides whether `event_id` is part of the PDU at all: it is a
/// PDU field for room versions 1 and 2 only (spec room v3 "Event format" — "the
/// `event_id` field is no longer included. A server receiving an event should
/// compute the relevant event ID for itself"). `None` means the version could
/// not be read; the legacy shape is then kept and the caller logs it, because
/// silently dropping the field for a v1/v2 room would hand the receiver a PDU it
/// cannot identify either.
pub fn state_pdu(server_name: &str, record: &StateEvent, room_version: Option<&str>) -> (Value, PduCompleteness) {
    let mut pdu = Map::new();
    match room_version {
        Some(version) if !synapse_common::pdu::event_id_is_a_pdu_field(version) => {}
        _ => {
            pdu.insert("event_id".to_string(), json!(record.event_id));
        }
    }
    pdu.insert("room_id".to_string(), json!(record.room_id));
    pdu.insert("sender".to_string(), json!(record.sender));
    pdu.insert("type".to_string(), json!(record.event_type.clone().unwrap_or_default()));
    pdu.insert("content".to_string(), record.content.clone());
    pdu.insert("origin_server_ts".to_string(), json!(record.origin_server_ts));
    pdu.insert("origin".to_string(), json!(normalized_origin(server_name, record.origin.as_deref())));

    if let Some(state_key) = &record.state_key {
        pdu.insert("state_key".to_string(), json!(state_key));
    }
    if let Some(unsigned) = &record.unsigned {
        pdu.insert("unsigned".to_string(), unsigned.clone());
    }

    let depth = record.depth;
    let prev_events = event_id_array(record.prev_events.as_ref());
    let auth_events = event_id_array(record.auth_events.as_ref());

    let completeness = match (depth, prev_events, auth_events) {
        (Some(depth), Some(prev_events), Some(auth_events)) => {
            pdu.insert("depth".to_string(), json!(depth));
            pdu.insert("prev_events".to_string(), json!(prev_events));
            pdu.insert("auth_events".to_string(), json!(auth_events));
            PduCompleteness::Complete
        }
        // Deliberately omit the keys: `[]` / `0` here would be a fabricated DAG
        // position, not a projection of stored state.
        _ => PduCompleteness::MissingGraphMetadata,
    };

    (Value::Object(pdu), completeness)
}

/// Membership rule for the **auth chain** lists that the federation membership
/// and room-auth routes emit.
///
/// This is the five-type rule those routes already used, now **version-aware**
/// for the one type whose membership changed (D-5).
///
/// The spec defines an event's auth chain as the transitive closure of its
/// `auth_events`. A type list is an approximation of that closure, and under
/// room v12 it stops being a good one for `m.room.create`: D-4 removed the create
/// event from every v12 event's `auth_events` (the room id *is* the create event's
/// id, so the reference is implied), so the closure of any v12 event can never
/// reach `m.room.create`. Listing it there would emit an auth chain the peer
/// cannot derive from the events it holds. Below v12 the create event is still
/// referenced, so it stays.
///
/// The remaining four types are unchanged for every version.
pub fn is_auth_chain_member(record: &StateEvent, room_version: &str) -> bool {
    let event_type = record.event_type.as_deref();

    if event_type == Some("m.room.create") {
        // v12+ never references the create event (D-4), so it is not in the
        // auth chain; v1-v11 do, so it is.
        return !synapse_common::room_versions::room_version_at_least(room_version, 12);
    }

    matches!(
        event_type,
        Some("m.room.member")
            | Some("m.room.power_levels")
            | Some("m.room.join_rules")
            | Some("m.room.history_visibility")
    )
}

/// The room version declared by the room's `m.room.create` state event.
///
/// Every auth-chain caller already holds the room's state records (they are what
/// it filters), so the version is read from them rather than with a second query.
/// Falls back to [`synapse_common::room_versions::DEFAULT_ROOM_VERSION`] when the
/// create event is absent or carries no version — the same fallback every other
/// version resolver in this repository uses.
pub fn room_version_from_state(records: &[StateEvent]) -> String {
    records
        .iter()
        .find(|record| record.event_type.as_deref() == Some("m.room.create"))
        .and_then(|record| record.content.get("room_version"))
        .and_then(|value| value.as_str())
        .unwrap_or(synapse_common::room_versions::DEFAULT_ROOM_VERSION)
        .to_string()
}

/// The ruled decision table for a projected PDU's hash/signature pair.
///
/// Completeness is checked **first**: refusing an incomplete PDU outranks
/// reusing stored material, so a malformed PDU can never become verifiable just
/// because the row happened to carry hashes.
pub fn signature_action(record: &StateEvent, completeness: PduCompleteness) -> SignatureAction {
    if completeness == PduCompleteness::MissingGraphMetadata {
        return SignatureAction::RefuseIncomplete;
    }
    if stored_signature_material(record).is_some() {
        SignatureAction::KeepStored
    } else {
        SignatureAction::SignLocally
    }
}

/// The stored `hashes` / `signatures` pair, when it is complete enough to reuse.
///
/// Both halves must be present and non-empty: stored hashes paired with a fresh
/// signature would describe two different byte sequences.
fn stored_signature_material(record: &StateEvent) -> Option<(Value, Value)> {
    signature_material(record.hashes.as_ref(), record.signatures.as_ref())
}

/// Attach the stored `hashes` / `signatures` pair, if there is one.
///
/// Returns `true` when the PDU was completed from stored material. Only called
/// for [`SignatureAction::KeepStored`].
pub fn apply_stored_signature_material(record: &StateEvent, pdu: &mut Value) -> bool {
    let Some((hashes, signatures)) = stored_signature_material(record) else {
        return false;
    };
    if let Some(object) = pdu.as_object_mut() {
        object.insert("hashes".to_string(), hashes);
        object.insert("signatures".to_string(), signatures);
        return true;
    }
    false
}

/// Build a batch of federation PDUs, applying [`signature_action`] to each.
///
/// Takes a concrete slice rather than a generic iterator on purpose: a generic
/// `impl IntoIterator<Item = &StateEvent>` whose items come from an inline
/// `.filter(closure)` makes the axum handler future non-`Send` and non-general
/// over lifetimes, which surfaces as bogus `implementation of FnOnce is not
/// general enough` errors at the router. Callers collect their selection first.
///
/// Selection is the caller's business (e.g. "auth events only"), so this helper
/// makes no assumption about *which* events belong in a state list versus an
/// auth chain.
///
/// Signing is in-memory only: this helper is reached from read paths
/// (`/state`, `/get_room_auth`, `/get_event_auth`) as well as `/send_join`, and
/// a GET must not grow a best-effort write. Nothing is persisted back.
pub async fn build_pdus(ctx: &FederationContext, records: &[&StateEvent]) -> Vec<Value> {
    let mut pdus = Vec::with_capacity(records.len());
    // Records in one call belong to one room in every current call site (a state
    // list or an auth chain), but resolve per distinct room so a future caller
    // cannot silently get the wrong version for half its records.
    let mut room_versions: HashMap<String, Option<String>> = HashMap::new();
    for record in records {
        let room_version = match room_versions.get(&record.room_id) {
            Some(cached) => cached.clone(),
            None => {
                let resolved = match ctx.room_service.state().get_room_version(&record.room_id).await {
                    Ok(version) => version,
                    Err(error) => {
                        ::tracing::warn!(
                            room_id = %record.room_id,
                            %error,
                            "failed to resolve room version; projecting the PDU with the legacy event_id field"
                        );
                        None
                    }
                };
                if resolved.is_none() {
                    ::tracing::warn!(
                        room_id = %record.room_id,
                        "no room version recorded; projecting the PDU with the legacy event_id field"
                    );
                }
                room_versions.insert(record.room_id.clone(), resolved.clone());
                resolved
            }
        };
        let (mut pdu, completeness) = state_pdu(&ctx.server_name, record, room_version.as_deref());
        match signature_action(record, completeness) {
            SignatureAction::KeepStored => {
                apply_stored_signature_material(record, &mut pdu);
            }
            SignatureAction::RefuseIncomplete => {
                ::tracing::warn!(
                    event_id = %record.event_id,
                    room_id = %record.room_id,
                    "PDU projection is missing graph metadata (depth/prev_events/auth_events) \
                     and was emitted unsigned without them; see federation::pdu module docs"
                );
                super::increment_counter(ctx, "federation_pdu_incomplete_total");
            }
            SignatureAction::SignLocally => {
                sign_locally(ctx, &record.event_id, &mut pdu).await;
            }
        }
        pdus.push(pdu);
    }
    pdus
}

/// Sign and hash a projected PDU with this server's current signing key.
async fn sign_locally(ctx: &FederationContext, event_id: &str, pdu: &mut Value) {
    // The signature material is room-version dependent (redaction differs per
    // version), so an unknown version must never be guessed. This projection is
    // best-effort: an unresolvable version emits the PDU unsigned rather than
    // signing bytes a peer cannot reproduce.
    let Some(room_id) = pdu.get("room_id").and_then(Value::as_str) else {
        ::tracing::warn!(
            event_id = %event_id,
            "projected PDU has no room_id — cannot resolve a room version; emitting unsigned"
        );
        return;
    };
    let room_version = match ctx.room_service.state().get_room_version(room_id).await {
        Ok(Some(room_version)) => room_version,
        Ok(None) => {
            ::tracing::warn!(
                event_id = %event_id,
                room_id = %room_id,
                "no room version recorded for room — refusing to sign; federation PDU will be emitted unsigned"
            );
            return;
        }
        Err(error) => {
            ::tracing::warn!(
                event_id = %event_id,
                room_id = %room_id,
                %error,
                "failed to resolve room version — refusing to sign; federation PDU will be emitted unsigned"
            );
            return;
        }
    };

    let key = match ctx.key_rotation_manager.get_current_key().await {
        Ok(Some(key)) => key,
        Ok(None) => {
            ::tracing::warn!(
                event_id = %event_id,
                "no signing key available — federation PDU will be emitted unsigned"
            );
            return;
        }
        Err(error) => {
            ::tracing::warn!(
                event_id = %event_id,
                %error,
                "failed to fetch signing key — federation PDU will be emitted unsigned"
            );
            return;
        }
    };

    if let Err(error) = crate::federation::signing::sign_and_hash_event(
        &room_version,
        &ctx.server_name,
        &key.key_id,
        &key.secret_key,
        pdu,
    ) {
        ::tracing::warn!(
            event_id = %event_id,
            %error,
            "sign_and_hash_event failed — federation PDU will be emitted unsigned"
        );
    }
}
