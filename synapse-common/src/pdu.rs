//! The single local-PDU assembly (U-13 step 2).
//!
//! Every locally-produced event that is persisted or federated must be assembled
//! by [`build_pdu`].  Before this module existed the same field list was written
//! out twice — once when the storage row was inserted and once in the outbound
//! broadcast path — and the two disagreed: the broadcast copy carried a
//! **non-spec `user_id` key** and, worse, an `event_id` field for every room
//! version.  For v3+ that made the reference hash self-referential (the ID is
//! derived from the redacted PDU, which must therefore not contain it) and made
//! the PDU unverifiable by any peer.
//!
//! Rules encoded here (they are protocol, not style):
//!
//! * `event_id` is a PDU field **only for room versions 1 and 2**.  Room
//!   version 3 stopped sending it over federation ("the `event_id` field is no
//!   longer included. A server receiving an event should compute the relevant
//!   event ID for itself", spec room v3 "Event format"); v4+ derive it from the
//!   reference hash.
//! * `user_id` is never a PDU field.  It is a storage-row convenience and must
//!   not leave the server.
//! * A redaction target goes in the top-level `redacts` field for v1–v10, and in
//!   `content.redacts` for v11+ (MSC2174/MSC3820).  A top-level copy must never
//!   be added when the content already carries it.
//!
//! Reference: Matrix Specification v1.18 (room versions 1, 3, 11) and
//! `element-hq/synapse` release-v1.161.
use serde_json::{Map, Value};

use crate::redaction::redacts_in_content;

/// Returns `true` when the room version's PDUs carry `event_id` as a field.
///
/// Only v1/v2 do: they assigned event IDs server-side.  v3+ derive the ID from
/// the event's reference hash and omit the field on the wire.
pub fn event_id_is_a_pdu_field(room_version: &str) -> bool {
    matches!(room_version, "1" | "2")
}

/// The primitive fields of one locally-produced PDU.
///
/// Borrowed rather than owned so the same value can be assembled cheaply on
/// both the write path (from `CreateEventParams` plus resolved graph fields) and
/// the federation path (from a persisted row).
#[derive(Debug, Clone)]
pub struct PduParts<'a> {
    /// Room version of the room the event belongs to.
    pub room_version: &'a str,
    /// The server-assigned event ID.  Required for v1/v2 ([`event_id_is_a_pdu_field`]),
    /// ignored for v3+ (the caller derives it from the reference hash instead).
    pub event_id: Option<&'a str>,
    /// The `room_id` field.
    pub room_id: &'a str,
    /// The `sender` field.
    pub sender: &'a str,
    /// The `type` field.
    pub event_type: &'a str,
    /// The `content` field.
    pub content: &'a Value,
    /// The `state_key` field, when the event is a state event.
    pub state_key: Option<&'a str>,
    /// The `origin_server_ts` field.
    pub origin_server_ts: i64,
    /// The `origin` field — this server's Matrix name.
    pub origin: &'a str,
    /// The `depth` field.
    pub depth: i64,
    /// The `prev_events` field.
    pub prev_events: &'a [String],
    /// The `auth_events` field.
    pub auth_events: &'a [String],
    /// Redaction target, when the event is an `m.room.redaction`.
    pub redacts: Option<&'a str>,
}

/// Assembles a local PDU with exactly the fields the protocol allows.
///
/// Deterministic and side-effect free: the same parts always produce the same
/// JSON, which is what lets the event ID (a hash of this value) be recomputed by
/// a peer from the PDU it receives.
pub fn build_pdu(parts: &PduParts<'_>) -> Value {
    let mut pdu = Map::new();

    if event_id_is_a_pdu_field(parts.room_version) {
        if let Some(event_id) = parts.event_id {
            pdu.insert("event_id".to_string(), Value::String(event_id.to_string()));
        }
    }

    // MSC4291 (room v12+): the `m.room.create` event carries **no** `room_id` —
    // it is derived from the event's own id (`$` swapped for `!`). Emitting it
    // here would be circular *and* self-defeating: the content hash covers the
    // whole PDU, `hashes` stays in the redacted event, and the reference hash is
    // taken over that — so a `room_id` in the PDU would leak into the event id
    // through `hashes`, and the room id could never be derived from an id that
    // already depends on it. On the wire the field is absent; non-federation
    // APIs re-introduce it from the id (same treatment as `event_id`, which is
    // absent on the federation wire for v3+).
    let omits_room_id =
        parts.event_type == "m.room.create" && crate::room_versions::room_version_at_least(parts.room_version, 12);
    if !omits_room_id {
        pdu.insert("room_id".to_string(), Value::String(parts.room_id.to_string()));
    }
    pdu.insert("sender".to_string(), Value::String(parts.sender.to_string()));
    pdu.insert("type".to_string(), Value::String(parts.event_type.to_string()));
    pdu.insert("content".to_string(), parts.content.clone());
    pdu.insert("origin_server_ts".to_string(), Value::Number(parts.origin_server_ts.into()));
    pdu.insert("origin".to_string(), Value::String(parts.origin.to_string()));
    pdu.insert("depth".to_string(), Value::Number(parts.depth.into()));
    pdu.insert(
        "prev_events".to_string(),
        Value::Array(parts.prev_events.iter().map(|id| Value::String(id.clone())).collect()),
    );
    pdu.insert(
        "auth_events".to_string(),
        Value::Array(parts.auth_events.iter().map(|id| Value::String(id.clone())).collect()),
    );

    if let Some(state_key) = parts.state_key {
        pdu.insert("state_key".to_string(), Value::String(state_key.to_string()));
    }

    if let Some(redacts) = parts.redacts {
        let content_carries_redacts = parts.content.get("redacts").is_some();
        if !redacts_in_content(parts.room_version) && !content_carries_redacts {
            pdu.insert("redacts".to_string(), Value::String(redacts.to_string()));
        }
    }

    Value::Object(pdu)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parts<'a>(room_version: &'a str, event_id: Option<&'a str>, content: &'a Value) -> PduParts<'a> {
        // Leaked once per test call: the slices only need to outlive the call.
        let prev: &'static [String] = Box::leak(vec!["$prev:example.com".to_string()].into_boxed_slice());
        let auth: &'static [String] = Box::leak(vec!["$create:example.com".to_string()].into_boxed_slice());
        PduParts {
            room_version,
            event_id,
            room_id: "!r:example.com",
            sender: "@u:example.com",
            event_type: "m.room.message",
            content,
            state_key: None,
            origin_server_ts: 1,
            origin: "example.com",
            depth: 5,
            prev_events: prev,
            auth_events: auth,
            redacts: None,
        }
    }

    #[test]
    fn event_id_is_a_field_only_for_v1_and_v2() {
        assert!(event_id_is_a_pdu_field("1"));
        assert!(event_id_is_a_pdu_field("2"));
        for version in ["3", "4", "10", "11", "12"] {
            assert!(!event_id_is_a_pdu_field(version), "v{version} must not carry event_id");
        }
    }

    #[test]
    fn v3_plus_pdu_omits_event_id_and_user_id() {
        let content = json!({"msgtype": "m.text", "body": "hi"});
        let pdu = build_pdu(&parts("10", Some("$placeholder:example.com"), &content));
        let obj = pdu.as_object().unwrap();

        assert!(!obj.contains_key("event_id"), "v10 PDUs must not carry event_id: {pdu}");
        assert!(!obj.contains_key("user_id"), "user_id is not a PDU field: {pdu}");
        assert_eq!(obj["room_id"], json!("!r:example.com"));
        assert_eq!(obj["type"], json!("m.room.message"));
        assert_eq!(obj["depth"], json!(5));
        assert_eq!(obj["prev_events"], json!(["$prev:example.com"]));
        assert_eq!(obj["auth_events"], json!(["$create:example.com"]));
        assert_eq!(obj["origin"], json!("example.com"));
        // room_id, sender, type, content, origin_server_ts, origin, depth, prev_events, auth_events
        assert_eq!(obj.len(), 9, "unexpected field set: {pdu}");
    }

    #[test]
    fn v1_and_v2_pdus_carry_the_server_assigned_event_id() {
        let content = json!({"body": "hi"});
        let pdu = build_pdu(&parts("1", Some("$0:domain"), &content));
        assert_eq!(pdu["event_id"], json!("$0:domain"));

        let pdu = build_pdu(&parts("1", None, &content));
        assert!(pdu.get("event_id").is_none());
    }

    #[test]
    fn redaction_target_placement_follows_the_room_version() {
        let pre_v11_content = json!({"reason": "spam"});
        let mut pre_v11 = parts("10", None, &pre_v11_content);
        pre_v11.event_type = "m.room.redaction";
        pre_v11.redacts = Some("$target:example.com");
        let pdu = build_pdu(&pre_v11);
        assert_eq!(pdu["redacts"], json!("$target:example.com"));

        // v11+ keeps it in content and must not duplicate it at the top level.
        let v11_content = json!({"reason": "spam", "redacts": "$target:example.com"});
        let mut v11 = parts("11", None, &v11_content);
        v11.event_type = "m.room.redaction";
        v11.redacts = Some("$target:example.com");
        let pdu = build_pdu(&v11);
        assert!(pdu.get("redacts").is_none(), "v11+ must not add a top-level redacts: {pdu}");
        assert_eq!(pdu["content"]["redacts"], json!("$target:example.com"));
    }

    #[test]
    fn state_events_gain_their_state_key() {
        let content = json!({"topic": "hi"});
        let mut state = parts("10", None, &content);
        state.event_type = "m.room.topic";
        state.state_key = Some("");
        let pdu = build_pdu(&state);
        assert_eq!(pdu["state_key"], json!(""));
    }

    // ── D-6 / MSC4291: the v12+ create PDU carries no room_id ──────────────

    /// The v12 create event must **omit** `room_id`: it is derived from the
    /// event's own id, and `hashes` (which stays in the redacted event) covers
    /// the whole PDU, so an emitted `room_id` would leak into the reference hash
    /// through the content hash — making the derivation circular. Every other
    /// version, and every other event type, keeps the field.
    ///
    /// The "identity must not depend on room_id" half lives in
    /// `synapse-federation::event_finalize`, which owns the finalize path
    /// (synapse-common cannot depend on it).
    #[test]
    fn v12_create_pdu_omits_room_id_and_nothing_else_does() {
        let create_content = json!({"creator": "@u:example.com", "room_version": "12"});
        let message_content = json!({"body": "hi"});

        let mut v12_create = parts("12", None, &create_content);
        v12_create.event_type = "m.room.create";
        v12_create.state_key = Some("");
        let pdu = build_pdu(&v12_create);
        assert!(pdu.get("room_id").is_none(), "v12 create must not carry room_id: {pdu}");
        assert_eq!(pdu["type"], json!("m.room.create"));

        // v11 create keeps it.
        let mut v11_create = parts("11", None, &create_content);
        v11_create.event_type = "m.room.create";
        v11_create.state_key = Some("");
        assert_eq!(build_pdu(&v11_create)["room_id"], json!("!r:example.com"));

        // v12 non-create keeps it.
        let mut v12_message = parts("12", None, &message_content);
        v12_message.event_type = "m.room.message";
        assert_eq!(build_pdu(&v12_message)["room_id"], json!("!r:example.com"));

        // v13 is "12 and later" for this rule even though it is parse-only.
        let mut v13_create = parts("13", None, &create_content);
        v13_create.event_type = "m.room.create";
        v13_create.state_key = Some("");
        assert!(build_pdu(&v13_create).get("room_id").is_none(), "v13 follows the 12+ rule");
    }
}
