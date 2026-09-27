//! The single grammar for Matrix room IDs (both historical forms).
//!
//! Matrix has *two* room-ID forms and a server must accept whichever one a peer
//! or a client presents:
//!
//! * the legacy `!opaque:server` form used by room versions 1–11, and
//! * the domainless `!` + 43 unpadded URL-safe base64 characters form introduced
//!   for room version 12 by
//!   [MSC4291](https://github.com/matrix-org/matrix-spec-proposals/blob/main/proposals/4291-room-ids-as-hashes.md)
//!   (room ID = the `m.room.create` event ID with the `$` sigil swapped to `!`),
//!   which has **no `:domain` part at all**.
//!
//! Upstream Synapse accepts both — `ROOM_ID_PATTERN_DOMAINLESS` is
//! `^[A-Za-z0-9\-_]{43}$` and `RoomID.is_valid()` dispatches on the presence of
//! `:` (`synapse/types/__init__.py`), with `get_domain()` returning `None` for
//! the domainless form. This module is the repo's one implementation of that
//! dispatch (AGENTS.md iron rule 2): every other validator —
//! [`crate::validation::Validator::validate_room_id`] and the route-layer
//! `synapse_web::routes::validators::validate_room_id` — delegates its
//! well-formedness decision here, so the two cannot drift.
//!
//! **Deliberately not here**: any question that needs the room version or the
//! room's state (e.g. "is this room local?"). Locality is a property of room
//! ownership, never of the ID's spelling: a domainless ID carries no server at
//! all, so a local-vs-remote decision must come from the room's own records.
//! `MembershipService::is_remote_room` is where that decision lives (it is still
//! ID-parsing based today and is being converted in a follow-up unit — see
//! `docs/audit/ROOM_V12_COMPLETION_PLAN_2026-09-27.md`).

use std::fmt;

/// Number of unpadded URL-safe base64 characters in a domainless (room v12)
/// room ID, per MSC4291.
pub const DOMAINLESS_ROOM_ID_LEN: usize = 43;

/// Maximum accepted length of a room ID, in bytes.
///
/// This mirrors the route-layer bound that predates the domainless form: it is
/// generous for both forms (a legacy `!opaque:server` is at most a few hundred
/// bytes, a domainless ID is exactly 44) while still rejecting pathological
/// inputs.
pub const MAX_ROOM_ID_LEN: usize = 255;

/// The domainless body: exactly 43 characters of unpadded URL-safe base64.
///
/// Checked byte-wise rather than with a regex: the pattern is one length plus a
/// character class, and a compiled regex would need `expect` (denied by the
/// workspace's `clippy::expect_used`) to live in a `static`.
fn is_domainless_body(body: &str) -> bool {
    body.len() == DOMAINLESS_ROOM_ID_LEN && body.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// The legacy form's server part (`[A-Za-z0-9.-]+`, with no second `:`), or
/// `None` when `room_id` is not `!opaque:server`.
///
/// The localpart is deliberately **permissive**: the pre-existing route-layer
/// check accepted any non-empty localpart, and this unit must not tighten what
/// the existing call sites already accept. The server part is the routed domain,
/// so it is restricted and may not contain a `:`.
fn legacy_server_name(room_id: &str) -> Option<&str> {
    let rest = room_id.strip_prefix('!')?;
    let (localpart, server_name) = rest.split_once(':')?;
    if localpart.is_empty() || server_name.is_empty() {
        return None;
    }
    if localpart.len() > MAX_ROOM_ID_LEN || server_name.len() > MAX_ROOM_ID_LEN {
        return None;
    }
    // `split_once` splits at the *first* `:`; the legacy grammar allows no `:`
    // in the server part, so a second one is malformed.
    if server_name.contains(':') {
        return None;
    }
    if !server_name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-') {
        return None;
    }
    Some(server_name)
}

/// The two accepted room-ID forms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoomIdForm<'a> {
    /// `!` + 43 unpadded URL-safe base64 characters, no `:domain`. Room v12 /
    /// MSC4291. The server that owns the room is *not* recoverable from the ID.
    Domainless,
    /// `!opaque:server` (room versions 1–11). The domain is recoverable.
    Legacy {
        /// The server-name part (everything after the last `:`, non-empty).
        server_name: &'a str,
    },
}

/// Why a string is not a room ID.
///
/// Variants carry only the *class* of failure; callers map them to their own
/// error envelope (route layer: `ApiError`; business layer:
/// [`crate::validation::ValidationError`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoomIdSyntaxError {
    /// The string does not start with `!`.
    MissingSigil,
    /// The string is longer than [`MAX_ROOM_ID_LEN`] bytes.
    TooLong,
    /// The string has no `:` part and is not `!` + 43 URL-safe base64 chars.
    DomainlessMalformed,
    /// The string has a `:` but does not match `!opaque:server`.
    LegacyMalformed,
}

impl fmt::Display for RoomIdSyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            Self::MissingSigil => "must start with !",
            Self::TooLong => "is too long",
            Self::DomainlessMalformed => "must be ! followed by 43 URL-safe base64 characters (room v12)",
            Self::LegacyMalformed => "must be !roomid:server",
        };
        f.write_str(msg)
    }
}

/// Parses `room_id` into one of the two accepted forms.
///
/// Mirrors upstream `synapse.types.RoomID.is_valid` / `from_string`: a `:` in
/// the string selects the legacy grammar, its absence selects the domainless
/// (MSC4291) grammar. `_:` (empty domain) is rejected by the legacy regex's
/// `+` quantifiers.
pub fn parse_room_id(room_id: &str) -> Result<RoomIdForm<'_>, RoomIdSyntaxError> {
    if !room_id.starts_with('!') {
        return Err(RoomIdSyntaxError::MissingSigil);
    }
    if room_id.len() > MAX_ROOM_ID_LEN {
        return Err(RoomIdSyntaxError::TooLong);
    }

    // `!` alone is neither form; report it as a malformed domainless ID rather
    // than framing it as a legacy error.
    let rest = &room_id[1..];
    if !rest.contains(':') {
        return if is_domainless_body(rest) {
            Ok(RoomIdForm::Domainless)
        } else {
            Err(RoomIdSyntaxError::DomainlessMalformed)
        };
    }

    match legacy_server_name(room_id) {
        Some(server_name) => Ok(RoomIdForm::Legacy { server_name }),
        None => Err(RoomIdSyntaxError::LegacyMalformed),
    }
}

/// `true` when `room_id` is a domainless (room v12 / MSC4291) room ID.
pub fn is_domainless_room_id(room_id: &str) -> bool {
    matches!(parse_room_id(room_id), Ok(RoomIdForm::Domainless))
}

/// `true` when `room_id` is well-formed in **either** accepted form.
pub fn is_well_formed_room_id(room_id: &str) -> bool {
    parse_room_id(room_id).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The 43-character MSC4291 hash from the MSC's own example
    /// (`$31hneApxJ_1o-63DmFrpeqnkFfWppnzWso1JvH3ogLM` ⇒ `!31hne…`).
    const MSC4291_HASH: &str = "31hneApxJ_1o-63DmFrpeqnkFfWppnzWso1JvH3ogLM";
    const DOMAINLESS: &str = "!31hneApxJ_1o-63DmFrpeqnkFfWppnzWso1JvH3ogLM";

    #[test]
    fn msc4291_example_hash_is_43_chars() {
        assert_eq!(MSC4291_HASH.len(), DOMAINLESS_ROOM_ID_LEN);
    }

    #[test]
    fn domainless_room_id_is_accepted() {
        assert_eq!(parse_room_id(DOMAINLESS), Ok(RoomIdForm::Domainless));
        assert!(is_domainless_room_id(DOMAINLESS));
        assert!(is_well_formed_room_id(DOMAINLESS));
    }

    #[test]
    fn domainless_room_id_accepts_full_urlsafe_base64_alphabet() {
        // Both cases and all three non-alphanumeric URL-safe base64 characters.
        let id = format!("!{}", "aZ0-_".repeat(8) + "aZ0");
        assert_eq!(id.len(), 44, "test fixture must be 43 chars + one sigil");
        assert!(is_well_formed_room_id(&id));
    }

    #[test]
    fn legacy_room_id_is_accepted_and_exposes_the_domain() {
        assert_eq!(parse_room_id("!opaque:example.com"), Ok(RoomIdForm::Legacy { server_name: "example.com" }));
        assert_eq!(
            parse_room_id("!room_id.with=chars:server.org"),
            Ok(RoomIdForm::Legacy { server_name: "server.org" })
        );
        assert!(!is_domainless_room_id("!opaque:example.com"));
    }

    #[test]
    fn domainless_room_id_of_wrong_length_is_rejected() {
        let short = format!("!{}", "A".repeat(DOMAINLESS_ROOM_ID_LEN - 1));
        let long = format!("!{}", "A".repeat(DOMAINLESS_ROOM_ID_LEN + 1));
        assert_eq!(parse_room_id(&short), Err(RoomIdSyntaxError::DomainlessMalformed));
        assert_eq!(parse_room_id(&long), Err(RoomIdSyntaxError::DomainlessMalformed));
    }

    #[test]
    fn domainless_room_id_with_illegal_characters_is_rejected() {
        // `+`, `/`, `=` are standard-base64-only characters; `.` is simply not
        // in the URL-safe alphabet.
        for illegal in ['+', '/', '=', '.'] {
            let mutated: String = std::iter::once(illegal).chain(MSC4291_HASH.chars().skip(1)).collect();
            assert_eq!(
                parse_room_id(&format!("!{mutated}")),
                Err(RoomIdSyntaxError::DomainlessMalformed),
                "character {illegal:?} must not be accepted in the domainless form"
            );
        }
        // A colon selects the legacy grammar instead — still a rejection, just a
        // differently-reported one.
        let with_colon: String = std::iter::once(':').chain(MSC4291_HASH.chars().skip(1)).collect();
        assert!(
            parse_room_id(&format!("!{with_colon}")).is_err(),
            "a colon makes this the legacy form, which it is not"
        );
    }

    #[test]
    fn genuinely_invalid_ids_are_rejected() {
        assert_eq!(parse_room_id(""), Err(RoomIdSyntaxError::MissingSigil));
        assert_eq!(parse_room_id("31hneApxJ_1o-63DmFrpeqnkFfWppnzWso1JvH3ogLM"), Err(RoomIdSyntaxError::MissingSigil));
        assert_eq!(parse_room_id("!opaque"), Err(RoomIdSyntaxError::DomainlessMalformed));
        assert_eq!(parse_room_id("!:"), Err(RoomIdSyntaxError::LegacyMalformed));
        assert_eq!(parse_room_id("!:example.com"), Err(RoomIdSyntaxError::LegacyMalformed));
        assert_eq!(parse_room_id("!opaque:"), Err(RoomIdSyntaxError::LegacyMalformed));
        assert_eq!(parse_room_id("!opaque:exa mple.com"), Err(RoomIdSyntaxError::LegacyMalformed));
        // The route-layer bound this grammar preserves: any ID over 255 bytes is
        // rejected *before* the per-form grammar, whichever form it looks like.
        assert_eq!(
            parse_room_id(&format!("!{}:example.com", "a".repeat(MAX_ROOM_ID_LEN))),
            Err(RoomIdSyntaxError::TooLong)
        );
        assert_eq!(parse_room_id(&format!("!{}", "a".repeat(MAX_ROOM_ID_LEN))), Err(RoomIdSyntaxError::TooLong));
    }

    #[test]
    fn legacy_room_id_with_a_colon_in_the_server_part_is_rejected() {
        // The server part is the routed domain and may not contain a second `:`
        // (`!a:b:c` would make "the server" ambiguous).
        assert_eq!(parse_room_id("!a:b:c"), Err(RoomIdSyntaxError::LegacyMalformed));
    }
}
