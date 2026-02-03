use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2, Params,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use hmac::{Hmac, Mac};
use rand::Rng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

type HmacSha256 = Hmac<Sha256>;

pub fn hash_password(password: &str) -> Result<String, String> {
    hash_password_with_params(password, 4096, 3, 1)
}

pub fn hash_password_with_params(
    password: &str,
    m_cost: u32,
    t_cost: u32,
    p_cost: u32,
) -> Result<String, String> {
    let salt = SaltString::generate(&mut rand::thread_rng());
    let params = Params::new(m_cost, t_cost, p_cost, None).map_err(|e| e.to_string())?;
    let argon2 = Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);

    let password_hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| e.to_string())?
        .to_string();

    Ok(password_hash)
}

pub fn verify_password(password: &str, password_hash: &str) -> Result<bool, String> {
    if password_hash.starts_with("$argon2") {
        let parsed_hash = PasswordHash::new(password_hash).map_err(|e| e.to_string())?;
        // Argon2::verify_password will use the algorithm and params from the parsed hash
        return Ok(Argon2::default()
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_ok());
    }

    // Fallback to SHA-256 (handling both with and without leading $)
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
        return Err(format!("Unsupported hash algorithm: {}", algo));
    }

    // Support both simple and iterated SHA-256
    if parts.len() >= 7 {
        // Iterated version from AuthService
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
        // Simple version from crypto.rs
        let mut hasher = Sha256::new();
        hasher.update(password);
        hasher.update(salt);
        let result = hasher.finalize();
        let encoded_hash = URL_SAFE_NO_PAD.encode(result);
        Ok(secure_compare(&encoded_hash, hash))
    }
}

fn secure_compare(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut result = 0u8;
    for (byte_a, byte_b) in a.bytes().zip(b.bytes()) {
        result |= byte_a ^ byte_b;
    }
    result == 0
}

pub fn verify_password_legacy(password: &str, password_hash: &str) -> bool {
    verify_password(password, password_hash).unwrap_or(false)
}

pub fn generate_token(length: usize) -> String {
    let mut bytes = vec![0u8; length];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn generate_room_id(server_name: &str) -> String {
    let mut bytes = [0u8; 18];
    rand::thread_rng().fill_bytes(&mut bytes);
    format!("!{}:{}", URL_SAFE_NO_PAD.encode(bytes), server_name)
}

pub fn generate_event_id(server_name: &str) -> String {
    let timestamp = chrono::Utc::now().timestamp_millis();
    let mut bytes = [0u8; 18];
    rand::thread_rng().fill_bytes(&mut bytes);
    format!(
        "${}${}:{}",
        timestamp,
        URL_SAFE_NO_PAD.encode(bytes),
        server_name
    )
}

pub fn generate_device_id() -> String {
    let mut bytes = [0u8; 10];
    rand::thread_rng().fill_bytes(&mut bytes);
    format!(
        "DEVICE{}",
        URL_SAFE_NO_PAD
            .encode(bytes)
            .get(..10)
            .unwrap_or("DEVICE0000")
    )
}

pub fn generate_salt() -> String {
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn compute_hash(data: impl AsRef<[u8]>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data.as_ref());
    URL_SAFE_NO_PAD.encode(&hasher.finalize()[..])
}

pub fn hmac_sha256(key: impl AsRef<[u8]>, data: impl AsRef<[u8]>) -> Vec<u8> {
    let key = key.as_ref();
    let data = data.as_ref();
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC can take key of any size");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

pub fn random_string(length: usize) -> String {
    static CHARSET: [u8; 62] = *b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::thread_rng();
    let result: String = (0..length)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerSigningKey {
    pub key_id: String,
    pub key: String,
    pub expired_ts: i64,
}

pub fn generate_signing_key() -> (String, String) {
    let key_id = format!("ed25519:{}", random_string(8));
    let key = random_string(44);
    (key_id, key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]

    fn test_hash_password_and_verify() {
        let password = "test_password_123";
        let hash = hash_password(password).unwrap();
        assert!(hash.starts_with("$argon2"));
        assert!(verify_password(password, &hash).unwrap());
        assert!(!verify_password("wrong_password", &hash).unwrap());
    }

    #[test]

    fn test_hash_password_with_params() {
        let password = "test_password";
        let hash = hash_password_with_params(password, 4096, 3, 1).unwrap();
        assert!(hash.starts_with("$argon2"));
    }

    #[test]

    fn test_verify_password_invalid_format() {
        let result = verify_password("password", "invalid");
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
        let hash = format!("sha256$v=1$m=32,p=1${}${}", salt, encoded);
        assert!(verify_password_legacy(password, &hash));
        assert!(!verify_password_legacy("wrong_password", &hash));
    }

    #[test]

    fn test_verify_password_legacy_invalid() {
        assert!(!verify_password_legacy("password", "invalid"));
        assert!(!verify_password_legacy(
            "password",
            "sha256$v=1$invalid$hash"
        ));
    }

    #[test]

    fn test_generate_token() {
        let token1 = generate_token(32);
        let token2 = generate_token(32);
        assert_eq!(token1.len(), 43); // URL_SAFE_NO_PAD is shorter
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
}
