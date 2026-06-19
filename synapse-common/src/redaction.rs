//! Matrix event redaction utilities (P0-05/06/07).
//!
//! This module is the single source of truth for:
//! - The content field retention table used when redacting events (v1-v10).
//! - The top-level field whitelist used when computing event content hashes.
//! - Extracting the `redacts` target from a redaction event across room
//!   versions.
//!
//! References:
//! - Matrix Specification v1.18, "Redactions" sections per room version.
//! - `element-hq/synapse` `synapse/events/utils.py` `prune_event`.
//! - MSC2174/MSC3820 for v11+ redaction format (now enabled for creation;
//!   see `room_versions::SUPPORTED_ROOM_VERSIONS`).

use serde_json::{Map, Value};

/// Top-level event fields that survive redaction (v1-v10).
///
/// Used by `redact_event_for_hash` and the runtime redaction path.  Note that
/// `prev_state` and `membership` are intentionally absent — they are not valid
/// top-level PDU fields and were incorrectly included in the previous
/// implementation (P0-07).
pub const CANONICAL_JSON_TOP_LEVEL_FIELDS: &[&str] = &[
    "event_id",
    "type",
    "room_id",
    "sender",
    "state_key",
    "content",
    "hashes",
    "signatures",
    "depth",
    "prev_events",
    "auth_events",
    "origin",
    "origin_server_ts",
];

/// Returns the set of content keys to retain after redaction for the given
/// event type (v1-v10 redaction rules).
///
/// Returns an empty slice for unrecognised event types, which means all
/// content fields are stripped.  This matches the Matrix specification and
/// Synapse behaviour: `m.room.message` is NOT specially handled, so its
/// content is fully stripped after redaction.
pub fn allowed_content_keys(event_type: &str) -> &'static [&'static str] {
    match event_type {
        "m.room.member" => &["membership", "third_party_invite", "displayname", "avatar_url"],
        "m.room.create" => &["creator", "room_version", "type", "m.federate"],
        "m.room.join_rules" => &["join_rule", "allow"],
        "m.room.power_levels" => &[
            "users",
            "users_default",
            "events",
            "events_default",
            "state_default",
            "ban",
            "kick",
            "redact",
            "invite",
            "notifications",
        ],
        "m.room.history_visibility" => &["history_visibility"],
        "m.room.encrypted" => &["algorithm", "ciphertext", "session_id", "sender_key", "device_id"],
        "m.room.third_party_invite" => &["displayname", "key_validity_url", "key_signature", "public_key"],
        _ => &[],
    }
}

/// Strips a JSON object's content to only the redaction-safe keys for the
/// given event type.  Returns a new JSON object.
///
/// For event types with no special-cased retention table, the result is an
/// empty object `{}`.  This is the runtime redaction path used by
/// `EventStorage::redact_event_content` (P0-06).
pub fn redact_content(event_type: &str, content: &Value) -> Value {
    let allowed = allowed_content_keys(event_type);
    let Some(obj) = content.as_object() else {
        // Non-object content (e.g. null, array) is replaced with an empty object.
        return Value::Object(Map::new());
    };

    if allowed.is_empty() {
        return Value::Object(Map::new());
    }

    let mut retained = Map::new();
    for &key in allowed {
        if let Some(value) = obj.get(key) {
            retained.insert(key.to_string(), value.clone());
        }
    }
    Value::Object(retained)
}

/// Produces a redacted copy of an event for content-hash computation.
///
/// This strips both the top-level fields (keeping only
/// `CANONICAL_JSON_TOP_LEVEL_FIELDS`) and the content fields (keeping only
/// `allowed_content_keys` for the event type).  The input is not mutated.
///
/// Used by `synapse_federation::signing::compute_event_content_hash`
/// (P0-07).  The previous implementation included illegal top-level fields
/// (`prev_state`, `membership`) and was missing `notifications` from
/// `m.room.power_levels`; both are fixed here.
pub fn redact_event_for_hash(event: &Value) -> Value {
    let mut redacted = event.clone();

    // Strip top-level fields not in the canonical whitelist.
    if let Some(obj) = redacted.as_object_mut() {
        obj.retain(|k, _| CANONICAL_JSON_TOP_LEVEL_FIELDS.contains(&k.as_str()));
    }

    // Strip content fields per event type.
    let event_type = redacted.get("type").and_then(|t| t.as_str()).unwrap_or("");

    let allowed = allowed_content_keys(event_type);
    if let Some(content) = redacted.get_mut("content").and_then(|c| c.as_object_mut()) {
        content.retain(|k, _| allowed.contains(&k.as_str()));
    }

    redacted
}

/// Extracts the `redacts` target event ID from a redaction event.
///
/// For room versions 1-10, `redacts` is a top-level field of the PDU.  For
/// v11+ (MSC2174/MSC3820), `redacts` lives in `content.redacts`.  This helper
/// checks both locations so callers do not need to know the room version.
///
/// Returns `None` if neither location contains a string value.
pub fn extract_redacts(event: &Value) -> Option<&str> {
    // v1-v10: top-level `redacts`.
    if let Some(redacts) = event.get("redacts").and_then(|v| v.as_str()) {
        return Some(redacts);
    }
    // v11+: `content.redacts` (MSC2174/MSC3820).
    event.get("content").and_then(|c| c.get("redacts")).and_then(|v| v.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_allowed_content_keys_known_types() {
        assert_eq!(
            allowed_content_keys("m.room.member"),
            &["membership", "third_party_invite", "displayname", "avatar_url"]
        );
        assert_eq!(
            allowed_content_keys("m.room.power_levels"),
            &[
                "users",
                "users_default",
                "events",
                "events_default",
                "state_default",
                "ban",
                "kick",
                "redact",
                "invite",
                "notifications"
            ]
        );
    }

    #[test]
    fn test_allowed_content_keys_unknown_type_returns_empty() {
        assert!(allowed_content_keys("m.room.message").is_empty());
        assert!(allowed_content_keys("m.reaction").is_empty());
        assert!(allowed_content_keys("com.example.custom").is_empty());
    }

    #[test]
    fn test_redact_content_strips_unknown_fields_for_member() {
        let content = json!({
            "membership": "join",
            "displayname": "Alice",
            "avatar_url": "mxc://example.com/abc",
            "reason": "should be stripped",
            "extra": "should be stripped"
        });
        let redacted = redact_content("m.room.member", &content);
        assert_eq!(redacted["membership"], "join");
        assert_eq!(redacted["displayname"], "Alice");
        assert_eq!(redacted["avatar_url"], "mxc://example.com/abc");
        assert!(redacted.get("reason").is_none());
        assert!(redacted.get("extra").is_none());
    }

    #[test]
    fn test_redact_content_strips_all_fields_for_message() {
        let content = json!({
            "body": "Hello",
            "msgtype": "m.text",
            "url": "mxc://example.com/file"
        });
        let redacted = redact_content("m.room.message", &content);
        assert!(redacted.as_object().map(|o| o.is_empty()).unwrap_or(true));
    }

    #[test]
    fn test_redact_content_power_levels_keeps_notifications() {
        let content = json!({
            "users": {"@a:example.com": 100},
            "notifications": {"room": 50},
            "extra": "stripped"
        });
        let redacted = redact_content("m.room.power_levels", &content);
        assert_eq!(redacted["users"]["@a:example.com"], 100);
        assert_eq!(redacted["notifications"]["room"], 50);
        assert!(redacted.get("extra").is_none());
    }

    #[test]
    fn test_redact_content_non_object_returns_empty_object() {
        let redacted = redact_content("m.room.member", &json!("string"));
        assert!(redacted.is_object());
        assert!(redacted.as_object().unwrap().is_empty());
    }

    #[test]
    fn test_redact_event_for_hash_strips_top_level_fields() {
        let event = json!({
            "event_id": "$abc",
            "type": "m.room.message",
            "room_id": "!room:example.com",
            "sender": "@user:example.com",
            "content": {"body": "hello"},
            "origin_server_ts": 1234,
            "unsigned": {"age": 10},
            "redacts": "$target",
            "prev_state": [],
            "membership": "join"
        });
        let redacted = redact_event_for_hash(&event);
        assert!(redacted.get("unsigned").is_none(), "unsigned should be stripped");
        assert!(redacted.get("redacts").is_none(), "redacts should be stripped at top level for hash");
        assert!(redacted.get("prev_state").is_none(), "prev_state is not a valid top-level field");
        assert!(redacted.get("membership").is_none(), "membership is not a valid top-level field");
        assert_eq!(redacted["event_id"], "$abc");
        assert_eq!(redacted["type"], "m.room.message");
        assert_eq!(redacted["room_id"], "!room:example.com");
    }

    #[test]
    fn test_redact_event_for_hash_strips_content_for_message() {
        let event = json!({
            "type": "m.room.message",
            "content": {"body": "hello", "msgtype": "m.text"}
        });
        let redacted = redact_event_for_hash(&event);
        assert!(redacted["content"].as_object().unwrap().is_empty());
    }

    #[test]
    fn test_redact_event_for_hash_keeps_power_levels_fields() {
        let event = json!({
            "type": "m.room.power_levels",
            "content": {
                "users": {"@a:example.com": 100},
                "notifications": {"room": 50},
                "ban": 50,
                "extra": "stripped"
            }
        });
        let redacted = redact_event_for_hash(&event);
        assert_eq!(redacted["content"]["users"]["@a:example.com"], 100);
        assert_eq!(redacted["content"]["notifications"]["room"], 50);
        assert_eq!(redacted["content"]["ban"], 50);
        assert!(redacted["content"].get("extra").is_none());
    }

    #[test]
    fn test_extract_redacts_top_level_v1_v10() {
        let event = json!({
            "type": "m.room.redaction",
            "redacts": "$target:example.com",
            "content": {"reason": "spam"}
        });
        assert_eq!(extract_redacts(&event), Some("$target:example.com"));
    }

    #[test]
    fn test_extract_redacts_content_v11_plus() {
        let event = json!({
            "type": "m.room.redaction",
            "content": {"reason": "spam", "redacts": "$target:example.com"}
        });
        assert_eq!(extract_redacts(&event), Some("$target:example.com"));
    }

    #[test]
    fn test_extract_redacts_missing_returns_none() {
        let event = json!({
            "type": "m.room.redaction",
            "content": {"reason": "spam"}
        });
        assert_eq!(extract_redacts(&event), None);
    }

    #[test]
    fn test_extract_redacts_top_level_takes_precedence() {
        // If both locations are present (malformed), top-level wins per v1-v10.
        let event = json!({
            "redacts": "$top:example.com",
            "content": {"redacts": "$content:example.com"}
        });
        assert_eq!(extract_redacts(&event), Some("$top:example.com"));
    }
}
