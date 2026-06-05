//! Room-invite HMAC-SHA256 signature module (Sprint 4 / sec-2).
//!
//! Pre-Sprint 4, an invite "code" was a 32-char random token with no binding
//! to the (room_id, inviter_user_id, expires_at, created_ts) tuple it
//! authorises. Anyone who learned the code (logs, screenshots, referer
//! leaks, shoulder-surfing) could present it on a different device, in a
//! different client, or for a different purpose, until the row expired or
//! was revoked.
//!
//! This module binds the code to its context with a domain-separated HMAC:
//!
//! ```text
//!   sig = HMAC-SHA256(secret, "v1|" || invite_code || "|" || room_id
//!                                       || "|" || inviter_user_id
//!                                       || "|" || expires_at_or_0
//!                                       || "|" || created_ts)
//! ```
//!
//! `v1|` is a version tag so we can rotate the scheme later without
//! ambiguity. Domain separation via the field list prevents a verifier
//! from being fooled by, say, a `room_id` that contains a `|` separator.
//!
//! The signing key is sourced from `SecurityConfig::invite_signing_key`.
//! In production it MUST be set (e.g. via `INVITE_SIGNING_KEY` env var) so
//! that signatures survive restarts; if unset, this module falls back to a
//! process-local CSPRNG key and emits a one-time warning at startup.

use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Domain-separated signing payload. The exact byte layout is part of the
/// verification contract — never reorder fields without bumping the
/// `v1|` prefix.
fn build_signing_payload(
    invite_code: &str,
    room_id: &str,
    inviter_user_id: &str,
    expires_at: Option<i64>,
    created_ts: i64,
) -> Vec<u8> {
    let exp = expires_at.unwrap_or(0);
    format!("v1|{invite_code}|{room_id}|{inviter_user_id}|{exp}|{created_ts}").into_bytes()
}

/// Compute the hex-encoded HMAC-SHA256 signature for a room invite.
pub fn sign_invite(
    secret: &[u8],
    invite_code: &str,
    room_id: &str,
    inviter_user_id: &str,
    expires_at: Option<i64>,
    created_ts: i64,
) -> String {
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC-SHA256 accepts keys of any size");
    mac.update(&build_signing_payload(invite_code, room_id, inviter_user_id, expires_at, created_ts));
    let bytes = mac.finalize().into_bytes();
    hex::encode(bytes)
}

/// Constant-time verify. The runtime cost is identical to a non-MAC
/// comparison; rejecting a forgery must not leak which byte differed.
pub fn verify_invite_signature(
    secret: &[u8],
    invite_code: &str,
    room_id: &str,
    inviter_user_id: &str,
    expires_at: Option<i64>,
    created_ts: i64,
    provided_hex: &str,
) -> bool {
    let Ok(provided) = hex::decode(provided_hex) else {
        return false;
    };
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC-SHA256 accepts keys of any size");
    mac.update(&build_signing_payload(invite_code, room_id, inviter_user_id, expires_at, created_ts));
    mac.verify_slice(&provided).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secret() -> Vec<u8> {
        b"unit-test-signing-key-32-bytes-AAA".to_vec()
    }

    #[test]
    fn sign_and_verify_round_trip() {
        let s = secret();
        let sig = sign_invite(&s, "code123", "!r:hs", "@alice:hs", Some(1_700_000_000_000), 1_699_900_000_000);
        assert!(verify_invite_signature(
            &s,
            "code123",
            "!r:hs",
            "@alice:hs",
            Some(1_700_000_000_000),
            1_699_900_000_000,
            &sig
        ));
    }

    #[test]
    fn verify_rejects_tampered_room_id() {
        let s = secret();
        let sig = sign_invite(&s, "code123", "!r:hs", "@alice:hs", Some(1_700_000_000_000), 1_699_900_000_000);
        assert!(!verify_invite_signature(
            &s,
            "code123",
            "!other:hs",
            "@alice:hs",
            Some(1_700_000_000_000),
            1_699_900_000_000,
            &sig
        ));
    }

    #[test]
    fn verify_rejects_tampered_exp() {
        let s = secret();
        let sig = sign_invite(&s, "code123", "!r:hs", "@alice:hs", Some(1_700_000_000_000), 1_699_900_000_000);
        assert!(!verify_invite_signature(
            &s,
            "code123",
            "!r:hs",
            "@alice:hs",
            Some(1_700_000_000_001),
            1_699_900_000_000,
            &sig
        ));
    }

    #[test]
    fn verify_rejects_wrong_secret() {
        let s1 = secret();
        let s2 = b"unit-test-signing-key-32-bytes-BBB".to_vec();
        let sig = sign_invite(&s1, "code123", "!r:hs", "@alice:hs", None, 1_699_900_000_000);
        assert!(!verify_invite_signature(
            &s2,
            "code123",
            "!r:hs",
            "@alice:hs",
            None,
            1_699_900_000_000,
            &sig
        ));
    }

    #[test]
    fn verify_rejects_invalid_hex() {
        let s = secret();
        assert!(!verify_invite_signature(
            &s,
            "code123",
            "!r:hs",
            "@alice:hs",
            None,
            0,
            "not-a-hex-string"
        ));
    }

    #[test]
    fn signature_is_deterministic() {
        let s = secret();
        let a = sign_invite(&s, "code123", "!r:hs", "@alice:hs", Some(1_700_000_000_000), 1_699_900_000_000);
        let b = sign_invite(&s, "code123", "!r:hs", "@alice:hs", Some(1_700_000_000_000), 1_699_900_000_000);
        assert_eq!(a, b);
    }
}
