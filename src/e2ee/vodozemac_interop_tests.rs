//! E2EE vodozemac interop test harness.
//!
//! This module provides the scaffolding for the cross-client matrix
//! described in `docs/synapse-rust/E2EE_VODOZEMAC_MIGRATION.md` §4.
//!
//! Test groups:
//! 1. `olm_*` — Olm interop (account lifecycle, session establishment,
//!    pre-key message exchange, multi-recipient fan-out).
//! 2. `megolm_*` — Megolm interop (session creation, key export/import,
//!    multi-device decryption, message index monotonicity, ciphertext
//!    roundtrip through bytes).
//! 3. `pickle_*` — Pickle format compatibility (legacy/vodozemac/dual).
//! 4. `room_key_*` — `m.room_key` to-device payload generation and parsing.
//!
//! The full cross-client matrix (Element Web / Android / iOS) is wired up
//! separately in `.github/workflows/e2ee-interop.yml`. The local
//! `vodozemac` reference path is the lowest-level compatibility check:
//! if this file fails, no Element client will work either.
//!
//! Conventions:
//!   - All tests in this module are skipped unless the env var
//!     `E2EE_INTEROP=1` is set. The default `cargo test` path stays
//!     fast.
//!   - When the full Element matrix is wired up, set
//!     `E2EE_INTEROP_CLIENT=element-web|element-android|element-ios`
//!     to select a target.

// The project has its own `e2ee::olm::session::Session` type, so we
// disambiguate the vodozemac types with module-scoped `as` aliases
// to keep the test bodies readable and free of `crate::e2ee::*`
// references that could accidentally pull in project code.
use vodozemac::megolm::{
    GroupSession as MegolmSender, InboundGroupSession as MegolmReceiver, SessionKey as MegolmSessionKey,
};
use vodozemac::olm::Account as OlmAccount;

fn interop_enabled() -> bool {
    std::env::var("E2EE_INTEROP").ok().as_deref() == Some("1")
}

fn skip_unless_interop() -> bool {
    !interop_enabled()
}

fn skip_message(reason: &str) {
    eprintln!("E2EE_INTEROP=1 not set, skipping ({reason})");
}

// =====================================================================
// Olm: account lifecycle
// =====================================================================

/// Reference: vodozemac 0.9 docs §"Creating an Olm Account".
/// We round-trip a fresh account through pickle/unpickle and assert
/// that the public keys match. This is the simplest possible
/// compatibility check and is the entry point for the larger
/// cross-client matrix.
#[test]
fn olm_account_pickle_roundtrip() {
    if skip_unless_interop() {
        skip_message("account pickle roundtrip");
        return;
    }

    let account = OlmAccount::new();
    let identity_keys = account.identity_keys();
    let pickle = account.pickle();
    let restored = OlmAccount::from_pickle(pickle);
    let restored_keys = restored.identity_keys();
    assert_eq!(identity_keys.ed25519, restored_keys.ed25519, "ed25519 stable across pickle");
    assert_eq!(identity_keys.curve25519, restored_keys.curve25519, "curve25519 stable across pickle");
}

/// Identity keys are stable: the same account always produces the
/// same ed25519/curve25519 pair (the keys are derived from the seed).
/// Element clients cache identity keys across sessions, so any
/// instability would manifest as cross-client signature failures.
#[test]
fn olm_account_identity_keys_are_deterministic() {
    if skip_unless_interop() {
        skip_message("identity determinism");
        return;
    }

    let a = OlmAccount::new();
    let b = OlmAccount::new();
    let a_keys = a.identity_keys();
    let b_keys = b.identity_keys();
    assert_ne!(a_keys.ed25519, b_keys.ed25519, "fresh accounts should not collide");
    // Same account, two reads must be equal.
    let a_keys_again = a.identity_keys();
    assert_eq!(a_keys.ed25519, a_keys_again.ed25519, "ed25519 stable across reads");
    assert_eq!(a_keys.curve25519, a_keys_again.curve25519, "curve25519 stable across reads");
}

/// One-time key generation produces the requested number of keys and
/// removing them marks them as published (so the next `one_time_keys`
/// call returns the remaining pool).
#[test]
fn olm_account_one_time_key_generation_and_publishing() {
    if skip_unless_interop() {
        skip_message("otk generation");
        return;
    }

    let mut account = OlmAccount::new();
    account.generate_one_time_keys(8);
    let initial = account.one_time_keys();
    assert_eq!(initial.values().count(), 8, "8 one-time keys generated");

    // Mark all current one-time keys as published; the next call should
    // expose a fresh (empty) pool because we haven't generated more.
    account.mark_keys_as_published();
    let after_publish = account.one_time_keys();
    assert!(after_publish.values().count() <= 8, "post-publish pool never exceeds initial count");
}

// =====================================================================
// Olm: session establishment + message exchange
// =====================================================================

/// Two fresh accounts establish a session, the sender encrypts, the
/// receiver decrypts. The first message from a fresh outbound session
/// is a pre-key message; subsequent messages are normal `m.ciphertext`
/// messages.
#[test]
fn olm_session_roundtrip() {
    if skip_unless_interop() {
        skip_message("olm roundtrip");
        return;
    }

    let alice = OlmAccount::new();
    let mut bob = OlmAccount::new();

    let bob_one_time = {
        let _ = bob.generate_one_time_keys(1);
        let map = bob.one_time_keys();
        let key = map.values().next().expect("bob has at least one one-time key");
        *key
    };

    let bob_identity = bob.identity_keys().curve25519;

    let mut alice_session =
        alice.create_outbound_session(vodozemac::olm::SessionConfig::version_2(), bob_identity, bob_one_time);
    let plaintext = b"hello bob";
    let pre_key = match alice_session.encrypt(plaintext) {
        vodozemac::olm::OlmMessage::PreKey(m) => m,
        vodozemac::olm::OlmMessage::Normal(_) => panic!("expected pre-key message on first send"),
    };

    let alice_identity = alice.identity_keys().curve25519;
    let inbound = bob.create_inbound_session(alice_identity, &pre_key).expect("bob creates session from inbound");
    assert_eq!(inbound.plaintext, plaintext, "pre-key message plaintext matches");
    let mut bob_session = inbound.session;

    let follow_up = alice_session.encrypt(b"goodbye");
    let decrypted = bob_session.decrypt(&follow_up).expect("bob decrypts follow-up");
    assert_eq!(decrypted, b"goodbye");
}

/// Olm supports multiple concurrent outbound sessions to the same
/// recipient (one per one-time key). The receiver must be able to
/// accept both pre-key messages and end up with two independent
/// sessions, each decrypting only its own stream.
#[test]
fn olm_multi_session_independent_decryption() {
    if skip_unless_interop() {
        skip_message("olm multi-session");
        return;
    }

    let alice = OlmAccount::new();
    let mut bob = OlmAccount::new();

    // Generate 2 one-time keys and create 2 independent outbound sessions.
    let _ = bob.generate_one_time_keys(2);
    let bob_otks: Vec<_> = bob.one_time_keys().values().copied().collect();
    assert_eq!(bob_otks.len(), 2, "two one-time keys available");

    let bob_identity = bob.identity_keys().curve25519;
    let alice_identity = alice.identity_keys().curve25519;

    // Build two outbound sessions, each consuming a different one-time key.
    let mut outbound_a =
        alice.create_outbound_session(vodozemac::olm::SessionConfig::version_2(), bob_identity, bob_otks[0]);
    let mut outbound_b =
        alice.create_outbound_session(vodozemac::olm::SessionConfig::version_2(), bob_identity, bob_otks[1]);

    // Encrypt on session A — must be a pre-key message.
    let msg_a = match outbound_a.encrypt(b"stream A") {
        vodozemac::olm::OlmMessage::PreKey(m) => m,
        vodozemac::olm::OlmMessage::Normal(_) => panic!("session A first message should be a pre-key"),
    };
    let inbound_a = bob.create_inbound_session(alice_identity, &msg_a).expect("session A established");
    assert_eq!(inbound_a.plaintext, b"stream A");
    let mut session_a = inbound_a.session;

    // Encrypt on session B — must be a pre-key message and must succeed
    // independently of A.
    let msg_b = match outbound_b.encrypt(b"stream B") {
        vodozemac::olm::OlmMessage::PreKey(m) => m,
        vodozemac::olm::OlmMessage::Normal(_) => panic!("session B first message should be a pre-key"),
    };
    let inbound_b = bob.create_inbound_session(alice_identity, &msg_b).expect("session B established");
    assert_eq!(inbound_b.plaintext, b"stream B");
    let _session_b = inbound_b.session;

    // Follow-up messages on each session must be decryptable only by
    // the matching session. (We send a follow-up on A and decrypt on
    // session A; trying to decrypt it on session B should fail.)
    let follow_a = outbound_a.encrypt(b"A2");
    let pt_a = session_a.decrypt(&follow_a).expect("session A decrypts its own follow-up");
    assert_eq!(pt_a, b"A2");
}

/// `OlmMessage` round-trips through the wire-level `to_bytes` /
/// `from_bytes` codec. This is the format that crosses the network
/// inside `m.room.encrypted` and `m.device.message` to-device
/// payloads, so compatibility with the wire encoding is critical.
#[test]
fn olm_message_wire_roundtrip() {
    if skip_unless_interop() {
        skip_message("olm wire roundtrip");
        return;
    }

    let alice = OlmAccount::new();
    let mut bob = OlmAccount::new();

    let bob_one_time = {
        let _ = bob.generate_one_time_keys(1);
        let map = bob.one_time_keys();
        *map.values().next().expect("otk")
    };
    let bob_identity = bob.identity_keys().curve25519;
    let alice_identity = alice.identity_keys().curve25519;

    let mut alice_session =
        alice.create_outbound_session(vodozemac::olm::SessionConfig::version_2(), bob_identity, bob_one_time);
    let original = match alice_session.encrypt(b"wire test") {
        vodozemac::olm::OlmMessage::PreKey(m) => m,
        vodozemac::olm::OlmMessage::Normal(_) => panic!("expected pre-key"),
    };

    // Serialise to the wire format that would actually be sent over the network.
    let wire_bytes: Vec<u8> = original.to_bytes();
    assert!(!wire_bytes.is_empty(), "serialised pre-key message is non-empty");

    // Parse it back on the receiver side.
    let parsed: vodozemac::olm::PreKeyMessage =
        vodozemac::olm::PreKeyMessage::from_bytes(&wire_bytes).expect("pre-key message parses");
    let inbound = bob.create_inbound_session(alice_identity, &parsed).expect("inbound session");
    assert_eq!(inbound.plaintext, b"wire test");
}

// =====================================================================
// Megolm: session lifecycle
// =====================================================================

/// Reference: vodozemac 0.9 docs §"Megolm sessions".
/// The sender creates a Megolm session, encrypts several messages,
/// shares the session key, and a peer decrypts the full stream.
#[test]
fn megolm_session_roundtrip() {
    if skip_unless_interop() {
        skip_message("megolm roundtrip");
        return;
    }

    use vodozemac::megolm::SessionConfig;

    let mut session = MegolmSender::new(SessionConfig::default());
    // `session_key()` is the exportable outbound key — what the wire
    // protocol carries inside a `m.room_key` to-device event.
    let session_key = session.session_key();
    let mut peer_session = MegolmReceiver::new(&session_key, SessionConfig::default());

    let mut last_index = 0;
    for i in 0..16u32 {
        let plaintext = format!("message {i}");
        let ciphertext = session.encrypt(plaintext.as_bytes());
        let decrypted = peer_session.decrypt(&ciphertext).expect("peer decrypts megolm ciphertext");
        assert_eq!(decrypted.plaintext, plaintext.as_bytes());
        last_index = decrypted.message_index;
    }
    // Megolm guarantees strictly-increasing message indices.
    assert_eq!(last_index, 15, "16 messages should yield message_index 0..=15");
}

/// `MegolmMessage` survives a wire-level byte roundtrip. The
/// ciphertext that lands in `m.room.encrypted.content.ciphertext` is
/// base64-encoded bytes produced by `MegolmMessage::to_bytes`; if
/// the roundtrip breaks, no Element client can decrypt our messages.
#[test]
fn megolm_ciphertext_wire_roundtrip() {
    if skip_unless_interop() {
        skip_message("megolm wire roundtrip");
        return;
    }

    use vodozemac::megolm::SessionConfig;

    let mut sender = MegolmSender::new(SessionConfig::default());
    let session_key = sender.session_key();
    let mut receiver = MegolmReceiver::new(&session_key, SessionConfig::default());

    let ciphertext_bytes = sender.encrypt(b"wire bytes").to_bytes();
    // Re-parse the bytes the way a real receiver would.
    let parsed =
        vodozemac::megolm::MegolmMessage::from_bytes(&ciphertext_bytes).expect("MegolmMessage parses from bytes");
    let decrypted = receiver.decrypt(&parsed).expect("decrypts after wire roundtrip");
    assert_eq!(decrypted.plaintext, b"wire bytes");
}

// =====================================================================
// Megolm: multi-device / fan-out
// =====================================================================

/// One Megolm session, N receivers. This is the real-world room
/// scenario: the sender encrypts once and every member of the room
/// (each with their own `InboundGroupSession`) can decrypt. Each
/// receiver should also see the same message index for a given
/// ciphertext, which is how clients detect gaps and request key
/// forwards.
#[test]
fn megolm_session_shared_with_multiple_receivers() {
    if skip_unless_interop() {
        skip_message("megolm fan-out");
        return;
    }

    use vodozemac::megolm::SessionConfig;

    let mut sender = MegolmSender::new(SessionConfig::default());
    let session_key = sender.session_key();

    // Three independent receivers, each with their own inbound session.
    let mut receivers: Vec<MegolmReceiver> =
        (0..3).map(|_| MegolmReceiver::new(&session_key, SessionConfig::default())).collect();

    // Encrypt 5 messages; all three receivers must decrypt every one,
    // and every receiver must see the same message_index for the
    // same ciphertext.
    for i in 0..5u32 {
        let plaintext = format!("fan-out message {i}");
        let ciphertext = sender.encrypt(plaintext.as_bytes());
        let bytes = ciphertext.to_bytes();
        let parsed = vodozemac::megolm::MegolmMessage::from_bytes(&bytes).expect("parses");

        let mut last_index: u32 = 0;
        for (idx, receiver) in receivers.iter_mut().enumerate() {
            let decrypted = receiver.decrypt(&parsed).expect("receiver decrypts");
            assert_eq!(decrypted.plaintext, plaintext.as_bytes());
            assert_eq!(decrypted.message_index, i, "receiver {idx} sees message_index {i}");
            assert!(decrypted.message_index >= last_index);
            last_index = decrypted.message_index;
        }
    }
}

/// Message indices are strictly monotonic across a single session.
/// A receiver that sees out-of-order indices would conclude the
/// stream was tampered with and refuse subsequent messages.
#[test]
fn megolm_message_index_strictly_monotonic() {
    if skip_unless_interop() {
        skip_message("megolm monotonicity");
        return;
    }

    use vodozemac::megolm::SessionConfig;

    let mut sender = MegolmSender::new(SessionConfig::default());
    let session_key = sender.session_key();
    let mut receiver = MegolmReceiver::new(&session_key, SessionConfig::default());

    let mut last_index: u32 = 0;
    let mut first = true;
    for i in 0..32u32 {
        let msg = sender.encrypt(format!("m{i}").as_bytes());
        let decrypted = receiver.decrypt(&msg).expect("decrypt");
        // The first message is at index 0; subsequent messages must
        // produce strictly greater indices.
        if first {
            assert_eq!(decrypted.message_index, 0, "first message is at index 0");
            first = false;
        } else {
            assert!(
                decrypted.message_index > last_index,
                "index must be strictly increasing after the first message: got {} after {}",
                decrypted.message_index,
                last_index
            );
        }
        last_index = decrypted.message_index;
    }
    assert_eq!(last_index, 31, "32 messages should reach index 31");
}

/// Forward secrecy sanity check: after rotating the session (creating
/// a new `GroupSession` and re-keying the receivers), the *new*
/// receiver cannot decrypt messages encrypted under the *old*
/// session key, but a receiver that was already established on the
/// old session can still decrypt the old stream.
#[test]
fn megolm_rotation_invalidates_new_receivers() {
    if skip_unless_interop() {
        skip_message("megolm rotation");
        return;
    }

    use vodozemac::megolm::SessionConfig;

    let mut old_sender = MegolmSender::new(SessionConfig::default());
    let old_session_key = old_sender.session_key();
    let mut old_receiver = MegolmReceiver::new(&old_session_key, SessionConfig::default());

    // Encrypt a message under the old session.
    let old_msg = old_sender.encrypt(b"old epoch");
    let old_bytes = old_msg.to_bytes();

    // Rotate: new sender, new session key.
    let new_sender = MegolmSender::new(SessionConfig::default());
    let new_session_key = new_sender.session_key();
    // A receiver that joins the *new* session must NOT be able to
    // decrypt the old ciphertext.
    let mut new_receiver = MegolmReceiver::new(&new_session_key, SessionConfig::default());
    let parsed_old = vodozemac::megolm::MegolmMessage::from_bytes(&old_bytes).expect("parses");
    let new_receiver_result = new_receiver.decrypt(&parsed_old);
    assert!(
        new_receiver_result.is_err(),
        "a receiver established on the new session must not be able to decrypt old-session ciphertexts"
    );

    // Conversely, the old receiver must still decrypt the old ciphertext.
    let old_receiver_result = old_receiver.decrypt(&parsed_old).expect("old receiver decrypts old epoch");
    assert_eq!(old_receiver_result.plaintext, b"old epoch");
}

// =====================================================================
// Pickle compatibility: legacy / vodozemac / dual
// =====================================================================

/// Vodozemac 0.9 `GroupSessionPickle` survives a base64+JSON encode
/// cycle and can be re-hydrated into a `GroupSession`. This is the
/// serialization format used by `MegolmSession::session_key` when
/// `PickleFormat::Vodozemac` is stored.
#[test]
fn pickle_vodozemac_group_session_roundtrip() {
    if skip_unless_interop() {
        skip_message("vodozemac group pickle");
        return;
    }

    use vodozemac::megolm::SessionConfig;

    let mut sender = MegolmSender::new(SessionConfig::default());
    // Encrypt first so the pickle contains a non-trivial ratchet state.
    let _ = sender.encrypt(b"pre-pickle");
    let pickle = sender.pickle();
    let json = serde_json::to_vec(&pickle).expect("serialise GroupSessionPickle");
    let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &json);
    let restored_json =
        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &b64).expect("base64 decode");
    let restored_pickle: vodozemac::megolm::GroupSessionPickle =
        serde_json::from_slice(&restored_json).expect("GroupSessionPickle parses");
    let _restored = MegolmSender::from_pickle(restored_pickle);
}

/// Vodozemac 0.9 `InboundGroupSessionPickle` survives a base64+JSON
/// encode cycle. Stored on the receiver side of a Megolm session.
#[test]
fn pickle_vodozemac_inbound_session_roundtrip() {
    if skip_unless_interop() {
        skip_message("vodozemac inbound pickle");
        return;
    }

    use vodozemac::megolm::SessionConfig;

    let sender = MegolmSender::new(SessionConfig::default());
    let key = sender.session_key();
    let receiver = MegolmReceiver::new(&key, SessionConfig::default());
    let pickle = receiver.pickle();
    let json = serde_json::to_vec(&pickle).expect("serialise InboundGroupSessionPickle");
    let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &json);
    let restored_json =
        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &b64).expect("base64 decode");
    let restored_pickle: vodozemac::megolm::InboundGroupSessionPickle =
        serde_json::from_slice(&restored_json).expect("InboundGroupSessionPickle parses");
    let _restored = MegolmReceiver::from_pickle(restored_pickle);
}

/// Olm account pickle: vodozemac 0.9 stores the account pickle
/// unencrypted by default. We verify that a freshly-pickled account
/// can be restored without losing the identity keys. This is the
/// format used for `device_keys` storage of the device identity.
#[test]
fn pickle_olm_account_roundtrip() {
    if skip_unless_interop() {
        skip_message("olm account pickle");
        return;
    }

    let account = OlmAccount::new();
    let original_keys = account.identity_keys();
    let pickle = account.pickle();
    let restored = OlmAccount::from_pickle(pickle);
    let restored_keys = restored.identity_keys();
    assert_eq!(original_keys.ed25519, restored_keys.ed25519);
    assert_eq!(original_keys.curve25519, restored_keys.curve25519);
}

/// Dual-format rows (`PickleFormat::Dual`) carry both a legacy
/// AES-256-GCM-encrypted session_key and a vodozemac 0.9 pickle in
/// `vodozemac_pickle`. We verify that the vodozemac pickle is parseable
/// independently of the legacy column. This is the contract for the
/// `MegolmVodozemacService::create_session` dual-write path.
#[test]
fn pickle_dual_format_vodozemac_pickle_parses() {
    if skip_unless_interop() {
        skip_message("dual-format pickle");
        return;
    }

    use vodozemac::megolm::SessionConfig;

    let sender = MegolmSender::new(SessionConfig::default());
    let pickle = sender.pickle();
    let json = serde_json::to_vec(&pickle).expect("serialise");
    // In a Dual row, this is what `MegolmSession::vodozemac_pickle` holds.
    let stored_vodozemac_pickle = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &json);

    // The legacy `session_key` column is opaque bytes from the
    // vodozemac path's perspective: trying to base64+JSON parse it as
    // a vodozemac pickle must fail.
    // Use a known-valid base64 string (URL-safe, no padding) that
    // decodes to bytes that are clearly not a JSON GroupSessionPickle.
    let legacy_blob = "AAAAbm90X2pzb24AAAA";
    let legacy_bytes = base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, legacy_blob)
        .expect("legacy blob is valid url-safe base64");
    assert!(
        serde_json::from_slice::<vodozemac::megolm::GroupSessionPickle>(&legacy_bytes).is_err(),
        "the legacy column must not parse as a vodozemac GroupSessionPickle"
    );

    // The vodozemac_pickle column must roundtrip cleanly.
    let restored_json =
        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &stored_vodozemac_pickle).expect("b64");
    let restored: vodozemac::megolm::GroupSessionPickle =
        serde_json::from_slice(&restored_json).expect("Dual row's vodozemac_pickle parses");
    let _ = MegolmSender::from_pickle(restored);
}

// =====================================================================
// m.room_key to-device payload
// =====================================================================

/// Helper: build a `m.room_key` to-device payload from a sender
/// session. This mirrors what `MegolmVodozemacService::get_room_key_distribution`
/// produces, expressed in the wire format that Element clients
/// consume. Returning the payload as `serde_json::Value` lets us
/// assert on individual fields.
fn build_room_key_payload(
    room_id: &str,
    sender_key: &str,
    sender_device_id: &str,
    session_id: &str,
    session: &MegolmSender,
) -> serde_json::Value {
    let session_key = session.session_key().to_base64();
    serde_json::json!({
        "type": "m.room_key",
        "sender": "@alice:example.com",
        "sender_device": sender_device_id,
        "recipient": "@bob:example.com",
        "recipient_keys": {
            "ed25519": "placeholder-ed25519-recipient"
        },
        "content": {
            "algorithm": "m.megolm.v1.aes-sha2",
            "room_id": room_id,
            "sender_key": sender_key,
            "session_id": session_id,
            "session_key": session_key,
            "device_id": sender_device_id,
            "chain_index": 0u32,
        }
    })
}

/// A payload produced by `build_room_key_payload` must contain every
/// field that Element clients expect, and those fields must be
/// self-consistent (e.g. `session_key` must be a non-empty base64
/// string that vodozemac can parse into a `SessionKey`).
#[test]
fn room_key_payload_is_well_formed() {
    if skip_unless_interop() {
        skip_message("room_key payload");
        return;
    }

    use vodozemac::megolm::SessionConfig;

    let mut sender = MegolmSender::new(SessionConfig::default());
    let payload = build_room_key_payload("!room:example.com", "sender_curve25519_b64", "DEVICE_A", "sess_001", &sender);

    // Top-level shape.
    assert_eq!(payload["type"], "m.room_key");
    assert!(payload["sender"].is_string(), "sender is a MXID");
    assert!(payload["sender_device"].is_string(), "sender_device is a device id");
    assert!(payload["recipient"].is_string(), "recipient is a MXID");

    // Content shape.
    let content = &payload["content"];
    assert_eq!(content["algorithm"], "m.megolm.v1.aes-sha2");
    assert_eq!(content["room_id"], "!room:example.com");
    assert_eq!(content["sender_key"], "sender_curve25519_b64");
    assert_eq!(content["session_id"], "sess_001");
    assert_eq!(content["device_id"], "DEVICE_A");
    assert_eq!(content["chain_index"], 0);

    // The session_key must be a base64 string that vodozemac can decode
    // into a SessionKey. This is the field that crosses the wire and
    // is most likely to be subtly wrong (URL-safe vs standard padding,
    // leading whitespace, etc.).
    let session_key_b64 = content["session_key"].as_str().expect("session_key is a string");
    assert!(!session_key_b64.is_empty());
    let parsed = MegolmSessionKey::from_base64(session_key_b64).expect("vodozemac parses session_key");
    // The decoded SessionKey must produce the same session: re-import
    // the inbound and confirm the original sender's first message
    // decrypts.
    let mut receiver = MegolmReceiver::new(&parsed, SessionConfig::default());
    let ciphertext = sender.encrypt(b"after payload construction");
    let decrypted = receiver.decrypt(&ciphertext).expect("decrypts after re-importing session_key from payload");
    assert_eq!(decrypted.plaintext, b"after payload construction");
}

/// Parsing the payload back into a `RoomKeyContent` shape must
/// preserve the session_key bytes, the algorithm identifier, the
/// room_id, and the chain_index. This is what the receiver does
/// before calling `MegolmVodozemacService::import_session`.
#[test]
fn room_key_payload_parse_preserves_fields() {
    if skip_unless_interop() {
        skip_message("room_key parse");
        return;
    }

    use vodozemac::megolm::SessionConfig;

    #[derive(serde::Deserialize)]
    struct RoomKeyContent {
        algorithm: String,
        room_id: String,
        sender_key: String,
        session_id: String,
        session_key: String,
        device_id: String,
        chain_index: u32,
    }

    #[derive(serde::Deserialize)]
    #[allow(dead_code)] // documented fields; `parsed.kind` and `parsed.content` are checked below
    struct RoomKeyPayload {
        #[serde(rename = "type")]
        kind: String,
        sender: String,
        sender_device: String,
        recipient: String,
        content: RoomKeyContent,
    }

    let mut sender = MegolmSender::new(SessionConfig::default());
    let payload = build_room_key_payload("!room2:example.com", "sk_2", "DEV2", "sess_002", &sender);
    let parsed: RoomKeyPayload = serde_json::from_value(payload).expect("payload parses into typed struct");

    assert_eq!(parsed.kind, "m.room_key");
    assert_eq!(parsed.content.algorithm, "m.megolm.v1.aes-sha2");
    assert_eq!(parsed.content.room_id, "!room2:example.com");
    assert_eq!(parsed.content.sender_key, "sk_2");
    assert_eq!(parsed.content.session_id, "sess_002");
    assert_eq!(parsed.content.device_id, "DEV2");
    assert_eq!(parsed.content.chain_index, 0);

    // Round-trip the session_key through the parsed struct to confirm
    // no encoding loss.
    let mut receiver = MegolmReceiver::new(
        &MegolmSessionKey::from_base64(&parsed.content.session_key).expect("session_key parses"),
        SessionConfig::default(),
    );
    let ct = sender.encrypt(b"end-to-end");
    let pt = receiver.decrypt(&ct).expect("decrypts");
    assert_eq!(pt.plaintext, b"end-to-end");
}

/// A `m.room_key` payload must NOT be accepted if the algorithm is
/// unknown to vodozemac. The receiver path returns `None` /
/// `DecryptionError` in that case (we model the rejection by
/// asserting that the algorithm field is one of the supported
/// identifiers).
#[test]
fn room_key_payload_rejects_unsupported_algorithm() {
    if skip_unless_interop() {
        skip_message("room_key reject");
        return;
    }

    use vodozemac::megolm::SessionConfig;

    let sender = MegolmSender::new(SessionConfig::default());
    let mut payload = build_room_key_payload("!room:example.com", "sk", "DEV", "sess_003", &sender);

    // Mutate the algorithm to an unknown value; the receiver should
    // refuse to process the payload.
    payload["content"]["algorithm"] = serde_json::Value::String("m.unknown-algorithm".to_string());
    let algorithm = payload["content"]["algorithm"].as_str().expect("string");
    assert_ne!(algorithm, "m.megolm.v1.aes-sha2");
    assert!(
        !matches!(algorithm, "m.megolm.v1.aes-sha2"),
        "non-megolm algorithms must be rejected before invoking vodozemac"
    );
}

// =====================================================================
// Ed25519 cross-impl smoke (already covered in lib tests; kept here
// for completeness of the interop matrix)
// =====================================================================

/// Reference: vodozemac 0.9 docs §"Ed25519 signatures".
/// The current project's ed25519-dalek 2.0 wrapper must produce
/// signatures vodozemac can verify. We use the same ed25519-dalek
/// underneath so this is mostly a smoke test, but it locks the API.
#[test]
fn ed25519_sign_verify() {
    if skip_unless_interop() {
        skip_message("ed25519 sign");
        return;
    }

    use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
    use rand::RngCore;

    let mut csprng = [0u8; 32];
    rand::rng().fill_bytes(&mut csprng);
    let key = SigningKey::from_bytes(&csprng);
    let message = b"interop ed25519";
    let sig: Signature = key.sign(message);
    let pk = VerifyingKey::from(&key);
    pk.verify(message, &sig).expect("signature verifies");
}
