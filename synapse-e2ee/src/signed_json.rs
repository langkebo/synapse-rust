use crate::crypto::{CryptoError, Ed25519PublicKey};
use base64::alphabet;
use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
use ed25519_dalek::Signature;
use serde_json::Value;

// Re-export the canonical canonical JSON implementation from synapse-common so
// that all signature verification paths use a single, spec-compliant source of
// truth.  Previously this module carried its own non-compliant copy that did
// not escape U+2028/U+2029 and did not validate numeric ranges.
pub use synapse_common::canonical_json;
pub use synapse_common::canonical_json_bytes;
pub use synapse_common::remove_signatures_and_unsigned;

/// Matrix protocol uses "unpadded base64" for signatures and keys, but some
/// clients (and our own historical encoding) emit padded variants. Accept
/// either by configuring the engine to be `Indifferent` to padding on decode.
const MATRIX_BASE64: GeneralPurpose = GeneralPurpose::new(
    &alphabet::STANDARD,
    GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
);

pub fn verify_signed_json(
    _user_id: &str,
    _key_id: &str,
    public_key_base64: &str,
    signature_base64: &str,
    json_value: &Value,
) -> Result<bool, CryptoError> {
    let public_key = Ed25519PublicKey::from_base64(public_key_base64)?;

    let signature_bytes =
        base64::Engine::decode(&MATRIX_BASE64, signature_base64).map_err(|_| CryptoError::InvalidBase64)?;

    if signature_bytes.len() != 64 {
        return Err(CryptoError::InvalidKeyLength);
    }

    let mut sig_array = [0u8; 64];
    sig_array.copy_from_slice(&signature_bytes);
    let ed25519_sig = Signature::from_slice(&sig_array).map_err(|_| CryptoError::SignatureVerificationFailed)?;

    let mut json_copy = json_value.clone();
    remove_signatures_and_unsigned(&mut json_copy);

    let message = canonical_json_bytes(&json_copy).map_err(|_| CryptoError::SignatureVerificationFailed)?;

    Ok(public_key.verify(&message, &ed25519_sig).is_ok())
}

pub fn verify_device_keys_signature(device_keys: &Value) -> Result<bool, CryptoError> {
    let user_id =
        device_keys.get("user_id").and_then(|v| v.as_str()).ok_or(CryptoError::SignatureVerificationFailed)?;

    let signatures = device_keys.get("signatures").and_then(|v| v.as_object());

    let keys = device_keys.get("keys").and_then(|v| v.as_object()).ok_or(CryptoError::SignatureVerificationFailed)?;

    let Some(signatures) = signatures else {
        return Ok(false);
    };

    let Some(user_sigs) = signatures.get(user_id).and_then(|v| v.as_object()) else {
        return Ok(false);
    };

    for (signing_key_id, signature_value) in user_sigs {
        let Some(signature) = signature_value.as_str() else {
            continue;
        };

        let algorithm = signing_key_id.split(':').next().unwrap_or("");
        if algorithm != "ed25519" {
            continue;
        }

        let Some(public_key) = keys.get(signing_key_id).and_then(|v| v.as_str()) else {
            continue;
        };

        if verify_signed_json(user_id, signing_key_id, public_key, signature, device_keys)? {
            return Ok(true);
        }
    }

    Ok(false)
}

pub fn verify_one_time_key_signature(
    user_id: &str,
    device_id: &str,
    algorithm: &str,
    _key_id: &str,
    key_data: &Value,
    device_ed25519_key: &str,
) -> Result<bool, CryptoError> {
    if algorithm != "signed_curve25519" {
        return Ok(true);
    }

    let signatures =
        key_data.get("signatures").and_then(|v| v.as_object()).ok_or(CryptoError::SignatureVerificationFailed)?;

    let user_sigs =
        signatures.get(user_id).and_then(|v| v.as_object()).ok_or(CryptoError::SignatureVerificationFailed)?;

    let signing_key_id = format!("ed25519:{device_id}");

    let signature =
        user_sigs.get(&signing_key_id).and_then(|v| v.as_str()).ok_or(CryptoError::SignatureVerificationFailed)?;

    let mut key_json = key_data.clone();
    remove_signatures_and_unsigned(&mut key_json);

    verify_signed_json(user_id, &signing_key_id, device_ed25519_key, signature, &key_json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aes_gcm::aead::OsRng;
    use base64::Engine;
    use ed25519_dalek::Signer;
    use ed25519_dalek::SigningKey;

    #[test]
    fn test_canonical_json_sorts_keys() {
        let json = serde_json::json!({
            "z_key": 1,
            "a_key": 2,
            "m_key": 3
        });
        let canonical = canonical_json(&json).unwrap();
        assert_eq!(canonical, r#"{"a_key":2,"m_key":3,"z_key":1}"#);
    }

    #[test]
    fn test_canonical_json_nested() {
        let json = serde_json::json!({
            "outer": {"z": 1, "a": 2},
            "inner": [3, 4]
        });
        let canonical = canonical_json(&json).unwrap();
        assert_eq!(canonical, r#"{"inner":[3,4],"outer":{"a":2,"z":1}}"#);
    }

    #[test]
    fn test_canonical_json_string_escaping() {
        let json = serde_json::json!({
            "key": "value with \"quotes\""
        });
        let canonical = canonical_json(&json).unwrap();
        assert_eq!(canonical, r#"{"key":"value with \"quotes\""}"#);
    }

    #[test]
    fn test_canonical_json_primitives() {
        assert_eq!(canonical_json(&Value::Null).unwrap(), "null");
        assert_eq!(canonical_json(&Value::Bool(true)).unwrap(), "true");
        assert_eq!(canonical_json(&Value::Bool(false)).unwrap(), "false");
        assert_eq!(canonical_json(&serde_json::json!(42)).unwrap(), "42");
        assert_eq!(canonical_json(&serde_json::json!("hello")).unwrap(), r#""hello""#);
    }

    #[test]
    fn test_remove_signatures_and_unsigned() {
        let mut json = serde_json::json!({
            "user_id": "@test:example.com",
            "signatures": {"key": "sig"},
            "unsigned": {"age": 10}
        });
        remove_signatures_and_unsigned(&mut json);
        assert!(json.get("signatures").is_none());
        assert!(json.get("unsigned").is_none());
        assert!(json.get("user_id").is_some());
    }

    #[test]
    fn test_verify_signed_json_roundtrip() {
        let mut rng = OsRng;
        let signing_key = SigningKey::generate(&mut rng);
        let verifying_key = signing_key.verifying_key();

        let mut json = serde_json::json!({
            "user_id": "@alice:example.com",
            "device_id": "DEVICE1",
            "keys": {
                "ed25519:DEVICE1": base64::engine::general_purpose::STANDARD.encode(verifying_key.as_bytes()),
                "curve25519:DEVICE1": "curve25519_key_base64"
            },
            "algorithms": ["m.olm.v1.curve25519-aes-sha2"],
            "signatures": {}
        });

        let mut json_for_signing = json.clone();
        remove_signatures_and_unsigned(&mut json_for_signing);
        let message = canonical_json_bytes(&json_for_signing).unwrap();
        let signature = signing_key.sign(&message);

        let sig_base64 = base64::engine::general_purpose::STANDARD.encode(signature.to_bytes());
        let pk_base64 = base64::engine::general_purpose::STANDARD.encode(verifying_key.as_bytes());

        json["signatures"] = serde_json::json!({
            "@alice:example.com": {
                "ed25519:DEVICE1": sig_base64
            }
        });

        let result = verify_signed_json("@alice:example.com", "ed25519:DEVICE1", &pk_base64, &sig_base64, &json);
        assert!(result.unwrap());
    }

    #[test]
    fn test_verify_signed_json_wrong_signature() {
        let mut rng = OsRng;
        let signing_key1 = SigningKey::generate(&mut rng);
        let signing_key2 = SigningKey::generate(&mut rng);
        let verifying_key1 = signing_key1.verifying_key();

        let json = serde_json::json!({
            "user_id": "@alice:example.com"
        });

        let message = canonical_json_bytes(&json).unwrap();
        let wrong_signature = signing_key2.sign(&message);

        let sig_base64 = base64::engine::general_purpose::STANDARD.encode(wrong_signature.to_bytes());
        let pk_base64 = base64::engine::general_purpose::STANDARD.encode(verifying_key1.as_bytes());

        let result = verify_signed_json("@alice:example.com", "ed25519:DEVICE1", &pk_base64, &sig_base64, &json);
        assert!(!result.unwrap());
    }

    #[test]
    fn test_verify_device_keys_signature_valid() {
        let mut rng = OsRng;
        let signing_key = SigningKey::generate(&mut rng);
        let verifying_key = signing_key.verifying_key();
        let pk_base64 = base64::engine::general_purpose::STANDARD.encode(verifying_key.as_bytes());

        let mut device_keys = serde_json::json!({
            "user_id": "@alice:example.com",
            "device_id": "DEVICE1",
            "algorithms": ["m.olm.v1.curve25519-aes-sha2"],
            "keys": {
                "ed25519:DEVICE1": pk_base64,
                "curve25519:DEVICE1": "curve25519_key_base64"
            }
        });

        let mut json_for_signing = device_keys.clone();
        remove_signatures_and_unsigned(&mut json_for_signing);
        let message = canonical_json_bytes(&json_for_signing).unwrap();
        let signature = signing_key.sign(&message);
        let sig_base64 = base64::engine::general_purpose::STANDARD.encode(signature.to_bytes());

        device_keys["signatures"] = serde_json::json!({
            "@alice:example.com": {
                "ed25519:DEVICE1": sig_base64
            }
        });

        assert!(verify_device_keys_signature(&device_keys).unwrap());
    }

    #[test]
    fn test_verify_device_keys_signature_missing() {
        let device_keys = serde_json::json!({
            "user_id": "@alice:example.com",
            "device_id": "DEVICE1",
            "algorithms": ["m.olm.v1.curve25519-aes-sha2"],
            "keys": {
                "ed25519:DEVICE1": "some_key",
                "curve25519:DEVICE1": "curve25519_key_base64"
            },
            "signatures": {}
        });

        assert!(!verify_device_keys_signature(&device_keys).unwrap());
    }

    #[test]
    fn test_verify_one_time_key_signature_valid() {
        let mut rng = OsRng;
        let signing_key = SigningKey::generate(&mut rng);
        let verifying_key = signing_key.verifying_key();
        let pk_base64 = base64::engine::general_purpose::STANDARD.encode(verifying_key.as_bytes());
        let curve_key = "curve25519_otk_base64";

        let mut otk = serde_json::json!({
            "key": curve_key
        });

        let mut json_for_signing = otk.clone();
        remove_signatures_and_unsigned(&mut json_for_signing);
        let message = canonical_json_bytes(&json_for_signing).unwrap();
        let signature = signing_key.sign(&message);
        let sig_base64 = base64::engine::general_purpose::STANDARD.encode(signature.to_bytes());

        otk["signatures"] = serde_json::json!({
            "@alice:example.com": {
                "ed25519:DEVICE1": sig_base64
            }
        });

        let result = verify_one_time_key_signature(
            "@alice:example.com",
            "DEVICE1",
            "signed_curve25519",
            "signed_curve25519:AAAAAAA",
            &otk,
            &pk_base64,
        );
        assert!(result.unwrap());
    }

    #[test]
    fn test_verify_one_time_key_unsigned_type_allowed() {
        let otk = serde_json::json!({
            "key": "curve25519_otk_base64"
        });

        let result = verify_one_time_key_signature(
            "@alice:example.com",
            "DEVICE1",
            "curve25519",
            "curve25519:AAAAAAA",
            &otk,
            "any_key",
        );
        assert!(result.unwrap());
    }
}
