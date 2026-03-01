use std::sync::Arc;
use parking_lot::RwLock;
use lru::LruCache;
use std::time::{Duration, Instant};
use sha2::{Digest, Sha256};
use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine as _};

const REPLAY_CACHE_SIZE: usize = 10000;
const REPLAY_PROTECTION_WINDOW_SECS: u64 = 300;

#[derive(Debug, Clone)]
pub struct ReplayProtectionConfig {
    pub enabled: bool,
    pub cache_size: usize,
    pub window_secs: u64,
}

impl Default for ReplayProtectionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            cache_size: REPLAY_CACHE_SIZE,
            window_secs: REPLAY_PROTECTION_WINDOW_SECS,
        }
    }
}

pub struct ReplayProtectionCache {
    cache: Arc<RwLock<LruCache<String, Instant>>>,
    config: ReplayProtectionConfig,
}

impl ReplayProtectionCache {
    pub fn new(config: ReplayProtectionConfig) -> Self {
        let cache = LruCache::new(std::num::NonZeroUsize::new(config.cache_size).unwrap());
        Self {
            cache: Arc::new(RwLock::new(cache)),
            config,
        }
    }

    pub fn check_and_record(&self, signature_hash: &str) -> bool {
        if !self.config.enabled {
            return true;
        }

        let mut cache = self.cache.write();
        
        if let Some(&timestamp) = cache.get(signature_hash) {
            let elapsed = Instant::now().duration_since(timestamp);
            if elapsed < Duration::from_secs(self.config.window_secs) {
                tracing::warn!(
                    target: "security_audit",
                    event = "replay_attack_detected",
                    signature_hash_prefix = &signature_hash[..8.min(signature_hash.len())],
                    elapsed_secs = elapsed.as_secs(),
                    "Potential replay attack detected: signature already used within window"
                );
                return false;
            }
        }
        
        cache.put(signature_hash.to_string(), Instant::now());
        true
    }

    pub fn cleanup_expired(&self) {
        let mut cache = self.cache.write();
        let now = Instant::now();
        let window = Duration::from_secs(self.config.window_secs);
        
        let expired_keys: Vec<String> = cache
            .iter()
            .filter(|(_, &timestamp)| now.duration_since(timestamp) >= window)
            .map(|(k, _)| k.clone())
            .collect();

        for key in expired_keys {
            cache.pop(&key);
        }

        tracing::debug!(
            target: "replay_protection",
            remaining_entries = cache.len(),
            "Cleaned up expired replay protection entries"
        );
    }

    pub fn stats(&self) -> ReplayProtectionStats {
        let cache = self.cache.read();
        ReplayProtectionStats {
            total_entries: cache.len(),
            capacity: self.config.cache_size,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ReplayProtectionStats {
    pub total_entries: usize,
    pub capacity: usize,
}

pub fn compute_signature_hash(origin: &str, key_id: &str, signature: &str, signed_bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(origin.as_bytes());
    hasher.update(key_id.as_bytes());
    hasher.update(signature.as_bytes());
    hasher.update(signed_bytes);
    STANDARD_NO_PAD.encode(hasher.finalize())
}

pub struct SecurityValidator;

impl SecurityValidator {
    pub fn validate_jwt_secret(secret: &str) -> Result<(), String> {
        if secret.is_empty() {
            return Err("JWT secret cannot be empty".to_string());
        }

        if secret.len() < 32 {
            return Err(format!(
                "JWT secret must be at least 32 characters (current: {}). \
                 Generate a secure secret with: openssl rand -hex 32",
                secret.len()
            ));
        }

        if secret.len() < 64 {
            tracing::warn!(
                "JWT secret is shorter than recommended 64 characters. \
                 Consider using a longer secret for production."
            );
        }

        let entropy = Self::calculate_entropy(secret);
        if entropy < 3.0 {
            return Err(format!(
                "JWT secret has low entropy ({:.2} bits/char). \
                 Use a cryptographically secure random secret.",
                entropy
            ));
        }

        Ok(())
    }

    pub fn validate_federation_timestamp(
        signature_ts: i64,
        tolerance_ms: i64,
    ) -> Result<(), String> {
        let now = chrono::Utc::now().timestamp_millis();
        let diff = (signature_ts - now).abs();

        if diff > tolerance_ms {
            return Err(format!(
                "Signature timestamp out of tolerance: {}ms (tolerance: {}ms)",
                diff, tolerance_ms
            ));
        }

        Ok(())
    }

    fn calculate_entropy(s: &str) -> f64 {
        if s.is_empty() {
            return 0.0;
        }

        let mut freq = [0usize; 256];
        for c in s.bytes() {
            freq[c as usize] += 1;
        }

        let len = s.len() as f64;
        let mut entropy = 0.0;

        for &count in &freq {
            if count > 0 {
                let p = count as f64 / len;
                entropy -= p * p.log2();
            }
        }

        entropy
    }

    pub fn validate_origin(origin: &str) -> Result<(), String> {
        if origin.is_empty() {
            return Err("Origin cannot be empty".to_string());
        }

        if origin.len() > 253 {
            return Err("Origin too long (max 253 characters)".to_string());
        }

        let valid_chars = origin
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '.' || c == ':' || c == '_');

        if !valid_chars {
            return Err("Origin contains invalid characters".to_string());
        }

        Ok(())
    }
}

pub struct ConstantTimeComparison;

impl ConstantTimeComparison {
    pub fn compare_bytes(a: &[u8], b: &[u8]) -> bool {
        if a.len() != b.len() {
            return false;
        }

        let mut result: u8 = 0;
        for (a_byte, b_byte) in a.iter().zip(b.iter()) {
            result |= a_byte ^ b_byte;
        }
        result == 0
    }

    pub fn compare_strings(a: &str, b: &str) -> bool {
        Self::compare_bytes(a.as_bytes(), b.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replay_protection_cache() {
        let config = ReplayProtectionConfig::default();
        let cache = ReplayProtectionCache::new(config);

        let sig_hash = "test_signature_hash_123";
        
        assert!(cache.check_and_record(sig_hash));
        
        assert!(!cache.check_and_record(sig_hash));
    }

    #[test]
    fn test_replay_protection_different_signatures() {
        let config = ReplayProtectionConfig::default();
        let cache = ReplayProtectionCache::new(config);

        assert!(cache.check_and_record("signature_1"));
        assert!(cache.check_and_record("signature_2"));
        assert!(cache.check_and_record("signature_3"));
    }

    #[test]
    fn test_compute_signature_hash_deterministic() {
        let hash1 = compute_signature_hash("origin", "key_id", "sig", b"bytes");
        let hash2 = compute_signature_hash("origin", "key_id", "sig", b"bytes");
        assert_eq!(hash1, hash2);

        let hash3 = compute_signature_hash("origin2", "key_id", "sig", b"bytes");
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_validate_jwt_secret_valid() {
        assert!(SecurityValidator::validate_jwt_secret(
            "this_is_a_very_secure_secret_key_with_32_chars"
        ).is_ok());
    }

    #[test]
    fn test_validate_jwt_secret_too_short() {
        assert!(SecurityValidator::validate_jwt_secret("short").is_err());
    }

    #[test]
    fn test_validate_jwt_secret_empty() {
        assert!(SecurityValidator::validate_jwt_secret("").is_err());
    }

    #[test]
    fn test_validate_jwt_secret_low_entropy() {
        assert!(SecurityValidator::validate_jwt_secret("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").is_err());
    }

    #[test]
    fn test_validate_federation_timestamp_valid() {
        let now = chrono::Utc::now().timestamp_millis();
        assert!(SecurityValidator::validate_federation_timestamp(now, 60000).is_ok());
    }

    #[test]
    fn test_validate_federation_timestamp_expired() {
        let old = chrono::Utc::now().timestamp_millis() - 120000;
        assert!(SecurityValidator::validate_federation_timestamp(old, 60000).is_err());
    }

    #[test]
    fn test_validate_origin_valid() {
        assert!(SecurityValidator::validate_origin("matrix.org").is_ok());
        assert!(SecurityValidator::validate_origin("example.com:8448").is_ok());
    }

    #[test]
    fn test_validate_origin_invalid() {
        assert!(SecurityValidator::validate_origin("").is_err());
        assert!(SecurityValidator::validate_origin(&"a".repeat(300)).is_err());
        assert!(SecurityValidator::validate_origin("invalid!@#").is_err());
    }

    #[test]
    fn test_constant_time_comparison() {
        assert!(ConstantTimeComparison::compare_strings("hello", "hello"));
        assert!(!ConstantTimeComparison::compare_strings("hello", "world"));
        assert!(!ConstantTimeComparison::compare_strings("hello", "hell"));
    }

    #[test]
    fn test_constant_time_comparison_bytes() {
        assert!(ConstantTimeComparison::compare_bytes(b"test", b"test"));
        assert!(!ConstantTimeComparison::compare_bytes(b"test", b"best"));
        assert!(!ConstantTimeComparison::compare_bytes(b"test", b"testing"));
    }

    #[test]
    fn test_entropy_calculation() {
        let high_entropy = "aB1!xY2@zQ3#mN4$";
        let entropy = SecurityValidator::calculate_entropy(high_entropy);
        assert!(entropy > 3.0);

        let low_entropy = "aaaaaaaaaaaaaaaa";
        let entropy = SecurityValidator::calculate_entropy(low_entropy);
        assert!(entropy < 1.0);
    }
}
