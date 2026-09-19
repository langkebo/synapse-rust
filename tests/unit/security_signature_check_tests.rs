#![allow(clippy::unwrap_used, clippy::expect_used)]
use hmac::{Hmac, Mac};
use sha2::Sha256;
use synapse_rust::common::crypto::hmac_sha256;

#[test]
fn test_hmac_sha256_consistency() {
    let key = b"test_secret_key";
    let data = b"test_message_data";

    // Test using the common crypto helper
    let signature1 = hmac_sha256(key, data);

    // Test using raw hmac crate with Sha256
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC can take key of any size");
    mac.update(data);
    let signature2 = mac.finalize().into_bytes().to_vec();

    assert_eq!(signature1, signature2, "HMAC-SHA256 implementations must be consistent");
}

#[test]
fn test_admin_registration_hmac_logic() {
    // Exercises the PRODUCTION framing via the extracted helper
    // (`synapse_services::admin_registration_service::admin_registration_signature_hex`).
    //
    // The previous version re-implemented the framing *in the test* and then
    // asserted only `hex.len() == 64`, so no change to the production framing —
    // reordering the separators, dropping the admin padding, appending
    // `user_type` — could ever fail it (gate-integrity sweep B14).
    use synapse_services::admin_registration_service::admin_registration_signature_hex;

    let shared_secret = b"change-me-admin-shared-secret";
    let signature = admin_registration_signature_hex(shared_secret, "test_nonce", "admin", "password", true, None)
        .expect("the framing helper must accept any key length");

    // 1) Pin the exact byte layout. Contract:
    //      nonce \0 username \0 password \0 "admin\0\0\0"
    //    (the trailing `\0\0\0` is the framing the external admin-registration
    //    script signs; `user_type`, when present, appends `\0` + the value).
    //    Re-derive this constant if the framing is intentionally changed.
    assert_eq!(
        signature, "655dc0be4a12940e0bebde890a0de0c1c6172bc3bdb771d770ebd6a2b5557e6a",
        "the admin-registration HMAC framing changed — this is the contract the admin script \
         signs against, so update it only together with the script"
    );

    // 2) Tie the helper to the shared crypto primitive through an independent
    //    framing of the same bytes.
    let framed: Vec<u8> = [
        b"test_nonce".as_slice(),
        b"\0",
        b"admin".as_slice(),
        b"\0",
        b"password".as_slice(),
        b"\0",
        b"admin\x00\x00\x00".as_slice(),
    ]
    .concat();
    let raw = hmac_sha256(shared_secret, &framed);
    let expected_hex = raw.iter().map(|b| format!("{b:02x}")).collect::<String>();
    assert_eq!(signature, expected_hex, "the helper and `hmac_sha256` must agree on the same framing");

    // 3) Every field participates. Without these, an implementation that ignored
    //    `admin`/`user_type`/`password` would still produce a 64-char hex string.
    assert_eq!(signature.len(), 64, "HMAC-SHA256 hex signature should be 64 characters");
    for (label, other) in [
        (
            "admin flag",
            admin_registration_signature_hex(shared_secret, "test_nonce", "admin", "password", false, None).unwrap(),
        ),
        (
            "user_type",
            admin_registration_signature_hex(shared_secret, "test_nonce", "admin", "password", true, Some("bot"))
                .unwrap(),
        ),
        (
            "password",
            admin_registration_signature_hex(shared_secret, "test_nonce", "admin", "other", true, None).unwrap(),
        ),
        (
            "nonce",
            admin_registration_signature_hex(shared_secret, "other_nonce", "admin", "password", true, None).unwrap(),
        ),
        (
            "secret",
            admin_registration_signature_hex(b"other-secret", "test_nonce", "admin", "password", true, None).unwrap(),
        ),
    ] {
        assert_ne!(other, signature, "changing the {label} must change the signature");
    }
}
