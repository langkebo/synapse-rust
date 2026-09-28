//! Authorising an event against a **state map** — the `_check_event_auth` half of
//! state resolution.
//!
//! State resolution's iterative auth checks ask, for an event and the state
//! resolved so far, "is this event authorised?". `event_auth::rules` answers a
//! different question (a single inbound event against its own `auth_events` and
//! the room version), so the two are separate seams rather than one function with
//! a mode flag — but the *rules* are shared wherever they already exist:
//!
//! * `m.room.member` transitions are decided by the pure rulebook
//!   `synapse_common::membership_transition::is_legal` + `TransitionCtx` — the
//!   same deep module the client and federation paths use (iron rule 2);
//! * the creator set comes from `synapse_common::room_creator`;
//! * MSC4289's unlimited creators (E-2) are honoured for room v12+ exactly as
//!   `AuthService::get_user_power_level` does.
//!
//! Everything is a pure function of `(event, state map, event map, room version)`
//! so state resolution can call it during replay.

use std::collections::{BTreeSet, HashMap};

use synapse_common::membership_transition::{is_legal, JoinRule, TransitionCtx};
use synapse_common::Membership;

use super::models::EventData;

/// The `power_levels` thresholds that matter for authorisation, resolved from a
/// state map. Defaults match the spec's `m.room.power_levels` defaults.
#[derive(Debug, Clone)]
struct PowerLevels {
    users: HashMap<String, i64>,
    users_default: i64,
    events: HashMap<String, i64>,
    events_default: i64,
    state_default: i64,
    ban: i64,
    kick: i64,
    invite: i64,
    redact: i64,
}

impl Default for PowerLevels {
    fn default() -> Self {
        // Spec defaults: users_default 0, events_default 0, state_default 50,
        // ban/kick/redact 50, invite 0.
        Self {
            users: HashMap::new(),
            users_default: 0,
            events: HashMap::new(),
            events_default: 0,
            state_default: 50,
            ban: 50,
            kick: 50,
            invite: 0,
            redact: 50,
        }
    }
}

/// Resolve the state event named by `key`, if the state map has it.
fn state_event<'a>(
    state: &HashMap<String, String>,
    events: &'a HashMap<String, EventData>,
    key: &str,
) -> Option<&'a EventData> {
    state.get(key).and_then(|event_id| events.get(event_id))
}

fn content_i64(content: &serde_json::Value, key: &str) -> Option<i64> {
    content.get(key).and_then(|v| v.as_i64())
}

fn power_levels_from_state(state: &HashMap<String, String>, events: &HashMap<String, EventData>) -> PowerLevels {
    let mut levels = PowerLevels::default();
    let Some(pl) = state_event(state, events, "m.room.power_levels:").and_then(|e| e.content.as_ref()) else {
        return levels;
    };

    if let Some(users) = pl.get("users").and_then(|v| v.as_object()) {
        for (user, level) in users {
            if let Some(level) = level.as_i64() {
                levels.users.insert(user.clone(), level);
            }
        }
    }
    if let Some(value) = content_i64(pl, "users_default") {
        levels.users_default = value;
    }
    if let Some(events_map) = pl.get("events").and_then(|v| v.as_object()) {
        for (event_type, level) in events_map {
            if let Some(level) = level.as_i64() {
                levels.events.insert(event_type.clone(), level);
            }
        }
    }
    for (key, field) in [
        ("events_default", &mut levels.events_default),
        ("state_default", &mut levels.state_default),
        ("ban", &mut levels.ban),
        ("kick", &mut levels.kick),
        ("invite", &mut levels.invite),
        ("redact", &mut levels.redact),
    ] {
        if let Some(value) = content_i64(pl, key) {
            *field = value;
        }
    }
    levels
}

/// The room's creator set, from the state map's `m.room.create` event.
fn creators_from_state(state: &HashMap<String, String>, events: &HashMap<String, EventData>) -> BTreeSet<String> {
    state_event(state, events, "m.room.create:")
        .map(|create| {
            let sender = if create.sender.is_empty() { "" } else { create.sender.as_str() };
            let content = create.content.clone().unwrap_or(serde_json::Value::Null);
            synapse_common::room_creator::creators_from_create_event(sender, &content)
        })
        .unwrap_or_default()
}

/// The membership recorded for `user` in `state`.
fn membership_of(
    state: &HashMap<String, String>,
    events: &HashMap<String, EventData>,
    user: &str,
) -> Option<Membership> {
    let event = state_event(state, events, &format!("m.room.member:{user}"))?;
    event.content.as_ref()?.get("membership")?.as_str()?.parse::<Membership>().ok()
}

/// The resolved join rule from `state` (default `invite`).
fn join_rule_from_state(state: &HashMap<String, String>, events: &HashMap<String, EventData>) -> JoinRule {
    state_event(state, events, "m.room.join_rules:")
        .and_then(|event| event.content.as_ref())
        .and_then(|content| content.get("join_rule"))
        .and_then(|value| value.as_str())
        .and_then(|raw| raw.parse::<JoinRule>().ok())
        .unwrap_or(JoinRule::Invite)
}

/// Whether `event` is authorised against `state` under `room_version`.
///
/// Covers the auth rules state resolution replays:
///
/// * `m.room.create` — the DAG root: no `auth_events`, and (v12+) the create
///   event must not carry a `room_id` (MSC4291 rule 1.2);
/// * `m.room.member` — delegated to the membership-transition rulebook with a
///   `TransitionCtx` resolved from `state`;
/// * `m.room.power_levels` / any other state event — the sender's power level must
///   meet `events[type]`, else `state_default`;
/// * any other event — the sender's power level must meet `events_default`.
///
/// Room v12+ creators have unlimited power (MSC4289), so they pass every
/// threshold.
///
/// Known simplification: a restricted join is **not** authorised here
/// (`restricted_join_authorized` is false) because deciding it needs the `allow`
/// rooms' state, which a single state map does not carry; such joins are left to
/// the inbound path, which resolves them with storage. This predicate
/// under-authorises rather than over-authorises.
pub fn is_authorised_against_state(
    event: &EventData,
    state: &HashMap<String, String>,
    events: &HashMap<String, EventData>,
    room_version: &str,
) -> bool {
    let v12_plus = synapse_common::room_versions::room_version_at_least(room_version, 12);

    // The create event is the DAG root: it has no auth events, and in v12+ it may
    // not carry a room_id (MSC4291 rule 1.2 — the room id *is* its event id).
    if event.event_type == "m.room.create" {
        if !event.auth_events.is_empty() {
            return false;
        }
        if v12_plus {
            let carries_room_id =
                event.content.as_ref().and_then(|content| content.get("room_id")).is_some_and(|value| !value.is_null());
            if carries_room_id {
                return false;
            }
        }
        return true;
    }

    let levels = power_levels_from_state(state, events);
    let creators = creators_from_state(state, events);
    let sender_power = if v12_plus && creators.contains(&event.sender) {
        i64::MAX
    } else {
        levels.users.get(&event.sender).copied().unwrap_or(levels.users_default)
    };

    if event.event_type == "m.room.member" {
        let Some(target) = event.state_key.as_ref().and_then(|key| key.as_str()) else {
            return false;
        };
        let Some(to) = event
            .content
            .as_ref()
            .and_then(|content| content.get("membership"))
            .and_then(|value| value.as_str())
            .and_then(|raw| raw.parse::<Membership>().ok())
        else {
            return false;
        };

        let from = membership_of(state, events, target);
        let target_power = levels.users.get(target).copied().unwrap_or(levels.users_default);
        let ctx = TransitionCtx {
            actor_pl: sender_power,
            target_pl: target_power,
            ban_level: levels.ban,
            kick_level: levels.kick,
            invite_level: levels.invite,
            join_rule: join_rule_from_state(state, events),
            actor_is_target: event.sender == target,
            target_is_banned: from == Some(Membership::Ban),
            target_is_creator: creators.contains(target),
            restricted_join_authorized: false,
        };
        return is_legal(from, to, &ctx).is_ok();
    }

    // Everything else requires the sender to be in the room. The sender's own
    // join is a member event and was handled above.
    if membership_of(state, events, &event.sender) != Some(Membership::Join) {
        return false;
    }

    let required = levels.events.get(&event.event_type).copied().unwrap_or(if event.state_key.is_some() {
        levels.state_default
    } else {
        levels.events_default
    });
    sender_power >= required
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn event(
        id: &str,
        event_type: &str,
        state_key: Option<&str>,
        sender: &str,
        content: serde_json::Value,
    ) -> EventData {
        EventData {
            event_id: id.to_string(),
            room_id: "!r:ex.com".to_string(),
            event_type: event_type.to_string(),
            auth_events: Vec::new(),
            prev_events: Vec::new(),
            state_key: state_key.map(|key| json!(key)),
            content: Some(content),
            sender: sender.to_string(),
            origin_server_ts: 1,
            depth: 1,
        }
    }

    /// A room state: create by alice, alice joined (100), join rule public.
    fn base_room() -> (HashMap<String, String>, HashMap<String, EventData>) {
        let events: HashMap<String, EventData> = vec![
            event("$create", "m.room.create", Some(""), "@alice:ex.com", json!({"room_version": "12"})),
            event("$pl", "m.room.power_levels", Some(""), "@alice:ex.com", json!({"users": {"@alice:ex.com": 100}, "users_default": 0, "state_default": 50, "events_default": 0, "ban": 50, "kick": 50, "redact": 50, "invite": 0})),
            event("$join_rules", "m.room.join_rules", Some(""), "@alice:ex.com", json!({"join_rule": "public"})),
            event("$alice_member", "m.room.member", Some("@alice:ex.com"), "@alice:ex.com", json!({"membership": "join"})),
        ]
        .into_iter()
        .map(|e| (e.event_id.clone(), e))
        .collect();

        let state: HashMap<String, String> = [
            ("m.room.create:", "$create"),
            ("m.room.power_levels:", "$pl"),
            ("m.room.join_rules:", "$join_rules"),
            ("m.room.member:@alice:ex.com", "$alice_member"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

        (state, events)
    }

    #[test]
    fn create_event_is_the_root_and_must_not_carry_a_room_id_in_v12() {
        let (state, events) = base_room();
        let ok = event("$new", "m.room.create", Some(""), "@a:ex.com", json!({"room_version": "12"}));
        assert!(is_authorised_against_state(&ok, &state, &events, "12"));

        let with_room_id =
            event("$bad", "m.room.create", Some(""), "@a:ex.com", json!({"room_version": "12", "room_id": "!x"}));
        assert!(!is_authorised_against_state(&with_room_id, &state, &events, "12"), "rule 1.2");
        // Below v12 the field is allowed.
        assert!(is_authorised_against_state(&with_room_id, &state, &events, "11"));
    }

    #[test]
    fn a_public_join_by_a_stranger_is_authorised_and_a_joined_member_message_too() {
        let (state, events) = base_room();
        let join =
            event("$bob_join", "m.room.member", Some("@bob:ex.com"), "@bob:ex.com", json!({"membership": "join"}));
        assert!(is_authorised_against_state(&join, &state, &events, "12"));

        let message = event("$msg", "m.room.message", None, "@alice:ex.com", json!({"body": "hi"}));
        assert!(is_authorised_against_state(&message, &state, &events, "12"), "a joined member may send");
    }

    #[test]
    fn a_message_from_a_non_member_is_not_authorised() {
        let (state, events) = base_room();
        let message = event("$msg", "m.room.message", None, "@stranger:ex.com", json!({"body": "hi"}));
        assert!(!is_authorised_against_state(&message, &state, &events, "12"));
    }

    #[test]
    fn an_invite_only_room_refuses_a_stranger_join() {
        let (mut state, mut events) = base_room();
        let rules =
            event("$rules_private", "m.room.join_rules", Some(""), "@alice:ex.com", json!({"join_rule": "invite"}));
        events.insert(rules.event_id.clone(), rules);
        state.insert("m.room.join_rules:".to_string(), "$rules_private".to_string());

        let join =
            event("$bob_join", "m.room.member", Some("@bob:ex.com"), "@bob:ex.com", json!({"membership": "join"}));
        assert!(!is_authorised_against_state(&join, &state, &events, "12"));
    }

    #[test]
    fn a_state_event_needs_the_state_default_and_the_creator_always_passes() {
        let (state, events) = base_room();

        // A stranger (power 0 < state_default 50) may not set a name.
        let stranger_name = event("$name", "m.room.name", Some(""), "@stranger:ex.com", json!({"name": "x"}));
        assert!(!is_authorised_against_state(&stranger_name, &state, &events, "12"));

        // The v12 creator has unlimited power (MSC4289), so even an event whose
        // explicit threshold is 100 passes for her.
        let creator_name = event("$name2", "m.room.name", Some(""), "@alice:ex.com", json!({"name": "x"}));
        assert!(is_authorised_against_state(&creator_name, &state, &events, "12"));
    }

    /// The creator's unlimited power is a v12 addition; below it the ordinary
    /// thresholds apply.
    #[test]
    fn creator_unlimited_power_is_v12_only() {
        let (mut state, mut events) = base_room();
        // A power_levels event that gives alice only 10, with state_default 50.
        let pl = event(
            "$pl_low",
            "m.room.power_levels",
            Some(""),
            "@alice:ex.com",
            json!({"users": {"@alice:ex.com": 10}, "users_default": 0, "state_default": 50, "events_default": 0}),
        );
        events.insert(pl.event_id.clone(), pl);
        state.insert("m.room.power_levels:".to_string(), "$pl_low".to_string());

        let name = event("$name", "m.room.name", Some(""), "@alice:ex.com", json!({"name": "x"}));
        assert!(!is_authorised_against_state(&name, &state, &events, "11"), "v11: 10 < 50");
        assert!(is_authorised_against_state(&name, &state, &events, "12"), "v12: creators are unlimited");
    }
}
