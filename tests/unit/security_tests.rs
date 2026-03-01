use synapse_rust::common::security::{
    compute_signature_hash, ConstantTimeComparison, ReplayProtectionCache, ReplayProtectionConfig,
    SecurityValidator,
};

#[test]
fn test_replay_protection_basic() {
    let config = ReplayProtectionConfig::default();
    let cache = ReplayProtectionCache::new(config);

    let sig = "test_signature_hash_123";
    
    assert!(cache.check_and_record(sig), "First check should pass");
    assert!(!cache.check_and_record(sig), "Second check should fail (replay)");
}

#[test]
fn test_replay_protection_multiple_signatures() {
    let config = ReplayProtectionConfig::default();
    let cache = ReplayProtectionCache::new(config);

    let signatures = vec![
        "signature_a",
        "signature_b",
        "signature_c",
        "signature_d",
        "signature_e",
    ];

    for sig in &signatures {
        assert!(cache.check_and_record(sig), "First use of {} should pass", sig);
    }

    for sig in &signatures {
        assert!(!cache.check_and_record(sig), "Second use of {} should fail", sig);
    }
}

#[test]
fn test_replay_protection_disabled() {
    let config = ReplayProtectionConfig {
        enabled: false,
        cache_size: 100,
        window_secs: 300,
    };
    let cache = ReplayProtectionCache::new(config);

    let sig = "test_signature";
    
    assert!(cache.check_and_record(sig));
    assert!(cache.check_and_record(sig));
    assert!(cache.check_and_record(sig));
}

#[test]
fn test_replay_protection_stats() {
    let config = ReplayProtectionConfig {
        enabled: true,
        cache_size: 100,
        window_secs: 300,
    };
    let cache = ReplayProtectionCache::new(config);

    cache.check_and_record("sig1");
    cache.check_and_record("sig2");
    cache.check_and_record("sig3");

    let stats = cache.stats();
    assert_eq!(stats.total_entries, 3);
    assert_eq!(stats.capacity, 100);
}

#[test]
fn test_signature_hash_consistency() {
    let origin = "matrix.org";
    let key_id = "ed25519:1";
    let signature = "abc123signature";
    let signed_bytes = b"request body content";

    let hash1 = compute_signature_hash(origin, key_id, signature, signed_bytes);
    let hash2 = compute_signature_hash(origin, key_id, signature, signed_bytes);

    assert_eq!(hash1, hash2, "Same inputs should produce same hash");
}

#[test]
fn test_signature_hash_different_inputs() {
    let base_hash = compute_signature_hash("origin1", "key1", "sig1", b"bytes1");

    assert_ne!(
        base_hash,
        compute_signature_hash("origin2", "key1", "sig1", b"bytes1"),
        "Different origin should produce different hash"
    );

    assert_ne!(
        base_hash,
        compute_signature_hash("origin1", "key2", "sig1", b"bytes1"),
        "Different key_id should produce different hash"
    );

    assert_ne!(
        base_hash,
        compute_signature_hash("origin1", "key1", "sig2", b"bytes1"),
        "Different signature should produce different hash"
    );

    assert_ne!(
        base_hash,
        compute_signature_hash("origin1", "key1", "sig1", b"bytes2"),
        "Different signed_bytes should produce different hash"
    );
}

#[test]
fn test_jwt_secret_validation_valid() {
    let valid_secrets = vec![
        "this_is_a_secure_secret_key_with_32_chars!",
        "aB1!xY2@zQ3#mN4$pL5%kM6^jK7&iH8*",
        "0123456789abcdef0123456789abcdef",
        "The quick brown fox jumps over the lazy dog 123!",
    ];

    for secret in valid_secrets {
        assert!(
            SecurityValidator::validate_jwt_secret(secret).is_ok(),
            "Secret '{}' should be valid",
            secret
        );
    }
}

#[test]
fn test_jwt_secret_validation_empty() {
    assert!(
        SecurityValidator::validate_jwt_secret("").is_err(),
        "Empty secret should be invalid"
    );
}

#[test]
fn test_jwt_secret_validation_too_short() {
    let short_secrets = vec![
        "short",
        "1234567890123456789012345678901", // 31 chars
        "a",
    ];

    for secret in short_secrets {
        assert!(
            SecurityValidator::validate_jwt_secret(secret).is_err(),
            "Secret '{}' (len={}) should be invalid",
            secret,
            secret.len()
        );
    }
}

#[test]
fn test_jwt_secret_validation_low_entropy() {
    let low_entropy_secrets = vec![
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "00000000000000000000000000000000",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    ];

    for secret in low_entropy_secrets {
        assert!(
            SecurityValidator::validate_jwt_secret(secret).is_err(),
            "Low entropy secret '{}' should be invalid",
            secret
        );
    }
}

#[test]
fn test_federation_timestamp_validation_current() {
    let now = chrono::Utc::now().timestamp_millis();
    let tolerance_ms = 60000i64;

    assert!(
        SecurityValidator::validate_federation_timestamp(now, tolerance_ms).is_ok(),
        "Current timestamp should be valid"
    );
}

#[test]
fn test_federation_timestamp_validation_within_tolerance() {
    let now = chrono::Utc::now().timestamp_millis();
    let tolerance_ms = 60000i64;

    let within_tolerance = now - 30000;
    assert!(
        SecurityValidator::validate_federation_timestamp(within_tolerance, tolerance_ms).is_ok(),
        "Timestamp within tolerance should be valid"
    );

    let within_tolerance_future = now + 30000;
    assert!(
        SecurityValidator::validate_federation_timestamp(within_tolerance_future, tolerance_ms).is_ok(),
        "Future timestamp within tolerance should be valid"
    );
}

#[test]
fn test_federation_timestamp_validation_expired() {
    let now = chrono::Utc::now().timestamp_millis();
    let tolerance_ms = 60000i64;

    let expired = now - 120000;
    assert!(
        SecurityValidator::validate_federation_timestamp(expired, tolerance_ms).is_err(),
        "Expired timestamp should be invalid"
    );

    let too_future = now + 120000;
    assert!(
        SecurityValidator::validate_federation_timestamp(too_future, tolerance_ms).is_err(),
        "Too far future timestamp should be invalid"
    );
}

#[test]
fn test_origin_validation_valid() {
    let valid_origins = vec![
        "matrix.org",
        "example.com",
        "sub.domain.example.com",
        "example.com:8448",
        "server-with-dashes.example.org",
        "a.co:443",
    ];

    for origin in valid_origins {
        assert!(
            SecurityValidator::validate_origin(origin).is_ok(),
            "Origin '{}' should be valid",
            origin
        );
    }
}

#[test]
fn test_origin_validation_invalid() {
    let invalid_origins = vec![
        "",
        "a".repeat(300).as_str(),
        "invalid!@#",
        "space in name",
        "dot.at.end.",
    ];

    for origin in invalid_origins {
        assert!(
            SecurityValidator::validate_origin(origin).is_err(),
            "Origin '{}' should be invalid",
            origin
        );
    }
}

#[test]
fn test_constant_time_comparison_equal() {
    let test_cases = vec![
        ("hello", "hello"),
        ("password123", "password123"),
        ("", ""),
        ("a", "a"),
        ("longer string for testing purposes", "longer string for testing purposes"),
    ];

    for (a, b) in test_cases {
        assert!(
            ConstantTimeComparison::compare_strings(a, b),
            "'{}' should equal '{}'",
            a,
            b
        );
    }
}

#[test]
fn test_constant_time_comparison_not_equal() {
    let test_cases = vec![
        ("hello", "world"),
        ("password123", "password124"),
        ("a", "b"),
        ("short", "longer"),
        ("prefix_match", "prefix_mismatch"),
    ];

    for (a, b) in test_cases {
        assert!(
            !ConstantTimeComparison::compare_strings(a, b),
            "'{}' should not equal '{}'",
            a,
            b
        );
    }
}

#[test]
fn test_constant_time_comparison_bytes() {
    assert!(ConstantTimeComparison::compare_bytes(b"test", b"test"));
    assert!(!ConstantTimeComparison::compare_bytes(b"test", b"best"));
    assert!(!ConstantTimeComparison::compare_bytes(b"test", b"testing"));
    assert!(ConstantTimeComparison::compare_bytes(&[], &[]));
}

#[test]
fn test_entropy_calculation_high() {
    let high_entropy = "aB1!xY2@zQ3#mN4$pL5%kM6^jK7&iH8*";
    let entropy = SecurityValidator::calculate_entropy(high_entropy);
    assert!(entropy > 4.0, "High entropy string should have entropy > 4.0, got {}", entropy);
}

#[test]
fn test_entropy_calculation_low() {
    let low_entropy = "aaaaaaaaaaaaaaaa";
    let entropy = SecurityValidator::calculate_entropy(low_entropy);
    assert!(entropy < 1.0, "Low entropy string should have entropy < 1.0, got {}", entropy);
}

#[test]
fn test_entropy_calculation_medium() {
    let medium_entropy = "abcdefghij123456";
    let entropy = SecurityValidator::calculate_entropy(medium_entropy);
    assert!(entropy > 2.0 && entropy < 4.0, "Medium entropy string should have entropy between 2.0 and 4.0, got {}", entropy);
}

#[test]
fn test_replay_protection_cleanup() {
    let config = ReplayProtectionConfig {
        enabled: true,
        cache_size: 10,
        window_secs: 1,
    };
    let cache = ReplayProtectionCache::new(config);

    for i in 0..10 {
        cache.check_and_record(&format!("sig_{}", i));
    }

    assert_eq!(cache.stats().total_entries, 10);

    std::thread::sleep(std::time::Duration::from_secs(2));

    cache.cleanup_expired();

    assert_eq!(cache.stats().total_entries, 0, "All entries should be expired and cleaned up");
}
