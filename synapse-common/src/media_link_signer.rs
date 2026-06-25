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

    #[test]
    fn test_verify_rejects_different_key() {
        let signer1 = MediaLinkSigner::new(b"key-one-32-bytes-xxxxxxxxxxxxx", 3600);
        let signer2 = MediaLinkSigner::new(b"key-two-32-bytes-xxxxxxxxxxxxx", 3600);
        let path = "example.com/abc123";
        let query = signer1.sign(path);

        let params: std::collections::HashMap<_, _> = query
            .split('&')
            .filter_map(|p| {
                let mut parts = p.splitn(2, '=');
                Some((parts.next()?.to_string(), parts.next()?.to_string()))
            })
            .collect();

        let signature = params.get("signature").unwrap();
        let expires: u64 = params.get("expires").unwrap().parse().unwrap();

        assert!(!signer2.verify(path, signature, expires));
    }

    #[test]
    fn test_different_keys_produce_different_signatures() {
        let signer1 = MediaLinkSigner::new(b"key-one-32-bytes-xxxxxxxxxxxxx", 3600);
        let signer2 = MediaLinkSigner::new(b"key-two-32-bytes-xxxxxxxxxxxxx", 3600);
        let path = "example.com/abc123";

        let query1 = signer1.sign(path);
        let query2 = signer2.sign(path);

        let sig1 = query1.split('&').next().unwrap();
        let sig2 = query2.split('&').next().unwrap();

        assert_ne!(sig1, sig2);
    }

    #[test]
    fn test_verify_empty_signature() {
        let signer = MediaLinkSigner::new(b"test-secret-key-32-bytes-xxxxx", 3600);
        let path = "example.com/abc123";
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();

        assert!(!signer.verify(path, "", now));
    }

    #[test]
    fn test_verify_empty_path() {
        let signer = MediaLinkSigner::new(b"test-secret-key-32-bytes-xxxxx", 3600);
        let path = "";
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

        assert!(signer.verify(path, signature, expires));
    }

    #[test]
    fn test_sign_and_verify_different_paths() {
        let signer = MediaLinkSigner::new(b"test-secret-key-32-bytes-xxxxx", 3600);

        let paths =
            vec!["example.com/abc123", "matrix.org/xyz789", "server.test/media-id", "example.com/path/with/slashes"];

        for path in &paths {
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

            assert!(signer.verify(path, signature, expires));
        }
    }

    #[test]
    fn test_sign_different_ttl_produces_different_expiry() {
        let signer_short = MediaLinkSigner::new(b"test-secret-key-32-bytes-xxxxx", 60);
        let signer_long = MediaLinkSigner::new(b"test-secret-key-32-bytes-xxxxx", 86400);
        let path = "example.com/abc123";

        let query_short = signer_short.sign(path);
        let query_long = signer_long.sign(path);

        let expires_short: u64 = query_short.split('&').nth(1).unwrap().split('=').nth(1).unwrap().parse().unwrap();
        let expires_long: u64 = query_long.split('&').nth(1).unwrap().split('=').nth(1).unwrap().parse().unwrap();

        assert!(expires_long > expires_short);
    }

    #[test]
    fn test_verify_expires_exactly_now() {
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

        assert!(signer.verify(path, signature, expires));
        assert!(!signer.verify(path, signature, expires.saturating_sub(1)));
    }

    #[test]
    fn test_default_ttl_constant() {
        assert_eq!(DEFAULT_MEDIA_LINK_TTL_SECS, 86400);
    }

    #[test]
    fn test_sign_and_verify_with_long_path() {
        let signer = MediaLinkSigner::new(b"test-secret-key-32-bytes-xxxxx", 3600);
        let path = "very-long-server-name.example.com/very-long-media-id-with-many-characters-1234567890";

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

        assert!(signer.verify(path, signature, expires));
    }
}
