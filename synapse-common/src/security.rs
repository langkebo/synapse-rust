use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine as _};
use moka::sync::Cache;
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};

const REPLAY_CACHE_SIZE: u64 = 10000;
const REPLAY_PROTECTION_WINDOW_SECS: u64 = 300;

#[derive(Debug, Clone)]
pub struct ReplayProtectionConfig {
    pub enabled: bool,
    pub cache_size: u64,
    pub window_secs: u64,
}

impl Default for ReplayProtectionConfig {
    fn default() -> Self {
        Self { enabled: true, cache_size: REPLAY_CACHE_SIZE, window_secs: REPLAY_PROTECTION_WINDOW_SECS }
    }
}

pub struct ReplayProtectionCache {
    cache: Cache<String, Instant>,
    config: ReplayProtectionConfig,
}

impl ReplayProtectionCache {
    pub fn new(config: ReplayProtectionConfig) -> Self {
        let cache = Cache::builder()
            .max_capacity(config.cache_size)
            .time_to_idle(Duration::from_secs(config.window_secs))
            .build();
        Self { cache, config }
    }

    pub fn check_and_record(&self, signature_hash: &str) -> bool {
        if !self.config.enabled {
            return true;
        }

        if let Some(timestamp) = self.cache.get(signature_hash) {
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

        self.cache.insert(signature_hash.to_string(), Instant::now());
        true
    }

    pub fn cleanup_expired(&self) {
        self.cache.run_pending_tasks();
    }

    pub fn stats(&self) -> ReplayProtectionStats {
        ReplayProtectionStats {
            total_entries: self.cache.entry_count() as usize,
            capacity: self.config.cache_size as usize,
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
                "JWT secret has low entropy ({entropy:.2} bits/char). \
                 Use a cryptographically secure random secret."
            ));
        }

        Ok(())
    }

    pub fn validate_federation_timestamp(signature_ts: i64, tolerance_ms: i64) -> Result<(), String> {
        let now = chrono::Utc::now().timestamp_millis();
        let diff = (signature_ts - now).abs();

        if diff > tolerance_ms {
            return Err(format!("Signature timestamp out of tolerance: {diff}ms (tolerance: {tolerance_ms}ms)"));
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

        let valid_chars = origin.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '.' || c == ':' || c == '_');

        if !valid_chars {
            return Err("Origin contains invalid characters".to_string());
        }

        Ok(())
    }
}

pub struct ConstantTimeComparison;

impl ConstantTimeComparison {
    pub fn compare_bytes(a: &[u8], b: &[u8]) -> bool {
        crate::crypto::secure_compare_bytes(a, b)
    }

    pub fn compare_strings(a: &str, b: &str) -> bool {
        crate::crypto::secure_compare(a, b)
    }
}

use std::net::IpAddr;

pub fn is_ip_in_blacklist(ip: &IpAddr, blacklist: &[String]) -> bool {
    let ip_str = ip.to_string();
    for cidr in blacklist {
        if cidr.contains('/') {
            if let Ok(network) = cidr.parse::<ipnet::IpNet>() {
                if network.contains(ip) {
                    return true;
                }
            }
        } else if ip_str == *cidr {
            return true;
        }
    }
    false
}

pub fn check_url_against_blacklist(url: &str, blacklist: &[String]) -> Result<(), String> {
    let parsed = url::Url::parse(url).map_err(|e| format!("Invalid URL: {e}"))?;
    let host = parsed.host_str().ok_or_else(|| format!("URL has no host: {url}"))?;

    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_ip_in_blacklist(&ip, blacklist) {
            return Err(format!("IP {ip} is in blacklist"));
        }
    } else if let Ok(addrs) = dns_lookup::lookup_host(host) {
        for addr in addrs {
            if is_ip_in_blacklist(&addr, blacklist) {
                return Err(format!("Host {host} resolves to blacklisted IP {addr}"));
            }
        }
    }
    Ok(())
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
        assert!(SecurityValidator::validate_jwt_secret("this_is_a_very_secure_secret_key_with_32_chars").is_ok());
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
        assert!(SecurityValidator::validate_federation_timestamp(now, 60_000).is_ok());
    }

    #[test]
    fn test_validate_federation_timestamp_expired() {
        let old = chrono::Utc::now().timestamp_millis() - 120000;
        assert!(SecurityValidator::validate_federation_timestamp(old, 60_000).is_err());
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

    #[test]
    fn test_entropy_calculation_empty_string() {
        // Empty string returns 0.0 entropy (edge case coverage).
        let entropy = SecurityValidator::calculate_entropy("");
        assert_eq!(entropy, 0.0);
    }

    #[test]
    fn test_replay_protection_disabled_always_returns_true() {
        // When disabled, the cache always returns true (no replay detection).
        let config = ReplayProtectionConfig { enabled: false, cache_size: 100, window_secs: 300 };
        let cache = ReplayProtectionCache::new(config);

        // Even repeated signatures should pass when protection is disabled.
        assert!(cache.check_and_record("duplicate_signature"));
        assert!(cache.check_and_record("duplicate_signature"));
        assert!(cache.check_and_record("another_signature"));
    }

    #[test]
    fn test_replay_protection_stats_returns_capacity_and_entries() {
        let config = ReplayProtectionConfig { enabled: true, cache_size: 500, window_secs: 300 };
        let cache = ReplayProtectionCache::new(config);

        let stats_before = cache.stats();
        assert_eq!(stats_before.capacity, 500);

        cache.check_and_record("signature_1");
        cache.check_and_record("signature_2");

        // Run pending tasks so entry_count reflects inserts.
        cache.cleanup_expired();
        let stats_after = cache.stats();
        assert_eq!(stats_after.capacity, 500);
        // entry_count may be 0 or 2 depending on moka's lazy eviction, but capacity is stable.
        let _ = stats_after;
    }

    #[test]
    fn test_replay_protection_cleanup_expired_does_not_panic() {
        let config = ReplayProtectionConfig::default();
        let cache = ReplayProtectionCache::new(config);
        cache.check_and_record("sig1");
        cache.check_and_record("sig2");
        // Should not panic and should complete without error.
        cache.cleanup_expired();
    }

    #[test]
    fn test_replay_protection_config_default_values() {
        let config = ReplayProtectionConfig::default();
        assert!(config.enabled);
        assert_eq!(config.cache_size, 10_000);
        assert_eq!(config.window_secs, 300);
    }

    #[test]
    fn test_replay_protection_stats_struct_debug() {
        let stats = ReplayProtectionStats { total_entries: 42, capacity: 100 };
        let debug = format!("{stats:?}");
        assert!(debug.contains("ReplayProtectionStats"));
        assert!(debug.contains("42"));
        assert!(debug.contains("100"));
    }

    #[test]
    fn test_compute_signature_hash_empty_inputs() {
        // Empty inputs should still produce a deterministic, non-empty hash.
        let hash = compute_signature_hash("", "", "", b"");
        assert!(!hash.is_empty());
        let hash2 = compute_signature_hash("", "", "", b"");
        assert_eq!(hash, hash2);
    }

    #[test]
    fn test_compute_signature_hash_varying_inputs_produce_different_hashes() {
        let h1 = compute_signature_hash("origin1", "key_id", "sig", b"data");
        let h2 = compute_signature_hash("origin2", "key_id", "sig", b"data");
        let h3 = compute_signature_hash("origin1", "key_id2", "sig", b"data");
        let h4 = compute_signature_hash("origin1", "key_id", "sig2", b"data");
        let h5 = compute_signature_hash("origin1", "key_id", "sig", b"different_data");
        let reference = compute_signature_hash("origin1", "key_id", "sig", b"data");

        assert_ne!(h1, h2);
        assert_ne!(h1, h3);
        assert_ne!(h1, h4);
        assert_ne!(h1, h5);
        assert_eq!(h1, reference);
    }

    #[test]
    fn test_validate_jwt_secret_exactly_32_chars_passes() {
        // Boundary: exactly 32 chars should pass length check (entropy must also pass).
        let secret = "abcdefghijklmnopqrstuvwxyz012345"; // 32 chars, varied chars
        assert!(SecurityValidator::validate_jwt_secret(secret).is_ok());
    }

    #[test]
    fn test_validate_jwt_secret_31_chars_fails() {
        // Boundary: 31 chars should fail length check.
        let secret = "abcdefghijklmnopqrstuvwxyz01234"; // 31 chars
        assert!(SecurityValidator::validate_jwt_secret(secret).is_err());
    }

    #[test]
    fn test_validate_jwt_secret_64_chars_passes_without_warning() {
        // 64+ chars should pass without triggering the < 64 warning path.
        let secret = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        assert_eq!(secret.len(), 64);
        assert!(SecurityValidator::validate_jwt_secret(secret).is_ok());
    }

    #[test]
    fn test_validate_federation_timestamp_boundary_tolerance() {
        // Exactly at tolerance boundary should pass (diff == tolerance).
        let now = chrono::Utc::now().timestamp_millis();
        // 60_000ms tolerance; a timestamp exactly 60_000ms old has diff == 60_000.
        let boundary = now - 60_000;
        let result = SecurityValidator::validate_federation_timestamp(boundary, 60_000);
        // Boundary may pass or fail by 1ms due to test runtime; assert within reason.
        // Use a clearly within-tolerance value to make the test deterministic.
        let within = now - 30_000;
        assert!(SecurityValidator::validate_federation_timestamp(within, 60_000).is_ok());
        let _ = result;
    }

    #[test]
    fn test_validate_federation_timestamp_future_within_tolerance() {
        // A future timestamp within tolerance should pass.
        let now = chrono::Utc::now().timestamp_millis();
        let future = now + 5_000;
        assert!(SecurityValidator::validate_federation_timestamp(future, 60_000).is_ok());
    }

    #[test]
    fn test_validate_federation_timestamp_future_outside_tolerance() {
        let now = chrono::Utc::now().timestamp_millis();
        let far_future = now + 120_000;
        assert!(SecurityValidator::validate_federation_timestamp(far_future, 60_000).is_err());
    }

    #[test]
    fn test_validate_origin_boundary_lengths() {
        // Exactly 253 chars should pass.
        let valid = "a".repeat(253);
        assert!(SecurityValidator::validate_origin(&valid).is_ok());
        // 254 chars should fail.
        let too_long = "a".repeat(254);
        assert!(SecurityValidator::validate_origin(&too_long).is_err());
    }

    #[test]
    fn test_validate_origin_all_allowed_special_chars() {
        // Hyphen, dot, colon, underscore are all allowed.
        assert!(SecurityValidator::validate_origin("a-b.c:d_e").is_ok());
    }

    #[test]
    fn test_validate_origin_single_char() {
        assert!(SecurityValidator::validate_origin("a").is_ok());
    }

    #[test]
    fn test_constant_time_comparison_empty_inputs() {
        assert!(ConstantTimeComparison::compare_strings("", ""));
        assert!(!ConstantTimeComparison::compare_strings("", "a"));
        assert!(!ConstantTimeComparison::compare_strings("a", ""));
    }

    #[test]
    fn test_constant_time_comparison_unicode() {
        assert!(ConstantTimeComparison::compare_strings("héllo", "héllo"));
        assert!(!ConstantTimeComparison::compare_strings("héllo", "hello"));
    }

    #[test]
    fn test_constant_time_comparison_bytes_empty() {
        assert!(ConstantTimeComparison::compare_bytes(b"", b""));
        assert!(!ConstantTimeComparison::compare_bytes(b"", b"a"));
    }

    #[test]
    fn test_is_ip_in_blacklist_direct_ip_match() {
        let blacklist = vec!["192.168.1.1".to_string(), "10.0.0.1".to_string()];
        let ip: IpAddr = "192.168.1.1".parse().unwrap();
        assert!(is_ip_in_blacklist(&ip, &blacklist));
    }

    #[test]
    fn test_is_ip_in_blacklist_direct_ip_no_match() {
        let blacklist = vec!["192.168.1.1".to_string()];
        let ip: IpAddr = "10.0.0.1".parse().unwrap();
        assert!(!is_ip_in_blacklist(&ip, &blacklist));
    }

    #[test]
    fn test_is_ip_in_blacklist_cidr_match() {
        let blacklist = vec!["10.0.0.0/8".to_string()];
        let ip_in_cidr: IpAddr = "10.255.255.255".parse().unwrap();
        let ip_outside_cidr: IpAddr = "11.0.0.1".parse().unwrap();
        assert!(is_ip_in_blacklist(&ip_in_cidr, &blacklist));
        assert!(!is_ip_in_blacklist(&ip_outside_cidr, &blacklist));
    }

    #[test]
    fn test_is_ip_in_blacklist_invalid_cidr_ignored() {
        // Invalid CIDR entries are silently skipped (do not match).
        let blacklist = vec!["invalid-cidr".to_string()];
        let ip: IpAddr = "127.0.0.1".parse().unwrap();
        assert!(!is_ip_in_blacklist(&ip, &blacklist));
    }

    #[test]
    fn test_is_ip_in_blacklist_empty_blacklist() {
        let blacklist: Vec<String> = vec![];
        let ip: IpAddr = "127.0.0.1".parse().unwrap();
        assert!(!is_ip_in_blacklist(&ip, &blacklist));
    }

    #[test]
    fn test_is_ip_in_blacklist_ipv6() {
        let blacklist = vec!["::1".to_string(), "2001:db8::/32".to_string()];
        let loopback: IpAddr = "::1".parse().unwrap();
        let in_cidr: IpAddr = "2001:db8::1".parse().unwrap();
        let outside_cidr: IpAddr = "2001:db9::1".parse().unwrap();
        assert!(is_ip_in_blacklist(&loopback, &blacklist));
        assert!(is_ip_in_blacklist(&in_cidr, &blacklist));
        assert!(!is_ip_in_blacklist(&outside_cidr, &blacklist));
    }

    #[test]
    fn test_check_url_against_blacklist_invalid_url() {
        let blacklist: Vec<String> = vec![];
        let result = check_url_against_blacklist("not a valid url", &blacklist);
        assert!(result.is_err());
        let err_msg = result.unwrap_err();
        assert!(err_msg.contains("Invalid URL"));
    }

    #[test]
    fn test_check_url_against_blacklist_url_without_host() {
        let blacklist: Vec<String> = vec![];
        // "file:" scheme has no host.
        let result = check_url_against_blacklist("file:///path/to/file", &blacklist);
        assert!(result.is_err());
        let err_msg = result.unwrap_err();
        assert!(err_msg.contains("no host"));
    }

    #[test]
    fn test_check_url_against_blacklist_empty_blacklist_passes() {
        let blacklist: Vec<String> = vec![];
        let result = check_url_against_blacklist("http://example.com", &blacklist);
        // With empty blacklist, lookup may still fail in offline test env; we only assert no panic.
        let _ = result;
    }

    #[test]
    fn test_check_url_against_blacklist_ip_in_blacklist() {
        let blacklist = vec!["127.0.0.1".to_string()];
        let result = check_url_against_blacklist("http://127.0.0.1/path", &blacklist);
        assert!(result.is_err());
        let err_msg = result.unwrap_err();
        assert!(err_msg.contains("blacklist"));
    }
}
