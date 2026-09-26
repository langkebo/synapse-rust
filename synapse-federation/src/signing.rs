use base64::Engine;
use ed25519_dalek::{Signer, SigningKey};
use serde_json::Value;
use synapse_common::canonical_json;
use synapse_common::pdu::event_id_is_a_pdu_field;
use synapse_common::redaction::redact_event;
use synapse_common::secure_compare;
use synapse_common::CanonicalEvent;

const MAX_PDU_SIZE_BYTES: usize = 65536;
const MAX_EVENT_KEYS: usize = 100;
const MAX_CONTENT_KEYS: usize = 100;
const MAX_STRING_LENGTH: usize = 65536;

/// See [`canonical_federation_request_bytes`.
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

/// See [`sign_json`.
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

/// Computes the Matrix event **content hash** (`hashes.sha256`).
///
/// This is the hash of the *unredacted* event, matching upstream Synapse
/// `synapse/crypto/event_signing.py::compute_content_hash` (release-v1.161):
/// remove `age_ts`, `unsigned`, `signatures`, `hashes`, `outlier` and
/// `destinations`, encode the rest as Matrix canonical JSON, SHA-256 it, and
/// encode the digest as unpadded standard Base64.
///
/// Redaction is **not** part of this computation — upstream applies it to the
/// signature material instead (`compute_event_signature`).  The previous
/// implementation redacted first, which produced a `hashes.sha256` that no
/// remote server could reproduce; it was replaced as part of the U-13 fix.
pub fn compute_event_content_hash(event: &Value) -> Option<String> {
    let mut stripped = event.clone();
    let obj = stripped.as_object_mut()?;
    for key in ["age_ts", "unsigned", "signatures", "hashes", "outlier", "destinations"] {
        obj.remove(key);
    }
    let canonical = canonical_json(&stripped).ok()?;
    use sha2::Digest;
    let hash = sha2::Sha256::digest(canonical.as_bytes());
    Some(base64::engine::general_purpose::STANDARD_NO_PAD.encode(hash))
}

/// See [`verify_event_content_hash`.
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

/// See [`check_pdu_size_limits`.
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

/// See [`check_event_federate`.
pub fn check_event_federate(room_create_event: &Value) -> bool {
    room_create_event.get("content").and_then(|c| c.get("m.federate")).and_then(|f| f.as_bool()).unwrap_or(true)
}

/// Sign and hash a locally-produced PDU so it can be federated to remote
/// servers.
///
/// This function:
/// 1. Computes and inserts the `hashes.sha256` content hash over the
///    **unredacted** event ([`compute_event_content_hash`]).
/// 2. Builds the signature material the way upstream Synapse
///    `synapse/crypto/event_signing.py::compute_event_signature` does:
///    `redact_event(room_version, event)`, then remove `age_ts` and `unsigned`,
///    then — for v3+ — remove `event_id` (v3+ PDUs do not carry it, so it must
///    not be signed; v1/v2 keep it, and the upstream known-answer vectors prove
///    it).
/// 3. Signs that material and writes the signature back into the original
///    `event`.
///
/// `origin` is **not** touched here.  It is a primitive PDU field owned by the
/// single assembler [`synapse_common::pdu::build_pdu`]; injecting it inside the
/// signer would mutate the signed bytes behind the assembler's back and make
/// the signature unreproducible by a peer.
///
/// The `secret_key_base64` and `key_id` come from
/// `KeyRotationManager::get_current_key`.
///
/// Reference: element-hq/synapse release-v1.161
/// `synapse/crypto/event_signing.py::add_hashes_and_signatures` /
/// `compute_event_signature`.
pub fn sign_and_hash_event(
    room_version: &str,
    server_name: &str,
    key_id: &str,
    secret_key_base64: &str,
    event: &mut Value,
) -> Result<(), String> {
    if !event.is_object() {
        return Err("Event must be a JSON object".to_string());
    }

    // 1. Compute and set the content hash (over the unredacted event).
    let hash = compute_event_content_hash(event).ok_or_else(|| "Failed to compute event content hash".to_string())?;
    if let Some(obj) = event.as_object_mut() {
        let hashes = obj.entry("hashes").or_insert_with(|| Value::Object(serde_json::Map::new()));
        if let Some(hashes_obj) = hashes.as_object_mut() {
            hashes_obj.insert("sha256".to_string(), Value::String(hash));
        }
    }

    // 2. Signature material: the redacted PDU (this already drops the top-level
    //    `age_ts` and every field redaction does not retain).
    let mut material =
        redact_event(room_version, event).map_err(|e| format!("Failed to redact event for signing: {e}"))?;
    if let Some(obj) = material.as_object_mut() {
        obj.remove("age_ts");
        obj.remove("unsigned");
        if !event_id_is_a_pdu_field(room_version) {
            obj.remove("event_id");
        }
    }

    // 3. Compute canonical form of the material once, then sign using the
    //    cached form.  `CanonicalEvent::from_event` strips `signatures`
    //    (retained by redaction) and `unsigned`, matching upstream `sign_json`.
    let canonical = CanonicalEvent::from_event(&material).map_err(|e| format!("Canonical JSON error: {e}"))?;
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
    let sender = pdu.get("sender").and_then(|v| v.as_str()).ok_or_else(|| "Missing sender on PDU".to_string())?;
    let sender_server = sender_server_name(sender).ok_or_else(|| format!("Unparseable sender mxid: {sender}"))?;

    let signatures =
        pdu.get("signatures").and_then(|v| v.as_object()).ok_or_else(|| "PDU missing signatures field".to_string())?;
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

    let verify_keys =
        server_keys.verify_keys.as_object().ok_or_else(|| "Server keys verify_keys is not an object".to_string())?;

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

        let pub_arr: [u8; 32] =
            pub_bytes.as_slice().try_into().map_err(|_| "Public key must be 32 bytes".to_string())?;

        let verifying_key =
            ed25519_dalek::VerifyingKey::from_bytes(&pub_arr).map_err(|e| format!("Invalid verifying key: {e}"))?;

        let sig_bytes = base64::engine::general_purpose::STANDARD_NO_PAD
            .decode(signature)
            .or_else(|_| base64::engine::general_purpose::STANDARD.decode(signature))
            .map_err(|e| format!("Invalid signature base64: {e}"))?;

        let sig_arr: [u8; 64] =
            sig_bytes.as_slice().try_into().map_err(|_| "Signature must be 64 bytes".to_string())?;

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

    /// Upstream known-answer vector #1: `tests/crypto/test_event_signing.py::test_sign_minimal`
    /// (`element-hq/synapse` release-v1.161).  The expected value is the
    /// `hashes.sha256` Synapse computes for this exact event dict, so it pins
    /// the whole content-hash pipeline (field removal + canonical JSON +
    /// SHA-256 + unpadded standard Base64) against an independent
    /// implementation.
    #[test]
    fn content_hash_matches_synapse_known_answer_minimal() {
        let event = serde_json::json!({
            "event_id": "$0:domain",
            "origin_server_ts": 1000000,
            "signatures": {},
            "type": "X",
            "content": {},
            "unsigned": {"age_ts": 1000000},
        });
        assert_eq!(compute_event_content_hash(&event).as_deref(), Some("mq4QfPPpC+QsBd6eqfVsmJIEz8uvMSVK0+AU67PLESk"));
    }

    /// Upstream known-answer vector #2: `test_sign_message` (same file).
    #[test]
    fn content_hash_matches_synapse_known_answer_message() {
        let event = serde_json::json!({
            "content": {"body": "Here is the message content"},
            "event_id": "$0:domain",
            "origin_server_ts": 1000000,
            "type": "m.room.message",
            "room_id": "!r:domain",
            "sender": "@u:domain",
            "signatures": {},
            "unsigned": {"age_ts": 1000000},
        });
        assert_eq!(compute_event_content_hash(&event).as_deref(), Some("rDCeYBepPlI891h/RkI2/Lkf9bt7u0TxFku4tMs7WKk"));
    }

    /// The content hash is taken over the **unredacted** event: unlike the
    /// signature material, an `m.room.message` body is part of it, while
    /// `hashes`/`signatures`/`unsigned`/`age_ts` are not.
    #[test]
    fn content_hash_covers_unredacted_content_but_not_hash_fields() {
        let base = serde_json::json!({
            "event_id": "$e",
            "type": "m.room.message",
            "room_id": "!r:domain",
            "sender": "@u:domain",
            "content": {"body": "hello", "msgtype": "m.text"},
        });
        let baseline = compute_event_content_hash(&base).unwrap();

        let mut other_body = base.clone();
        other_body["content"] = serde_json::json!({"body": "goodbye", "msgtype": "m.text"});
        assert_ne!(compute_event_content_hash(&other_body).unwrap(), baseline, "content is hashed unredacted");

        let mut with_noise = base.clone();
        with_noise["unsigned"] = serde_json::json!({"age_ts": 1, "transaction_id": "t"});
        with_noise["signatures"] = serde_json::json!({"domain": {"ed25519:1": "sig"}});
        with_noise["hashes"] = serde_json::json!({"sha256": "placeholder"});
        with_noise["age_ts"] = serde_json::json!(999);
        with_noise["outlier"] = serde_json::json!(true);
        with_noise["destinations"] = serde_json::json!(["other.example"]);
        assert_eq!(
            compute_event_content_hash(&with_noise).unwrap(),
            baseline,
            "hashes/signatures/unsigned/age_ts/outlier/destinations are excluded"
        );
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
        mock.seed_server_keys(server_name, make_server_keys(server_name, key_id, &signing_key)).await;

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
        mock.seed_server_keys(server_name, make_server_keys(server_name, key_id, &signing_key)).await;

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
        mock.seed_server_keys(server_name, make_server_keys(server_name, key_id, &wrong_signing_key)).await;

        let result = verify_pdu_signature_with_client(&mock, &pdu).await;
        assert!(result.is_err(), "PDU signed with different key should fail");
    }

    // ------------------------------------------------------------------
    // F-03: sign_and_hash_event must add server signature to invite PDU
    // ------------------------------------------------------------------

    #[test]
    fn test_sign_and_hash_event_invite_pdu() {
        // F-03: after re-signing an invite PDU locally, the resulting JSON
        // must contain `signatures.<local_server>.<key_id>` so third-party
        // origins can verify the invite in their `verify_pdu_sender_signature`
        // step.
        let (secret_b64, signing_key) = generate_test_key();
        let mut pdu = serde_json::json!({
            "event_id": "$invite:local.test",
            "room_id": "!room:local.test",
            "sender": "@remote:remote.test",
            "type": "m.room.member",
            "state_key": "@invitee:remote.test",
            "content": {"membership": "invite"},
            "origin_server_ts": 1_700_000_000_000_i64,
            "origin": "remote.test",
        });

        // v1 keeps every one of these fields in the signature material, so the
        // signed bytes are exactly the PDU minus `signatures`/`unsigned`.
        sign_and_hash_event("1", "local.test", "ed25519:1", &secret_b64, &mut pdu)
            .expect("sign_and_hash_event must succeed");

        let sigs = pdu.get("signatures").expect("signatures must be present after signing");
        let local_sigs = sigs.get("local.test").expect("local.test must have signatures");
        let sig_value = local_sigs.get("ed25519:1").expect("ed25519:1 key id must be present");
        assert!(sig_value.as_str().is_some_and(|s| !s.is_empty()), "signature must be a non-empty string");

        // Also verify the signature is actually a valid ed25519 signature over the
        // canonical JSON (without `signatures`/`unsigned`). This guarantees the
        // signature in `signatures.local.test.ed25519:1` is cryptographically valid.
        let mut copy = pdu.clone();
        copy.as_object_mut().unwrap().remove("signatures");
        copy.as_object_mut().unwrap().remove("unsigned");
        let canonical = canonical_json(&copy).unwrap();
        let verifying_key: VerifyingKey = signing_key.verifying_key();
        let sig_bytes = base64::engine::general_purpose::STANDARD_NO_PAD.decode(sig_value.as_str().unwrap()).unwrap();
        let sig = ed25519_dalek::Signature::from_bytes(&sig_bytes.try_into().unwrap());
        verifying_key.verify(canonical.as_bytes(), &sig).expect("signature must verify against canonical PDU");
    }

    #[test]
    fn test_sign_and_hash_event_invite_preserves_remote_signature() {
        // F-03: re-signing locally must NOT overwrite any existing remote
        // signatures on the PDU. Both must coexist so third-party verifiers
        // see the remote signature and the local server's acceptance signature.
        let (secret_b64, _signing_key) = generate_test_key();
        let mut pdu = serde_json::json!({
            "event_id": "$invite:local.test",
            "room_id": "!room:local.test",
            "sender": "@remote:remote.test",
            "type": "m.room.member",
            "state_key": "@invitee:remote.test",
            "content": {"membership": "invite"},
            "origin_server_ts": 1_700_000_000_000_i64,
            "origin": "remote.test",
            "signatures": {
                "remote.test": {
                    "ed25519:abc": "remote_sig_value"
                }
            }
        });

        sign_and_hash_event("1", "local.test", "ed25519:1", &secret_b64, &mut pdu).expect("re-sign must succeed");

        let sigs = pdu.get("signatures").unwrap();
        assert_eq!(
            sigs.get("remote.test").and_then(|r| r.get("ed25519:abc")).and_then(|v| v.as_str()),
            Some("remote_sig_value"),
            "F-03: remote signature must be preserved after local re-sign"
        );
        assert!(
            sigs.get("local.test").and_then(|r| r.get("ed25519:1")).is_some(),
            "F-03: local.test signature must be added"
        );
    }

    // ------------------------------------------------------------------
    // U-21: the signature material is the **redacted** PDU, not the raw event
    // (upstream `synapse/crypto/event_signing.py::compute_event_signature`).
    // ------------------------------------------------------------------

    /// Upstream deterministic signing seed for both vectors below.
    const SYNAPSE_SIGNING_KEY_SEED: &str = "YJDBA9Xnr2sVqXD9Vj7XVUnmFZcZrlw8Md7kMW+3XA1";

    /// Re-encodes the upstream seed as canonical unpadded Base64.
    ///
    /// The upstream string is **not** canonical unpadded Base64: its final
    /// symbol (`1`, value 53) has two non-zero trailing bits.  Python's
    /// `base64.b64decode` silently discards them (that is how signedjson decodes
    /// the key), while the strict `base64` crate rejects the string with
    /// "Invalid last symbol".  Decoding leniently and re-encoding yields
    /// `...MW+3XA0`, which decodes to the **same 32 bytes** and which the
    /// production decoder accepts.
    fn canonical_seed(seed: &str) -> String {
        let lenient = base64::engine::GeneralPurpose::new(
            &base64::alphabet::STANDARD,
            base64::engine::GeneralPurposeConfig::new()
                .with_decode_padding_mode(base64::engine::DecodePaddingMode::Indifferent)
                .with_decode_allow_trailing_bits(true),
        );
        let bytes = lenient.decode(seed).expect("the upstream seed must decode leniently");
        assert_eq!(bytes.len(), 32, "an ed25519 signing seed must be 32 bytes");
        base64::engine::general_purpose::STANDARD_NO_PAD.encode(bytes)
    }

    /// Upstream known-answer vector #1:
    /// `element-hq/synapse` release-v1.161
    /// `tests/crypto/test_event_signing.py::test_sign_minimal`, room version 1.
    ///
    /// This pins the whole *signing* pipeline against an independent
    /// implementation: content hash over the unredacted event, redacted signing
    /// material, canonical JSON, ed25519, unpadded standard Base64.  v1 keeps
    /// `event_id` in the signed bytes (upstream vector 2 in §6.6's checklist
    /// pins the v3+ "no `event_id`" branch).
    #[test]
    fn signature_matches_synapse_known_answer_minimal() {
        let mut event = serde_json::json!({
            "event_id": "$0:domain",
            "origin_server_ts": 1000000,
            "signatures": {},
            "type": "X",
            "content": {},
            "unsigned": {"age_ts": 1000000},
        });

        let seed = canonical_seed(SYNAPSE_SIGNING_KEY_SEED);
        assert_eq!(seed, "YJDBA9Xnr2sVqXD9Vj7XVUnmFZcZrlw8Md7kMW+3XA0", "normalized upstream seed");
        sign_and_hash_event("1", "domain", "ed25519:1", &seed, &mut event)
            .expect("signing the upstream minimal vector must succeed");

        assert_eq!(
            event["hashes"]["sha256"].as_str(),
            Some("mq4QfPPpC+QsBd6eqfVsmJIEz8uvMSVK0+AU67PLESk"),
            "content hash must match upstream test_sign_minimal"
        );
        assert_eq!(
            event["signatures"]["domain"]["ed25519:1"].as_str(),
            Some("18rGIkd4JJXxw9m+1j3BtN+TmqmLip4VHvFbyXLngpBLXOqbxlQViQABRzep2cODQ2aa5FnFgz+Llt2P03WiAw"),
            "signature must match upstream test_sign_minimal"
        );
    }

    /// Upstream known-answer vector #2:
    /// `tests/crypto/test_event_signing.py::test_sign_message`, room version 1.
    ///
    /// The `m.room.message` body is stripped by redaction, so a signer that
    /// signs the unredacted dict (the pre-U-21 behaviour) cannot reproduce this
    /// value while still matching the content hash above.
    #[test]
    fn signature_matches_synapse_known_answer_message() {
        let mut event = serde_json::json!({
            "content": {"body": "Here is the message content"},
            "event_id": "$0:domain",
            "origin_server_ts": 1000000,
            "type": "m.room.message",
            "room_id": "!r:domain",
            "sender": "@u:domain",
            "signatures": {},
            "unsigned": {"age_ts": 1000000},
        });

        let seed = canonical_seed(SYNAPSE_SIGNING_KEY_SEED);
        sign_and_hash_event("1", "domain", "ed25519:1", &seed, &mut event)
            .expect("signing the upstream message vector must succeed");

        assert_eq!(
            event["hashes"]["sha256"].as_str(),
            Some("rDCeYBepPlI891h/RkI2/Lkf9bt7u0TxFku4tMs7WKk"),
            "content hash must match upstream test_sign_message"
        );
        assert_eq!(
            event["signatures"]["domain"]["ed25519:1"].as_str(),
            Some("Ay4aj2b5oJ1k8INYZ9n3KnszCflM0emwcmQQ7vxpbdcSv9bkJxIZdWX1IJllcZLq89+D3sSabE+vqPtZs9akDw"),
            "signature must match upstream test_sign_message"
        );
    }

    /// v3+ PDUs do not carry `event_id`, so it must not take part in the signed
    /// bytes.
    ///
    /// **Why this is not two `sign_and_hash_event` calls compared for equality**
    /// (the literal wording of the acceptance item): the *content hash*
    /// legitimately covers `event_id` — upstream `compute_content_hash` does not
    /// strip it either, and `test_sign_minimal` above proves our function
    /// matches — so an event that carries an `event_id` necessarily hashes
    /// differently, and therefore signs differently, before the question of the
    /// signature material even arises.  What must hold instead is that the
    /// signature the function produced covers the material **with `event_id`
    /// removed**: this test signs a v3+ PDU that carries one, then verifies the
    /// signature against the redacted material with `event_id` (and
    /// `age_ts`/`unsigned`) stripped.  That verification succeeds only if the
    /// signer dropped it.  Mutation (b) in the U-21 self-proof (keep `event_id`
    /// in the v3+ material) turns this red.
    #[test]
    fn v3_signature_material_excludes_event_id() {
        let (secret_b64, signing_key) = generate_test_key();
        let mut pdu = serde_json::json!({
            "event_id": "$0:server",
            "room_id": "!r:server",
            "sender": "@u:server",
            "type": "m.room.message",
            "content": {"body": "hello", "msgtype": "m.text"},
            "origin": "server",
            "origin_server_ts": 1_700_000_000_000_i64,
            "depth": 3,
            "prev_events": ["$p:server"],
            "auth_events": ["$a:server"],
        });

        sign_and_hash_event("10", "server", "ed25519:1", &secret_b64, &mut pdu).unwrap();
        let signature = pdu["signatures"]["server"]["ed25519:1"].as_str().unwrap().to_string();

        // Reference material for a v3+ PDU: redact, then drop `age_ts` /
        // `unsigned` / `event_id`. `CanonicalEvent::from_event` drops
        // `signatures` / `unsigned`, matching upstream `sign_json`.
        let mut material = synapse_common::redaction::redact_event("10", &pdu).unwrap();
        {
            let obj = material.as_object_mut().unwrap();
            obj.remove("event_id");
            obj.remove("age_ts");
            obj.remove("unsigned");
        }
        assert!(material.get("event_id").is_none(), "v3+: `event_id` must not be signed: {material}");
        assert_eq!(material["hashes"], pdu["hashes"], "the material keeps `hashes`");

        let canonical = synapse_common::CanonicalEvent::from_event(&material).unwrap();
        let sig_bytes = base64::engine::general_purpose::STANDARD_NO_PAD.decode(signature).unwrap();
        let sig = ed25519_dalek::Signature::from_bytes(&sig_bytes.try_into().unwrap());
        signing_key
            .verifying_key()
            .verify(canonical.canonical_bytes(), &sig)
            .expect("v3+ signature must cover the redacted material without `event_id`");
    }

    /// v10 keeps `origin` in the signature material, v11 drops it (MSC2174/MSC3820
    /// `updated_redaction_rules`).  The content hash is version-independent, so a
    /// difference between the two signatures can only come from the
    /// room-version-aware redaction applied to the signing material — this is the
    /// "#3 proves the material went through versioned redaction" check.
    #[test]
    fn v10_and_v11_signatures_differ_over_origin() {
        let (secret_b64, _) = generate_test_key();
        let base = serde_json::json!({
            "event_id": "$0:server",
            "room_id": "!r:server",
            "sender": "@u:server",
            "type": "m.room.message",
            "content": {"body": "hello"},
            "origin": "server",
            "origin_server_ts": 1_700_000_000_000_i64,
            "depth": 3,
            "prev_events": ["$p:server"],
            "auth_events": ["$a:server"],
        });

        let mut v10 = base.clone();
        let mut v11 = base.clone();
        sign_and_hash_event("10", "server", "ed25519:1", &secret_b64, &mut v10).unwrap();
        sign_and_hash_event("11", "server", "ed25519:1", &secret_b64, &mut v11).unwrap();

        assert_eq!(v10["hashes"], v11["hashes"], "the content hash does not depend on the room version");
        assert!(v10["signatures"]["server"]["ed25519:1"].is_string());
        assert_ne!(
            v10["signatures"]["server"]["ed25519:1"], v11["signatures"]["server"]["ed25519:1"],
            "v11 stops protecting `origin`, so the signed bytes must differ from v10"
        );
    }
}
