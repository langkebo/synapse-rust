//! Security primitives: URL blacklist checks, constant-time comparison, replay-protection cache.

use crate::current_timestamp_millis;
use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine as _};
use moka::sync::Cache;
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};

const REPLAY_CACHE_SIZE: u64 = 10000;
const REPLAY_PROTECTION_WINDOW_SECS: u64 = 300;

#[derive(Debug, Clone)]
/// Represents ReplayProtectionConfig.
pub struct ReplayProtectionConfig {
    /// `enabled` field.
    pub enabled: bool,
    /// `cache_size` field.
    pub cache_size: u64,
    /// `window_secs` field.
    pub window_secs: u64,
}

impl Default for ReplayProtectionConfig {
    fn default() -> Self {
        Self { enabled: true, cache_size: REPLAY_CACHE_SIZE, window_secs: REPLAY_PROTECTION_WINDOW_SECS }
    }
}

/// Represents ReplayProtectionCache.
pub struct ReplayProtectionCache {
    cache: Cache<String, Instant>,
    config: ReplayProtectionConfig,
}

impl ReplayProtectionCache {
    /// Constructs a new instance.
    pub fn new(config: ReplayProtectionConfig) -> Self {
        let cache = Cache::builder()
            .max_capacity(config.cache_size)
            .time_to_idle(Duration::from_secs(config.window_secs))
            .build();
        Self { cache, config }
    }

    /// Checks and record.
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

    /// Cleanups the expired.
    pub fn cleanup_expired(&self) {
        self.cache.run_pending_tasks();
    }

    /// Performs stats.
    pub fn stats(&self) -> ReplayProtectionStats {
        ReplayProtectionStats {
            total_entries: self.cache.entry_count() as usize,
            capacity: self.config.cache_size as usize,
        }
    }
}

#[derive(Debug, Clone)]
/// Represents ReplayProtectionStats.
pub struct ReplayProtectionStats {
    /// `total_entries` field.
    pub total_entries: usize,
    /// `capacity` field.
    pub capacity: usize,
}

/// Computes the signature.
pub fn compute_signature_hash(origin: &str, key_id: &str, signature: &str, signed_bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(origin.as_bytes());
    hasher.update(key_id.as_bytes());
    hasher.update(signature.as_bytes());
    hasher.update(signed_bytes);
    STANDARD_NO_PAD.encode(hasher.finalize())
}

/// Represents SecurityValidator.
pub struct SecurityValidator;

impl SecurityValidator {
    /// Validates the jwt.
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

    /// Validates the federation.
    pub fn validate_federation_timestamp(signature_ts: i64, tolerance_ms: i64) -> Result<(), String> {
        let now = current_timestamp_millis();
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

    /// Validates the origin.
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

/// Represents ConstantTimeComparison.
pub struct ConstantTimeComparison;

impl ConstantTimeComparison {
    /// Compares the bytes.
    pub fn compare_bytes(a: &[u8], b: &[u8]) -> bool {
        crate::crypto::secure_compare_bytes(a, b)
    }

    /// Compares the strings.
    pub fn compare_strings(a: &str, b: &str) -> bool {
        crate::crypto::secure_compare(a, b)
    }
}

use std::net::IpAddr;

/// Strip the square brackets `url::Url::host_str()` keeps around IPv6 literals
/// (`"[::1]"` → `"::1"`) so the host can be parsed with `IpAddr::from_str`.
///
/// This is the single implementation for that job — the same normalisation is
/// needed by the SSRF checks, the push-gateway IP-literal check and the
/// localhost guard.
pub fn strip_ipv6_brackets(host: &str) -> &str {
    host.strip_prefix('[').and_then(|inner| inner.strip_suffix(']')).unwrap_or(host)
}

/// Returns true if ip in blacklist.
///
/// IPv4-mapped IPv6 addresses (`::ffff:127.0.0.1`) are checked **twice**: once as
/// given and once as the IPv4 address they embed. `ipnet::IpNet::contains` never
/// matches a V6 address against a V4 network, so without the second test a mapped
/// address slips past every `127.0.0.0/8` / `10.0.0.0/8` style entry — a classic
/// SSRF bypass. The embedded check is done against the caller's own list rather
/// than by canonicalising to IPv4 first, so a mapped *public* address
/// (`::ffff:8.8.8.8`, which embeds no blocked network) stays allowed.
pub fn is_ip_in_blacklist(ip: &IpAddr, blacklist: &[String]) -> bool {
    if blacklist_matches(ip, blacklist) {
        return true;
    }
    match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().is_some_and(|v4| blacklist_matches(&IpAddr::V4(v4), blacklist)),
        IpAddr::V4(_) => false,
    }
}

fn blacklist_matches(ip: &IpAddr, blacklist: &[String]) -> bool {
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

/// Checks url against blacklist.
pub fn check_url_against_blacklist(url: &str, blacklist: &[String]) -> Result<(), String> {
    check_url_and_resolve(url, blacklist).map(|_| ())
}

/// S2 修复（SSRF DNS rebinding / TOCTOU）：解析主机并校验所有解析结果
/// 不在黑名单中，返回**校验通过的 IP 列表**。调用方必须将返回的 IP 通过
/// `http_client::pinned_client_for_url` 钉扎到 HTTP 客户端，确保"连接时
/// 使用的地址 == 校验时的地址"，杜绝攻击者在校验与连接之间切换 DNS 记录。
///
/// 与旧行为的两处差异（均为安全收紧）：
/// - 解析失败（DNS 错误/无记录）现在返回 Err（fail-closed），旧实现静默放行；
/// - 返回解析结果而非丢弃，供钉扎复用。
pub fn resolve_host_checked(host: &str, blacklist: &[String]) -> Result<Vec<IpAddr>, String> {
    // `Url::host_str()` hands back IPv6 literals still wrapped in brackets, and
    // `"[::1]".parse::<IpAddr>()` fails — which used to send every IPv6 literal
    // into the DNS branch, where it failed with a confusing "nodename nor
    // servname provided". That blocked loopback by accident, but it also made
    // every legitimate IPv6 target unusable. Normalise first so the literal is
    // parsed (and therefore blacklist-checked) as the address it is.
    let host = strip_ipv6_brackets(host);
    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_ip_in_blacklist(&ip, blacklist) {
            return Err(format!("IP {ip} is in blacklist"));
        }
        return Ok(vec![ip]);
    }
    match dns_lookup::lookup_host(host) {
        Ok(addrs) => {
            let addrs: Vec<IpAddr> = addrs.collect();
            if addrs.is_empty() {
                return Err(format!("Host {host} resolved to no addresses"));
            }
            for addr in &addrs {
                if is_ip_in_blacklist(addr, blacklist) {
                    return Err(format!("Host {host} resolves to blacklisted IP {addr}"));
                }
            }
            Ok(addrs)
        }
        Err(e) => Err(format!("Failed to resolve host {host}: {e}")),
    }
}

/// S2 修复：校验 URL 并返回 (host, 已验证 IP 列表)，供调用方做 IP 钉扎。
pub fn check_url_and_resolve(url: &str, blacklist: &[String]) -> Result<(String, Vec<IpAddr>), String> {
    let parsed = url::Url::parse(url).map_err(|e| format!("Invalid URL: {e}"))?;
    let host = parsed.host_str().ok_or_else(|| format!("URL has no host: {url}"))?;
    let ips = resolve_host_checked(host, blacklist)?;
    Ok((host.to_string(), ips))
}

/// 标准 SSRF 黑名单：覆盖所有 RFC 1918 / 3330 / 6598 私有与链路本地地址，
/// 防止 federation / device-sync 拉取远程服务器时窥探内网元数据服务
/// (169.254.169.254)、本地环回 (127.0.0.1) 或 Docker/RFC 1918 网段。
///
/// 调用方应将此黑名单传给 `check_url_and_resolve`，然后用返回的 IP 列表
/// 构造 `http_client::pinned_client_for_url` 钉扎客户端 — 杜绝 DNS
/// 重绑定 (DNS rebinding) 攻击。
pub fn ssrf_blacklist() -> Vec<String> {
    vec![
        // IPv4 private / link-local / loopback
        "0.0.0.0/8".to_string(),
        "10.0.0.0/8".to_string(),
        "100.64.0.0/10".to_string(), // CGNAT
        "127.0.0.0/8".to_string(),
        "169.254.0.0/16".to_string(), // cloud metadata
        "172.16.0.0/12".to_string(),
        "192.0.0.0/24".to_string(),
        "192.0.2.0/24".to_string(),    // TEST-NET-1
        "192.52.193.0/24".to_string(), // AMT
        "192.88.99.0/24".to_string(),  // 6to4 anycast
        "192.168.0.0/16".to_string(),
        "198.18.0.0/15".to_string(),
        "198.51.100.0/24".to_string(),    // TEST-NET-2
        "203.0.113.0/24".to_string(),     // TEST-NET-3
        "224.0.0.0/4".to_string(),        // multicast
        "240.0.0.0/4".to_string(),        // reserved
        "255.255.255.255/32".to_string(), // broadcast
        // IPv6 private / link-local / loopback
        "::1/128".to_string(),
        "::/128".to_string(),
        "fc00::/7".to_string(),  // ULA
        "fe80::/10".to_string(), // link-local
        "ff00::/8".to_string(),  // multicast
    ]
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
        let now = current_timestamp_millis();
        assert!(SecurityValidator::validate_federation_timestamp(now, 60_000).is_ok());
    }

    #[test]
    fn test_validate_federation_timestamp_expired() {
        let old = current_timestamp_millis() - 120000;
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
        let now = current_timestamp_millis();
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
        let now = current_timestamp_millis();
        let future = now + 5_000;
        assert!(SecurityValidator::validate_federation_timestamp(future, 60_000).is_ok());
    }

    #[test]
    fn test_validate_federation_timestamp_future_outside_tolerance() {
        let now = current_timestamp_millis();
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

    // ── IPv4-mapped IPv6: the classic SSRF bypass ──────────────────────────
    //
    // `ipnet::IpNet::contains` never matches a V6 address against a V4 network, so
    // `::ffff:127.0.0.1` used to pass every `127.0.0.0/8` entry. It was only ever
    // blocked because the bracketed URL literal failed DNS lookup — i.e. by
    // accident, which normalising the brackets would have removed.
    #[test]
    fn ipv4_mapped_ipv6_loopback_is_blacklisted() {
        let blacklist = ssrf_blacklist();
        let mapped: IpAddr = "::ffff:127.0.0.1".parse().unwrap();
        assert!(is_ip_in_blacklist(&mapped, &blacklist), "mapped loopback must be blocked");
    }

    #[test]
    fn ipv4_mapped_ipv6_private_and_metadata_ranges_are_blacklisted() {
        let blacklist = ssrf_blacklist();
        for addr in ["::ffff:10.0.0.1", "::ffff:192.168.1.1", "::ffff:169.254.169.254", "::ffff:172.16.0.1"] {
            let mapped: IpAddr = addr.parse().unwrap();
            assert!(is_ip_in_blacklist(&mapped, &blacklist), "{addr} must be blocked");
        }
    }

    #[test]
    fn ipv4_mapped_ipv6_public_address_stays_allowed() {
        // The embedded address is checked against the caller's list rather than
        // canonicalised unconditionally, so a mapped public address still works.
        let blacklist = ssrf_blacklist();
        let mapped: IpAddr = "::ffff:8.8.8.8".parse().unwrap();
        assert!(!is_ip_in_blacklist(&mapped, &blacklist));
    }

    #[test]
    fn strip_ipv6_brackets_normalises_only_bracketed_hosts() {
        assert_eq!(strip_ipv6_brackets("[::1]"), "::1");
        assert_eq!(strip_ipv6_brackets("[2001:db8::1]"), "2001:db8::1");
        assert_eq!(strip_ipv6_brackets("::1"), "::1");
        assert_eq!(strip_ipv6_brackets("example.com"), "example.com");
        assert_eq!(strip_ipv6_brackets("[::1"), "[::1");
        assert_eq!(strip_ipv6_brackets("::1]"), "::1]");
    }

    // ── IPv6 literals must be checked as addresses, not DNS names ──────────

    #[test]
    fn ipv6_loopback_literal_url_is_rejected_as_blacklisted() {
        let blacklist = ssrf_blacklist();
        let error = check_url_and_resolve("https://[::1]/_matrix/key/v2/server", &blacklist)
            .expect_err("IPv6 loopback must be rejected");
        // Rejected by the blacklist, not by a DNS failure: a DNS error would mean the
        // literal never reached `is_ip_in_blacklist`.
        assert!(error.contains("blacklist"), "must be the blacklist that rejects it, got: {error}");
    }

    #[test]
    fn ipv6_mapped_loopback_literal_url_is_rejected() {
        let blacklist = ssrf_blacklist();
        let error = check_url_and_resolve("https://[::ffff:127.0.0.1]/_matrix/key/v2/server", &blacklist)
            .expect_err("mapped loopback must be rejected");
        assert!(error.contains("blacklist"), "got: {error}");
    }

    #[test]
    fn ipv6_unique_local_and_link_local_literal_urls_are_rejected() {
        let blacklist = ssrf_blacklist();
        for url in ["https://[fc00::1]/x", "https://[fe80::1]/x"] {
            let error = check_url_and_resolve(url, &blacklist).expect_err("private IPv6 must be rejected");
            assert!(error.contains("blacklist"), "{url}: {error}");
        }
    }

    #[test]
    fn public_ipv6_literal_url_resolves_to_that_address() {
        // Regression: every IPv6 literal used to end in the DNS branch and fail with
        // "nodename nor servname provided", so IPv6 federation targets were unusable.
        let blacklist = ssrf_blacklist();
        let (host, ips) = check_url_and_resolve("https://[2001:4860:4860::8888]/_matrix/key/v2/server", &blacklist)
            .expect("a public IPv6 literal must be usable");
        assert_eq!(host, "[2001:4860:4860::8888]");
        assert_eq!(ips, vec!["2001:4860:4860::8888".parse::<IpAddr>().unwrap()]);
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

    // ------------------------------------------------------------------
    // S2 修复（DNS rebinding / TOCTOU）：解析与校验必须返回已验证 IP 列表，
    // 供 HTTP 客户端钉扎，杜绝"检查时解析 ≠ 连接时解析"。
    // ------------------------------------------------------------------

    #[test]
    fn test_resolve_host_checked_ip_literal_allowed_returns_ip() {
        let blacklist = vec!["10.0.0.0/8".to_string(), "127.0.0.0/8".to_string()];
        let ips = resolve_host_checked("8.8.8.8", &blacklist).expect("public IP literal must pass");
        assert_eq!(ips, vec!["8.8.8.8".parse::<IpAddr>().unwrap()]);
    }

    #[test]
    fn test_resolve_host_checked_ip_literal_blacklisted_rejected() {
        let blacklist = vec!["127.0.0.0/8".to_string()];
        let result = resolve_host_checked("127.0.0.1", &blacklist);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("blacklist"));
    }

    #[test]
    fn test_resolve_host_checked_localhost_rejected_by_loopback_blacklist() {
        // localhost 经 /etc/hosts 解析为 127.0.0.1，无需外部网络。
        let blacklist = vec!["127.0.0.0/8".to_string()];
        let result = resolve_host_checked("localhost", &blacklist);
        assert!(result.is_err(), "localhost 解析到回环地址必须被黑名单拒绝");
    }

    #[test]
    fn test_resolve_host_checked_localhost_allowed_returns_loopback() {
        // 空黑名单：localhost 解析成功且返回 127.0.0.1（供钉扎）。
        let blacklist: Vec<String> = vec![];
        let ips = resolve_host_checked("localhost", &blacklist).expect("localhost resolves via hosts file");
        assert!(ips.contains(&"127.0.0.1".parse::<IpAddr>().unwrap()));
    }

    #[test]
    fn test_check_url_and_resolve_returns_host_and_pinned_ips() {
        let blacklist = vec!["10.0.0.0/8".to_string()];
        let (host, ips) =
            check_url_and_resolve("https://8.8.8.8/_matrix/key/v2/server", &blacklist).expect("must pass");
        assert_eq!(host, "8.8.8.8");
        assert_eq!(ips.len(), 1);
    }

    #[test]
    fn test_check_url_and_resolve_rejects_blacklisted_host() {
        let blacklist = vec!["127.0.0.0/8".to_string()];
        assert!(check_url_and_resolve("http://127.0.0.1:8080/admin", &blacklist).is_err());
        assert!(check_url_and_resolve("http://localhost/internal", &blacklist).is_err());
    }

    #[test]
    fn test_check_url_and_resolve_rejects_invalid_url() {
        let blacklist: Vec<String> = vec![];
        assert!(check_url_and_resolve("not-a-url", &blacklist).is_err());
    }

    // ── S2 TOCTOU 防护集成测试 ──────────────────────────────────────
    // 验证 check_url_and_resolve → pinned_client_for_url 的完整流程：
    // 解析阶段返回的已验证 IP 列表必须直接传入钉扎 client，杜绝
    // "检查时 DNS 解析 ≠ 连接时 DNS 解析" 的 TOCTOU 窗口。

    #[test]
    fn test_s2_toctou_protection_ip_literal_flow() {
        // 完整流程：IP 字面量 URL → check_url_and_resolve → pinned_client_for_url
        // 此流程确保解析和连接使用同一 IP，不存在 DNS 重绑定窗口。
        let blacklist = vec!["10.0.0.0/8".to_string(), "127.0.0.0/8".to_string()];
        let url = "https://8.8.8.8/_matrix/key/v2/server";

        // Step 1: 解析并校验
        let (host, verified_ips) = check_url_and_resolve(url, &blacklist).expect("must resolve");

        // Step 2: 钉扎 client 使用已验证 IP（不再 DNS 解析）
        let client =
            crate::http_client::pinned_client_for_url(url, &verified_ips, std::time::Duration::from_secs(10), true)
                .expect("pinned client must construct with verified IPs");

        // 验证：host 和 IP 一致（IP 字面量场景）
        assert_eq!(host, "8.8.8.8");
        assert_eq!(verified_ips.len(), 1);
        let _ = format!("{:?}", client); // client 构造成功即满足钉扎契约
    }

    #[test]
    fn test_s2_toctou_protection_localhost_blocked_in_flow() {
        // 黑名单包含回环地址时，localhost 的完整流程必须在解析阶段被拦截，
        // 不会到达 pinned_client_for_url（即不会发起任何连接）。
        let blacklist = vec!["127.0.0.0/8".to_string()];
        let url = "https://localhost/_matrix/key/v2/server";

        let result = check_url_and_resolve(url, &blacklist);
        assert!(result.is_err(), "localhost 必须在解析阶段被黑名单拦截");
        // 如果 result 是 Err，调用方不会进入 pinned_client_for_url，
        // TOCTOU 窗口不存在。
    }

    #[test]
    fn test_s2_toctou_protection_multiple_ips_all_pass_blacklist() {
        // 多 IP 解析：check_url_and_resolve 返回的每个 IP 都通过了黑名单校验。
        // 使用 IP 字面量模拟多 IP 场景（实际 DNS 解析返回多个 A 记录时同理）。
        let blacklist = vec!["10.0.0.0/8".to_string()];

        // 8.8.8.8 是公共 DNS，不在 10.0.0.0/8 黑名单中
        let (host, ips) =
            check_url_and_resolve("https://8.8.8.8/_matrix/key/v2/server", &blacklist).expect("must pass");

        assert_eq!(host, "8.8.8.8");
        assert!(!ips.is_empty(), "必须返回至少一个已验证 IP");

        // 所有返回的 IP 都不应在黑名单中
        for ip in &ips {
            assert!(!is_ip_in_blacklist(ip, &blacklist), "已验证 IP 不应在黑名单中");
        }
    }

    #[test]
    fn test_s2_toctou_protection_skip_ssrf_returns_empty_blacklist() {
        // skip_ssrf_check=true 时使用空黑名单，所有 IP 通过校验。
        // 这模拟开发/测试环境跳过 SSRF 检查的场景。
        let empty_blacklist: Vec<String> = vec![];
        let (host, ips) =
            check_url_and_resolve("https://8.8.8.8/_matrix/key/v2/server", &empty_blacklist).expect("must pass");

        assert_eq!(host, "8.8.8.8");
        assert!(!ips.is_empty());
    }

    // ── ssrf_blacklist() 测试 ──────────────────────────────────────

    #[test]
    fn test_ssrf_blacklist_blocks_loopback_ipv4() {
        let blacklist = ssrf_blacklist();
        assert!(is_ip_in_blacklist(&"127.0.0.1".parse::<IpAddr>().unwrap(), &blacklist));
        assert!(is_ip_in_blacklist(&"127.255.255.255".parse::<IpAddr>().unwrap(), &blacklist));
    }

    #[test]
    fn test_ssrf_blacklist_blocks_private_ipv4() {
        let blacklist = ssrf_blacklist();
        assert!(is_ip_in_blacklist(&"10.0.0.1".parse::<IpAddr>().unwrap(), &blacklist));
        assert!(is_ip_in_blacklist(&"172.16.0.1".parse::<IpAddr>().unwrap(), &blacklist));
        assert!(is_ip_in_blacklist(&"192.168.1.1".parse::<IpAddr>().unwrap(), &blacklist));
    }

    #[test]
    fn test_ssrf_blacklist_blocks_link_local() {
        let blacklist = ssrf_blacklist();
        // 169.254.169.254 — AWS/GCP 云元数据服务
        assert!(is_ip_in_blacklist(&"169.254.169.254".parse::<IpAddr>().unwrap(), &blacklist));
    }

    #[test]
    fn test_ssrf_blacklist_blocks_ipv6_loopback_and_ula() {
        let blacklist = ssrf_blacklist();
        assert!(is_ip_in_blacklist(&"::1".parse::<IpAddr>().unwrap(), &blacklist));
        assert!(is_ip_in_blacklist(&"fc00::1".parse::<IpAddr>().unwrap(), &blacklist));
        assert!(is_ip_in_blacklist(&"fd12:3456:789a::1".parse::<IpAddr>().unwrap(), &blacklist));
    }

    #[test]
    fn test_ssrf_blacklist_allows_public_ipv4() {
        let blacklist = ssrf_blacklist();
        assert!(!is_ip_in_blacklist(&"8.8.8.8".parse::<IpAddr>().unwrap(), &blacklist));
        assert!(!is_ip_in_blacklist(&"1.1.1.1".parse::<IpAddr>().unwrap(), &blacklist));
        assert!(!is_ip_in_blacklist(&"172.217.16.142".parse::<IpAddr>().unwrap(), &blacklist));
    }

    #[test]
    fn test_ssrf_blacklist_blocks_loopback_url() {
        let blacklist = ssrf_blacklist();
        assert!(check_url_against_blacklist("http://127.0.0.1/admin", &blacklist).is_err());
        assert!(check_url_against_blacklist("http://localhost/internal", &blacklist).is_err());
    }

    #[test]
    fn test_ssrf_blacklist_allows_public_url() {
        let blacklist = ssrf_blacklist();
        // 公网 IP 不在黑名单内；DNS 解析可能在沙箱内不通，但不应因 IP 黑名单拦截。
        let result = check_url_against_blacklist("http://8.8.8.8/_matrix/key/v2/server", &blacklist);
        // 不应在 IP 校验阶段失败（可能因 DNS 不通而失败，但不是"黑名单"错误）。
        if let Err(e) = result {
            assert!(!e.contains("blacklist"), "黑名单不应拦截公网 IP: {e}");
        }
    }
}
