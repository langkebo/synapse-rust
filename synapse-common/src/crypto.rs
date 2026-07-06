use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use hmac::{Hmac, Mac};
use rand::Rng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::argon2_config::Argon2Config;

type HmacSha256 = Hmac<Sha256>;

pub fn hash_password(password: &str) -> Result<String, String> {
    let config = Argon2Config::get_global();
    hash_password_with_config(password, &config)
}

pub fn hash_password_with_config(password: &str, config: &Argon2Config) -> Result<String, String> {
    let salt = SaltString::generate(argon2::password_hash::rand_core::OsRng);
    let params = config.to_argon2_params().map_err(|e| e.to_string())?;
    let argon2 = Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);

    let password_hash = argon2.hash_password(password.as_bytes(), &salt).map_err(|e| e.to_string())?.to_string();

    Ok(password_hash)
}

pub fn hash_password_with_params(password: &str, m_cost: u32, t_cost: u32, p_cost: u32) -> Result<String, String> {
    let config = Argon2Config::new(m_cost, t_cost, p_cost).map_err(|e| format!("Invalid Argon2 parameters: {e}"))?;
    hash_password_with_config(password, &config)
}

pub fn verify_password(password: &str, password_hash: &str, allow_legacy: bool) -> Result<bool, String> {
    if password_hash.starts_with("$argon2") {
        let parsed_hash = PasswordHash::new(password_hash).map_err(|e| e.to_string())?;
        return Ok(Argon2::default().verify_password(password.as_bytes(), &parsed_hash).is_ok());
    }

    if !allow_legacy {
        tracing::warn!("Legacy password hash detected but legacy hashes are disabled");
        return Err("Legacy password hashes are no longer supported".to_string());
    }

    tracing::warn!("Legacy password hash detected - please update password");

    let password_hash_trimmed = password_hash.trim_start_matches('$');
    let parts: Vec<&str> = password_hash_trimmed.split('$').collect();

    if parts.len() < 4 {
        return Err("Invalid hash format".to_string());
    }

    let algo = parts[0];
    let version = parts[1];
    let _params = parts[2];
    let salt = parts[3];
    let hash = parts[parts.len() - 1];

    if algo != "sha256" || version != "v=1" {
        return Err(format!("Unsupported hash algorithm: {algo}"));
    }

    if parts.len() >= 7 {
        let iterations = parts[5].parse::<u32>().unwrap_or(10000);
        let mut computed_hash = [0u8; 32];
        let mut input = password.as_bytes().to_vec();
        input.extend_from_slice(salt.as_bytes());

        for _ in 0..iterations {
            let mut hasher = Sha256::new();
            hasher.update(&input);
            let result = hasher.finalize();
            computed_hash.copy_from_slice(&result);
            input = computed_hash.to_vec();
        }
        let encoded_hash = URL_SAFE_NO_PAD.encode(computed_hash);
        Ok(secure_compare(&encoded_hash, hash))
    } else {
        let mut hasher = Sha256::new();
        hasher.update(password);
        hasher.update(salt);
        let result = hasher.finalize();
        let encoded_hash = URL_SAFE_NO_PAD.encode(result);
        Ok(secure_compare(&encoded_hash, hash))
    }
}

pub fn secure_compare(a: &str, b: &str) -> bool {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    let max_len = a_bytes.len().max(b_bytes.len());

    let mut result: u8 = if a_bytes.len() != b_bytes.len() { 0xFF } else { 0 };

    for i in 0..max_len {
        let a_byte = a_bytes.get(i).copied().unwrap_or(0);
        let b_byte = b_bytes.get(i).copied().unwrap_or(0);
        result |= a_byte ^ b_byte;
    }
    result == 0
}

pub fn verify_password_legacy(password: &str, password_hash: &str) -> bool {
    verify_password(password, password_hash, true).unwrap_or(false)
}

pub fn is_legacy_hash(password_hash: &str) -> bool {
    !password_hash.starts_with("$argon2")
}

pub fn migrate_password_hash(password: &str, m_cost: u32, t_cost: u32, p_cost: u32) -> Result<String, String> {
    hash_password_with_params(password, m_cost, t_cost, p_cost)
}

pub fn migrate_password_hash_with_config(password: &str, config: &Argon2Config) -> Result<String, String> {
    hash_password_with_config(password, config)
}

pub fn generate_token(length: usize) -> String {
    let mut bytes = vec![0u8; length];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn generate_room_id(server_name: &str) -> String {
    let mut bytes = [0u8; 18];
    rand::rng().fill_bytes(&mut bytes);
    format!("!{}:{}", URL_SAFE_NO_PAD.encode(bytes), server_name)
}

pub fn generate_event_id(server_name: &str) -> String {
    let timestamp = chrono::Utc::now().timestamp_millis();
    let mut bytes = [0u8; 18];
    rand::rng().fill_bytes(&mut bytes);
    format!("${}${}:{}", timestamp, URL_SAFE_NO_PAD.encode(bytes), server_name)
}

pub fn generate_device_id() -> String {
    let mut bytes = [0u8; 10];
    rand::rng().fill_bytes(&mut bytes);
    format!("DEVICE{}", URL_SAFE_NO_PAD.encode(bytes).get(..10).unwrap_or("DEVICE0000"))
}

pub fn generate_salt() -> String {
    let mut bytes = [0u8; 16];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn compute_hash(data: impl AsRef<[u8]>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data.as_ref());
    URL_SAFE_NO_PAD.encode(&hasher.finalize()[..])
}

/// The known dev/test fallback secret. In production, this value must NEVER be used.
const DEV_TEST_TOKEN_HASH_SECRET: &str = "dev-test-token-hash-secret-do-not-use-in-production";

/// Validate that `TOKEN_HASH_SECRET` is properly configured for the current build mode.
///
/// In production (release builds): the secret must be set, at least 32 bytes, and must
/// not equal the known dev/test fallback. Returns `Err` if any check fails.
///
/// In dev/test (debug builds): a missing or weak secret is allowed with a warning.
///
/// Call this at server startup before any request is processed.
pub fn validate_token_hash_secret() -> Result<(), String> {
    match std::env::var("TOKEN_HASH_SECRET") {
        Ok(secret) => {
            if secret.len() < 32 {
                return Err(format!("TOKEN_HASH_SECRET must be at least 32 bytes, got {} bytes", secret.len()));
            }
            if secret == DEV_TEST_TOKEN_HASH_SECRET && !cfg!(debug_assertions) {
                return Err("TOKEN_HASH_SECRET is set to the known dev/test fallback value. \
                     This is NOT safe for production."
                    .to_string());
            }
            if secret == DEV_TEST_TOKEN_HASH_SECRET {
                tracing::warn!(
                    "TOKEN_HASH_SECRET is set to the dev/test fallback. \
                     This is NOT safe for production."
                );
            }
            Ok(())
        }
        Err(_) => {
            if cfg!(debug_assertions) {
                tracing::warn!(
                    "TOKEN_HASH_SECRET not set, using dev/test fallback. \
                     This is NOT safe for production."
                );
                Ok(())
            } else {
                Err("TOKEN_HASH_SECRET environment variable must be set in production".to_string())
            }
        }
    }
}

#[allow(clippy::expect_used, clippy::unnecessary_literal_unwrap)]
pub fn hash_token(token: &str) -> String {
    let server_secret = std::env::var("TOKEN_HASH_SECRET").unwrap_or_else(|_| {
        if cfg!(debug_assertions) {
            // Dev/test fallback only — never used in production (startup validation enforces this).
            DEV_TEST_TOKEN_HASH_SECRET.to_string()
        } else {
            // Production: this should never be reached because validate_token_hash_secret()
            // is called at startup. Defense in depth: fail rather than use a weak secret.
            None::<String>.expect("TOKEN_HASH_SECRET environment variable must be set in production")
        }
    });
    encode_base64(hmac_sha256(server_secret, token))
}

pub fn hash_token_legacy(token: &str) -> String {
    compute_hash(token)
}

pub fn verify_token_hash(token: &str, stored_hash: &str) -> bool {
    if secure_compare(&hash_token(token), stored_hash) {
        return true;
    }
    secure_compare(&hash_token_legacy(token), stored_hash)
}

#[allow(clippy::expect_used)]
pub fn hmac_sha256(key: impl AsRef<[u8]>, data: impl AsRef<[u8]>) -> Vec<u8> {
    let key = key.as_ref();
    let data = data.as_ref();
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC-SHA256 accepts keys of any size");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

pub fn random_string(length: usize) -> String {
    static CHARSET: [u8; 62] = *b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::rng();
    let result: String = (0..length)
        .map(|_| {
            let idx = rng.random_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect();
    result
}

pub fn encode_base64(data: impl AsRef<[u8]>) -> String {
    URL_SAFE_NO_PAD.encode(data.as_ref())
}

pub fn decode_base64(s: &str) -> Result<Vec<u8>, base64::DecodeError> {
    URL_SAFE_NO_PAD.decode(s)
}

/// Decode a base64 string into a fixed-size 32-byte array.
/// Tries multiple base64 engines (STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD)
/// to maximize compatibility with various key formats.
pub fn decode_base64_32(value: &str) -> Option<[u8; 32]> {
    use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE};
    let value = value.trim();
    let try_decode = |engine: &base64::engine::general_purpose::GeneralPurpose| -> Option<[u8; 32]> {
        let bytes = engine.decode(value).ok()?;
        if bytes.len() == 32 {
            let mut out = [0u8; 32];
            out.copy_from_slice(&bytes);
            Some(out)
        } else {
            None
        }
    };
    try_decode(&STANDARD)
        .or_else(|| try_decode(&STANDARD_NO_PAD))
        .or_else(|| try_decode(&URL_SAFE))
        .or_else(|| try_decode(&URL_SAFE_NO_PAD))
}

/// Constant-time comparison for byte slices.
/// Returns `true` if `a` and `b` are equal in both length and content.
/// The comparison runs in time proportional to the longer slice, preventing timing attacks.
/// Length differences are folded into the comparison result rather than returning early.
pub fn secure_compare_bytes(a: &[u8], b: &[u8]) -> bool {
    let max_len = a.len().max(b.len());
    let mut result: u8 = if a.len() != b.len() { 0xFF } else { 0 };
    for i in 0..max_len {
        let a_byte = a.get(i).copied().unwrap_or(0);
        let b_byte = b.get(i).copied().unwrap_or(0);
        result |= a_byte ^ b_byte;
    }
    result == 0
}

/// Encode binary data as a lowercase hexadecimal string.
pub fn encode_hex(data: impl AsRef<[u8]>) -> String {
    hex::encode(data.as_ref())
}

/// Decode a hexadecimal string into bytes.
pub fn decode_hex(s: &str) -> Result<Vec<u8>, hex::FromHexError> {
    hex::decode(s)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerSigningKey {
    pub key_id: String,
    pub key: String,
    pub expired_ts: i64,
}

/// Generate an Ed25519 signing key pair for testing.
/// Returns `(key_id, base64_public_key)`.
#[cfg(test)]
pub fn generate_signing_key() -> (String, String) {
    let key_id = format!("ed25519:{}", random_string(8));
    let key = random_string(44);
    (key_id, key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::argon2_config::Argon2Config;

    #[test]
    fn test_hash_password_and_verify() {
        let config = Argon2Config::new(65536, 3, 1).unwrap();
        let password = "test_password_123";
        let hash = hash_password_with_config(password, &config).unwrap();
        assert!(hash.starts_with("$argon2"));
        assert!(verify_password(password, &hash, false).unwrap());
        assert!(!verify_password("wrong_password", &hash, false).unwrap());
    }

    #[test]
    fn test_hash_password_with_params() {
        let password = "test_password";
        let hash = hash_password_with_params(password, 65536, 3, 1).unwrap();
        assert!(hash.starts_with("$argon2"));
    }

    #[test]
    fn test_hash_password_backward_compatible() {
        let password = "test_password";
        let hash = hash_password_with_params(password, 65536, 3, 1).unwrap();
        assert!(hash.starts_with("$argon2"));
        assert!(verify_password(password, &hash, false).unwrap());
    }

    #[test]
    fn test_hash_password_with_config() {
        let config = Argon2Config::new(65536, 3, 1).unwrap();
        let password = "test_password_with_config";
        let hash = hash_password_with_config(password, &config).unwrap();
        assert!(hash.starts_with("$argon2id$"));
        assert!(verify_password(password, &hash, false).unwrap());
    }

    #[test]
    fn test_verify_password_invalid_format() {
        let result = verify_password("password", "invalid", true);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Invalid hash format".to_string());
    }

    #[test]
    fn test_verify_password_legacy_valid() {
        let password = "test_password";
        let salt = "testsalt12345678";
        let mut hasher = Sha256::new();
        hasher.update(password);
        hasher.update(salt);
        let result = hasher.finalize();
        let encoded = URL_SAFE_NO_PAD.encode(result);
        let hash = format!("sha256$v=1$m=32,p=1${salt}${encoded}");
        assert!(verify_password_legacy(password, &hash));
        assert!(!verify_password_legacy("wrong_password", &hash));
    }

    #[test]
    fn test_verify_password_legacy_invalid() {
        assert!(!verify_password_legacy("password", "invalid"));
        assert!(!verify_password_legacy("password", "sha256$v=1$invalid$hash"));
    }

    #[test]
    fn test_is_legacy_hash() {
        let argon2_hash = "$argon2id$v=19$m=65536,t=3,p=1$salt$hash";
        assert!(!is_legacy_hash(argon2_hash));

        let legacy_hash = "sha256$v=1$m=32,p=1$salt$hash";
        assert!(is_legacy_hash(legacy_hash));

        let bcrypt_hash = "$2b$12$abcdefghijklmnopqrstuv";
        assert!(is_legacy_hash(bcrypt_hash));
    }

    #[test]
    fn test_migrate_password_hash() {
        let password = "password_to_migrate";
        let new_hash = migrate_password_hash(password, 65536, 3, 1).unwrap();

        assert!(new_hash.starts_with("$argon2"));
        assert!(verify_password(password, &new_hash, false).unwrap());
    }

    #[test]
    fn test_migrate_password_hash_with_config() {
        let config = Argon2Config::new(65536, 3, 1).unwrap();
        let password = "password_to_migrate_with_config";
        let new_hash = migrate_password_hash_with_config(password, &config).unwrap();

        assert!(new_hash.starts_with("$argon2id$"));
        assert!(verify_password(password, &new_hash, false).unwrap());
    }

    #[test]
    fn test_password_migration_flow() {
        let password = "test_migration_flow";
        let salt = "migrationtestsalt";
        let mut hasher = Sha256::new();
        hasher.update(password);
        hasher.update(salt);
        let result = hasher.finalize();
        let encoded = URL_SAFE_NO_PAD.encode(result);
        let legacy_hash = format!("sha256$v=1$m=32,p=1${salt}${encoded}");

        assert!(is_legacy_hash(&legacy_hash));
        assert!(verify_password_legacy(password, &legacy_hash));

        let new_hash = migrate_password_hash(password, 65536, 3, 1).unwrap();
        assert!(!is_legacy_hash(&new_hash));
        assert!(verify_password(password, &new_hash, false).unwrap());
    }

    #[test]
    fn test_generate_token() {
        let token1 = generate_token(32);
        let token2 = generate_token(32);
        assert_eq!(token1.len(), 43);
        assert_eq!(token2.len(), 43);
        assert_ne!(token1, token2);
    }

    #[test]
    fn test_generate_room_id() {
        let room_id = generate_room_id("example.com");
        assert!(room_id.starts_with('!'));
        assert!(room_id.contains(":example.com"));
    }

    #[test]
    fn test_generate_event_id() {
        let event_id = generate_event_id("example.com");
        assert!(event_id.starts_with('$'));
        assert!(event_id.contains(":example.com"));
    }

    #[test]
    fn test_generate_device_id() {
        let device_id = generate_device_id();
        assert!(device_id.starts_with("DEVICE"));
        assert_eq!(device_id.len(), 16);
    }

    #[test]
    fn test_generate_salt() {
        let salt1 = generate_salt();
        let salt2 = generate_salt();
        assert_eq!(salt1.len(), 22);
        assert_eq!(salt2.len(), 22);
        assert_ne!(salt1, salt2);
    }

    #[test]
    fn test_compute_hash() {
        let data = b"test data";
        let hash = compute_hash(data);
        assert_eq!(hash.len(), 43);
        assert_ne!(hash, compute_hash(b"different data"));
    }

    #[test]
    fn test_hmac_sha256() {
        let key = b"test_key";
        let data = b"test_data";
        let hmac1 = hmac_sha256(key, data);
        let hmac2 = hmac_sha256(key, data);
        assert_eq!(hmac1.len(), 32);
        assert_eq!(hmac1, hmac2);
        assert_ne!(hmac1, hmac_sha256(b"wrong_key", data));
    }

    #[test]
    fn test_random_string() {
        let s1 = random_string(10);
        let s2 = random_string(10);
        assert_eq!(s1.len(), 10);
        assert_eq!(s2.len(), 10);
        assert_ne!(s1, s2);
        assert!(s1.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn test_encode_decode_base64() {
        let original = b"hello world test data";
        let encoded = encode_base64(original);
        let decoded = decode_base64(&encoded).unwrap();
        assert_eq!(original, decoded.as_slice());
    }

    #[test]
    fn test_decode_base64_invalid() {
        let result = decode_base64("!!!invalid base64!!!");
        assert!(result.is_err());
    }

    #[test]
    fn test_generate_signing_key() {
        let (key_id, key) = generate_signing_key();
        assert!(key_id.starts_with("ed25519:"));
        assert_eq!(key_id.len(), 8 + 8);
        assert_eq!(key.len(), 44);
    }

    #[test]
    fn test_hash_token_deterministic() {
        let token = "syt_abc123_def456_ghijk789";
        let h1 = hash_token(token);
        let h2 = hash_token(token);
        assert_eq!(h1, h2, "hash_token must be deterministic");
    }

    #[test]
    fn test_hash_token_different_inputs_produce_different_hashes() {
        let h1 = hash_token("token_a");
        let h2 = hash_token("token_b");
        assert_ne!(h1, h2, "different inputs must produce different hashes");
    }

    #[test]
    fn test_hash_token_non_empty() {
        let result = hash_token("some_token_value");
        assert!(!result.is_empty(), "hash must not be empty");
    }

    #[test]
    fn test_hash_token_legacy_deterministic() {
        let token = "MDAxYWxvY2F0aW9uIGV4YW1wbGUuY29t";
        let h1 = hash_token_legacy(token);
        let h2 = hash_token_legacy(token);
        assert_eq!(h1, h2, "hash_token_legacy must be deterministic");
    }

    #[test]
    fn test_hash_token_legacy_different_from_current() {
        let token = "same_token_for_both";
        let h_cur = hash_token(token);
        let h_leg = hash_token_legacy(token);
        // Legacy and current hash algorithms should produce different outputs
        assert_ne!(h_cur, h_leg, "legacy and current hashes must differ");
    }

    #[test]
    fn test_hash_token_legacy_non_empty() {
        let result = hash_token_legacy("another_token");
        assert!(!result.is_empty(), "legacy hash must not be empty");
    }

    #[test]
    fn test_hash_token_edge_empty_string() {
        let result = hash_token("");
        assert!(!result.is_empty(), "hash of empty string must still produce output");
    }

    #[test]
    fn test_hash_token_legacy_edge_empty_string() {
        let result = hash_token_legacy("");
        assert!(!result.is_empty(), "legacy hash of empty string must still produce output");
    }

    // ── decode_base64_32 ─────────────────────────────────────────────

    #[test]
    fn decode_base64_32_standard_padded() {
        // "A" * 32 bytes → base64 with padding
        let result = decode_base64_32("QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE=");
        assert!(result.is_some());
        assert_eq!(result.unwrap(), [b'A'; 32]);
    }

    #[test]
    fn decode_base64_32_standard_no_pad() {
        let result = decode_base64_32("QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE");
        assert!(result.is_some());
        assert_eq!(result.unwrap(), [b'A'; 32]);
    }

    #[test]
    fn decode_base64_32_url_safe() {
        // 32 bytes where the standard encoding would produce '+' and '/'
        // \xFB\xFF... produces -/_ in URL-safe encoding
        let mut bytes = [0u8; 32];
        bytes[0] = 0xFB;
        bytes[1] = 0xFF;
        let encoded = URL_SAFE_NO_PAD.encode(bytes);
        let result = decode_base64_32(&encoded);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), bytes);
    }

    #[test]
    fn decode_base64_32_wrong_length() {
        // "A" * 31 bytes → base64 will decode to 31 bytes, not 32
        let result = decode_base64_32("QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQ");
        assert!(result.is_none());
    }

    #[test]
    fn decode_base64_32_empty() {
        let result = decode_base64_32("");
        assert!(result.is_none());
    }

    #[test]
    fn decode_base64_32_invalid() {
        let result = decode_base64_32("!!!invalid!!!");
        assert!(result.is_none());
    }

    #[test]
    fn decode_base64_32_trims_whitespace() {
        let result = decode_base64_32(" QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE= ");
        assert!(result.is_some());
        assert_eq!(result.unwrap(), [b'A'; 32]);
    }

    // ── encode_hex / decode_hex ──────────────────────────────────────

    #[test]
    fn encode_hex_roundtrip() {
        let data = [0x00u8, 0xFF, 0xAB, 0x12];
        let encoded = encode_hex(data);
        assert_eq!(encoded, "00ffab12");
        let decoded = decode_hex(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn decode_hex_invalid() {
        let result = decode_hex("xyz");
        assert!(result.is_err());
    }

    #[test]
    fn encode_hex_empty() {
        let result = encode_hex([]);
        assert!(result.is_empty());
    }
}
