//! Outbound federation PDU construction and broadcast for locally-produced events.
//!
//! # Why this module exists
//!
//! Two services used to build and broadcast the outbound PDU with their own
//! copy of the same ~80 lines (`messaging/service.rs` and
//! `membership/service.rs`). They had **diverged on policy**:
//!
//! * messaging failed *closed* when the room's extremities could not be read
//!   (skip the broadcast), membership failed *open* (broadcast with
//!   `prev_events: []`);
//! * messaging placed `redacts` with the room-version rule, membership always
//!   wrote the top-level field;
//! * neither included `depth` or `auth_events`, so every outbound PDU was
//!   missing two keys a v3+ PDU requires — while the inbound projector
//!   (`synapse-web/src/routes/federation/pdu.rs`) refuses to *emit* a PDU
//!   without them. The two halves of the server disagreed about what a
//!   valid PDU is.
//!
//! Both now delegate here. The event's graph fields are read back from the row
//! that was just persisted — the write paths (`GraphMetadataWriter` for
//! auto-commit events, `CreationGraph` for room creation, the inbound
//! `create_event_with_graph`) own that data, so the broadcast must not
//! re-derive it from a "latest events" query, which reports ancestors as tips.
//!
//! # Failure policy
//!
//! Fail-closed, uniformly: if the graph fields are absent or unusable, the
//! event is **not** signed and **not** broadcast. Signing an incomplete PDU
//! would only make an invalid event *look* verifiable to a lenient peer.

use std::sync::Arc;

use serde_json::Value;
use synapse_common::event_utils::event_id_array;
use synapse_common::pdu::{build_pdu, PduParts};
use synapse_common::{ApiError, ApiResult};
use synapse_federation::event_broadcaster::EventBroadcaster;
use synapse_federation::signing::sign_and_hash_event;
use synapse_federation::KeyRotationManager;
use synapse_storage::event::{EventReader, EventWriter, PersistedGraphFields, RoomEvent};
use synapse_storage::room::RoomStoreApi;

/// The dependencies the outbound broadcast path needs.
///
/// Assembled by each service from its own fields, so the *logic* exists once
/// while the two services keep their existing method signatures.
pub(crate) struct BroadcastContext {
    /// This server's Matrix name (`origin`).
    pub server_name: String,
    /// Reads back the persisted graph fields.
    pub event_reader: Arc<dyn EventReader>,
    /// Persists the signature/hash pair after signing.
    pub event_writer: Arc<dyn EventWriter>,
    /// Federation signing key source. `None` disables broadcasting entirely.
    pub key_rotation_manager: Option<Arc<KeyRotationManager>>,
    /// Destination transport. `None` keeps the signing/persistence half only.
    pub event_broadcaster: Option<Arc<EventBroadcaster>>,
    /// Room-version source. The PDU's shape depends on it (`event_id` is a PDU
    /// field for v1/v2 only), so an unreadable version refuses the broadcast
    /// rather than guessing one.
    pub room_storage: Arc<dyn RoomStoreApi>,
}

/// Build the outbound PDU for a locally-produced event.
///
/// Returns `None` when `depth`, `prev_events` or `auth_events` is absent or
/// unusable. The caller must then skip the broadcast instead of emitting a PDU
/// that a spec-compliant peer would reject — the same rule the inbound
/// projector applies (`SignatureAction::RefuseIncomplete`).
pub(crate) fn build_broadcast_pdu(
    server_name: &str,
    event: &RoomEvent,
    graph: &PersistedGraphFields,
    room_version: &str,
) -> Option<Value> {
    let depth = graph.depth?;
    let prev_events = event_id_array(graph.prev_events.as_ref())?;
    let auth_events = event_id_array(graph.auth_events.as_ref())?;

    // The field list lives in `synapse_common::pdu::build_pdu` — the single
    // assembly shared with the write path. In particular this no longer emits
    // `event_id` for v3+ (the receiver derives it from the reference hash) and
    // never emits the non-spec `user_id` key.
    Some(build_pdu(&PduParts {
        room_version,
        event_id: Some(event.event_id.as_str()),
        room_id: &event.room_id,
        sender: &event.user_id,
        event_type: &event.event_type,
        content: &event.content,
        state_key: event.state_key.as_deref(),
        origin_server_ts: event.origin_server_ts,
        origin: server_name,
        depth,
        prev_events: &prev_events,
        auth_events: &auth_events,
        redacts: event.redacts.as_deref(),
    }))
}

/// Sign a locally-produced event and broadcast it to every remote server with a
/// joined member in the room.
///
/// Best-effort by design: a missing signing key, an unreadable graph row or a
/// transport failure is logged and swallowed — the event is already persisted
/// locally, and failing the caller would report a local write as failed.
pub(crate) async fn sign_and_broadcast_event(ctx: &BroadcastContext, event: &RoomEvent) -> ApiResult<()> {
    // 0. Federation signing disabled (tests, or a deployment without keys).
    let Some(key_rotation_manager) = &ctx.key_rotation_manager else {
        return Ok(());
    };

    // 1. The graph fields the row was written with.
    let graph = match ctx.event_reader.get_event_graph_fields(&event.event_id).await {
        Ok(Some(graph)) => graph,
        Ok(None) => {
            ::tracing::warn!(
                event_id = %event.event_id,
                room_id = %event.room_id,
                "event row vanished before broadcast; skipping federation PDU"
            );
            return Ok(());
        }
        Err(e) => {
            ::tracing::warn!(
                event_id = %event.event_id,
                room_id = %event.room_id,
                error = %e,
                "failed to read persisted graph metadata; skipping federation PDU"
            );
            return Ok(());
        }
    };

    // 1b. The room version decides the PDU's shape (v1/v2 carry `event_id`,
    //     v3+ must not). Refuse to guess it.
    let room_version = match ctx.room_storage.get_room_version_only(&event.room_id).await {
        Ok(Some(version)) => version,
        Ok(None) => {
            ::tracing::warn!(
                event_id = %event.event_id,
                room_id = %event.room_id,
                "room version unknown; skipping federation PDU"
            );
            return Ok(());
        }
        Err(e) => {
            ::tracing::warn!(
                event_id = %event.event_id,
                room_id = %event.room_id,
                error = %e,
                "failed to read room version; skipping federation PDU"
            );
            return Ok(());
        }
    };

    let Some(mut pdu) = build_broadcast_pdu(&ctx.server_name, event, &graph, &room_version) else {
        ::tracing::warn!(
            event_id = %event.event_id,
            room_id = %event.room_id,
            "event has no persisted depth/prev_events/auth_events; \
             refusing to sign or broadcast an incomplete PDU"
        );
        return Ok(());
    };

    // 2. Sign and hash.
    let Some(signing_key) = key_rotation_manager
        .get_current_key()
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to get signing key", e))?
    else {
        ::tracing::warn!(
            event_id = %event.event_id,
            "no signing key available — federation PDU not broadcast"
        );
        return Ok(());
    };

    sign_and_hash_event(&ctx.server_name, &signing_key.key_id, &signing_key.secret_key, &mut pdu)
        .map_err(|e| ApiError::internal(format!("Failed to sign event: {e}")))?;

    // 3. Persist the signature/hash pair so a later PDU projection can re-emit
    //    this event byte-identically (`SignatureAction::KeepStored`).
    let signatures = pdu.get("signatures").cloned().unwrap_or(Value::Null);
    let hashes = pdu.get("hashes").cloned().unwrap_or(Value::Null);
    if let Err(e) = ctx.event_writer.update_event_signatures_and_hashes(&event.event_id, &signatures, &hashes).await {
        ::tracing::warn!(
            event_id = %event.event_id,
            room_id = %event.room_id,
            error = %e,
            "Failed to persist event signatures/hashes"
        );
    }

    // 4. Broadcast to remote servers.
    if let Some(broadcaster) = &ctx.event_broadcaster {
        if let Err(e) = broadcaster.broadcast_event(&event.room_id, &pdu, &ctx.server_name).await {
            ::tracing::warn!(
                event_id = %event.event_id,
                room_id = %event.room_id,
                error = %e,
                "Failed to broadcast event to federation peers"
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn room_event() -> RoomEvent {
        RoomEvent {
            event_id: "$e:example.com".to_string(),
            room_id: "!r:example.com".to_string(),
            user_id: "@alice:example.com".to_string(),
            event_type: "m.room.message".to_string(),
            content: json!({"body": "hi", "msgtype": "m.text"}),
            state_key: None,
            depth: 8,
            origin_server_ts: 1_700_000_000_000,
            processed_ts: 1_700_000_000_000,
            not_before: 0,
            status: None,
            origin: "self".to_string(),
            stream_ordering: Some(1),
            redacts: None,
        }
    }

    fn complete_graph() -> PersistedGraphFields {
        PersistedGraphFields {
            depth: Some(8),
            prev_events: Some(json!(["$p:example.com"])),
            auth_events: Some(json!(["$create:example.com", "$pl:example.com"])),
        }
    }

    #[test]
    fn complete_graph_yields_a_v3_pdu() {
        let pdu = build_broadcast_pdu("example.com", &room_event(), &complete_graph(), "10").expect("complete PDU");
        assert_eq!(pdu["depth"], json!(8));
        assert_eq!(pdu["prev_events"], json!(["$p:example.com"]));
        assert_eq!(pdu["auth_events"], json!(["$create:example.com", "$pl:example.com"]));
        assert_eq!(pdu["origin"], json!("example.com"));
        assert_eq!(pdu["origin_server_ts"], json!(1_700_000_000_000_i64));
    }

    /// U-13 step 2: v3+ outbound PDUs must not carry `event_id` (the receiver
    /// derives it from the reference hash) nor the non-spec `user_id` key.
    #[test]
    fn v3_plus_outbound_pdu_carries_neither_event_id_nor_user_id() {
        let pdu = build_broadcast_pdu("example.com", &room_event(), &complete_graph(), "10").unwrap();
        assert!(pdu.get("event_id").is_none(), "v10 PDU must not carry event_id: {pdu}");
        assert!(pdu.get("user_id").is_none(), "user_id is not a PDU field: {pdu}");
    }

    /// …while v1/v2 PDUs keep the server-assigned `event_id`.
    #[test]
    fn v1_outbound_pdu_keeps_the_event_id() {
        let pdu = build_broadcast_pdu("example.com", &room_event(), &complete_graph(), "1").unwrap();
        assert_eq!(pdu["event_id"], json!("$e:example.com"));
        assert!(pdu.get("user_id").is_none());
    }

    #[test]
    fn missing_depth_is_refused() {
        let mut graph = complete_graph();
        graph.depth = None;
        assert!(build_broadcast_pdu("example.com", &room_event(), &graph, "10").is_none());
    }

    #[test]
    fn missing_or_null_graph_arrays_are_refused_not_substituted() {
        for (prev, auth) in [
            (None, Some(json!(["$a:example.com"]))),
            (Some(Value::Null), Some(json!(["$a:example.com"]))),
            (Some(json!(["$p:example.com"])), None),
            (Some(json!(["$p:example.com"])), Some(Value::Null)),
        ] {
            let graph = PersistedGraphFields { depth: Some(8), prev_events: prev, auth_events: auth };
            assert!(
                build_broadcast_pdu("example.com", &room_event(), &graph, "10").is_none(),
                "an incomplete graph must not produce a PDU: {graph:?}"
            );
        }
    }

    #[test]
    fn non_array_and_non_string_graph_metadata_are_refused() {
        for prev in [json!({"not": "an array"}), json!([1, 2]), json!(["ok", 7])] {
            let label = prev.to_string();
            let graph = PersistedGraphFields {
                depth: Some(8),
                prev_events: Some(prev),
                auth_events: Some(json!(["$a:example.com"])),
            };
            assert!(build_broadcast_pdu("example.com", &room_event(), &graph, "10").is_none(), "prev={label}");
        }
    }

    #[test]
    fn empty_arrays_are_usable_graph_metadata() {
        // The create event legitimately has `[]` for both — it is the DAG root.
        let graph = PersistedGraphFields { depth: Some(1), prev_events: Some(json!([])), auth_events: Some(json!([])) };
        let pdu = build_broadcast_pdu("example.com", &room_event(), &graph, "10").expect("root PDU");
        assert_eq!(pdu["prev_events"], json!([]));
        assert_eq!(pdu["auth_events"], json!([]));
        assert_eq!(pdu["depth"], json!(1));
    }

    #[test]
    fn state_key_and_room_version_aware_redacts_placement() {
        let mut event = room_event();
        event.state_key = Some("".to_string());
        let pdu = build_broadcast_pdu("example.com", &event, &complete_graph(), "10").expect("state PDU");
        assert_eq!(pdu["state_key"], json!(""));

        // v11+ carries the target inside content: no top-level `redacts` field.
        let mut v11 = room_event();
        v11.event_type = "m.room.redaction".to_string();
        v11.content = json!({"reason": "spam", "redacts": "$target:example.com"});
        v11.redacts = Some("$target:example.com".to_string());
        let pdu = build_broadcast_pdu("example.com", &v11, &complete_graph(), "11").expect("v11 redaction");
        assert!(pdu.get("redacts").is_none(), "v11+ must not gain a top-level redacts: {pdu}");

        // v1-v10 keeps the top-level field.
        let mut v10 = room_event();
        v10.event_type = "m.room.redaction".to_string();
        v10.content = json!({"reason": "spam"});
        v10.redacts = Some("$target:example.com".to_string());
        let pdu = build_broadcast_pdu("example.com", &v10, &complete_graph(), "10").expect("v10 redaction");
        assert_eq!(pdu["redacts"], json!("$target:example.com"));
    }
}
