//! `auth_events` selection for locally-created events.
//!
//! # Why this module exists
//!
//! A Matrix PDU must carry `auth_events` (the subset of room state that gives
//! the sender permission to send the event). Locally-produced events never
//! persisted `depth` / `prev_events` / `auth_events`, so the federation PDU
//! projector (`synapse-web/src/routes/federation/pdu.rs`) classified them as
//! `MissingGraphMetadata` and **refused to sign them**. A peer joining one of
//! our rooms therefore received unsigned state and could not verify it.
//!
//! This module owns the *selection* half of the fix: given the current room
//! state and the event being created, which state event IDs belong in
//! `auth_events`.
//!
//! # Algorithm
//!
//! Mirrors the spec's "Auth events selection" (Matrix server-server API) and
//! upstream Synapse `synapse/event_auth.py::auth_types_for_event`
//! (release-v1.161):
//!
//! - `m.room.create` → no auth events at all (it is the DAG root).
//! - otherwise: current `m.room.power_levels`, the sender's `m.room.member`,
//!   and — for room versions **below 12** — the `m.room.create`;
//!   v12+ (MSC4291) omits the create event because the room id *is* the create
//!   event's id, so the reference is implied and must not be spelled out;
//! - `m.room.member` additionally: the **target's** `m.room.member`; the
//!   current `m.room.join_rules` when membership is `join` / `invite` /
//!   `knock`; the `m.room.third_party_invite` named by
//!   `content.third_party_invite.signed.token` on `invite`; and the
//!   authorising user's `m.room.member` for restricted joins when the room
//!   version supports the restricted join rule.
//!
//! A selected `(type, state_key)` that is absent from the current state is
//! simply skipped — the algorithm selects *types*, not events.
//!
//! The output is sorted by `(type, state_key)` so the event's canonical JSON
//! (and therefore its content hash) is reproducible regardless of hash-map
//! iteration order.

use std::collections::HashMap;

use serde_json::Value;

use synapse_common::redaction::redaction_rules;
use synapse_common::room_versions::room_version_at_least;
use synapse_storage::event::StateEvent;

/// The subset of room state used to select `auth_events`.
///
/// Deliberately a plain `(event_type, state_key) -> event_id` map rather than a
/// storage handle: selection is a pure function of the state, so it can be
/// tested against known answers without a database.
#[derive(Debug, Default, Clone)]
pub struct AuthStateSnapshot {
    ids: HashMap<(String, String), String>,
}

impl AuthStateSnapshot {
    /// Builds a snapshot from persisted state events.
    ///
    /// Rows without a `state_key` are not state and are ignored; when the same
    /// `(type, state_key)` appears twice the **later** element wins, so callers
    /// must order their input as "oldest first, newest last" if they intend the
    /// newest to take effect. Callers holding the current state (one row per
    /// `(type, state_key)`, as `EventStorage::get_state_events` returns) need
    /// not care.
    pub fn from_state_events(events: &[StateEvent]) -> Self {
        let mut ids = HashMap::with_capacity(events.len());
        for event in events {
            if let Some(state_key) = &event.state_key {
                ids.insert((event.event_type.clone().unwrap_or_default(), state_key.clone()), event.event_id.clone());
            }
        }
        Self { ids }
    }

    /// The event ID currently occupying `(event_type, state_key)`, if any.
    pub fn event_id(&self, event_type: &str, state_key: &str) -> Option<&str> {
        self.ids.get(&(event_type.to_string(), state_key.to_string())).map(String::as_str)
    }

    /// Records `event_id` as the occupant of `(event_type, state_key)`.
    ///
    /// Used by callers that build state incrementally as they emit events (the
    /// room-creation sequence) rather than reading a snapshot from storage.
    pub fn insert(&mut self, event_type: &str, state_key: &str, event_id: &str) {
        self.ids.insert((event_type.to_string(), state_key.to_string()), event_id.to_string());
    }
}

/// Whether `room_version` supports the MSC3083 restricted join rule — and so
/// whether a restricted join must name its authorising user in `auth_events`.
///
/// The answer comes from [`redaction_rules`], this workspace's single
/// per-version capability table, whose flags mirror upstream Synapse's
/// `RoomVersion` by name. Upstream's `auth_types_for_event`
/// (`synapse/event_auth.py:1287-1293`, release-v1.161) gates on exactly
/// `room_version.restricted_join_rule`, so the two agree by construction and
/// there is no second version list here to drift.
///
/// The flag is `true` from v8 onwards. **v9 is not an exception**: the spec's
/// room version 9 page ("This room version builds on version 8 to add
/// additional redaction rules … See room version 8 for specific details
/// regarding the addition of restricted rooms") and upstream's Rust
/// `RoomVersion::V9` — which inherits `restricted_join_rule: true` from `V8` and
/// adds only `restricted_join_rule_fix` — agree that v9 keeps restricted rooms.
/// v9 merely starts protecting `join_authorised_via_users_server` when
/// redacting.
///
/// A version the table does not know is **not** granted the rule. An
/// unrecognised version must never silently borrow another version's semantics,
/// because the result feeds `auth_events` and therefore event identity.
fn supports_restricted_join_rule(room_version: &str) -> bool {
    redaction_rules(room_version).is_some_and(|rules| rules.restricted_join_rule)
}

/// The `(type, state_key)` pairs selected as auth events for an event.
///
/// Returns an empty list for `m.room.create`. Order is deterministic (sorted).
///
/// **Room version 12+ (MSC4291 rule 2.5 / the create-implied room id):** the
/// `m.room.create` entry is **not** selected. In v12 the room id is the create
/// event's id with the sigil swapped, so the create event is implied by the room
/// id the event already carries; listing it in `auth_events` is redundant. The
/// spec's v12 auth rules also remove the old rule that required it (deleted rule
/// 2.4), and 3.5/MSC4307 then validates that every `auth_events` entry belongs to
/// the same room — which an implied create reference cannot satisfy by
/// construction. Below v12 the create event stays in the list: that is the
/// long-standing behaviour every existing room's DAG depends on.
pub fn auth_types_for_event(
    room_version: &str,
    event_type: &str,
    state_key: Option<&str>,
    sender: &str,
    content: &Value,
) -> Vec<(String, String)> {
    if event_type == "m.room.create" {
        return Vec::new();
    }

    let mut types: Vec<(String, String)> =
        vec![("m.room.power_levels".to_string(), String::new()), ("m.room.member".to_string(), sender.to_string())];
    if !room_version_at_least(room_version, 12) {
        types.push(("m.room.create".to_string(), String::new()));
    }

    if event_type == "m.room.member" {
        let membership = content.get("membership").and_then(Value::as_str).unwrap_or_default();

        if matches!(membership, "join" | "invite" | "knock") {
            types.push(("m.room.join_rules".to_string(), String::new()));
        }

        if let Some(target) = state_key {
            types.push(("m.room.member".to_string(), target.to_string()));
        }

        if membership == "invite" {
            if let Some(token) = content
                .get("third_party_invite")
                .and_then(|v| v.get("signed"))
                .and_then(|v| v.get("token"))
                .and_then(Value::as_str)
            {
                types.push(("m.room.third_party_invite".to_string(), token.to_string()));
            }
        }

        // MSC3083: the authorising user must be reachable from the auth chain,
        // otherwise a peer's `_check_joined_room` finds no member event for them
        // and rejects the join. Upstream selects the same pair here.
        if membership == "join" && supports_restricted_join_rule(room_version) {
            if let Some(authorising) = content.get("join_authorised_via_users_server").and_then(Value::as_str) {
                types.push(("m.room.member".to_string(), authorising.to_string()));
            }
        }
    }

    types.sort();
    types.dedup();
    types
}

/// Resolves the auth event IDs an event should reference.
///
/// Selected `(type, state_key)` pairs that are absent from `state` are skipped.
pub fn select_auth_events(
    room_version: &str,
    state: &AuthStateSnapshot,
    event_type: &str,
    state_key: Option<&str>,
    sender: &str,
    content: &Value,
) -> Vec<String> {
    auth_types_for_event(room_version, event_type, state_key, sender, content)
        .into_iter()
        .filter_map(|(event_type, state_key)| state.event_id(&event_type, &state_key).map(str::to_string))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn state_event(event_type: &str, state_key: &str, event_id: &str) -> StateEvent {
        StateEvent {
            event_id: event_id.to_string(),
            room_id: "!r:example.com".to_string(),
            sender: "@someone:example.com".to_string(),
            event_type: Some(event_type.to_string()),
            content: json!({}),
            state_key: Some(state_key.to_string()),
            unsigned: None,
            is_redacted: Some(false),
            origin_server_ts: 1_700_000_000_000,
            depth: None,
            processed_ts: None,
            not_before: None,
            status: None,
            origin: None,
            user_id: None,
            stream_ordering: None,
            prev_events: None,
            auth_events: None,
            signatures: None,
            hashes: None,
        }
    }

    /// The canonical room state used by most cases below.
    fn snapshot() -> AuthStateSnapshot {
        AuthStateSnapshot::from_state_events(&[
            state_event("m.room.create", "", "$create"),
            state_event("m.room.power_levels", "", "$pl"),
            state_event("m.room.join_rules", "", "$join_rules"),
            state_event("m.room.member", "@alice:example.com", "$alice_member"),
            state_event("m.room.member", "@bob:example.com", "$bob_member"),
            state_event("m.room.third_party_invite", "token123", "$tpi"),
        ])
    }

    // ── auth_types_for_event ───────────────────────────────────────────────

    #[test]
    fn create_event_has_no_auth_events() {
        let types = auth_types_for_event("11", "m.room.create", Some(""), "@alice:example.com", &json!({}));
        assert!(types.is_empty(), "m.room.create must not reference auth events, got {types:?}");
        assert!(select_auth_events("11", &snapshot(), "m.room.create", Some(""), "@alice:example.com", &json!({}))
            .is_empty());
    }

    #[test]
    fn message_selects_create_power_levels_and_sender_member_only() {
        let selected = select_auth_events(
            "11",
            &snapshot(),
            "m.room.message",
            None,
            "@alice:example.com",
            &json!({"body": "hi", "msgtype": "m.text"}),
        );
        // Order follows the sorted `(type, state_key)` selection order:
        // m.room.create, m.room.member(@alice), m.room.power_levels.
        assert_eq!(selected, vec!["$create".to_string(), "$alice_member".to_string(), "$pl".to_string()]);
    }

    #[test]
    fn missing_state_entries_are_skipped() {
        let partial = AuthStateSnapshot::from_state_events(&[state_event("m.room.create", "", "$create")]);
        let selected = select_auth_events("11", &partial, "m.room.message", None, "@alice:example.com", &json!({}));
        assert_eq!(selected, vec!["$create".to_string()]);
    }

    // ── G-28 / D-4: v12+ must not select the create event ───────────────────

    /// v12 (MSC4291) makes the room id the create event's id, so the create
    /// event is implied and must not appear in `auth_events`. v11 and below keep
    /// selecting it — the two versions must diverge here, not merely both pass.
    #[test]
    fn v12_omits_the_create_event_where_v11_selects_it() {
        let v11 = select_auth_events(
            "11",
            &snapshot(),
            "m.room.message",
            None,
            "@alice:example.com",
            &json!({"body": "hi", "msgtype": "m.text"}),
        );
        let v12 = select_auth_events(
            "12",
            &snapshot(),
            "m.room.message",
            None,
            "@alice:example.com",
            &json!({"body": "hi", "msgtype": "m.text"}),
        );

        assert_eq!(v11, vec!["$create".to_string(), "$alice_member".to_string(), "$pl".to_string()]);
        assert_eq!(v12, vec!["$alice_member".to_string(), "$pl".to_string()], "v12 must omit $create");
        assert!(!v12.contains(&"$create".to_string()), "the create reference is implied by the room id");
    }

    /// The `(type, state_key)` selection itself must drop the create pair from
    /// v12 on, so the exclusion does not depend on the snapshot happening to
    /// contain (or omit) a create event.
    #[test]
    fn v12_auth_types_have_no_create_entry() {
        let v12 = auth_types_for_event("12", "m.room.message", None, "@alice:example.com", &json!({}));
        assert!(
            !v12.iter().any(|(event_type, _)| event_type == "m.room.create"),
            "v12 auth types must not name m.room.create, got {v12:?}"
        );

        // v13 is a parse-only placeholder upstream, but the threshold rule is
        // "12 and later"; it must not fall back to the v11 list either.
        let v13 = auth_types_for_event("13", "m.room.message", None, "@alice:example.com", &json!({}));
        assert!(!v13.iter().any(|(event_type, _)| event_type == "m.room.create"), "got {v13:?}");

        // A version that does not parse as a number must fail closed to the
        // *older* behaviour rather than silently claiming 12+ semantics.
        let unknown = auth_types_for_event("hydra", "m.room.message", None, "@alice:example.com", &json!({}));
        assert!(
            unknown.iter().any(|(event_type, _)| event_type == "m.room.create"),
            "an unparseable version must keep the pre-v12 selection, got {unknown:?}"
        );
    }

    /// `m.room.create` itself never has auth events, in any version.
    #[test]
    fn create_event_has_no_auth_events_in_v12_either() {
        for version in ["1", "11", "12", "13"] {
            let types = auth_types_for_event(version, "m.room.create", Some(""), "@alice:example.com", &json!({}));
            assert!(types.is_empty(), "v{version}: m.room.create must not reference auth events, got {types:?}");
        }
    }

    #[test]
    fn join_selects_join_rules_and_target_member() {
        let selected = select_auth_events(
            "11",
            &snapshot(),
            "m.room.member",
            Some("@bob:example.com"),
            "@bob:example.com",
            &json!({"membership": "join"}),
        );
        assert_eq!(
            selected,
            vec!["$create".to_string(), "$join_rules".to_string(), "$bob_member".to_string(), "$pl".to_string(),]
        );
    }

    #[test]
    fn leave_does_not_select_join_rules() {
        let types = auth_types_for_event(
            "11",
            "m.room.member",
            Some("@bob:example.com"),
            "@bob:example.com",
            &json!({"membership": "leave"}),
        );
        assert!(
            !types.iter().any(|(event_type, _)| event_type == "m.room.join_rules"),
            "leave must not pull in join_rules: {types:?}"
        );
        // The target's member event is still selected.
        assert!(types.iter().any(|(event_type, key)| event_type == "m.room.member" && key == "@bob:example.com"));
    }

    #[test]
    fn invite_with_third_party_invite_selects_the_token_key() {
        let selected = select_auth_events(
            "11",
            &snapshot(),
            "m.room.member",
            Some("@bob:example.com"),
            "@alice:example.com",
            &json!({"membership": "invite", "third_party_invite": {"signed": {"token": "token123"}}}),
        );
        assert!(selected.contains(&"$tpi".to_string()), "expected the third-party invite event, got {selected:?}");
    }

    #[test]
    fn restricted_join_selects_authorising_user_member() {
        let selected = select_auth_events(
            "10",
            &snapshot(),
            "m.room.member",
            Some("@bob:example.com"),
            "@bob:example.com",
            &json!({"membership": "join", "join_authorised_via_users_server": "@alice:example.com"}),
        );
        // The authorising user is @alice, whose member event is already selected
        // as the sender member here; use a distinct authoriser to make the
        // assertion meaningful.
        assert!(selected.contains(&"$alice_member".to_string()), "got {selected:?}");

        let with_other_authoriser = select_auth_events(
            "10",
            &snapshot(),
            "m.room.member",
            Some("@bob:example.com"),
            "@bob:example.com",
            &json!({"membership": "join", "join_authorised_via_users_server": "@bob:example.com"}),
        );
        assert!(with_other_authoriser.contains(&"$bob_member".to_string()), "got {with_other_authoriser:?}");
    }

    /// The MSC3083 gate must follow the workspace's per-version capability
    /// table, **not** a hand-written version list. The former
    /// `matches!("8" | "10" | "11")` excluded v9 *and* v12 — and v12 is a
    /// creatable version (the default one, in fact), so a v12 restricted join
    /// dropped the authorising user from `auth_events` and a peer's
    /// `_check_joined_room` would reject it.
    ///
    /// v8 introduced restricted joins; v9 **keeps** them (the spec's v9 page
    /// defers to v8 for "the addition of restricted rooms"; v9 only adds the
    /// redaction fix); v10/v11/v12 keep them too.
    #[test]
    fn restricted_join_gate_follows_the_version_table() {
        let join_content = json!({"membership": "join", "join_authorised_via_users_server": "@alice:example.com"});

        for version in ["8", "9", "10", "11", "12"] {
            let types = auth_types_for_event(
                version,
                "m.room.member",
                Some("@bob:example.com"),
                "@bob:example.com",
                &join_content,
            );
            assert!(
                types.iter().any(|(event_type, key)| event_type == "m.room.member" && key == "@alice:example.com"),
                "v{version} must select the authorising user: {types:?}"
            );
            assert!(supports_restricted_join_rule(version), "v{version} supports restricted joins");
        }

        // v7 predates MSC3083: the authorising user is not selected, so the
        // negation is real rather than merely a missing assertion.
        let v7 =
            auth_types_for_event("7", "m.room.member", Some("@bob:example.com"), "@bob:example.com", &join_content);
        assert!(
            !v7.iter().any(|(_, key)| key == "@alice:example.com"),
            "v7 must not select the authorising user: {v7:?}"
        );
        assert!(!supports_restricted_join_rule("7"));

        // An unknown version fails closed instead of borrowing v8's semantics.
        assert!(!supports_restricted_join_rule("hydra"));
    }

    /// The full v12 selection for a restricted join: the authorising user's
    /// membership is present (the A7 fix) while the create event is absent
    /// (MSC4291). v11 differs on exactly the create entry.
    #[test]
    fn v12_restricted_join_selects_authoriser_and_omits_create() {
        let content = json!({"membership": "join", "join_authorised_via_users_server": "@alice:example.com"});
        let select = |version: &str| {
            select_auth_events(
                version,
                &snapshot(),
                "m.room.member",
                Some("@bob:example.com"),
                "@bob:example.com",
                &content,
            )
        };

        assert_eq!(
            select("12"),
            vec!["$join_rules".to_string(), "$alice_member".to_string(), "$bob_member".to_string(), "$pl".to_string(),],
            "v12: authorising member selected, create omitted"
        );
        assert_eq!(
            select("11"),
            vec![
                "$create".to_string(),
                "$join_rules".to_string(),
                "$alice_member".to_string(),
                "$bob_member".to_string(),
                "$pl".to_string(),
            ],
            "v11: same list plus the create event"
        );
    }

    #[test]
    fn selection_is_deterministic_and_deduplicated() {
        let first = auth_types_for_event(
            "11",
            "m.room.member",
            Some("@alice:example.com"),
            "@alice:example.com",
            &json!({"membership": "join"}),
        );
        let second = auth_types_for_event(
            "11",
            "m.room.member",
            Some("@alice:example.com"),
            "@alice:example.com",
            &json!({"membership": "join"}),
        );
        assert_eq!(first, second);

        let mut sorted = first.clone();
        sorted.sort();
        assert_eq!(first, sorted, "output must be sorted");
        let mut deduped = first.clone();
        deduped.dedup();
        assert_eq!(first, deduped, "output must not repeat a (type, state_key) pair");
    }
}
