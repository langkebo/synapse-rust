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

/// Returns `true` when a redaction event must carry its target in
/// `content.redacts` instead of the top-level `redacts` PDU field.
///
/// Room versions 11+ use the MSC2174/MSC3820 format; v1-v10 use the top-level
/// field.  Unparsable version strings fall back to the v1-v10 shape so that
/// unknown or experimental versions keep the historical behaviour.
pub fn redacts_in_content(room_version: &str) -> bool {
    room_version.parse::<u32>().map(|version| version >= 11).unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Room-version-aware redaction (U-13 §6.6 step 1)
// ---------------------------------------------------------------------------
//
// The tables above (`allowed_content_keys` / `redact_event_for_hash`) are the
// *unversioned* path still used by the federation signature and runtime
// redaction call sites.  They model a single retention table and are therefore
// only correct for a subset of room versions;  see the U-13 finding recorded in
// `docs/audit/REMAINING_ISSUES_VERIFICATION_AND_OPTIMIZATION_PLAN_2026-09-25.md`
// §6.6.  `redact_event` below is the room-version-aware implementation (the
// single source of truth for the reference-hash/event-ID algorithm);  the
// legacy call sites are migrated onto it in step 2 of the same item.

/// Redaction-relevant per-room-version flags.
///
/// Field names mirror upstream Synapse's `RoomVersion` flags
/// (`synapse/api/room_versions.py`, release-v1.161) so the two can be diffed
/// by name during review.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RedactionRules {
    /// Room versions 11+ use the MSC2174/MSC3820 algorithms: the top-level
    /// `origin`/`membership`/`prev_state` fields are no longer protected,
    /// `m.room.create` keeps all of its content, `m.room.redaction` keeps
    /// `content.redacts` and `m.room.power_levels` keeps `invite`.
    pub updated_redaction_rules: bool,
    /// Room versions 8+ (MSC3083): `m.room.join_rules` keeps `content.allow`.
    pub restricted_join_rule: bool,
    /// Room versions 9+ (MSC3083 auth fix): `m.room.member` keeps
    /// `content.join_authorised_via_users_server`.
    pub restricted_join_rule_fix: bool,
    /// Room versions 11+ (MSC2176): `m.room.create` has no `creator`, and its
    /// content is never redacted.
    pub implicit_room_creator: bool,
    /// Room versions 1-5 only: `m.room.aliases` is a state event and keeps
    /// `content.aliases`.  Room version 6 removed the special case.
    pub special_case_aliases_auth: bool,
    /// Room versions using MSC4291: `room_id` is derived from the event ID and
    /// is therefore dropped from `m.room.create` when redacting.
    pub room_ids_as_hashes: bool,
}

impl RedactionRules {
    /// v1-v5: the v1 redaction algorithm (`v1-redactions` spec fragment).
    const fn legacy() -> Self {
        Self {
            updated_redaction_rules: false,
            restricted_join_rule: false,
            restricted_join_rule_fix: false,
            implicit_room_creator: false,
            special_case_aliases_auth: true,
            room_ids_as_hashes: false,
        }
    }

    /// v11+: the MSC2174/MSC3820 algorithm (`v11-redactions` spec fragment).
    const fn updated() -> Self {
        Self {
            updated_redaction_rules: true,
            restricted_join_rule: true,
            restricted_join_rule_fix: true,
            implicit_room_creator: true,
            special_case_aliases_auth: false,
            room_ids_as_hashes: false,
        }
    }
}

/// Returns the redaction rules for a room version, or `None` when the version
/// is unknown/unsupported.
///
/// Fail-closed: an unrecognised version must never silently fall back to
/// another version's rules, because the result feeds event IDs (and therefore
/// event identity).  v13 is deliberately absent: it is parse-only in
/// `room_versions::SUPPORTED_ROOM_VERSIONS` and its MSC4291-derived redaction
/// semantics have not been pinned against upstream yet.
pub fn redaction_rules(room_version: &str) -> Option<RedactionRules> {
    match room_version {
        "1" | "2" | "3" | "4" | "5" => Some(RedactionRules::legacy()),
        // v6: `m.room.aliases` stopped being a state event (spec v6 changelog).
        "6" | "7" => Some(RedactionRules { special_case_aliases_auth: false, ..RedactionRules::legacy() }),
        // v8: MSC3083 restricted join rules.
        "8" => Some(RedactionRules {
            special_case_aliases_auth: false,
            restricted_join_rule: true,
            ..RedactionRules::legacy()
        }),
        // v9-v10: MSC3083 auth fix.
        "9" | "10" => Some(RedactionRules {
            special_case_aliases_auth: false,
            restricted_join_rule: true,
            restricted_join_rule_fix: true,
            ..RedactionRules::legacy()
        }),
        // v11: MSC2174/MSC3820.
        "11" => Some(RedactionRules::updated()),
        // v12: Synapse release-v1.161 groups V12 with MSC4291 rooms
        // (`test_redact_m_room_create`), i.e. v11 rules plus the room_id drop
        // for `m.room.create`.  Reconciling this with this repository's own
        // MSC4239-based v12 description is a step-2 prerequisite; that is
        // recorded as an open item in the U-13 section of the plan doc.
        "12" => Some(RedactionRules { room_ids_as_hashes: true, ..RedactionRules::updated() }),
        _ => None,
    }
}

/// Errors from [`redact_event`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RedactionError {
    /// The room version is unknown/unsupported.
    #[error("unknown or unsupported room version: {0}")]
    UnknownRoomVersion(String),
    /// The event JSON is not an object.
    #[error("event is not a JSON object")]
    NotAnObject,
    /// A required top-level field is absent.
    #[error("event is missing required field `{0}`")]
    MissingField(&'static str),
    /// A required top-level field has the wrong JSON type.
    #[error("event field `{0}` has the wrong type")]
    FieldType(&'static str),
}

/// Redacts `event` according to the redaction algorithm of `room_version`.
///
/// Mirrors upstream Synapse `redact()` (`rust/src/events/utils.rs`,
/// release-v1.161) exactly for the stable room versions 1-12, which is what the
/// reference hash / event ID is computed over.  Two upstream branches are
/// deliberately not modelled because no stable room version uses them:
/// `msc3389_relation_redactions` (experimental relation redactions) and
/// `msc4242_state_dags` (`prev_state_events` instead of `auth_events`).
pub fn redact_event(room_version: &str, event: &Value) -> Result<Value, RedactionError> {
    let rules =
        redaction_rules(room_version).ok_or_else(|| RedactionError::UnknownRoomVersion(room_version.to_string()))?;

    let obj = event.as_object().ok_or(RedactionError::NotAnObject)?;
    let event_type = obj
        .get("type")
        .ok_or(RedactionError::MissingField("type"))?
        .as_str()
        .ok_or(RedactionError::FieldType("type"))?;
    let content = obj.get("content").and_then(Value::as_object);

    let mut redacted_content = Map::new();
    fn copy_key(content: Option<&Map<String, Value>>, key: &str, out: &mut Map<String, Value>) {
        if let Some(value) = content.and_then(|c| c.get(key)) {
            out.insert(key.to_string(), value.clone());
        }
    }
    let copy = |key: &str, out: &mut Map<String, Value>| copy_key(content, key, out);

    match event_type {
        "m.room.member" => {
            copy("membership", &mut redacted_content);
            if rules.restricted_join_rule_fix {
                copy("join_authorised_via_users_server", &mut redacted_content);
            }
            if rules.updated_redaction_rules {
                // v11 keeps `third_party_invite`, but only its `signed` key.
                if let Some(tpi) = content.and_then(|c| c.get("third_party_invite")).and_then(Value::as_object) {
                    let mut kept = Map::new();
                    if let Some(signed) = tpi.get("signed") {
                        kept.insert("signed".to_string(), signed.clone());
                    }
                    redacted_content.insert("third_party_invite".to_string(), Value::Object(kept));
                }
            }
        }
        "m.room.create" => {
            if rules.updated_redaction_rules {
                if let Some(content) = content {
                    for (key, value) in content {
                        redacted_content.insert(key.clone(), value.clone());
                    }
                }
            }
            if !rules.implicit_room_creator {
                copy("creator", &mut redacted_content);
            }
        }
        "m.room.join_rules" => {
            copy("join_rule", &mut redacted_content);
            if rules.restricted_join_rule {
                copy("allow", &mut redacted_content);
            }
        }
        "m.room.power_levels" => {
            for key in ["users", "users_default", "events", "events_default", "state_default", "ban", "kick", "redact"]
            {
                copy(key, &mut redacted_content);
            }
            if rules.updated_redaction_rules {
                copy("invite", &mut redacted_content);
            }
        }
        "m.room.aliases" if rules.special_case_aliases_auth => copy("aliases", &mut redacted_content),
        "m.room.history_visibility" => copy("history_visibility", &mut redacted_content),
        "m.room.redaction" if rules.updated_redaction_rules => copy("redacts", &mut redacted_content),
        _ => {}
    }

    let mut allowed = vec![
        "event_id",
        "sender",
        "room_id",
        "hashes",
        "signatures",
        "content",
        "type",
        "state_key",
        "depth",
        "prev_events",
        "origin_server_ts",
        "auth_events",
    ];
    if !rules.updated_redaction_rules {
        allowed.extend(["prev_state", "membership", "origin"]);
    }
    if event_type == "m.room.create" && rules.room_ids_as_hashes {
        // room_id is derived from the event ID in MSC4291 rooms, so it must not
        // take part in the hash.
        allowed.retain(|key| *key != "room_id");
    }

    let mut redacted = Map::new();
    for key in allowed {
        if let Some(value) = obj.get(key) {
            redacted.insert(key.to_string(), value.clone());
        }
    }
    redacted.insert("content".to_string(), Value::Object(redacted_content));

    // Copy over the known-good `unsigned` keys only (Synapse: age_ts,
    // replaces_state).  The reference hash strips `unsigned` afterwards, but
    // keeping this identical to upstream keeps the two functions diffable.
    if let Some(unsigned) = obj.get("unsigned").and_then(Value::as_object) {
        let mut kept = Map::new();
        for key in ["age_ts", "replaces_state"] {
            if let Some(value) = unsigned.get(key) {
                kept.insert(key.to_string(), value.clone());
            }
        }
        redacted.insert("unsigned".to_string(), Value::Object(kept));
    }

    Ok(Value::Object(redacted))
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

    #[test]
    fn test_redacts_in_content_v11_and_above() {
        assert!(redacts_in_content("11"));
        assert!(redacts_in_content("12"));
        assert!(redacts_in_content("13"));
    }

    #[test]
    fn test_redacts_in_content_v10_and_below() {
        assert!(!redacts_in_content("10"));
        assert!(!redacts_in_content("1"));
    }

    #[test]
    fn test_redacts_in_content_unparsable_falls_back_to_top_level() {
        // Non-numeric versions (unknown/experimental) keep the v1-v10 shape.
        assert!(!redacts_in_content("org.example.unknown"));
        assert!(!redacts_in_content(""));
    }
}

/// Version-table tests for [`redact_event`] (U-13 §6.6 step 1).
///
/// Every expectation below is transcribed from upstream Synapse's own unit
/// tests in `rust/src/events/utils.rs` (release-v1.161), which are the
/// independent oracle for the per-version tables: a wrong flag shows up here as
/// a wrong redacted JSON, not just a wrong event ID.
#[cfg(test)]
mod room_version_redaction_tests {
    use super::*;
    use serde_json::json;

    fn content_of(redacted: &Value) -> Value {
        redacted.get("content").cloned().unwrap_or(Value::Null)
    }

    #[test]
    fn member_content_matches_synapse_per_version() {
        let event = json!({
            "type": "m.room.member",
            "content": {
                "unknown_key": "unknown_value",
                "membership": "join",
                "join_authorised_via_users_server": "server",
                "third_party_invite": {"signed": {}},
            },
        });

        // v1-v8: only `membership` (restricted_join_rule_fix lands in v9).
        for version in ["1", "2", "3", "4", "5", "6", "7", "8"] {
            assert_eq!(
                content_of(&redact_event(version, &event).unwrap()),
                json!({"membership": "join"}),
                "v{version}"
            );
        }
        // v9-v10: + join_authorised_via_users_server.
        for version in ["9", "10"] {
            assert_eq!(
                content_of(&redact_event(version, &event).unwrap()),
                json!({"membership": "join", "join_authorised_via_users_server": "server"}),
                "v{version}"
            );
        }
        // v11+: + third_party_invite, reduced to its `signed` key.
        for version in ["11", "12"] {
            assert_eq!(
                content_of(&redact_event(version, &event).unwrap()),
                json!({
                    "membership": "join",
                    "join_authorised_via_users_server": "server",
                    "third_party_invite": {"signed": {}},
                }),
                "v{version}"
            );
        }
    }

    #[test]
    fn create_content_and_room_id_match_synapse_per_version() {
        let event = json!({
            "type": "m.room.create",
            "room_id": "!roomid",
            "content": {"unknown_key": "unknown_value", "other_key": "value", "creator": "user"},
        });

        // v1-v10: only `creator` survives, room_id is protected.
        for version in ["1", "2", "3", "4", "5", "6", "7", "8", "9", "10"] {
            let redacted = redact_event(version, &event).unwrap();
            assert_eq!(content_of(&redacted), json!({"creator": "user"}), "v{version}");
            assert_eq!(redacted.get("room_id"), Some(&json!("!roomid")), "v{version}");
        }
        // v11: whole content survives, room_id still protected.
        let v11 = redact_event("11", &event).unwrap();
        assert_eq!(content_of(&v11), json!({"unknown_key": "unknown_value", "other_key": "value", "creator": "user"}));
        assert_eq!(v11.get("room_id"), Some(&json!("!roomid")));
        // v12 (MSC4291): whole content survives, room_id dropped.
        let v12 = redact_event("12", &event).unwrap();
        assert_eq!(content_of(&v12), json!({"unknown_key": "unknown_value", "other_key": "value", "creator": "user"}));
        assert_eq!(v12.get("room_id"), None);
    }

    #[test]
    fn join_rules_content_matches_synapse_per_version() {
        let event = json!({
            "type": "m.room.join_rules",
            "content": {"unknown_key": "unknown_value", "join_rule": "invite", "allow": "user"},
        });

        for version in ["1", "2", "3", "4", "5", "6", "7"] {
            assert_eq!(
                content_of(&redact_event(version, &event).unwrap()),
                json!({"join_rule": "invite"}),
                "v{version}"
            );
        }
        for version in ["8", "9", "10", "11", "12"] {
            assert_eq!(
                content_of(&redact_event(version, &event).unwrap()),
                json!({"join_rule": "invite", "allow": "user"}),
                "v{version}"
            );
        }
    }

    #[test]
    fn power_levels_content_matches_synapse_per_version() {
        let event = json!({
            "type": "m.room.power_levels",
            "content": {
                "unknown_key": "unknown_value",
                "users": {}, "users_default": {}, "events": {}, "events_default": {},
                "state_default": {}, "ban": {}, "kick": {}, "redact": {}, "invite": {},
                "notifications": {"room": 50},
            },
        });
        let pre_v11 = json!({
            "users": {}, "users_default": {}, "events": {}, "events_default": {},
            "state_default": {}, "ban": {}, "kick": {}, "redact": {},
        });
        let post_v11 = json!({
            "users": {}, "users_default": {}, "events": {}, "events_default": {},
            "state_default": {}, "ban": {}, "kick": {}, "redact": {}, "invite": {},
        });

        for version in ["1", "2", "3", "4", "5", "6", "7", "8", "9", "10"] {
            assert_eq!(content_of(&redact_event(version, &event).unwrap()), pre_v11, "v{version}");
        }
        for version in ["11", "12"] {
            assert_eq!(content_of(&redact_event(version, &event).unwrap()), post_v11, "v{version}");
        }
    }

    #[test]
    fn aliases_content_matches_synapse_per_version() {
        let event = json!({
            "type": "m.room.aliases",
            "content": {"unknown_key": "unknown_value", "aliases": {}},
        });

        // v1-v5 special-case `m.room.aliases`; v6 dropped it.
        for version in ["1", "2", "3", "4", "5"] {
            assert_eq!(content_of(&redact_event(version, &event).unwrap()), json!({"aliases": {}}), "v{version}");
        }
        for version in ["6", "7", "8", "9", "10", "11", "12"] {
            assert_eq!(content_of(&redact_event(version, &event).unwrap()), json!({}), "v{version}");
        }
    }

    #[test]
    fn redaction_event_content_matches_synapse_per_version() {
        let event = json!({
            "type": "m.room.redaction",
            "content": {"unknown_key": "unknown_value", "redacts": "event"},
        });

        for version in ["1", "2", "3", "4", "5", "6", "7", "8", "9", "10"] {
            assert_eq!(content_of(&redact_event(version, &event).unwrap()), json!({}), "v{version}");
        }
        for version in ["11", "12"] {
            assert_eq!(content_of(&redact_event(version, &event).unwrap()), json!({"redacts": "event"}), "v{version}");
        }
    }

    #[test]
    fn history_visibility_is_protected_in_every_version() {
        let event = json!({
            "type": "m.room.history_visibility",
            "content": {"unknown_key": "unknown_value", "history_visibility": "visibility"},
        });

        for version in ["1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12"] {
            assert_eq!(
                content_of(&redact_event(version, &event).unwrap()),
                json!({"history_visibility": "visibility"}),
                "v{version}"
            );
        }
    }

    #[test]
    fn unknown_event_types_have_their_content_stripped() {
        let message = json!({
            "type": "m.room.message",
            "content": {"msgtype": "m.text", "body": "hi"},
        });
        for version in ["1", "4", "9", "10", "11", "12"] {
            assert_eq!(content_of(&redact_event(version, &message).unwrap()), json!({}), "v{version}");
        }
    }

    #[test]
    fn top_level_fields_match_synapse_per_version() {
        let event = json!({
            "type": "m.room.message",
            "prev_state": {},
            "membership": {},
            "origin": "some_place",
            "content": {},
        });

        let pre_v11 = json!({
            "type": "m.room.message",
            "prev_state": {}, "membership": {}, "origin": "some_place", "content": {},
        });
        let post_v11 = json!({"type": "m.room.message", "content": {}});

        for version in ["1", "2", "3", "4", "5", "6", "7", "8", "9", "10"] {
            assert_eq!(redact_event(version, &event).unwrap(), pre_v11, "v{version}");
        }
        for version in ["11", "12"] {
            assert_eq!(redact_event(version, &event).unwrap(), post_v11, "v{version}");
        }
    }

    #[test]
    fn only_known_unsigned_keys_survive() {
        let event = json!({
            "type": "m.room.message",
            "content": {},
            "unsigned": {"age_ts": 1, "replaces_state": "$x", "transaction_id": "t", "prev_content": {}},
        });
        let redacted = redact_event("10", &event).unwrap();
        assert_eq!(redacted.get("unsigned"), Some(&json!({"age_ts": 1, "replaces_state": "$x"})));

        let without_unsigned = json!({"type": "m.room.message", "content": {}});
        assert_eq!(redact_event("10", &without_unsigned).unwrap().get("unsigned"), None);
    }

    #[test]
    fn missing_type_or_non_object_event_fails_closed() {
        assert_eq!(redact_event("10", &json!({"content": {}})).unwrap_err(), RedactionError::MissingField("type"));
        assert_eq!(
            redact_event("10", &json!({"type": 5, "content": {}})).unwrap_err(),
            RedactionError::FieldType("type")
        );
        assert_eq!(redact_event("10", &json!([])).unwrap_err(), RedactionError::NotAnObject);
    }

    #[test]
    fn unsupported_room_versions_fail_closed() {
        let event = json!({"type": "m.room.message", "content": {}});
        for version in ["0", "13", "14", "org.example.unknown", ""] {
            assert_eq!(
                redact_event(version, &event).unwrap_err(),
                RedactionError::UnknownRoomVersion(version.to_string()),
                "v{version}"
            );
        }
    }

    #[test]
    fn rules_table_is_fail_closed_and_monotone() {
        assert!(redaction_rules("13").is_none());
        assert!(redaction_rules("org.matrix.msc4242").is_none());

        // Compatibility flags never regress as the room version increases.
        assert!(redaction_rules("4").unwrap().special_case_aliases_auth);
        assert!(!redaction_rules("6").unwrap().special_case_aliases_auth);
        assert!(!redaction_rules("7").unwrap().restricted_join_rule);
        assert!(redaction_rules("8").unwrap().restricted_join_rule);
        assert!(!redaction_rules("8").unwrap().restricted_join_rule_fix);
        assert!(redaction_rules("9").unwrap().restricted_join_rule_fix);
        assert!(!redaction_rules("10").unwrap().updated_redaction_rules);
        assert!(redaction_rules("11").unwrap().updated_redaction_rules);
        assert!(!redaction_rules("11").unwrap().room_ids_as_hashes);
        assert!(redaction_rules("12").unwrap().room_ids_as_hashes);
    }
}
