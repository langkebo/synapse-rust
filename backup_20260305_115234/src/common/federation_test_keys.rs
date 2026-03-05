use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine as _};
use ed25519_dalek::{Signer, SigningKey as DalekSigningKey, Verifier, VerifyingKey};
use rand::{Rng, RngCore};

pub struct FederationTestKeypair {
    pub key_id: String,
    pub secret_key: String,
    pub public_key: String,
}

pub fn generate_federation_test_keypair() -> FederationTestKeypair {
    let mut rng = rand::thread_rng();
    let mut secret_bytes = [0u8; 32];
    rng.fill_bytes(&mut secret_bytes);

    let signing_key = DalekSigningKey::from_bytes(&secret_bytes);
    let verifying_key = signing_key.verifying_key();

    let key_id = format!("ed25519:test{}", generate_random_string(6));
    let secret_key = STANDARD_NO_PAD.encode(secret_bytes);
    let public_key = STANDARD_NO_PAD.encode(verifying_key.as_bytes());

    FederationTestKeypair {
        key_id,
        secret_key,
        public_key,
    }
}

pub fn sign_federation_request(
    secret_key: &str,
    method: &str,
    path: &str,
    origin: &str,
    destination: &str,
    body: Option<&str>,
) -> Result<String, String> {
    let secret_bytes_vec = STANDARD_NO_PAD
        .decode(secret_key)
        .map_err(|e| format!("Invalid base64 secret key: {}", e))?;
    let secret_bytes: [u8; 32] = secret_bytes_vec
        .try_into()
        .map_err(|_| "Secret key must be 32 bytes".to_string())?;
    let signing_key = DalekSigningKey::from_bytes(&secret_bytes);

    let mut signing_string = format!("{}\n{}\n{}\n{}\n", method, path, origin, destination);

    if let Some(body_content) = body {
        signing_string.push_str(body_content);
    }

    let signature = signing_key.sign(signing_string.as_bytes());
    let sig_b64 = STANDARD_NO_PAD.encode(signature.to_bytes());

    Ok(format!(
        "X-Matrix origin={},destination={},key_id={},sig={}",
        origin, destination, "ed25519:test", sig_b64
    ))
}

pub fn verify_federation_signature(
    public_key: &str,
    method: &str,
    path: &str,
    origin: &str,
    destination: &str,
    body: Option<&str>,
    signature_header: &str,
) -> Result<bool, String> {
    let pub_key_bytes_vec = STANDARD_NO_PAD
        .decode(public_key)
        .map_err(|e| format!("Invalid base64 public key: {}", e))?;
    let pub_key_bytes: [u8; 32] = pub_key_bytes_vec
        .try_into()
        .map_err(|_| "Public key must be 32 bytes".to_string())?;
    let verifying_key = VerifyingKey::from_bytes(&pub_key_bytes)
        .map_err(|e| format!("Invalid verifying key: {}", e))?;

    let mut signing_string = format!("{}\n{}\n{}\n{}\n", method, path, origin, destination);

    if let Some(body_content) = body {
        signing_string.push_str(body_content);
    }

    let sig_b64 = extract_signature_from_header(signature_header)?;
    let signature_bytes = STANDARD_NO_PAD
        .decode(&sig_b64)
        .map_err(|e| format!("Invalid base64 signature: {}", e))?;

    let dalek_signature = ed25519_dalek::Signature::from_slice(&signature_bytes)
        .map_err(|e| format!("Invalid signature format: {}", e))?;

    verifying_key
        .verify(signing_string.as_bytes(), &dalek_signature)
        .map_err(|e| format!("Signature verification failed: {}", e))?;

    Ok(true)
}

fn extract_signature_from_header(header: &str) -> Result<String, String> {
    let parts: Vec<&str> = header.split(',').collect();

    for part in parts {
        let key_value: Vec<&str> = part.split('=').collect();
        if key_value.len() == 2 && key_value[0] == "sig" {
            return Ok(key_value[1].to_string());
        }
    }

    Err("Signature not found in header".to_string())
}

fn generate_random_string(length: usize) -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::thread_rng();
    let result: String = (0..length)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_federation_test_keypair() {
        let keypair = generate_federation_test_keypair();
        assert!(keypair.key_id.starts_with("ed25519:test"));
        assert!(keypair.secret_key.len() >= 43 && keypair.secret_key.len() <= 44);
        assert!(keypair.public_key.len() >= 43 && keypair.public_key.len() <= 44);
    }

    #[test]
    fn test_sign_federation_request() {
        let keypair = generate_federation_test_keypair();
        let signature = sign_federation_request(
            &keypair.secret_key,
            "GET",
            "/_matrix/federation/v1/version",
            "example.com",
            "destination.com",
            None,
        )
        .unwrap();

        assert!(signature.contains("X-Matrix origin="));
        assert!(signature.contains("sig="));
    }

    #[test]
    fn test_verify_federation_signature() {
        let keypair = generate_federation_test_keypair();
        let method = "GET";
        let path = "/_matrix/federation/v1/version";
        let origin = "example.com";
        let destination = "destination.com";

        let signature =
            sign_federation_request(&keypair.secret_key, method, path, origin, destination, None)
                .unwrap();

        let is_valid = verify_federation_signature(
            &keypair.public_key,
            method,
            path,
            origin,
            destination,
            None,
            &signature,
        )
        .unwrap();

        assert!(is_valid);
    }

    #[test]
    fn test_verify_federation_signature_wrong_key() {
        let keypair1 = generate_federation_test_keypair();
        let keypair2 = generate_federation_test_keypair();

        let signature = sign_federation_request(
            &keypair1.secret_key,
            "GET",
            "/_matrix/federation/v1/version",
            "example.com",
            "destination.com",
            None,
        )
        .unwrap();

        let result = verify_federation_signature(
            &keypair2.public_key,
            "GET",
            "/_matrix/federation/v1/version",
            "example.com",
            "destination.com",
            None,
            &signature,
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_sign_federation_request_with_body() {
        let keypair = generate_federation_test_keypair();
        let body = r#"{"test": "data"}"#;

        let signature = sign_federation_request(
            &keypair.secret_key,
            "POST",
            "/_matrix/federation/v1/send/test123",
            "example.com",
            "destination.com",
            Some(body),
        )
        .unwrap();

        assert!(signature.contains("X-Matrix origin="));

        let is_valid = verify_federation_signature(
            &keypair.public_key,
            "POST",
            "/_matrix/federation/v1/send/test123",
            "example.com",
            "destination.com",
            Some(body),
            &signature,
        )
        .unwrap();

        assert!(is_valid);
    }
}
