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

use serde_json::{json, Map, Value};
use synapse_common::event_utils::event_id_array;
use synapse_common::{ApiError, ApiResult};
use synapse_federation::event_broadcaster::EventBroadcaster;
use synapse_federation::signing::sign_and_hash_event;
use synapse_federation::KeyRotationManager;
use synapse_storage::event::{EventReader, EventWriter, PersistedGraphFields, RoomEvent};

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
}

/// Build the outbound PDU for a locally-produced event.
///
/// Returns `None` when `depth`, `prev_events` or `auth_events` is absent or
/// unusable. The caller must then skip the broadcast instead of emitting a PDU
/// that a spec-compliant peer would reject — the same rule the inbound
/// projector applies (`SignatureAction::RefuseIncomplete`).
pub(crate) fn build_broadcast_pdu(server_name: &str, event: &RoomEvent, graph: &PersistedGraphFields) -> Option<Value> {
    let depth = graph.depth?;
    let prev_events = event_id_array(graph.prev_events.as_ref())?;
    let auth_events = event_id_array(graph.auth_events.as_ref())?;

    let mut pdu = Map::new();
    pdu.insert("event_id".to_string(), json!(event.event_id));
    pdu.insert("room_id".to_string(), json!(event.room_id));
    pdu.insert("sender".to_string(), json!(event.user_id));
    pdu.insert("user_id".to_string(), json!(event.user_id));
    pdu.insert("type".to_string(), json!(event.event_type));
    pdu.insert("content".to_string(), event.content.clone());
    pdu.insert("origin_server_ts".to_string(), json!(event.origin_server_ts));
    pdu.insert("origin".to_string(), json!(server_name));
    pdu.insert("depth".to_string(), json!(depth));
    pdu.insert("prev_events".to_string(), json!(prev_events));
    pdu.insert("auth_events".to_string(), json!(auth_events));

    if let Some(state_key) = &event.state_key {
        pdu.insert("state_key".to_string(), json!(state_key));
    }

    if let Some(redacts) = &event.redacts {
        apply_redacts(&mut pdu, redacts);
    }

    Some(Value::Object(pdu))
}

/// Places a redaction target on the outbound PDU.
///
/// v11+ (MSC2174/MSC3820) already carries the target in `content.redacts`
/// (injected in `RoomMessagingService::create_event`), so the top-level field
/// must **not** be added. v1-v10 keeps the top-level `redacts` field.
fn apply_redacts(pdu: &mut Map<String, Value>, redacts: &str) {
    let content_has_redacts = pdu.get("content").and_then(|content| content.get("redacts")).is_some();
    if !content_has_redacts {
        pdu.insert("redacts".to_string(), json!(redacts));
    }
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

    let Some(mut pdu) = build_broadcast_pdu(&ctx.server_name, event, &graph) else {
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
        let pdu = build_broadcast_pdu("example.com", &room_event(), &complete_graph()).expect("complete PDU");
        assert_eq!(pdu["depth"], json!(8));
        assert_eq!(pdu["prev_events"], json!(["$p:example.com"]));
        assert_eq!(pdu["auth_events"], json!(["$create:example.com", "$pl:example.com"]));
        assert_eq!(pdu["origin"], json!("example.com"));
        assert_eq!(pdu["origin_server_ts"], json!(1_700_000_000_000_i64));
    }

    #[test]
    fn missing_depth_is_refused() {
        let mut graph = complete_graph();
        graph.depth = None;
        assert!(build_broadcast_pdu("example.com", &room_event(), &graph).is_none());
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
                build_broadcast_pdu("example.com", &room_event(), &graph).is_none(),
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
            assert!(build_broadcast_pdu("example.com", &room_event(), &graph).is_none(), "prev={label}");
        }
    }

    #[test]
    fn empty_arrays_are_usable_graph_metadata() {
        // The create event legitimately has `[]` for both — it is the DAG root.
        let graph = PersistedGraphFields { depth: Some(1), prev_events: Some(json!([])), auth_events: Some(json!([])) };
        let pdu = build_broadcast_pdu("example.com", &room_event(), &graph).expect("root PDU");
        assert_eq!(pdu["prev_events"], json!([]));
        assert_eq!(pdu["auth_events"], json!([]));
        assert_eq!(pdu["depth"], json!(1));
    }

    #[test]
    fn state_key_and_room_version_aware_redacts_placement() {
        let mut event = room_event();
        event.state_key = Some("".to_string());
        let pdu = build_broadcast_pdu("example.com", &event, &complete_graph()).expect("state PDU");
        assert_eq!(pdu["state_key"], json!(""));

        // v11+ carries the target inside content: no top-level `redacts` field.
        let mut v11 = room_event();
        v11.event_type = "m.room.redaction".to_string();
        v11.content = json!({"reason": "spam", "redacts": "$target:example.com"});
        v11.redacts = Some("$target:example.com".to_string());
        let pdu = build_broadcast_pdu("example.com", &v11, &complete_graph()).expect("v11 redaction");
        assert!(pdu.get("redacts").is_none(), "v11+ must not gain a top-level redacts: {pdu}");

        // v1-v10 keeps the top-level field.
        let mut v10 = room_event();
        v10.event_type = "m.room.redaction".to_string();
        v10.content = json!({"reason": "spam"});
        v10.redacts = Some("$target:example.com".to_string());
        let pdu = build_broadcast_pdu("example.com", &v10, &complete_graph()).expect("v10 redaction");
        assert_eq!(pdu["redacts"], json!("$target:example.com"));
    }
}
