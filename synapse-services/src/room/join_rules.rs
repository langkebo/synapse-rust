//! Canonical parser for the MSC3083 `allow` array of a `m.room.join_rules`
//! state event.
//!
//! # Why this module exists
//!
//! Two call sites used to parse the same `allow` array with two different
//! semantics:
//!
//! - [`crate::room::membership::service`] — the authorization gate for
//!   restricted joins — filtered entries by `type == m.room_membership`,
//!   validated the room-ID syntax, then deduped and sorted.
//! - [`crate::room::summary::service`] — the informational `allowed_room_ids`
//!   field of `/summary` — accepted any entry carrying a string `room_id`,
//!   skipped validation, and preserved declaration order.
//!
//! Same input, two answers: a latent drift source recorded in
//! `docs/audit/AUDIT_SUMMARY_2026-09-12.md` §3 and
//! `docs/audit/sdk-encapsulation-audit.md` §8. Both call sites now go through
//! [`extract_allowed_join_rooms`]; `/summary` adds only the join-rule gate in
//! [`extract_allowed_room_ids`].
//!
//! The semantics are fail-closed by construction: malformed input yields fewer
//! rooms, never more.

use serde_json::Value;

/// Join rules whose `allow` array grants join rights.
pub(crate) const RESTRICTED_JOIN_RULES: [&str; 2] = ["restricted", "knock_restricted"];

/// Extract allowed room IDs from the `allow` array of a `m.room.join_rules`
/// state event. For `restricted` / `knock_restricted` rules this returns the
/// list of rooms whose `m.room_membership` grants join rights as per MSC3083.
/// Returns deduped room IDs, validated for basic syntax. Entries whose `type`
/// is not `m.room_membership` (or missing, which defaults to that type) are
/// ignored. Malformed IDs are silently dropped (fail-closed).
///
/// The output is sorted so downstream consumers (and the `/summary` wire
/// payload) are deterministic instead of declaration-order dependent.
pub(crate) fn extract_allowed_join_rooms(content: &Value) -> Vec<String> {
    let allow = match content.get("allow").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => return Vec::new(),
    };

    let mut rooms: Vec<String> = allow
        .iter()
        .filter_map(|entry| {
            // Skip entries with an explicit non-membership type.
            let typ = entry.get("type").and_then(|v| v.as_str()).unwrap_or("m.room_membership");
            if typ != "m.room_membership" {
                return None;
            }
            let room_id = entry.get("room_id").and_then(|v| v.as_str())?;
            if !is_valid_matrix_id(room_id) {
                return None;
            }
            Some(room_id.to_string())
        })
        .collect();

    rooms.sort_unstable();
    rooms.dedup();
    rooms
}

/// The `/summary` projection of [`extract_allowed_join_rooms`].
///
/// Returns `Some(room_ids)` when the join rule is `restricted` or
/// `knock_restricted` (per Matrix v1.15 `/summary` spec), or `None` for any
/// other join rule. When the join rule is restricted but the `allow` array is
/// missing, returns `Some(vec![])` so callers can distinguish "restricted with
/// no parents" from "not restricted".
pub(crate) fn extract_allowed_room_ids(join_rules_content: &Value) -> Option<Vec<String>> {
    let join_rule = join_rules_content.get("join_rule").and_then(|v| v.as_str())?;
    if !RESTRICTED_JOIN_RULES.contains(&join_rule) {
        return None;
    }

    Some(extract_allowed_join_rooms(join_rules_content))
}

/// Minimal Matrix ID validation for room IDs (and aliases) used in `allow` entries.
/// - Must start with '!' or '#'
/// - Contains a ':' separating localpart from server
///
/// This is intentionally conservative: we only need the room ID syntax for
/// federation lookups, not a full Matrix ID parser.
pub(crate) fn is_valid_matrix_id(id: &str) -> bool {
    if id.is_empty() {
        return false;
    }
    let sigil = id.chars().next().unwrap_or('\0');
    if sigil != '!' && sigil != '#' {
        return false;
    }
    // Find the last ':' to split localpart from server (servers may contain ':')
    let Some(pos) = id.rfind(':') else { return false };
    if pos <= 1 {
        // at least one char localpart
        return false;
    }
    let server = &id[pos + 1..];
    if server.is_empty() {
        return false;
    }
    // Basic server name checks (reject path separators/whitespace/control)
    if server.contains('/') || server.contains('\\') || server.contains(' ') || server.contains('\0') {
        return false;
    }
    if server.len() > 253 {
        return false;
    }
    // Allowed charset for a server name (case matters only for comparison but we accept it)
    server.bytes().all(|b| matches!(b, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'.' | b'-' | b'_' | b':'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The whole point of this module: the authorization parser and the
    /// `/summary` projection must never disagree about the same input.
    #[test]
    fn summary_projection_agrees_with_authorization_parser() {
        let content = json!({
            "join_rule": "restricted",
            "allow": [
                {"room_id": "!zz:example.org", "type": "m.room_membership"},
                {"room_id": "!aa:example.org"},
                {"room_id": "!zz:example.org", "type": "m.room_membership"},
                {"room_id": "!drop:example.org", "type": "org.example.custom"},
                {"room_id": "no-sigil:example.org", "type": "m.room_membership"}
            ]
        });

        assert_eq!(
            extract_allowed_room_ids(&content),
            Some(extract_allowed_join_rooms(&content)),
            "the /summary projection must be the authorization parser's answer"
        );
    }

    #[test]
    fn summary_projection_is_none_for_non_restricted_rules() {
        assert_eq!(extract_allowed_room_ids(&json!({"join_rule": "public", "allow": []})), None);
        assert_eq!(extract_allowed_room_ids(&json!({"join_rule": "invite"})), None);
        assert_eq!(extract_allowed_room_ids(&json!({"allow": []})), None);
    }

    #[test]
    fn summary_projection_is_some_empty_for_restricted_without_allow() {
        assert_eq!(extract_allowed_room_ids(&json!({"join_rule": "knock_restricted"})), Some(vec![]));
    }
}
