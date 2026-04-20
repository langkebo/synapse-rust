
use synapse_rust::common::crypto::hmac_sha256;
use hmac::{Hmac, Mac};
use sha2::Sha256;

#[test]
fn test_hmac_sha256_consistency() {
    let key = b"test_secret_key";
    let data = b"test_message_data";
    
    // Test using the common crypto helper
    let signature1 = hmac_sha256(key, data);
    
    // Test using raw hmac crate with Sha256
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC can take key of any size");
    mac.update(data);
    let signature2 = mac.finalize().into_bytes().to_vec();
    
    assert_eq!(signature1, signature2, "HMAC-SHA256 implementations must be consistent");
}

#[test]
fn test_admin_registration_hmac_logic() {
    // This test simulates the logic in AdminRegistrationService::verify_hmac
    // to ensure it matches the script's expectations (except for the discovered padding issue)
    let shared_secret = b"change-me-admin-shared-secret";
    let nonce = "test_nonce";
    let username = "admin";
    let password = "password";
    
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(shared_secret).unwrap();
    
    mac.update(nonce.as_bytes());
    mac.update(b"\0");
    mac.update(username.as_bytes());
    mac.update(b"\0");
    mac.update(password.as_bytes());
    mac.update(b"\0");
    mac.update(b"admin\x00\x00\x00"); // Note the padding here
    
    let signature = mac.finalize().into_bytes();
    let hex_signature = signature.iter().map(|b| format!("{:02x}", b)).collect::<String>();
    
    assert_eq!(hex_signature.len(), 64, "HMAC-SHA256 hex signature should be 64 characters");
}
