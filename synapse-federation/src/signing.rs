use base64::Engine;
use ed25519_dalek::{Signer, SigningKey};
use serde_json::Value;
use synapse_common::canonical_json;
use synapse_common::secure_compare;
use synapse_common::CanonicalEvent;

const MAX_PDU_SIZE_BYTES: usize = 65536;
const MAX_EVENT_KEYS: usize = 100;
const MAX_CONTENT_KEYS: usize = 100;
const MAX_STRING_LENGTH: usize = 65536;

pub fn canonical_federation_request_bytes(
    method: &str,
    uri: &str,
    origin: &str,
    destination: &str,
    content: Option<&Value>,
) -> Result<Vec<u8>, synapse_common::CanonicalJsonError> {
    let mut obj = serde_json::Map::new();
    obj.insert("method".to_string(), Value::String(method.to_string()));
    obj.insert("uri".to_string(), Value::String(uri.to_string()));
    obj.insert("origin".to_string(), Value::String(origin.to_string()));
    obj.insert("destination".to_string(), Value::String(destination.to_string()));
    if let Some(content) = content {
        obj.insert("content".to_string(), content.clone());
    }
    Ok(canonical_json(&Value::Object(obj))?.into_bytes())
}

pub fn sign_json(server_name: &str, key_id: &str, secret_key_base64: &str, value: &mut Value) -> Result<(), String> {
    let canonical = CanonicalEvent::from_event(value).map_err(|e| format!("Canonical JSON error: {e}"))?;
    sign_json_with_canonical(server_name, key_id, secret_key_base64, value, &canonical)
}

/// Sign using a pre-computed [`CanonicalEvent`]. Avoids re-sorting and
/// re-serializing the event when the canonical form is already known
/// (e.g. in `sign_and_hash_event` where the content hash was just computed).
pub fn sign_json_with_canonical(
    server_name: &str,
    key_id: &str,
    secret_key_base64: &str,
    value: &mut Value,
    canonical: &synapse_common::CanonicalEvent,
) -> Result<(), String> {
    let unsigned = canonical.canonical_bytes();

    let secret_bytes: [u8; 32] = base64::engine::general_purpose::STANDARD_NO_PAD
        .decode(secret_key_base64)
        .map_err(|e| format!("Invalid secret key base64: {e}"))?
        .try_into()
        .map_err(|_| "Secret key must be 32 bytes".to_string())?;

    let signing_key = SigningKey::from_bytes(&secret_bytes);
    let signature = signing_key.sign(unsigned);
    let sig_b64 = base64::engine::general_purpose::STANDARD_NO_PAD.encode(signature.to_bytes());

    let signatures = value
        .as_object_mut()
        .ok_or_else(|| "Value must be a JSON object".to_string())?
        .entry("signatures")
        .or_insert_with(|| Value::Object(serde_json::Map::new()));

    let server_sigs = signatures
        .as_object_mut()
        .ok_or_else(|| "signatures must be a JSON object".to_string())?
        .entry(server_name.to_string())
        .or_insert_with(|| Value::Object(serde_json::Map::new()));

    server_sigs
        .as_object_mut()
        .ok_or_else(|| "Server signatures must be a JSON object".to_string())?
        .insert(key_id.to_string(), Value::String(sig_b64));

    Ok(())
}

pub fn compute_event_content_hash(event: &Value) -> Option<String> {
    let mut redacted = redact_event_for_hash(event);
    redacted.as_object_mut()?.remove("hashes");
    redacted.as_object_mut()?.remove("signatures");
    redacted.as_object_mut()?.remove("unsigned");
    let canonical = canonical_json(&redacted).ok()?;
    use sha2::Digest;
    let hash = sha2::Sha256::digest(canonical.as_bytes());
    Some(base64::engine::general_purpose::STANDARD_NO_PAD.encode(hash))
}

pub fn verify_event_content_hash(event: &Value) -> Result<(), String> {
    let hashes =
        event.get("hashes").and_then(|h| h.as_object()).ok_or_else(|| "Event missing hashes field".to_string())?;

    let sha256_hash =
        hashes.get("sha256").and_then(|h| h.as_str()).ok_or_else(|| "Event missing sha256 hash".to_string())?;

    let computed =
        compute_event_content_hash(event).ok_or_else(|| "Failed to compute event content hash".to_string())?;

    // P3-03: constant-time comparison to avoid leaking hash bytes via timing.
    if !secure_compare(&computed, sha256_hash) {
        return Err(format!("Event content hash mismatch: expected {sha256_hash}, computed {computed}"));
    }

    Ok(())
}

pub fn check_pdu_size_limits(event: &Value) -> Result<(), String> {
    let event_json = serde_json::to_string(event).map_err(|e| format!("Failed to serialize event: {e}"))?;

    if event_json.len() > MAX_PDU_SIZE_BYTES {
        return Err(format!("Event too large: {} bytes (max {})", event_json.len(), MAX_PDU_SIZE_BYTES));
    }

    if let Some(obj) = event.as_object() {
        if obj.len() > MAX_EVENT_KEYS {
            return Err(format!("Event has too many top-level keys: {} (max {})", obj.len(), MAX_EVENT_KEYS));
        }
    }

    if let Some(content) = event.get("content").and_then(|c| c.as_object()) {
        if content.len() > MAX_CONTENT_KEYS {
            return Err(format!("Event content has too many keys: {} (max {})", content.len(), MAX_CONTENT_KEYS));
        }
    }

    check_string_depth(event, 0)
}

fn check_string_depth(value: &Value, depth: usize) -> Result<(), String> {
    if depth > 20 {
        return Err("Event nesting too deep".to_string());
    }

    match value {
        Value::String(s) => {
            if s.len() > MAX_STRING_LENGTH {
                return Err(format!("String value too long: {} bytes (max {})", s.len(), MAX_STRING_LENGTH));
            }
        }
        Value::Array(arr) => {
            if arr.len() > 1000 {
                return Err(format!("Array too long: {} (max 1000)", arr.len()));
            }
            for v in arr {
                check_string_depth(v, depth + 1)?;
            }
        }
        Value::Object(map) => {
            for (k, v) in map {
                if k.len() > MAX_STRING_LENGTH {
                    return Err(format!("Object key too long: {} bytes", k.len()));
                }
                check_string_depth(v, depth + 1)?;
            }
        }
        _ => {}
    }

    Ok(())
}

fn redact_event_for_hash(event: &Value) -> Value {
    // P0-07: delegate to the shared redaction module so that the field
    // retention table is consistent between hash computation and runtime
    // redaction.  The previous inline implementation included illegal
    // top-level fields (`prev_state`, `membership`) and was missing
    // `notifications` from `m.room.power_levels`.
    synapse_common::redaction::redact_event_for_hash(event)
}

pub fn check_event_federate(room_create_event: &Value) -> bool {
    room_create_event.get("content").and_then(|c| c.get("m.federate")).and_then(|f| f.as_bool()).unwrap_or(true)
}

/// Sign and hash a locally-produced PDU so it can be federated to remote
/// servers.
///
/// This function:
/// 1. Ensures the `origin` field is set to `server_name`
/// 2. Computes the `hashes.sha256` content hash
/// 3. Signs the event with `sign_json` using the provided key
///
/// The `secret_key_base64` and `key_id` come from
/// `KeyRotationManager::get_current_key`.
///
/// Reference: element-hq/synapse `synapse/events/utils.py::maybe_upsert_event_field`
/// and `synapse/crypto/event_signing.py::add_hashes_and_signatures`
pub fn sign_and_hash_event(
    server_name: &str,
    key_id: &str,
    secret_key_base64: &str,
    event: &mut Value,
) -> Result<(), String> {
    // 1. Ensure `origin` is set.
    if let Some(obj) = event.as_object_mut() {
        if !obj.contains_key("origin") {
            obj.insert("origin".to_string(), Value::String(server_name.to_string()));
        }
    } else {
        return Err("Event must be a JSON object".to_string());
    }

    // 2. Compute and set the content hash.
    let hash = compute_event_content_hash(event).ok_or_else(|| "Failed to compute event content hash".to_string())?;
    if let Some(obj) = event.as_object_mut() {
        let hashes = obj.entry("hashes").or_insert_with(|| Value::Object(serde_json::Map::new()));
        if let Some(hashes_obj) = hashes.as_object_mut() {
            hashes_obj.insert("sha256".to_string(), Value::String(hash));
        }
    }

    // 3. Compute canonical form once, then sign using the cached form.
    let canonical =
        synapse_common::CanonicalEvent::from_event(event).map_err(|e| format!("Canonical JSON error: {e}"))?;
    sign_json_with_canonical(server_name, key_id, secret_key_base64, event, &canonical)?;

    Ok(())
}

// ============================================================================
// PDU sender-server signature verification (shared utility)
// ============================================================================

/// Extract the server name from a Matrix MXID.
///
/// Example: `@user:example.com` → `Some("example.com")`
pub fn sender_server_name(sender: &str) -> Option<&str> {
    sender
        .strip_prefix('@')
        .and_then(|user| user.rsplit_once(':').map(|(_, server)| server))
        .filter(|server| !server.is_empty())
}

/// Verify a PDU's sender-server signature using a [`FederationClientApi`]
/// to fetch the sender server's verify keys.
///
/// This is the backfill/outbound counterpart to the inbound transaction
/// path's `verify_pdu_sender_signature` (which uses the `FederationContext`
/// cache).  Both perform the same cryptographic check: the PDU must carry
/// at least one valid ed25519 signature from the sender's home server.
///
/// The real `FederationClient::get_server_keys` already validates the
/// server-key self-signature (FED-01), so keys returned here are trusted.
pub async fn verify_pdu_signature_with_client(
    federation_client: &dyn crate::client_api::FederationClientApi,
    pdu: &Value,
) -> Result<(), String> {
    let sender = pdu
        .get("sender")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing sender on PDU".to_string())?;
    let sender_server =
        sender_server_name(sender).ok_or_else(|| format!("Unparseable sender mxid: {sender}"))?;

    let signatures = pdu
        .get("signatures")
        .and_then(|v| v.as_object())
        .ok_or_else(|| "PDU missing signatures field".to_string())?;
    let server_sigs = signatures
        .get(sender_server)
        .and_then(|v| v.as_object())
        .ok_or_else(|| format!("PDU has no signatures from sender server {sender_server}"))?;
    if server_sigs.is_empty() {
        return Err(format!("PDU signatures.{sender_server} is empty"));
    }

    // Compute signed bytes: canonical JSON without signatures/unsigned.
    let mut signing_payload = pdu.clone();
    if let Some(obj) = signing_payload.as_object_mut() {
        obj.remove("signatures");
        obj.remove("unsigned");
    }
    let signed_bytes = synapse_common::canonical_json_bytes(&signing_payload)
        .map_err(|e| format!("Canonical JSON error for PDU signature verification: {e}"))?;

    // Fetch server keys — try cache first, then fetch from remote.
    let server_keys = match federation_client.get_cached_key(sender_server).await {
        Some(keys) => keys,
        None => federation_client
            .get_server_keys(sender_server)
            .await
            .map_err(|e| format!("Failed to fetch server keys for {sender_server}: {e}"))?,
    };

    let verify_keys = server_keys
        .verify_keys
        .as_object()
        .ok_or_else(|| "Server keys verify_keys is not an object".to_string())?;

    let mut last_error: Option<String> = None;
    for (key_id, sig_value) in server_sigs {
        let Some(signature) = sig_value.as_str() else {
            continue;
        };

        let Some(key_data) = verify_keys.get(key_id) else {
            last_error = Some(format!("No verify key for {key_id} on {sender_server}"));
            continue;
        };

        let public_key_b64 = key_data
            .get("key")
            .and_then(|v| v.as_str())
            .or_else(|| key_data.as_str())
            .ok_or_else(|| format!("Invalid verify key format for {key_id}"))?;

        let pub_bytes = base64::engine::general_purpose::STANDARD_NO_PAD
            .decode(public_key_b64)
            .or_else(|_| base64::engine::general_purpose::STANDARD.decode(public_key_b64))
            .map_err(|e| format!("Invalid public key base64: {e}"))?;

        let pub_arr: [u8; 32] = pub_bytes
            .as_slice()
            .try_into()
            .map_err(|_| "Public key must be 32 bytes".to_string())?;

        let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&pub_arr)
            .map_err(|e| format!("Invalid verifying key: {e}"))?;

        let sig_bytes = base64::engine::general_purpose::STANDARD_NO_PAD
            .decode(signature)
            .or_else(|_| base64::engine::general_purpose::STANDARD.decode(signature))
            .map_err(|e| format!("Invalid signature base64: {e}"))?;

        let sig_arr: [u8; 64] = sig_bytes
            .as_slice()
            .try_into()
            .map_err(|_| "Signature must be 64 bytes".to_string())?;

        let sig = ed25519_dalek::Signature::from_bytes(&sig_arr);

        match verifying_key.verify_strict(&signed_bytes, &sig) {
            Ok(()) => return Ok(()),
            Err(e) => last_error = Some(format!("Signature verification failed for {key_id}: {e}")),
        }
    }

    Err(last_error.unwrap_or_else(|| "No verifiable PDU signature".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Verifier, VerifyingKey};
    use synapse_common::canonical_json;

    fn generate_test_key() -> (String, ed25519_dalek::SigningKey) {
        let secret_bytes: [u8; 32] = [
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29,
            30, 31, 32,
        ];
        let signing_key = SigningKey::from_bytes(&secret_bytes);
        let secret_b64 = base64::engine::general_purpose::STANDARD_NO_PAD.encode(secret_bytes);
        (secret_b64, signing_key)
    }

    #[test]
    fn test_sign_and_verify_json() {
        let (secret_b64, signing_key) = generate_test_key();
        let mut value = serde_json::json!({
            "event_id": "$event1",
            "type": "m.room.message",
            "room_id": "!room:server",
            "sender": "@user:server",
            "content": {"body": "hello"}
        });

        sign_json("server", "ed25519:1", &secret_b64, &mut value).unwrap();

        let sigs = value.get("signatures").unwrap();
        let server_sigs = sigs.get("server").unwrap();
        let sig_value = server_sigs.get("ed25519:1").unwrap().as_str().unwrap();
        assert!(!sig_value.is_empty());

        let verifying_key: VerifyingKey = signing_key.verifying_key();
        let mut copy = value.clone();
        copy.as_object_mut().unwrap().remove("signatures");
        copy.as_object_mut().unwrap().remove("unsigned");
        let canonical = canonical_json(&copy).unwrap();
        let sig_bytes = base64::engine::general_purpose::STANDARD_NO_PAD.decode(sig_value).unwrap();
        let signature = ed25519_dalek::Signature::from_slice(&sig_bytes).unwrap();
        assert!(verifying_key.verify(canonical.as_bytes(), &signature).is_ok());
    }

    #[test]
    fn test_verify_tampered_json_fails() {
        let (secret_b64, _) = generate_test_key();
        let mut value = serde_json::json!({
            "event_id": "$event1",
            "type": "m.room.message",
            "room_id": "!room:server",
            "sender": "@user:server",
            "content": {"body": "hello"}
        });

        sign_json("server", "ed25519:1", &secret_b64, &mut value).unwrap();

        value["content"]["body"] = serde_json::Value::String("tampered".to_string());

        let mut copy = value.clone();
        copy.as_object_mut().unwrap().remove("signatures");
        copy.as_object_mut().unwrap().remove("unsigned");
        let canonical = canonical_json(&copy).unwrap();

        let sig_value = value["signatures"]["server"]["ed25519:1"].as_str().unwrap();
        let sig_bytes = base64::engine::general_purpose::STANDARD_NO_PAD.decode(sig_value).unwrap();

        let tampered_secret: [u8; 32] = [
            99, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29,
            30, 31, 32,
        ];
        let tampered_signing_key = SigningKey::from_bytes(&tampered_secret);
        let verifying_key = tampered_signing_key.verifying_key();
        let signature = ed25519_dalek::Signature::from_slice(&sig_bytes).unwrap();
        assert!(verifying_key.verify(canonical.as_bytes(), &signature).is_err());
    }

    #[test]
    fn test_canonical_json_deterministic() {
        let value1 = serde_json::json!({
            "z_key": "last",
            "a_key": "first",
            "m_key": "middle"
        });
        let value2 = serde_json::json!({
            "a_key": "first",
            "m_key": "middle",
            "z_key": "last"
        });

        let canonical1 = canonical_json(&value1).unwrap();
        let canonical2 = canonical_json(&value2).unwrap();
        assert_eq!(canonical1, canonical2);

        assert!(canonical1.starts_with("{\"a_key\""));
    }

    #[test]
    fn test_sign_federation_request() {
        let bytes = canonical_federation_request_bytes(
            "GET",
            "/_matrix/federation/v1/event/$event",
            "origin.server",
            "destination.server",
            None,
        )
        .unwrap();

        let decoded: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(decoded["method"], "GET");
        assert_eq!(decoded["uri"], "/_matrix/federation/v1/event/$event");
        assert_eq!(decoded["origin"], "origin.server");
        assert_eq!(decoded["destination"], "destination.server");
        assert!(decoded.get("content").is_none());

        let content = serde_json::json!({"key": "value"});
        let bytes_with_content = canonical_federation_request_bytes(
            "PUT",
            "/_matrix/federation/v1/send/$txn",
            "origin.server",
            "destination.server",
            Some(&content),
        )
        .unwrap();
        let decoded_with: Value = serde_json::from_slice(&bytes_with_content).unwrap();
        assert_eq!(decoded_with["method"], "PUT");
        assert!(decoded_with.get("content").is_some());
    }

    #[test]
    fn test_sign_json_rejects_integer_valued_float() {
        let (secret_b64, _) = generate_test_key();
        let mut value: Value = serde_json::from_str(
            r#"{
                "event_id":"$event1",
                "type":"m.room.message",
                "content":{"body":"hello","order":1.0}
            }"#,
        )
        .unwrap();

        let err = sign_json("server", "ed25519:1", &secret_b64, &mut value).unwrap_err();
        assert!(err.contains("Floats are not permitted in canonical JSON"));
    }

    #[test]
    fn test_verify_expired_key_fails() {
        let (secret_b64, _) = generate_test_key();
        let mut value = serde_json::json!({
            "event_id": "$event1",
            "type": "m.room.message",
            "content": {"body": "hello"}
        });

        sign_json("server", "ed25519:1", &secret_b64, &mut value).unwrap();

        let new_secret: [u8; 32] = [
            99, 98, 97, 96, 95, 94, 93, 92, 91, 90, 89, 88, 87, 86, 85, 84, 83, 82, 81, 80, 79, 78, 77, 76, 75, 74, 73,
            72, 71, 70, 69, 68,
        ];
        let new_signing_key = SigningKey::from_bytes(&new_secret);
        let new_verifying_key = new_signing_key.verifying_key();

        let sig_value = value["signatures"]["server"]["ed25519:1"].as_str().unwrap();
        let sig_bytes = base64::engine::general_purpose::STANDARD_NO_PAD.decode(sig_value).unwrap();
        let signature = ed25519_dalek::Signature::from_slice(&sig_bytes).unwrap();

        let mut copy = value.clone();
        copy.as_object_mut().unwrap().remove("signatures");
        copy.as_object_mut().unwrap().remove("unsigned");
        let canonical = canonical_json(&copy).unwrap();

        assert!(new_verifying_key.verify(canonical.as_bytes(), &signature).is_err());
    }

    #[test]
    fn test_sign_with_old_key() {
        let (secret_b64, _) = generate_test_key();

        let mut value = serde_json::json!({
            "event_id": "$event1",
            "type": "m.room.message",
            "content": {"body": "hello"}
        });
        sign_json("server", "ed25519:old", &secret_b64, &mut value).unwrap();

        let old_sig = value["signatures"]["server"]["ed25519:old"].as_str().unwrap();
        assert!(!old_sig.is_empty());

        sign_json("server", "ed25519:new", &secret_b64, &mut value).unwrap();
        let new_sig = value["signatures"]["server"]["ed25519:new"].as_str().unwrap();
        assert!(!new_sig.is_empty());

        assert!(value["signatures"]["server"]["ed25519:old"].is_string());
        assert!(value["signatures"]["server"]["ed25519:new"].is_string());
    }

    #[test]
    fn test_compute_event_content_hash() {
        let event = serde_json::json!({
            "event_id": "$event1",
            "type": "m.room.message",
            "room_id": "!room:server",
            "content": {"body": "hello"},
            "hashes": {}
        });

        let hash = compute_event_content_hash(&event);
        assert!(hash.is_some());
        let hash = hash.unwrap();
        assert_eq!(hash.len(), 43);
    }

    #[test]
    fn test_verify_event_content_hash_valid() {
        let mut event = serde_json::json!({
            "event_id": "$event1",
            "type": "m.room.message",
            "room_id": "!room:server",
            "content": {"body": "hello"}
        });

        let hash = compute_event_content_hash(&event).unwrap();
        event["hashes"] = serde_json::json!({"sha256": hash});

        assert!(verify_event_content_hash(&event).is_ok());
    }

    #[test]
    fn test_verify_event_content_hash_mismatch() {
        let event = serde_json::json!({
            "event_id": "$event1",
            "type": "m.room.message",
            "room_id": "!room:server",
            "content": {"body": "hello"},
            "hashes": {"sha256": "invalidhash"}
        });

        assert!(verify_event_content_hash(&event).is_err());
    }

    #[test]
    fn test_check_pdu_size_limits_valid() {
        let event = serde_json::json!({
            "event_id": "$event1",
            "type": "m.room.message",
            "content": {"body": "hello"}
        });
        assert!(check_pdu_size_limits(&event).is_ok());
    }

    #[test]
    fn test_check_pdu_size_limits_too_large() {
        let big_string = "x".repeat(70000);
        let event = serde_json::json!({
            "event_id": "$event1",
            "type": "m.room.message",
            "content": {"body": big_string}
        });
        assert!(check_pdu_size_limits(&event).is_err());
    }

    #[test]
    fn test_check_event_federate() {
        let federating = serde_json::json!({"content": {"m.federate": true}});
        assert!(check_event_federate(&federating));

        let no_federate = serde_json::json!({"content": {"m.federate": false}});
        assert!(!check_event_federate(&no_federate));

        let missing = serde_json::json!({"content": {}});
        assert!(check_event_federate(&missing));
    }

    #[test]
    fn test_canonical_json_types() {
        assert_eq!(canonical_json(&Value::Null).unwrap(), "null");
        assert_eq!(canonical_json(&Value::Bool(true)).unwrap(), "true");
        assert_eq!(canonical_json(&Value::Bool(false)).unwrap(), "false");
        assert_eq!(canonical_json(&serde_json::json!(42)).unwrap(), "42");
        assert_eq!(canonical_json(&serde_json::json!("hello")).unwrap(), "\"hello\"");
        assert_eq!(canonical_json(&serde_json::json!([1, 2, 3])).unwrap(), "[1,2,3]");
    }

    // ── sender_server_name tests ─────────────────────────────────────

    #[test]
    fn test_sender_server_name_valid() {
        assert_eq!(sender_server_name("@user:example.com"), Some("example.com"));
        assert_eq!(sender_server_name("@alice:matrix.org"), Some("matrix.org"));
    }

    #[test]
    fn test_sender_server_name_invalid() {
        assert_eq!(sender_server_name("not_an_mxid"), None);
        assert_eq!(sender_server_name("@no_colon"), None);
        assert_eq!(sender_server_name("@user:"), None); // empty server name
        assert_eq!(sender_server_name(""), None);
    }

    // ── verify_pdu_signature_with_client tests ───────────────────────

    use crate::client::ServerKeys;
    use crate::test_mocks::MockFederationClient;

    /// Helper: build a ServerKeys struct from a signing key's public key.
    fn make_server_keys(server_name: &str, key_id: &str, signing_key: &SigningKey) -> ServerKeys {
        let pub_b64 = base64::engine::general_purpose::STANDARD_NO_PAD.encode(signing_key.verifying_key().to_bytes());
        ServerKeys {
            server_name: server_name.to_string(),
            verify_keys: serde_json::json!({ key_id: { "key": pub_b64 } }),
            old_verify_keys: serde_json::json!({}),
            signatures: serde_json::json!({}),
            valid_until_ts: synapse_common::current_timestamp_millis() + 3_600_000,
        }
    }

    /// Helper: sign a PDU with the given key, matching the Matrix spec's
    /// signing algorithm (canonical JSON without signatures/unsigned).
    fn sign_pdu(server_name: &str, key_id: &str, secret_b64: &str, pdu: &mut Value) {
        sign_json(server_name, key_id, secret_b64, pdu).unwrap();
    }

    #[tokio::test]
    async fn test_verify_pdu_signature_valid() {
        let (secret_b64, signing_key) = generate_test_key();
        let server_name = "example.com";
        let key_id = "ed25519:1";

        let mut pdu = serde_json::json!({
            "event_id": "$evt:example.com",
            "type": "m.room.message",
            "room_id": "!room:example.com",
            "sender": "@user:example.com",
            "content": {"body": "hello"},
            "origin": "example.com",
            "origin_server_ts": 1000,
        });
        sign_pdu(server_name, key_id, &secret_b64, &mut pdu);

        let mock = MockFederationClient::new("local.test");
        mock.seed_server_keys(server_name, make_server_keys(server_name, key_id, &signing_key))
            .await;

        let result = verify_pdu_signature_with_client(&mock, &pdu).await;
        assert!(result.is_ok(), "valid PDU signature should verify: {:?}", result.err());
    }

    #[tokio::test]
    async fn test_verify_pdu_signature_tampered_content() {
        let (secret_b64, signing_key) = generate_test_key();
        let server_name = "example.com";
        let key_id = "ed25519:1";

        let mut pdu = serde_json::json!({
            "event_id": "$evt:example.com",
            "type": "m.room.message",
            "room_id": "!room:example.com",
            "sender": "@user:example.com",
            "content": {"body": "hello"},
            "origin": "example.com",
            "origin_server_ts": 1000,
        });
        sign_pdu(server_name, key_id, &secret_b64, &mut pdu);

        // Tamper with the content after signing.
        pdu["content"]["body"] = serde_json::Value::String("tampered".to_string());

        let mock = MockFederationClient::new("local.test");
        mock.seed_server_keys(server_name, make_server_keys(server_name, key_id, &signing_key))
            .await;

        let result = verify_pdu_signature_with_client(&mock, &pdu).await;
        assert!(result.is_err(), "tampered PDU should fail signature verification");
    }

    #[tokio::test]
    async fn test_verify_pdu_signature_missing_signatures() {
        let pdu = serde_json::json!({
            "event_id": "$evt:example.com",
            "type": "m.room.message",
            "room_id": "!room:example.com",
            "sender": "@user:example.com",
            "content": {"body": "hello"},
        });

        let mock = MockFederationClient::new("local.test");
        let result = verify_pdu_signature_with_client(&mock, &pdu).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("signatures"));
    }

    #[tokio::test]
    async fn test_verify_pdu_signature_wrong_server_key() {
        let (secret_b64, _) = generate_test_key();
        let server_name = "example.com";
        let key_id = "ed25519:1";

        let mut pdu = serde_json::json!({
            "event_id": "$evt:example.com",
            "type": "m.room.message",
            "room_id": "!room:example.com",
            "sender": "@user:example.com",
            "content": {"body": "hello"},
            "origin": "example.com",
            "origin_server_ts": 1000,
        });
        sign_pdu(server_name, key_id, &secret_b64, &mut pdu);

        // Use a different key pair for the server keys.
        let wrong_secret: [u8; 32] = [
            99, 98, 97, 96, 95, 94, 93, 92, 91, 90, 89, 88, 87, 86, 85, 84, 83, 82, 81, 80, 79, 78, 77, 76, 75, 74, 73,
            72, 71, 70, 69, 68,
        ];
        let wrong_signing_key = SigningKey::from_bytes(&wrong_secret);

        let mock = MockFederationClient::new("local.test");
        mock.seed_server_keys(server_name, make_server_keys(server_name, key_id, &wrong_signing_key))
            .await;

        let result = verify_pdu_signature_with_client(&mock, &pdu).await;
        assert!(result.is_err(), "PDU signed with different key should fail");
    }
}
