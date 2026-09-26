//! U-13 step 2: derive a local event's identity (`event_id`) and `hashes` in one
//! place.
//!
//! The chain is fixed by the Matrix specification and by upstream Synapse; each
//! step feeds the next, so it must not be split across call sites:
//!
//! 1. assemble the PDU with its final graph fields and **without `event_id`**
//!    for v3+ ([`synapse_common::pdu::build_pdu`]);
//! 2. `hashes.sha256` = the **unredacted** content hash
//!    ([`compute_event_content_hash`], which removes `age_ts`/`unsigned`/
//!    `signatures`/`hashes`/`outlier`/`destinations`);
//! 3. `event_id` = `"$"` + unpadded Base64 of the SHA-256 of the **redacted**
//!    event (minus `signatures`/`unsigned`/`age_ts`) — i.e.
//!    [`synapse_common::event_id::compute_event_id`] over the PDU carrying the
//!    step-2 `hashes` value.  v1/v2 keep their server-assigned random ID.
//!
//! Upstream reference: `element-hq/synapse` release-v1.161
//! `synapse/crypto/event_signing.py` (`compute_content_hash`,
//! `compute_event_reference_hash`) and spec room v3/v4 "Event IDs".

use serde_json::{json, Value};
use synapse_common::event_id::{compute_event_id, uses_reference_hash_event_id};
use synapse_common::pdu::{build_pdu, PduParts};

use crate::signing::compute_event_content_hash;

/// A local event's derived identity and content hash.
#[derive(Debug, Clone, PartialEq)]
pub struct FinalizedPdu {
    /// The event ID to persist (and, for v1/v2, to send inside the PDU).
    pub event_id: String,
    /// The `hashes` object to persist (`{"sha256": …}`).
    pub hashes: Value,
}

/// Errors from [`finalize_local_pdu`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FinalizeError {
    /// v1/v2 need the caller's server-assigned ID, but none was supplied.
    #[error("room version {0} requires a server-assigned event_id in PduParts")]
    MissingServerAssignedEventId(String),
    /// The assembled PDU could not be hashed (not an object / non-canonical JSON).
    #[error("could not compute the content hash: {0}")]
    ContentHash(&'static str),
    /// The reference hash could not be computed.
    #[error("could not compute the reference hash: {0}")]
    ReferenceHash(String),
}

/// Runs steps 1–3 for one locally-produced event.
///
/// Deterministic: the same parts always produce the same ID and hashes, which is
/// exactly what lets a receiving server recompute them from the PDU it is sent.
pub fn finalize_local_pdu(parts: &PduParts<'_>) -> Result<FinalizedPdu, FinalizeError> {
    let mut pdu = build_pdu(parts);

    let content_hash =
        compute_event_content_hash(&pdu).ok_or(FinalizeError::ContentHash("event is not a JSON object"))?;
    let hashes = json!({ "sha256": content_hash });
    if let Some(object) = pdu.as_object_mut() {
        object.insert("hashes".to_string(), hashes.clone());
    }

    let event_id = if uses_reference_hash_event_id(parts.room_version) {
        compute_event_id(parts.room_version, &pdu).map_err(|error| FinalizeError::ReferenceHash(error.to_string()))?
    } else {
        parts
            .event_id
            .ok_or_else(|| FinalizeError::MissingServerAssignedEventId(parts.room_version.to_string()))?
            .to_string()
    };

    Ok(FinalizedPdu { event_id, hashes })
}

#[cfg(test)]
mod tests {
    use super::*;
    use synapse_common::event_id::{compute_event_id, uses_reference_hash_event_id};

    fn parts<'a>(room_version: &'a str, event_id: Option<&'a str>) -> PduParts<'a> {
        let prev: &'static [String] = Box::leak(vec!["$prev:example.com".to_string()].into_boxed_slice());
        let auth: &'static [String] = Box::leak(vec!["$create:example.com".to_string()].into_boxed_slice());
        PduParts {
            room_version,
            event_id,
            room_id: "!r:example.com",
            sender: "@u:example.com",
            event_type: "m.room.message",
            content: Box::leak(Box::new(json!({"msgtype": "m.text", "body": "hi"}))),
            state_key: None,
            origin_server_ts: 1_731_769_874_137,
            origin: "example.com",
            depth: 7,
            prev_events: prev,
            auth_events: auth,
            redacts: None,
        }
    }

    #[test]
    fn v3_plus_derives_the_id_and_ignores_the_placeholder() {
        let finalized = finalize_local_pdu(&parts("10", Some("$placeholder:example.com"))).unwrap();

        assert!(finalized.event_id.starts_with('$'));
        assert_ne!(finalized.event_id, "$placeholder:example.com");
        assert_eq!(finalized.event_id.len(), 44, "`$` + 43 base64 chars: {}", finalized.event_id);
        assert!(!finalized.event_id.contains(':'), "v4+ IDs carry no origin suffix");
        assert!(!finalized.event_id.contains('='), "unpadded Base64 only");
        assert!(finalized.hashes["sha256"].as_str().is_some());
    }

    /// Acceptance item 4: the ID the finalizer hands out must equal the ID
    /// recomputed from the very PDU it produced (that is the receiver's view).
    #[test]
    fn derived_id_is_self_consistent_with_the_emitted_pdu() {
        for version in ["3", "4", "9", "10", "11", "12"] {
            let source = parts(version, None);
            let finalized = finalize_local_pdu(&source).unwrap();

            // Rebuild the same PDU, inject the finalized hashes, and recompute.
            let mut emitted = build_pdu(&source);
            emitted.as_object_mut().unwrap().insert("hashes".to_string(), finalized.hashes.clone());
            assert_eq!(
                compute_event_id(version, &emitted).unwrap(),
                finalized.event_id,
                "v{version}: recomputation from the emitted PDU must match the assigned id"
            );
            assert!(uses_reference_hash_event_id(version));
        }
    }

    #[test]
    fn v1_and_v2_keep_the_server_assigned_id() {
        let finalized = finalize_local_pdu(&parts("1", Some("$0:domain"))).unwrap();
        assert_eq!(finalized.event_id, "$0:domain");
        // Upstream `add_hashes_and_signatures` also sets `hashes` for v1/v2.
        assert!(finalized.hashes["sha256"].as_str().is_some());

        let missing = finalize_local_pdu(&parts("2", None)).unwrap_err();
        assert_eq!(missing, FinalizeError::MissingServerAssignedEventId("2".to_string()));
    }

    /// Changing any protected field must change the ID; the content hash is part
    /// of it (that is why the chain order matters).
    #[test]
    fn id_reacts_to_graph_fields_and_hashes() {
        let baseline = finalize_local_pdu(&parts("10", None)).unwrap();

        let mut deeper = parts("10", None);
        deeper.depth = 8;
        assert_ne!(finalize_local_pdu(&deeper).unwrap().event_id, baseline.event_id);

        let mut other_prev: Vec<String> = vec!["$other:example.com".to_string()];
        let mut different_parents = parts("10", None);
        different_parents.prev_events = &other_prev;
        assert_ne!(finalize_local_pdu(&different_parents).unwrap().event_id, baseline.event_id);

        // `hashes` is protected by the redaction algorithm, so a different
        // content hash implies a different ID.
        let mut emitted = build_pdu(&parts("10", None));
        emitted.as_object_mut().unwrap().insert("hashes".to_string(), json!({"sha256": "some-other-hash"}));
        assert_ne!(compute_event_id("10", &emitted).unwrap(), baseline.event_id);

        other_prev.clear();
    }

    #[test]
    fn hashes_match_the_unredacted_content_hash() {
        let source = parts("10", None);
        let finalized = finalize_local_pdu(&source).unwrap();
        let pdu = build_pdu(&source);
        let expected = compute_event_content_hash(&pdu).unwrap();
        assert_eq!(finalized.hashes["sha256"], json!(expected));
    }
}
