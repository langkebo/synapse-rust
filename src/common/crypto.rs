use base64::{engine::general_purpose::STANDARD, Engine as _};
use hmac::{Hmac, Mac};
use rand::Rng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

type HmacSha256 = Hmac<Sha256>;

pub fn hash_password(password: &str) -> Result<String, String> {
    let salt = generate_salt();
    Ok(hash_password_with_salt(password, &salt))
}

pub fn hash_password_with_salt(password: &str, salt: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(password);
    hasher.update(salt);
    let result = hasher.finalize();
    format!("sha256$v=1$m=32,p=1${}${}", salt, STANDARD.encode(result))
}

pub fn verify_password(password: &str, password_hash: &str) -> Result<bool, String> {
    let parts: Vec<&str> = password_hash.split('$').collect();
    if parts.len() < 5 {
        return Err("Invalid hash format".to_string());
    }

    let algo = parts[0];
    let version = parts[1];
    let _params = parts[2];
    let salt = parts[3];
    let _hash = parts[4];

    if algo != "sha256" || version != "v=1" {
        return Err("Unsupported hash algorithm".to_string());
    }

    let computed_hash = hash_password_with_salt(password, salt);
    Ok(computed_hash == password_hash)
}

pub fn verify_password_legacy(password: &str, password_hash: &str) -> bool {
    let parts: Vec<&str> = password_hash.split('$').collect();
    if parts.len() < 5 {
        return false;
    }

    let algo = parts[0];
    let version = parts[1];
    let _params = parts[2];
    let salt = parts[3];
    let _hash = parts[4];

    if algo != "sha256" || version != "v=1" {
        return false;
    }

    let computed_hash = hash_password_with_salt(password, salt);
    computed_hash == password_hash
}

pub fn generate_token(length: usize) -> String {
    let mut bytes = vec![0u8; length];
    rand::thread_rng().fill_bytes(&mut bytes);
    STANDARD.encode(bytes)
}

pub fn generate_room_id(server_name: &str) -> String {
    let mut bytes = [0u8; 18];
    rand::thread_rng().fill_bytes(&mut bytes);
    format!("!{}:{}", STANDARD.encode(bytes), server_name)
}

pub fn generate_event_id(server_name: &str) -> String {
    let timestamp = chrono::Utc::now().timestamp_millis();
    let mut bytes = [0u8; 18];
    rand::thread_rng().fill_bytes(&mut bytes);
    format!("${}${}:{}", timestamp, STANDARD.encode(bytes), server_name)
}

pub fn generate_device_id() -> String {
    let mut bytes = [0u8; 10];
    rand::thread_rng().fill_bytes(&mut bytes);
    format!(
        "DEVICE{}",
        STANDARD.encode(bytes).get(..10).unwrap_or("DEVICE0000")
    )
}

pub fn generate_salt() -> String {
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    STANDARD.encode(bytes)
}

pub fn compute_hash(data: impl AsRef<[u8]>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data.as_ref());
    STANDARD.encode(&hasher.finalize()[..])
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
    STANDARD.encode(data.as_ref())
}

pub fn decode_base64(s: &str) -> Result<Vec<u8>, base64::DecodeError> {
    STANDARD.decode(s)
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
        assert!(hash.starts_with("sha256$v=1$m=32,p=1$"));
        assert!(verify_password(password, &hash).unwrap());
        assert!(!verify_password("wrong_password", &hash).unwrap());
    }

    #[test]

    fn test_hash_password_with_salt() {
        let password = "test_password";
        let salt = "testsalt12345678";
        let hash = hash_password_with_salt(password, salt);
        assert!(hash.starts_with("sha256$v=1$m=32,p=1$testsalt12345678$"));
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
        let hash = hash_password_with_salt(password, salt);
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
        assert_eq!(token1.len(), 44);
        assert_eq!(token2.len(), 44);
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
        assert_eq!(salt1.len(), 24);
        assert_eq!(salt2.len(), 24);
        assert_ne!(salt1, salt2);
    }

    #[test]

    fn test_compute_hash() {
        let data = b"test data";
        let hash = compute_hash(data);
        assert_eq!(hash.len(), 44);
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
