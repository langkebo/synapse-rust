use base64::Engine;
use ed25519_dalek::{Signer, SigningKey};
use serde_json::Value;

const MAX_PDU_SIZE_BYTES: usize = 65536;
const MAX_EVENT_KEYS: usize = 100;
const MAX_CONTENT_KEYS: usize = 100;
const MAX_STRING_LENGTH: usize = 65536;

pub fn canonical_json_string(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(b) => {
            if *b {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        Value::Number(n) => n.to_string(),
        Value::String(s) => serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_string()),
        Value::Array(arr) => {
            let mut out = String::from("[");
            let mut first = true;
            for v in arr {
                if !first {
                    out.push(',');
                }
                first = false;
                out.push_str(&canonical_json_string(v));
            }
            out.push(']');
            out
        }
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut out = String::from("{");
            let mut first = true;
            for k in keys {
                if !first {
                    out.push(',');
                }
                first = false;
                out.push_str(&serde_json::to_string(k).unwrap_or_else(|_| "\"\"".to_string()));
                out.push(':');
                if let Some(v) = map.get(k) {
                    out.push_str(&canonical_json_string(v));
                } else {
                    out.push_str("null");
                }
            }
            out.push('}');
            out
        }
    }
}

pub fn canonical_federation_request_bytes(
    method: &str,
    uri: &str,
    origin: &str,
    destination: &str,
    content: Option<&Value>,
) -> Vec<u8> {
    let mut obj = serde_json::Map::new();
    obj.insert("method".to_string(), Value::String(method.to_string()));
    obj.insert("uri".to_string(), Value::String(uri.to_string()));
    obj.insert("origin".to_string(), Value::String(origin.to_string()));
    obj.insert(
        "destination".to_string(),
        Value::String(destination.to_string()),
    );
    if let Some(content) = content {
        obj.insert("content".to_string(), content.clone());
    }
    canonical_json_string(&Value::Object(obj)).into_bytes()
}

pub fn sign_json(
    server_name: &str,
    key_id: &str,
    secret_key_base64: &str,
    value: &mut Value,
) -> Result<(), String> {
    let unsigned = {
        let mut copy = value.clone();
        if let Some(obj) = copy.as_object_mut() {
            obj.remove("signatures");
            obj.remove("unsigned");
        }
        canonical_json_string(&copy)
    };

    let secret_bytes: [u8; 32] = base64::engine::general_purpose::STANDARD_NO_PAD
        .decode(secret_key_base64)
        .map_err(|e| format!("Invalid secret key base64: {}", e))?
        .try_into()
        .map_err(|_| "Secret key must be 32 bytes".to_string())?;

    let signing_key = SigningKey::from_bytes(&secret_bytes);
    let signature = signing_key.sign(unsigned.as_bytes());
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
    let mut redacted = redact_event_for_hash(event)?;
    redacted.as_object_mut()?.remove("signatures");
    redacted.as_object_mut()?.remove("unsigned");
    let canonical = canonical_json_string(&redacted);
    use sha2::Digest;
    let hash = sha2::Sha256::digest(canonical.as_bytes());
    Some(base64::engine::general_purpose::STANDARD_NO_PAD.encode(hash))
}

pub fn verify_event_content_hash(event: &Value) -> Result<(), String> {
    let hashes = event
        .get("hashes")
        .and_then(|h| h.as_object())
        .ok_or_else(|| "Event missing hashes field".to_string())?;

    let sha256_hash = hashes
        .get("sha256")
        .and_then(|h| h.as_str())
        .ok_or_else(|| "Event missing sha256 hash".to_string())?;

    let computed = compute_event_content_hash(event)
        .ok_or_else(|| "Failed to compute event content hash".to_string())?;

    if computed != sha256_hash {
        return Err(format!(
            "Event content hash mismatch: expected {}, computed {}",
            sha256_hash, computed
        ));
    }

    Ok(())
}

pub fn check_pdu_size_limits(event: &Value) -> Result<(), String> {
    let event_json =
        serde_json::to_string(event).map_err(|e| format!("Failed to serialize event: {}", e))?;

    if event_json.len() > MAX_PDU_SIZE_BYTES {
        return Err(format!(
            "Event too large: {} bytes (max {})",
            event_json.len(),
            MAX_PDU_SIZE_BYTES
        ));
    }

    if let Some(obj) = event.as_object() {
        if obj.len() > MAX_EVENT_KEYS {
            return Err(format!(
                "Event has too many top-level keys: {} (max {})",
                obj.len(),
                MAX_EVENT_KEYS
            ));
        }
    }

    if let Some(content) = event.get("content").and_then(|c| c.as_object()) {
        if content.len() > MAX_CONTENT_KEYS {
            return Err(format!(
                "Event content has too many keys: {} (max {})",
                content.len(),
                MAX_CONTENT_KEYS
            ));
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
                return Err(format!(
                    "String value too long: {} bytes (max {})",
                    s.len(),
                    MAX_STRING_LENGTH
                ));
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

fn redact_event_for_hash(event: &Value) -> Option<Value> {
    let mut event = event.clone();

    let allowed_top_level: &[&str] = &[
        "event_id",
        "type",
        "room_id",
        "sender",
        "state_key",
        "content",
        "hashes",
        "signatures",
        "depth",
        "prev_events",
        "prev_state",
        "auth_events",
        "origin",
        "origin_server_ts",
        "membership",
    ];

    if let Some(obj) = event.as_object_mut() {
        obj.retain(|k, _| allowed_top_level.contains(&k.as_str()));
    }

    let event_type = event.get("type").and_then(|t| t.as_str()).unwrap_or("");

    let allowed_content_keys: &[&str] = match event_type {
        "m.room.member" => &[
            "membership",
            "third_party_invite",
            "displayname",
            "avatar_url",
        ],
        "m.room.create" => &["creator", "room_version", "type", "m.federate"],
        "m.room.join_rules" => &["join_rule", "allow"],
        "m.room.power_levels" => &[
            "users",
            "users_default",
            "events",
            "events_default",
            "state_default",
            "ban",
            "kick",
            "redact",
            "invite",
        ],
        "m.room.history_visibility" => &["history_visibility"],
        "m.room.encrypted" => &[
            "algorithm",
            "ciphertext",
            "session_id",
            "sender_key",
            "device_id",
        ],
        _ => &[],
    };

    if !allowed_content_keys.is_empty() {
        if let Some(content) = event.get_mut("content").and_then(|c| c.as_object_mut()) {
            content.retain(|k, _| allowed_content_keys.contains(&k.as_str()));
        }
    }

    Some(event)
}

pub fn check_event_federate(room_create_event: &Value) -> bool {
    room_create_event
        .get("content")
        .and_then(|c| c.get("m.federate"))
        .and_then(|f| f.as_bool())
        .unwrap_or(true)
}
