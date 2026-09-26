//! U-13 step 3 — cross-implementation interop fixtures.
//!
//! The ideal gate is a live `/send_join` + `/send` run against a peer Synapse,
//! but this sandbox cannot host one: `/etc/hosts` is not writable, `sudo` is
//! blocked, `*.localhost` does not resolve, Docker Hub is unreachable, and this
//! repository's federation client has no custom-CA / skip-verification knob.
//!
//! The strongest achievable substitute is to verify the bytes **our** pipeline
//! emits with the peer's own implementation:
//!
//! 1. this test rebuilds the fixture PDUs from fixed inputs through the real
//!    pipeline (`build_pdu` → `finalize_local_pdu` → `sign_and_hash_event`) and
//!    asserts they equal the committed JSON byte-for-byte, so a fixture can
//!    never rot away from the code that produced it;
//! 2. `scripts/interop/verify_pdu_with_upstream_synapse.py` feeds the same JSON
//!    to a real `matrix-synapse==1.161.0` and re-derives `hashes`, the event ID
//!    and the server signature with Synapse's own functions.
//!
//! Regenerate the fixtures after an intentional pipeline change:
//!
//! ```text
//! U13_WRITE_INTEROP_FIXTURE=1 cargo nextest run --test unit \
//!     -E 'test(/u13_interop_fixture/)'
//! ```

use serde_json::{json, Value};
use synapse_common::pdu::{build_pdu, PduParts};

/// Deterministic ed25519 signing seed (test key only — never a real key), in
/// the canonical unpadded Base64 form `sign_and_hash_event` takes. The bytes
/// encode the `[7u8; 32]` seed.
const SIGNING_SEED_B64: &str = "BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc";
const SERVER_NAME: &str = "example.com";
const KEY_ID: &str = "ed25519:1";
const ROOM_ID: &str = "!interop:example.com";
const SENDER: &str = "@alice:example.com";

fn fixture_path(room_version: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/interop/fixtures")
        .join(format!("local_pdu_v{room_version}.json"))
}

/// Build one PDU through the production pipeline.
///
/// The inputs are fixed so the result is reproducible; `room_version` selects
/// the redaction rules (hence the ID and the signed bytes).
fn build_fixture_pdu(room_version: &str) -> (String, Value) {
    let prev_events = vec!["$prev:example.com".to_string()];
    let auth_events = vec!["$create:example.com".to_string()];
    let content = json!({
        "msgtype": "m.text",
        "body": "U-13 interop fixture",
        "m.mentions": {},
    });

    let parts = PduParts {
        room_version,
        event_id: None,
        room_id: ROOM_ID,
        sender: SENDER,
        event_type: "m.room.message",
        content: &content,
        state_key: None,
        origin_server_ts: 1_731_769_874_137,
        origin: SERVER_NAME,
        depth: 24,
        prev_events: &prev_events,
        auth_events: &auth_events,
        redacts: None,
    };

    let finalized =
        synapse_federation::event_finalize::finalize_local_pdu(&parts).expect("the fixture PDU must finalize");

    let mut pdu = build_pdu(&parts);
    if let Some(object) = pdu.as_object_mut() {
        object.insert("hashes".to_string(), finalized.hashes.clone());
    }
    synapse_federation::signing::sign_and_hash_event(room_version, SERVER_NAME, KEY_ID, SIGNING_SEED_B64, &mut pdu)
        .expect("the fixture PDU must sign");

    (finalized.event_id, pdu)
}

fn build_fixture(room_version: &str) -> Value {
    let (event_id, pdu) = build_fixture_pdu(room_version);
    json!({
        "produced_by": "tests/unit/u13_interop_fixture_tests.rs (build_pdu + finalize_local_pdu + sign_and_hash_event)",
        "room_version": room_version,
        "signing_server_name": SERVER_NAME,
        "signing_key_id": KEY_ID,
        "signing_seed_base64": SIGNING_SEED_B64,
        "event_id": event_id,
        "pdu": pdu,
    })
}

/// Regenerate or verify every fixture.  Fails with the exact regeneration
/// command when the committed fixture no longer matches the pipeline.
#[test]
fn u13_interop_fixtures_match_the_pipeline() {
    let write = std::env::var("U13_WRITE_INTEROP_FIXTURE").as_deref() == Ok("1");

    // v3 exercises the standard Base64 alphabet; v10/v11 the URL-safe one that
    // every later version uses.
    for room_version in ["3", "10", "11"] {
        let expected = build_fixture(room_version);
        let path = fixture_path(room_version);

        if write {
            std::fs::create_dir_all(path.parent().expect("fixture path has a parent")).unwrap();
            std::fs::write(&path, format!("{}\n", serde_json::to_string_pretty(&expected).unwrap())).unwrap();
            continue;
        }

        let raw = std::fs::read_to_string(&path).unwrap_or_else(|error| {
            panic!(
                "{} unreadable ({error}); regenerate with:\n  \
                 U13_WRITE_INTEROP_FIXTURE=1 cargo nextest run --test unit -E 'test(/u13_interop_fixture/)'",
                path.display()
            )
        });
        let committed: Value = serde_json::from_str(&raw).expect("fixture must be valid JSON");
        assert_eq!(
            committed,
            expected,
            "{} is stale — regenerate it with:\n  \
             U13_WRITE_INTEROP_FIXTURE=1 cargo nextest run --test unit -E 'test(/u13_interop_fixture/)'",
            path.display()
        );
    }
}
