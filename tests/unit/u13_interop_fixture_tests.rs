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
//!    and the server signature with Synapse's own functions — plus, for v12,
//!    upstream's own `m.room.create` auth rules (MSC4291) and the
//!    `auth_events`-must-not-name-the-create-event rule (MSC4307).
//!
//! Regenerate the fixtures after an intentional pipeline change:
//!
//! ```text
//! U13_WRITE_INTEROP_FIXTURE=1 cargo nextest run --test unit \
//!     -E 'test(/u13_interop_fixture/)'
//! ```

use serde_json::{json, Value};
use synapse_common::pdu::{build_pdu, PduParts};
use synapse_common::room_id::{is_domainless_room_id, room_id_from_create_event_id};

/// Canonical unpadded Base64 of a deterministic test signing key (7u8 x 32).
const SIGNING_SEED_B64: &str = "BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc";
const SERVER_NAME: &str = "example.com";
const KEY_ID: &str = "ed25519:1";
/// Room ID for room versions 1–11: the historical `!opaque:server` form.
const LEGACY_ROOM_ID: &str = "!interop:example.com";
/// Room ID for room version 12+: `!` + 43 unpadded URL-safe Base64 characters,
/// with **no** `:server` part (MSC4291). The test asserts this through the shared
/// grammar helper, so a typo here cannot silently produce an invalid fixture.
const DOMAINLESS_ROOM_ID: &str = "!31hneApxJ_1o-63DmFrpeqnkFfWppnzWso1JvH3ogLM";
const SENDER: &str = "@alice:example.com";
const ORIGIN_SERVER_TS: i64 = 1_731_769_874_137;

fn fixture_path(room_version: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/interop/fixtures")
        .join(format!("local_pdu_v{room_version}.json"))
}

fn create_fixture_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/interop/fixtures").join("local_pdu_v12_create.json")
}

/// The room id a message PDU for this version must carry.
///
/// v12 moved room IDs to the domainless form (MSC4291), so a fixture that kept
/// the legacy spelling would not be a v12 vector at all.
fn room_id_for(room_version: &str) -> &'static str {
    if synapse_common::room_versions::room_version_at_least(room_version, 12) {
        DOMAINLESS_ROOM_ID
    } else {
        LEGACY_ROOM_ID
    }
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
        room_id: room_id_for(room_version),
        sender: SENDER,
        event_type: "m.room.message",
        content: &content,
        state_key: None,
        origin_server_ts: ORIGIN_SERVER_TS,
        origin: SERVER_NAME,
        depth: 24,
        prev_events: &prev_events,
        auth_events: &auth_events,
        redacts: None,
    };

    finalize_and_sign(&parts)
}

/// Build a v12 `m.room.create` PDU through the production pipeline.
///
/// This is the MSC4291 vector: the create event is the **only** event whose
/// `room_id` is not a field — it is derived from the event's own id by swapping
/// the `$` sigil for `!`. So a v12 create must emit neither `room_id` (D-6) nor
/// `event_id` (v3+ wire shape), while the room id still has to come out of the
/// reference hash.
///
/// The `room_id` handed to `PduParts` is therefore never written to the bytes;
/// it is the value a caller would have to invent before the id exists. The test
/// asserts the field is absent, so a regression that starts emitting it fails
/// here rather than silently producing a circular id.
fn build_fixture_v12_create() -> (String, Value, String) {
    let content = json!({
        "creator": SENDER,
        "room_version": "12",
    });

    let parts = PduParts {
        room_version: "12",
        event_id: None,
        room_id: DOMAINLESS_ROOM_ID,
        sender: SENDER,
        event_type: "m.room.create",
        content: &content,
        state_key: Some(""),
        origin_server_ts: ORIGIN_SERVER_TS,
        origin: SERVER_NAME,
        depth: 1,
        prev_events: &[],
        auth_events: &[],
        redacts: None,
    };

    let (event_id, pdu) = finalize_and_sign(&parts);
    let derived_room_id =
        room_id_from_create_event_id(&event_id).expect("a finalized create event id must derive a room id");
    (event_id, pdu, derived_room_id)
}

/// Run a `PduParts` through finalize + sign, returning `(event_id, pdu)`.
fn finalize_and_sign(parts: &PduParts<'_>) -> (String, Value) {
    let finalized =
        synapse_federation::event_finalize::finalize_local_pdu(parts).expect("the fixture PDU must finalize");

    let mut pdu = build_pdu(parts);
    if let Some(object) = pdu.as_object_mut() {
        object.insert("hashes".to_string(), finalized.hashes.clone());
    }
    synapse_federation::signing::sign_and_hash_event(
        parts.room_version,
        SERVER_NAME,
        KEY_ID,
        SIGNING_SEED_B64,
        &mut pdu,
    )
    .expect("the fixture PDU must sign");

    (finalized.event_id, pdu)
}

fn envelope(room_version: &str, event_id: &str, pdu: &Value, extra: &Value) -> Value {
    let mut envelope = json!({
        "produced_by": "tests/unit/u13_interop_fixture_tests.rs (build_pdu + finalize_local_pdu + sign_and_hash_event)",
        "room_version": room_version,
        "signing_server_name": SERVER_NAME,
        "signing_key_id": KEY_ID,
        "signing_seed_base64": SIGNING_SEED_B64,
        "event_id": event_id,
        "pdu": pdu,
    });
    if let (Some(object), Value::Object(extra)) = (envelope.as_object_mut(), extra) {
        for (key, value) in extra {
            object.insert(key.clone(), value.clone());
        }
    }
    envelope
}

fn build_fixture(room_version: &str) -> Value {
    let (event_id, pdu) = build_fixture_pdu(room_version);
    envelope(room_version, &event_id, &pdu, &json!({}))
}

fn build_create_fixture() -> Value {
    let (event_id, pdu, derived_room_id) = build_fixture_v12_create();
    envelope("12", &event_id, &pdu, &json!({ "derived_room_id": derived_room_id }))
}

/// Read a committed fixture, or write it when regenerating.
///
/// Returns `None` while regenerating (nothing to compare against).
fn read_or_write(path: &std::path::Path, expected: &Value) -> Option<Value> {
    if std::env::var("U13_WRITE_INTEROP_FIXTURE").as_deref() == Ok("1") {
        std::fs::create_dir_all(path.parent().expect("fixture path has a parent")).unwrap();
        std::fs::write(path, format!("{}\n", serde_json::to_string_pretty(expected).unwrap())).unwrap();
        return None;
    }

    let raw = std::fs::read_to_string(path).unwrap_or_else(|error| {
        panic!(
            "{} unreadable ({error}); regenerate with:\n  \
             U13_WRITE_INTEROP_FIXTURE=1 cargo nextest run --test unit -E 'test(/u13_interop_fixture/)'",
            path.display()
        )
    });
    Some(serde_json::from_str(&raw).expect("fixture must be valid JSON"))
}

fn assert_committed(path: &std::path::Path, expected: &Value) {
    let Some(committed) = read_or_write(path, expected) else {
        return;
    };
    assert_eq!(
        committed,
        *expected,
        "{} is stale — regenerate it with:\n  \
         U13_WRITE_INTEROP_FIXTURE=1 cargo nextest run --test unit -E 'test(/u13_interop_fixture/)'",
        path.display()
    );
}

/// Regenerate or verify every fixture.  Fails with the exact regeneration
/// command when the committed fixture no longer matches the pipeline.
#[test]
fn u13_interop_fixtures_match_the_pipeline() {
    // v3 exercises the standard Base64 alphabet; v10/v11 the URL-safe one that
    // every later version uses. v12 additionally moves the room ID to the
    // domainless form (MSC4291), so its fixture carries a `!` + 43 chars id.
    for room_version in ["3", "10", "11", "12"] {
        assert_committed(&fixture_path(room_version), &build_fixture(room_version));
    }

    assert_committed(&create_fixture_path(), &build_create_fixture());
}

/// The v12 create vector's invariant: `room_id` is derived, never emitted.
///
/// This is the repo-side half of A-3; the other half is the oracle
/// (`scripts/interop/verify_pdu_with_upstream_synapse.py`), which feeds these
/// bytes to upstream Synapse's own `_check_create` and asserts that a create
/// carrying a `room_id` is rejected.
#[test]
fn v12_create_fixture_derives_its_room_id_and_omits_both_ids() {
    // A property the fixture data must satisfy: the message fixture's room id is
    // a well-formed domainless (v12) room id, not a legacy spelling.
    assert!(
        is_domainless_room_id(DOMAINLESS_ROOM_ID),
        "the v12 fixture room id must be `!` + 43 URL-safe base64 chars: {DOMAINLESS_ROOM_ID}"
    );
    assert!(
        synapse_common::room_id::is_well_formed_room_id(DOMAINLESS_ROOM_ID),
        "the shared grammar must accept the v12 fixture room id"
    );

    let (event_id, pdu, derived_room_id) = build_fixture_v12_create();

    assert!(pdu.get("room_id").is_none(), "a v12 create PDU must not carry room_id: {pdu}");
    assert!(pdu.get("event_id").is_none(), "v12 create PDUs must not carry event_id on the wire: {pdu}");
    assert_eq!(pdu["type"], json!("m.room.create"));
    assert_eq!(pdu["state_key"], json!(""));

    // `$` + 43 URL-safe base64, and the room id is the same string with `!`.
    let body = event_id.strip_prefix('$').expect("a v12 create event id starts with `$`");
    assert_eq!(body.len(), synapse_common::room_id::DOMAINLESS_ROOM_ID_LEN, "reference-hash body length");
    assert!(body.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'), "URL-safe base64 body");
    assert_eq!(derived_room_id, format!("!{body}"));
    assert_eq!(
        room_id_from_create_event_id(&event_id).expect("derivation must succeed"),
        derived_room_id,
        "the room id is the create event id with `$` swapped for `!`"
    );
    assert!(is_domainless_room_id(&derived_room_id));
}
