//! Media link signing — HMAC-SHA256 signed download URLs for Matrix media.
//!
//! Per Matrix spec, media download URLs may be signed with an HMAC to
//! prevent unauthorised access. The signature covers `{server_name}/{media_id}`
//! and includes an expiry timestamp.
//!
//! URL format:
//!   /_matrix/media/v3/download/{server_name}/{media_id}?signature={hex}&expires={timestamp}

use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::crypto::{encode_hex, secure_compare};

type HmacSha256 = Hmac<Sha256>;

/// Default validity window for signed media URLs (24 hours).
pub const DEFAULT_MEDIA_LINK_TTL_SECS: u64 = 86400;

/// Signer for media download URLs.
pub struct MediaLinkSigner {
    key: Vec<u8>,
    ttl_secs: u64,
}

impl MediaLinkSigner {
    /// Create a new signer with the given HMAC key and optional TTL.
    pub fn new(key: &[u8], ttl_secs: u64) -> Self {
        Self { key: key.to_vec(), ttl_secs }
    }

    /// Sign a media download path and return the query string.
    /// `path` should be `{server_name}/{media_id}`.
    #[allow(clippy::expect_used)]
    pub fn sign(&self, path: &str) -> String {
        let expires =
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs().saturating_add(self.ttl_secs);

        let payload = format!("{path}:{expires}");
        let mut mac = HmacSha256::new_from_slice(&self.key).expect("HMAC key length is valid");
        mac.update(payload.as_bytes());
        let signature = encode_hex(mac.finalize().into_bytes());

        format!("signature={signature}&expires={expires}")
    }

    /// Verify a signed media download request.
    /// Returns `Ok(())` if the signature is valid and not expired.
    pub fn verify(&self, path: &str, signature: &str, expires: u64) -> bool {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();

        if now > expires {
            return false;
        }

        let payload = format!("{path}:{expires}");
        let mut mac = match HmacSha256::new_from_slice(&self.key) {
            Ok(m) => m,
            Err(_) => return false,
        };
        mac.update(payload.as_bytes());

        let expected = encode_hex(mac.finalize().into_bytes());
        // Constant-time comparison
        secure_compare(&expected, signature)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sign_and_verify() {
        let signer = MediaLinkSigner::new(b"test-secret-key-32-bytes-xxxxx", 3600);
        let path = "example.com/abc123";
        let query = signer.sign(path);

        // Parse query string
        let params: std::collections::HashMap<_, _> = query
            .split('&')
            .filter_map(|p| {
                let mut parts = p.splitn(2, '=');
                Some((parts.next()?.to_string(), parts.next()?.to_string()))
            })
            .collect();

        let signature = params.get("signature").unwrap();
        let expires: u64 = params.get("expires").unwrap().parse().unwrap();

        assert!(signer.verify(path, signature, expires));
    }

    #[test]
    fn test_verify_rejects_wrong_path() {
        let signer = MediaLinkSigner::new(b"test-secret-key-32-bytes-xxxxx", 3600);
        let path = "example.com/abc123";
        let query = signer.sign(path);

        let params: std::collections::HashMap<_, _> = query
            .split('&')
            .filter_map(|p| {
                let mut parts = p.splitn(2, '=');
                Some((parts.next()?.to_string(), parts.next()?.to_string()))
            })
            .collect();

        let signature = params.get("signature").unwrap();
        let expires: u64 = params.get("expires").unwrap().parse().unwrap();

        assert!(!signer.verify("example.com/wrong", signature, expires));
    }

    #[test]
    fn test_verify_rejects_wrong_signature() {
        let signer = MediaLinkSigner::new(b"test-secret-key-32-bytes-xxxxx", 3600);
        let path = "example.com/abc123";
        let _query = signer.sign(path);

        let params: std::collections::HashMap<_, _> = _query
            .split('&')
            .filter_map(|p| {
                let mut parts = p.splitn(2, '=');
                Some((parts.next()?.to_string(), parts.next()?.to_string()))
            })
            .collect();

        let expires: u64 = params.get("expires").unwrap().parse().unwrap();
        assert!(!signer.verify(path, "deadbeef", expires));
    }

    #[test]
    fn test_verify_rejects_expired() {
        let signer = MediaLinkSigner::new(b"test-secret-key-32-bytes-xxxxx", 3600);
        let path = "example.com/abc123";
        let query = signer.sign(path);

        let params: std::collections::HashMap<_, _> = query
            .split('&')
            .filter_map(|p| {
                let mut parts = p.splitn(2, '=');
                Some((parts.next()?.to_string(), parts.next()?.to_string()))
            })
            .collect();

        let signature = params.get("signature").unwrap();
        // Use an expired timestamp (1 hour ago)
        let past_expires = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs().saturating_sub(3600);

        assert!(!signer.verify(path, signature, past_expires));
    }
}
