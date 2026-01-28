use base64::{Engine as _, engine::general_purpose::STANDARD};
use hmac::{Hmac, Mac};
use rand::Rng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::Sha256;

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
    format!("sha256$v=1$m=32,p=1${}${}", salt, STANDARD.encode(&result))
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
    STANDARD.encode(&bytes)
}

pub fn generate_room_id(server_name: &str) -> String {
    let mut bytes = [0u8; 18];
    rand::thread_rng().fill_bytes(&mut bytes);
    format!("!{}:{}", STANDARD.encode(&bytes), server_name)
}

pub fn generate_event_id(server_name: &str) -> String {
    let timestamp = chrono::Utc::now().timestamp_millis();
    let mut bytes = [0u8; 18];
    rand::thread_rng().fill_bytes(&mut bytes);
    format!("${}${}:{}", timestamp, STANDARD.encode(&bytes), server_name)
}

pub fn generate_device_id() -> String {
    let mut bytes = [0u8; 10];
    rand::thread_rng().fill_bytes(&mut bytes);
    format!("DEVICE{}", STANDARD.encode(&bytes).get(..10).unwrap_or("DEVICE0000"))
}

pub fn generate_salt() -> String {
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    STANDARD.encode(&bytes)
}

pub fn compute_hash(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    STANDARD.encode(&hasher.finalize())
}

pub fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC can take key of any size");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

pub fn random_string(length: usize) -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::thread_rng();
    let result: String = (0..length)
        .map(|_| {
            let idx = rng.gen::<usize>() % CHARSET.len();
            CHARSET[idx] as char
        })
        .collect();
    result
}

pub fn encode_base64(data: &[u8]) -> String {
    STANDARD.encode(data)
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
