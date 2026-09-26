//! Event IDs derived from the event reference hash (Matrix room versions 3+).
//!
//! U-13 / plan doc §6.6 step 1: this module is the **pure** implementation plus
//! its known-answer tests.  It is deliberately **not wired** into the local
//! event-creation path yet — `crypto::generate_event_id` still produces the
//! random `$<ts>$<b64>:<server>` form for every room version, which is not a
//! valid event ID for v3+ rooms.  Wiring it is step 2, and step 3 is the
//! cross-implementation interoperability gate; see the plan doc for the blast
//! radius checklist (local `event_id`, txn dedup table, redaction targets,
//! cache keys, E2EE references, fixtures/snapshots).
//!
//! Algorithm (spec room v4 "Event IDs", `v4-event-ids` fragment):
//!
//! 1. Redact the event with the room version's redaction algorithm
//!    ([`crate::redaction::redact_event`]).
//! 2. Remove `signatures`, `unsigned` and `age_ts`.
//! 3. Encode the result as Matrix canonical JSON and hash it with SHA-256.
//! 4. Encode the digest with **unpadded** Base64, prefixed with `$`.
//!
//! Room version 3 is the one exception: it uses the *standard* Base64 alphabet
//! (so IDs may contain `/` and `+`), which room version 4 replaced with the
//! URL-safe alphabet.  `signatures`/`unsigned`/`age_ts` are stripped after
//! redaction, exactly as upstream Synapse does
//! (`synapse/crypto/event_signing.py::compute_event_reference_hash` and
//! `rust/src/events/utils.rs::compute_event_reference_hash`).
//!
//! Reference: `element-hq/synapse` release-v1.161; Matrix Specification v1.18
//! (`v1-redactions`, `v6-redactions`, `v9-redactions`, `v11-redactions`,
//! `v3`/`v4` event-ID fragments).

use base64::engine::general_purpose::{STANDARD_NO_PAD, URL_SAFE_NO_PAD};
use base64::Engine;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::canonical_json::canonical_json;
use crate::redaction::{redact_event, redaction_rules, RedactionError};

/// Errors from the reference-hash / event-ID functions.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EventIdError {
    /// The room version is known but does not derive event IDs from the
    /// reference hash (v1/v2 use server-assigned random IDs).
    #[error("room version {0} does not use reference-hash event IDs")]
    ReferenceHashUnsupported(String),
    /// The room version is unknown/unsupported.
    #[error("unknown or unsupported room version: {0}")]
    UnknownRoomVersion(String),
    /// Redaction failed.
    #[error("redaction failed: {0}")]
    Redaction(#[from] RedactionError),
    /// The redacted event could not be encoded as canonical JSON.
    #[error("canonical JSON encoding failed: {0}")]
    CanonicalJson(String),
    /// A v3+ (reference-hash) PDU carried an explicit `event_id`.
    ///
    /// Upstream rejects this outright; accepting it would mean trusting a
    /// sender-supplied identity instead of deriving it.
    #[error("room version {room_version} events must not carry an explicit event_id (got {event_id})")]
    UnexpectedEventId {
        /// The room version whose event format was violated.
        room_version: String,
        /// The carried value, for the log line.
        event_id: String,
    },
    /// A v1/v2 PDU arrived without the sender-assigned `event_id`.
    #[error("room version {0} events must carry a server-assigned event_id")]
    MissingEventId(String),
}

/// Returns `true` when `room_version` derives event IDs from the reference hash
/// (room versions 3 and above).
///
/// Returns `false` for unknown versions as well; use [`compute_reference_hash`]
/// when the distinction between "v1/v2" and "unknown version" matters, since it
/// reports them with different errors.
pub fn uses_reference_hash_event_id(room_version: &str) -> bool {
    matches!(room_version, "3" | "4" | "5" | "6" | "7" | "8" | "9" | "10" | "11" | "12")
}

/// Computes the reference hash of `event` for `room_version` (step 1 + 2 of the
/// algorithm above).
///
/// `event` is the PDU *without* its `event_id` (event IDs are derived from this
/// hash, so the field must not take part in it).  A present `event_id` is
/// removed by the redaction step only if the room version protects it — which
/// every version does — so callers must not pass one.
pub fn compute_reference_hash(room_version: &str, event: &Value) -> Result<[u8; 32], EventIdError> {
    if !uses_reference_hash_event_id(room_version) {
        return if redaction_rules(room_version).is_some() {
            Err(EventIdError::ReferenceHashUnsupported(room_version.to_string()))
        } else {
            Err(EventIdError::UnknownRoomVersion(room_version.to_string()))
        };
    }

    let mut redacted = redact_event(room_version, event)?;
    if let Some(obj) = redacted.as_object_mut() {
        obj.remove("signatures");
        obj.remove("unsigned");
        obj.remove("age_ts");
    }

    let canonical = canonical_json(&redacted).map_err(|e| EventIdError::CanonicalJson(e.to_string()))?;
    Ok(Sha256::digest(canonical.as_bytes()).into())
}

/// Encodes a reference hash as a Matrix event ID (`$` + unpadded Base64).
///
/// Uses the standard Base64 alphabet for room version 3 and the URL-safe
/// alphabet for every later version, matching upstream Synapse.
pub fn encode_reference_hash_event_id(room_version: &str, hash: &[u8]) -> Result<String, EventIdError> {
    if !uses_reference_hash_event_id(room_version) {
        return if redaction_rules(room_version).is_some() {
            Err(EventIdError::ReferenceHashUnsupported(room_version.to_string()))
        } else {
            Err(EventIdError::UnknownRoomVersion(room_version.to_string()))
        };
    }
    let encoded = if room_version == "3" { STANDARD_NO_PAD.encode(hash) } else { URL_SAFE_NO_PAD.encode(hash) };
    Ok(format!("${encoded}"))
}

/// Computes the event ID of `event` for `room_version` (the full algorithm).
pub fn compute_event_id(room_version: &str, event: &Value) -> Result<String, EventIdError> {
    let hash = compute_reference_hash(room_version, event)?;
    encode_reference_hash_event_id(room_version, &hash)
}

/// Resolves the event ID of an event **received from another server**.
///
/// This is the inbound mirror of the local write path's `finalize_event_id`:
/// the receiver must arrive at the same identity the sender computed.
///
/// * **v3+**: the PDU carries **no** `event_id` (spec room v3 "Event format":
///   "the `event_id` field is no longer included. A server receiving an event
///   should compute the relevant event ID for itself"), so the ID is the
///   reference hash of the received event.  A PDU that *does* carry one is
///   malformed — upstream `synapse_rust.events.Event` refuses it outright
///   (`"v2/v3 events must not have an explicit event_id"`, verified against
///   matrix-synapse 1.161.0) — and is rejected here rather than trusted.
/// * **v1/v2**: the sender-assigned `event_id` is authoritative and required.
///
/// Callers must never fabricate an ID for a PDU that cannot supply one: a
/// fabricated ID would not match the one the sender will use in `prev_events`,
/// so every later reference to that event would dangle.
pub fn resolve_received_event_id(room_version: &str, event: &Value) -> Result<String, EventIdError> {
    let carried = event.get("event_id").and_then(Value::as_str);

    if uses_reference_hash_event_id(room_version) {
        if let Some(carried) = carried {
            return Err(EventIdError::UnexpectedEventId {
                room_version: room_version.to_string(),
                event_id: carried.to_string(),
            });
        }
        return compute_event_id(room_version, event);
    }

    match carried {
        Some(carried) => Ok(carried.to_string()),
        None => Err(EventIdError::MissingEventId(room_version.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The event used by upstream Synapse's `test_calculate_event_id`
    /// (`rust/src/events/utils.rs`, release-v1.161).  Together with
    /// [`SYNAPSE_V10_EVENT_ID`] / [`SYNAPSE_V3_EVENT_ID`] this is the
    /// known-answer vector that pins the whole pipeline against an independent
    /// implementation.
    fn synapse_vector_event() -> Value {
        json!({
            "auth_events": [
                "$gbHO7IPUHybc7ULFnT7P0r3iWlZFHGmr6zBfEYCUyKw",
                "$hy1eZFYcgNxMFNNBgCD5fyOzyWRcBkxfNcrUI5ZpZlE",
                "$Nt_z68EwFfPqBeHjzEHGsp461Z4EfNEzR-KH5bOYdOY"
            ],
            "prev_events": ["$4FbpZrgPQTwoLD9H5y7jcikucCypUOn78mXhQX7WliY"],
            "type": "m.room.message",
            "room_id": "!DHmIIVvxFASSFgDGzr:localhost:8008",
            "sender": "@tester_b:localhost:8008",
            "content": {"msgtype": "m.text", "body": "invited people can see history", "m.mentions": {}},
            "depth": 24,
            "origin": "localhost:8008",
            "origin_server_ts": 1731769874137_i64,
            "hashes": {"sha256": "FoYV1w3TW/B2mVT0gX2/BZKpCwrrvGXqXFdUhN9LZYU"},
            "signatures": {
                "localhost:8008": {
                    "ed25519:a_phSE": "G8cfk/m97sndxMNrEZ2nMMSXkVeJE05G7if4JiVzAwGfD3TwnF/jfSHt2acWrpNqv/aEhZug3WLofc2id+rVBw"
                }
            },
            "unsigned": {"age_ts": 1731769874137_i64}
        })
    }

    /// Upstream expectation for room version 10 (URL-safe Base64).
    const SYNAPSE_V10_EVENT_ID: &str = "$zRz9jjiT9wZc3Hl9ij_74aCmTjqV3YMlj9sj3Uqxg6o";
    /// Upstream expectation for room version 3 (standard Base64).
    const SYNAPSE_V3_EVENT_ID: &str = "$zRz9jjiT9wZc3Hl9ij/74aCmTjqV3YMlj9sj3Uqxg6o";

    #[test]
    fn synapse_known_answer_vector_v10() {
        let event = synapse_vector_event();
        assert_eq!(compute_event_id("10", &event).unwrap(), SYNAPSE_V10_EVENT_ID);
    }

    #[test]
    fn synapse_known_answer_vector_v3_uses_standard_base64() {
        let event = synapse_vector_event();
        assert_eq!(compute_event_id("3", &event).unwrap(), SYNAPSE_V3_EVENT_ID);
        // Same digest, different alphabet: this is the only v3/v4 difference.
        assert_eq!(
            compute_reference_hash("3", &event).unwrap(),
            compute_reference_hash("4", &event).unwrap(),
            "the reference hash itself does not depend on the encoding alphabet"
        );
    }

    #[test]
    fn redacted_event_has_the_same_event_id() {
        // Upstream asserts this too: a redacted event keeps its reference hash,
        // which is what makes event IDs stable across redactions.
        let event = synapse_vector_event();
        let redacted = crate::redaction::redact_event("10", &event).unwrap();
        assert_eq!(compute_event_id("10", &redacted).unwrap(), SYNAPSE_V10_EVENT_ID);
    }

    #[test]
    fn signatures_unsigned_and_age_ts_do_not_affect_the_hash() {
        let event = synapse_vector_event();
        let baseline = compute_event_id("10", &event).unwrap();

        let mut stripped = event.clone();
        let obj = stripped.as_object_mut().unwrap();
        obj.remove("signatures");
        obj.remove("unsigned");
        assert_eq!(compute_event_id("10", &stripped).unwrap(), baseline);

        let mut different_sig = event.clone();
        different_sig["signatures"] = json!({});
        different_sig["unsigned"] = json!({"age_ts": 1, "transaction_id": "t"});
        assert_eq!(compute_event_id("10", &different_sig).unwrap(), baseline);

        // `hashes` is a *protected* top-level field: unlike signatures/unsigned
        // it survives redaction and therefore must change the event ID.
        let mut without_hashes = event.clone();
        without_hashes.as_object_mut().unwrap().remove("hashes");
        assert_ne!(compute_event_id("10", &without_hashes).unwrap(), baseline);
    }

    #[test]
    fn unprotected_content_does_not_affect_the_hash_but_depth_does() {
        let event = synapse_vector_event();
        let baseline = compute_event_id("10", &event).unwrap();

        // m.room.message content is fully stripped by redaction.
        let mut other_body = event.clone();
        other_body["content"] = json!({"msgtype": "m.text", "body": "totally different"});
        assert_eq!(compute_event_id("10", &other_body).unwrap(), baseline);

        // depth is a protected top-level field.
        let mut other_depth = event.clone();
        other_depth["depth"] = json!(25);
        assert_ne!(compute_event_id("10", &other_depth).unwrap(), baseline);
    }

    #[test]
    fn v11_differs_from_v10_because_origin_is_no_longer_protected() {
        let event = synapse_vector_event();
        assert_ne!(compute_event_id("11", &event).unwrap(), SYNAPSE_V10_EVENT_ID);

        // v11 redacts `origin` away, so a v11 event that still carries `origin`
        // must hash exactly like the same v10 event with `origin` removed.
        let mut without_origin = event.clone();
        without_origin.as_object_mut().unwrap().remove("origin");
        assert_eq!(compute_event_id("11", &event).unwrap(), compute_event_id("10", &without_origin).unwrap());
    }

    #[test]
    fn event_id_is_unpadded_and_prefixed() {
        let event = synapse_vector_event();
        let id = compute_event_id("10", &event).unwrap();
        assert!(id.starts_with('$'));
        assert!(!id.contains('='), "unpadded Base64 must not contain `=`");
        assert!(!id.contains(':'), "v4+ event IDs carry no origin suffix");
        assert_eq!(id.len(), 1 + 43, "sha256 in unpadded Base64 is 43 characters");
    }

    #[test]
    fn v1_and_v2_have_no_reference_hash_and_unknown_versions_fail_closed() {
        let event = synapse_vector_event();
        for version in ["1", "2"] {
            assert_eq!(
                compute_reference_hash(version, &event).unwrap_err(),
                EventIdError::ReferenceHashUnsupported(version.to_string()),
                "v{version}"
            );
            assert!(!uses_reference_hash_event_id(version));
        }
        for version in ["0", "13", "org.example.unknown", ""] {
            assert_eq!(
                compute_reference_hash(version, &event).unwrap_err(),
                EventIdError::UnknownRoomVersion(version.to_string()),
                "v{version}"
            );
            assert!(!uses_reference_hash_event_id(version));
        }
    }

    #[test]
    fn encoding_takes_the_alphabet_from_the_room_version() {
        let hash = [0xff_u8; 32];
        assert!(encode_reference_hash_event_id("3", &hash).unwrap().contains('/'));
        assert!(encode_reference_hash_event_id("4", &hash).unwrap().contains('_'));
        assert!(!encode_reference_hash_event_id("11", &hash).unwrap().contains('/'));
        assert_eq!(encode_reference_hash_event_id("10", &hash).unwrap(), format!("${}", URL_SAFE_NO_PAD.encode(hash)));
        assert_eq!(
            encode_reference_hash_event_id("1", &hash).unwrap_err(),
            EventIdError::ReferenceHashUnsupported("1".to_string())
        );
        assert_eq!(
            encode_reference_hash_event_id("13", &hash).unwrap_err(),
            EventIdError::UnknownRoomVersion("13".to_string())
        );
    }

    // ── resolve_received_event_id (inbound identity) ──────────────────

    /// v3+ PDUs carry no `event_id`: the receiver derives it, and the derived
    /// value is exactly what the local write path would have produced for the
    /// same bytes (upstream known-answer vector).
    #[test]
    fn received_v3_plus_pdu_derives_the_reference_hash() {
        let event = synapse_vector_event();
        assert!(event.get("event_id").is_none(), "the upstream vector carries no event_id");
        assert_eq!(resolve_received_event_id("10", &event).unwrap(), SYNAPSE_V10_EVENT_ID);
        assert_eq!(resolve_received_event_id("3", &event).unwrap(), SYNAPSE_V3_EVENT_ID);
    }

    /// A v3+ PDU that carries `event_id` is malformed; upstream refuses to
    /// parse it, so trusting the field would let a sender pick its own identity.
    #[test]
    fn received_v3_plus_pdu_rejects_a_carried_event_id() {
        let mut event = synapse_vector_event();
        event["event_id"] = serde_json::json!("$attacker_chosen:example.com");
        assert_eq!(
            resolve_received_event_id("10", &event).unwrap_err(),
            EventIdError::UnexpectedEventId {
                room_version: "10".to_string(),
                event_id: "$attacker_chosen:example.com".to_string(),
            }
        );
    }

    /// v1/v2 keep the sender-assigned ID, and it is required.
    #[test]
    fn received_v1_v2_pdu_uses_the_carried_id_and_requires_it() {
        let mut event = synapse_vector_event();
        event["event_id"] = serde_json::json!("$0:example.com");
        assert_eq!(resolve_received_event_id("1", &event).unwrap(), "$0:example.com");
        assert_eq!(resolve_received_event_id("2", &event).unwrap(), "$0:example.com");

        event.as_object_mut().unwrap().remove("event_id");
        assert_eq!(resolve_received_event_id("2", &event).unwrap_err(), EventIdError::MissingEventId("2".to_string()));
    }
}
