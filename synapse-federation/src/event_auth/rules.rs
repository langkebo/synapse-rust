//! Room-version-dispatched authorisation rules for inbound events.
//!
//! This is the single entry point for the spec's "auth rules" as applied to an
//! event received from another server. The rules are version-dependent: each
//! room version inherits the previous version's list with modifications, so
//! the entry point dispatches on the room version and only then applies the
//! rules that version defines. Keeping the dispatch in one place is what lets
//! later rules (v12 rules 1.2 / 2 / 2.5 / 3.1 / 3.2 / 10.4) be added as
//! further checks here instead of as another set of ad-hoc branches in the
//! federation transaction handler.
//!
//! Implemented today:
//!
//! * **rule 3.5 (MSC4307, room version 12)** — every `auth_events` entry must
//!   refer to an event whose `room_id` matches the room of the event being
//!   authorised. `matrix-spec` `content/rooms/v12.md`: *"If any event in
//!   `auth_events` has a `room_id` which does not match that of the event being
//!   authorised, reject."* MSC4304 defines room version 12 as including
//!   MSC4307.
//! * **rule 1.4 (MSC4289, room version 12)** — an `m.room.create` event's
//!   `content.additional_creators`, when present, must be an array of strings,
//!   each of which passes the same user-ID validation as the create event's
//!   `sender`.
//!
//! # Version scope
//!
//! Rule 3.5 is applied **only to room version 12 and later**. It is a v12
//! addition: v1–v11 do not have it, so applying it to those rooms would reject
//! events that those room versions define as valid — a protocol violation
//! relative to the version's own contract. [`room_version_at_least`] performs
//! the comparison (never a string comparison, which orders `"2" >= "12"`).
//!
//! # Fail closed
//!
//! A rule that inspects an `auth_events` entry cannot be decided when the entry
//! is not available locally: "the referenced event's room does not match" is
//! false for an event we have never seen. Rather than treating an unknown room
//! as a match, an unresolvable entry is rejected
//! ([`EventAuthError::AuthEventUnavailable`]). Callers must therefore resolve
//! the entries before calling in and propagate resolution failures as
//! rejections — never as an empty `auth_events` list, which would silently skip
//! the rule.

use synapse_common::room_versions::room_version_at_least;
use synapse_storage::event::RoomEvent;

/// First room version whose auth rules include rule 3.5 (MSC4307).
///
/// MSC4304 defines room version 12 as MSC4291 + MSC4289 + MSC4297 + MSC4307;
/// rule 3.5 does not exist in v1–v11.
const AUTH_EVENTS_ROOM_CHECK_MIN_VERSION: u32 = 12;

/// First room version whose auth rules include rule 1.4 (MSC4289).
///
/// Both v12 rules are introduced by the same room version; the constants are
/// kept separate because they are separate rules with separate semantics.
const ADDITIONAL_CREATORS_MIN_VERSION: u32 = 12;

/// One `auth_events` entry of the event being authorised, resolved locally.
#[derive(Debug, Clone, Copy)]
pub struct ResolvedAuthEvent<'a> {
    /// The event ID named in the referencing event's `auth_events`.
    pub event_id: &'a str,
    /// The referenced event, or `None` when it is not available locally.
    ///
    /// Rules that inspect an entry reject on `None` (fail closed).
    pub event: Option<&'a RoomEvent>,
}

/// Everything the room-version authorisation rules inspect about one inbound
/// event.
///
/// The struct is the extension point: a new rule reads the fields it needs and
/// adds its own field here when it needs data the others do not (room state, for
/// example) — the entry point and its call site do not change shape.
#[derive(Debug, Clone, Copy)]
pub struct InboundEventAuth<'a> {
    /// The room version the event is processed under.
    pub room_version: &'a str,
    /// The room the event claims to be in.
    pub room_id: &'a str,
    /// The event's `type` (rules 1.2 / 1.4 only apply to `m.room.create`).
    pub event_type: &'a str,
    /// The event's `content` (rule 1.4 reads `additional_creators`; rule 10.4
    /// will read the power-level `users`).
    pub content: &'a serde_json::Value,
    /// Every `auth_events` entry, in the order the PDU lists them.
    ///
    /// The list must contain an element for each referenced event ID — an entry
    /// whose `event` is `None` is rejected, and a referenced ID that is missing
    /// from this list entirely would skip the rule, so callers must not filter
    /// unresolved IDs out.
    pub auth_events: &'a [ResolvedAuthEvent<'a>],
}

/// Why an inbound event failed the room-version authorisation rules.
///
/// One variant per rule; new rules append variants rather than reshaping the
/// entry point.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EventAuthError {
    /// Rule 3.5 (MSC4307): an `auth_events` entry belongs to another room.
    #[error(
        "auth_events entry {auth_event_id} belongs to room {auth_event_room_id}, not {room_id} \
         (MSC4307 / room v12 rule 3.5)"
    )]
    AuthEventRoomMismatch {
        /// The referenced event ID.
        auth_event_id: String,
        /// The room the referenced event actually belongs to.
        auth_event_room_id: String,
        /// The room of the event being authorised.
        room_id: String,
    },
    /// Rule 1.4 (MSC4289): `m.room.create` carries an `additional_creators`
    /// value that is not an array of valid user IDs.
    #[error("invalid additional_creators on m.room.create (MSC4289 / room v12 rule 1.4): {reason}")]
    InvalidAdditionalCreators {
        /// Which requirement failed.
        reason: String,
    },
    /// An `auth_events` entry could not be resolved locally, so the rules that
    /// inspect it cannot be decided. Rejected rather than waved through.
    #[error(
        "auth_events entry {auth_event_id} is not available locally; cannot authorise the event \
         against it (fail closed)"
    )]
    AuthEventUnavailable {
        /// The referenced event ID.
        auth_event_id: String,
    },
}

/// Version-dispatched entry point for the auth rules that apply to an event
/// received from another server.
///
/// Applies every rule the event's room version defines and returns the first
/// violation. The caller is expected to reject the PDU on any error and to have
/// already performed the transport-level checks (signature, sender↔origin).
///
/// The raw PDU is not consumed yet because rule 3.5 operates on `auth_events`
/// alone; rules 1.2 (create carrying a `room_id`), 10.4 (power levels naming a
/// creator) and the room-identity rules 2 / 2.5 will take the PDU and the room
/// state, which extend [`InboundEventAuth`] rather than this signature.
pub fn check_inbound_event_auth(input: &InboundEventAuth<'_>) -> Result<(), EventAuthError> {
    if room_version_at_least(input.room_version, ADDITIONAL_CREATORS_MIN_VERSION) {
        check_additional_creators(input)?;
    }
    if room_version_at_least(input.room_version, AUTH_EVENTS_ROOM_CHECK_MIN_VERSION) {
        check_auth_events_belong_to_room(input)?;
    }
    Ok(())
}

/// Rule 1.4 (MSC4289 / room version 12): `content.additional_creators` on an
/// `m.room.create` event must be a list of valid user IDs.
///
/// The create event's `sender` is the creator; `additional_creators` names
/// further creators, who get the same unlimited power as the sender. A malformed
/// entry would therefore mint a creator whose identity cannot be resolved, so a
/// non-array, a non-string element, or an element that fails the shared user-ID
/// grammar is rejected. An absent field is valid (it is optional).
///
/// The grammar is `synapse_common::validation::is_well_formed_user_id` — the
/// same implementation `Validator::validate_matrix_id` uses, so "the same
/// validation as the sender" is structural, not a second copy.
fn check_additional_creators(input: &InboundEventAuth<'_>) -> Result<(), EventAuthError> {
    if input.event_type != "m.room.create" {
        return Ok(());
    }
    let Some(value) = input.content.get("additional_creators") else {
        return Ok(());
    };
    let Some(entries) = value.as_array() else {
        return Err(EventAuthError::InvalidAdditionalCreators { reason: "must be an array of user IDs".to_string() });
    };
    for entry in entries {
        let Some(user_id) = entry.as_str() else {
            return Err(EventAuthError::InvalidAdditionalCreators {
                reason: "every entry must be a string user ID".to_string(),
            });
        };
        if !synapse_common::validation::is_well_formed_user_id(user_id) {
            return Err(EventAuthError::InvalidAdditionalCreators {
                reason: format!("{user_id:?} is not a valid user ID"),
            });
        }
    }
    Ok(())
}

/// Whether `room_version` defines rule 1.4 (MSC4289).
pub fn enforces_additional_creators_rule(room_version: &str) -> bool {
    room_version_at_least(room_version, ADDITIONAL_CREATORS_MIN_VERSION)
}

/// Rule 3.5 (MSC4307 / room version 12): every `auth_events` entry must refer
/// to an event in the room the event is being authorised in.
fn check_auth_events_belong_to_room(input: &InboundEventAuth<'_>) -> Result<(), EventAuthError> {
    for entry in input.auth_events {
        let Some(event) = entry.event else {
            return Err(EventAuthError::AuthEventUnavailable { auth_event_id: entry.event_id.to_string() });
        };
        if event.room_id != input.room_id {
            return Err(EventAuthError::AuthEventRoomMismatch {
                auth_event_id: entry.event_id.to_string(),
                auth_event_room_id: event.room_id.clone(),
                room_id: input.room_id.to_string(),
            });
        }
    }
    Ok(())
}

/// Whether `room_version` defines rule 3.5 (MSC4307).
///
/// Exposed so callers (and tests) can reason about the scope without
/// re-deriving the version comparison.
pub fn enforces_auth_events_room_rule(room_version: &str) -> bool {
    room_version_at_least(room_version, AUTH_EVENTS_ROOM_CHECK_MIN_VERSION)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A shared `{}` content for cases that do not inspect the event body.
    fn empty_content() -> &'static serde_json::Value {
        static EMPTY: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
        EMPTY.get_or_init(|| json!({}))
    }

    fn room_event(event_id: &str, room_id: &str) -> RoomEvent {
        RoomEvent {
            event_id: event_id.to_string(),
            room_id: room_id.to_string(),
            user_id: "@alice:example.org".to_string(),
            event_type: "m.room.create".to_string(),
            content: json!({}),
            state_key: Some(String::new()),
            depth: 1,
            origin_server_ts: 1,
            processed_ts: 1,
            not_before: 0,
            status: None,
            origin: "example.org".to_string(),
            stream_ordering: Some(1),
            redacts: None,
        }
    }

    fn entry<'a>(event_id: &'a str, event: Option<&'a RoomEvent>) -> ResolvedAuthEvent<'a> {
        ResolvedAuthEvent { event_id, event }
    }

    #[test]
    fn empty_auth_events_pass_in_every_version() {
        for version in ["1", "10", "11", "12", "13"] {
            let input = InboundEventAuth {
                room_version: version,
                room_id: "!a:example.org",
                event_type: "m.room.topic",
                content: empty_content(),
                auth_events: &[],
            };
            assert!(check_inbound_event_auth(&input).is_ok(), "v{version} must accept an empty auth_events list");
        }
    }

    #[test]
    fn v12_rejects_auth_event_from_another_room() {
        let foreign = room_event("$foreign", "!other:example.org");
        let input = InboundEventAuth {
            room_version: "12",
            room_id: "!a:example.org",
            event_type: "m.room.topic",
            content: empty_content(),
            auth_events: &[entry("$foreign", Some(&foreign))],
        };
        assert_eq!(
            check_inbound_event_auth(&input),
            Err(EventAuthError::AuthEventRoomMismatch {
                auth_event_id: "$foreign".to_string(),
                auth_event_room_id: "!other:example.org".to_string(),
                room_id: "!a:example.org".to_string(),
            })
        );
    }

    #[test]
    fn v12_accepts_auth_event_from_the_same_room() {
        let same = room_event("$same", "!a:example.org");
        let input = InboundEventAuth {
            room_version: "12",
            room_id: "!a:example.org",
            event_type: "m.room.topic",
            content: empty_content(),
            auth_events: &[entry("$same", Some(&same))],
        };
        assert!(check_inbound_event_auth(&input).is_ok());
    }

    #[test]
    fn v12_rejects_unresolvable_auth_event() {
        let input = InboundEventAuth {
            room_version: "12",
            room_id: "!a:example.org",
            event_type: "m.room.topic",
            content: empty_content(),
            auth_events: &[entry("$gone", None)],
        };
        assert_eq!(
            check_inbound_event_auth(&input),
            Err(EventAuthError::AuthEventUnavailable { auth_event_id: "$gone".to_string() })
        );
    }

    #[test]
    fn pre_v12_versions_do_not_define_rule_3_5() {
        let foreign = room_event("$foreign", "!other:example.org");
        for version in ["1", "2", "9", "10", "11"] {
            let input = InboundEventAuth {
                room_version: version,
                room_id: "!a:example.org",
                event_type: "m.room.topic",
                content: empty_content(),
                auth_events: &[entry("$foreign", Some(&foreign))],
            };
            assert!(
                check_inbound_event_auth(&input).is_ok(),
                "v{version} does not define rule 3.5 and must not reject a cross-room auth event"
            );
            assert!(!enforces_auth_events_room_rule(version), "v{version} must not enforce rule 3.5");
        }
    }

    #[test]
    fn a_numeric_version_above_twelve_inherits_the_v12_rules() {
        // Room versions are cumulative: a later version inherits the previous
        // version's rules unless the spec says otherwise. `"13"` is **not** a
        // real room version (Q5 removed it from the capability table), but the
        // rule layer orders by the number, and the caller's version-resolution
        // gate is what rejects an unsupported one before it gets here.
        assert!(enforces_auth_events_room_rule("13"));
        assert!(enforces_additional_creators_rule("13"));
    }

    #[test]
    fn unknown_room_version_is_not_treated_as_v12() {
        // `room_version_at_least` fails closed for unparseable identifiers:
        // they are not ordered into the v12 branch, so an unknown version gets
        // no v12 rule (the caller's version-resolution gate rejects it first).
        assert!(!enforces_auth_events_room_rule("org.example.experimental"));
    }

    #[test]
    fn rule_3_5_reports_the_first_offending_entry() {
        let same = room_event("$same", "!a:example.org");
        let foreign = room_event("$foreign", "!other:example.org");
        let input = InboundEventAuth {
            room_version: "12",
            room_id: "!a:example.org",
            event_type: "m.room.topic",
            content: empty_content(),
            auth_events: &[entry("$same", Some(&same)), entry("$foreign", Some(&foreign))],
        };
        assert_eq!(
            check_inbound_event_auth(&input),
            Err(EventAuthError::AuthEventRoomMismatch {
                auth_event_id: "$foreign".to_string(),
                auth_event_room_id: "!other:example.org".to_string(),
                room_id: "!a:example.org".to_string(),
            })
        );
    }

    // ── rule 1.4 (MSC4289): additional_creators ────────────────────────────

    fn create_auth<'a>(content: &'a serde_json::Value, version: &'a str) -> InboundEventAuth<'a> {
        InboundEventAuth {
            room_version: version,
            room_id: "!a:example.org",
            event_type: "m.room.create",
            content,
            auth_events: &[],
        }
    }

    #[test]
    fn v12_accepts_create_without_additional_creators() {
        let content = json!({"creator": "@alice:example.org", "room_version": "12"});
        assert!(check_inbound_event_auth(&create_auth(&content, "12")).is_ok());
    }

    #[test]
    fn v12_accepts_valid_additional_creators() {
        let content = json!({"additional_creators": ["@bob:example.org", "@carol:other.example"]});
        assert!(check_inbound_event_auth(&create_auth(&content, "12")).is_ok());
    }

    #[test]
    fn v12_rejects_a_non_array_additional_creators() {
        let content = json!({"additional_creators": "@bob:example.org"});
        assert!(matches!(
            check_inbound_event_auth(&create_auth(&content, "12")),
            Err(EventAuthError::InvalidAdditionalCreators { .. })
        ));
    }

    #[test]
    fn v12_rejects_a_non_string_additional_creator() {
        let content = json!({"additional_creators": ["@bob:example.org", 7]});
        assert!(matches!(
            check_inbound_event_auth(&create_auth(&content, "12")),
            Err(EventAuthError::InvalidAdditionalCreators { .. })
        ));
    }

    #[test]
    fn v12_rejects_an_invalid_user_id_in_additional_creators() {
        for bad in ["bob:example.org", "@bob", "@:example.org", "@bob:", "", "@Bob:example.org"] {
            let content = json!({"additional_creators": [bad]});
            assert!(
                matches!(
                    check_inbound_event_auth(&create_auth(&content, "12")),
                    Err(EventAuthError::InvalidAdditionalCreators { .. })
                ),
                "{bad:?} must be rejected"
            );
        }
    }

    /// The rule is a v12 addition: v1-v11 have no `additional_creators`, so a
    /// malformed value there is not this rule's business.
    #[test]
    fn pre_v12_is_not_subject_to_rule_1_4() {
        let content = json!({"additional_creators": "not-an-array"});
        for version in ["1", "10", "11"] {
            assert!(check_inbound_event_auth(&create_auth(&content, version)).is_ok(), "v{version}");
        }
        assert!(enforces_additional_creators_rule("12"));
        assert!(!enforces_additional_creators_rule("11"));
    }

    /// Only `m.room.create` carries creators; another event type with the same
    /// key is ignored.
    #[test]
    fn rule_1_4_only_applies_to_create_events() {
        let content = json!({"additional_creators": "not-an-array"});
        let input = InboundEventAuth {
            room_version: "12",
            room_id: "!a:example.org",
            event_type: "m.room.topic",
            content: &content,
            auth_events: &[],
        };
        assert!(check_inbound_event_auth(&input).is_ok());
    }
}
