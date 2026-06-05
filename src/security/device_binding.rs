//! Device-upload HMAC binding module (Sprint 5 / sec-3).
//!
//! When a device uploads a cross-signing key (master / self_signing /
//! user_signing) to `POST /_matrix/client/v3/keys/device_signing/upload`,
//! the server stores the row in `cross_signing_keys` and computes a
//! per-row HMAC binding:
//!
//! ```text
//!   token = HMAC-SHA256(secret,
//!                       "v1|" || user_id || "|" || device_id || "|" ||
//!                       key_type || "|" || added_ts)
//! ```
//!
//! The token is then stored on the row **and** returned to the client as
//! proof-of-upload. A later verifier (audit log, snapshot importer, sync
//! engine on a peer server) can recompute the token from the row's own
//! fields and reject rows where the token does not match — meaning the row
//! was fabricated rather than produced by a server process holding the
//! secret.
//!
//! Pre-Sprint 5 rows have `binding_token = NULL` and are accepted during
//! the migration window.

use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Domain-separated payload. Bump the `v1|` prefix on any change to keep
/// the verification contract well-defined.
fn build_binding_payload(user_id: &str, device_id: &str, key_type: &str, added_ts: i64) -> Vec<u8> {
    format!("v1|{user_id}|{device_id}|{key_type}|{added_ts}").into_bytes()
}

/// Compute the hex-encoded HMAC-SHA256 binding token for a device upload.
pub fn sign_device_binding(
    secret: &[u8],
    user_id: &str,
    device_id: &str,
    key_type: &str,
    added_ts: i64,
) -> String {
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC-SHA256 accepts keys of any size");
    mac.update(&build_binding_payload(user_id, device_id, key_type, added_ts));
    hex::encode(mac.finalize().into_bytes())
}

/// Constant-time verify a binding token against the row's claimed fields.
pub fn verify_device_binding(
    secret: &[u8],
    user_id: &str,
    device_id: &str,
    key_type: &str,
    added_ts: i64,
    provided_hex: &str,
) -> bool {
    let Ok(provided) = hex::decode(provided_hex) else {
        return false;
    };
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC-SHA256 accepts keys of any size");
    mac.update(&build_binding_payload(user_id, device_id, key_type, added_ts));
    mac.verify_slice(&provided).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secret() -> Vec<u8> {
        b"unit-test-device-binding-key-A".to_vec()
    }

    #[test]
    fn sign_and_verify_round_trip() {
        let s = secret();
        let t = sign_device_binding(&s, "@alice:hs", "ABCDEFGH", "master", 1_700_000_000_000);
        assert!(verify_device_binding(&s, "@alice:hs", "ABCDEFGH", "master", 1_700_000_000_000, &t));
    }

    #[test]
    fn rejects_tampered_device() {
        let s = secret();
        let t = sign_device_binding(&s, "@alice:hs", "ABCDEFGH", "master", 1_700_000_000_000);
        assert!(!verify_device_binding(&s, "@alice:hs", "OTHER", "master", 1_700_000_000_000, &t));
    }

    #[test]
    fn rejects_tampered_key_type() {
        let s = secret();
        let t = sign_device_binding(&s, "@alice:hs", "ABCDEFGH", "master", 1_700_000_000_000);
        assert!(!verify_device_binding(&s, "@alice:hs", "ABCDEFGH", "self_signing", 1_700_000_000_000, &t));
    }

    #[test]
    fn rejects_tampered_ts() {
        let s = secret();
        let t = sign_device_binding(&s, "@alice:hs", "ABCDEFGH", "master", 1_700_000_000_000);
        assert!(!verify_device_binding(&s, "@alice:hs", "ABCDEFGH", "master", 1_700_000_000_001, &t));
    }

    #[test]
    fn rejects_wrong_secret() {
        let s1 = secret();
        let s2 = b"unit-test-device-binding-key-B".to_vec();
        let t = sign_device_binding(&s1, "@alice:hs", "ABCDEFGH", "master", 1_700_000_000_000);
        assert!(!verify_device_binding(&s2, "@alice:hs", "ABCDEFGH", "master", 1_700_000_000_000, &t));
    }
}
