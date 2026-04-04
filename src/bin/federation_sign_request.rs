use base64::engine::general_purpose::STANDARD_NO_PAD;
use base64::Engine;
use ed25519_dalek::Signer;
use serde_json::Value;

fn canonical_json_string(value: &Value) -> String {
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

fn canonical_federation_request_bytes(
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

fn decode_base64_32(s: &str) -> Option<[u8; 32]> {
    let decoded = STANDARD_NO_PAD.decode(s).ok()?;
    let bytes: [u8; 32] = decoded.try_into().ok()?;
    Some(bytes)
}

fn main() {
    let method = std::env::args().nth(1).unwrap_or_default();
    let uri = std::env::args().nth(2).unwrap_or_default();
    let origin = std::env::args().nth(3).unwrap_or_default();
    let destination = std::env::args().nth(4).unwrap_or_default();
    let content_raw = std::env::args().nth(5);

    if method.is_empty() || uri.is_empty() || origin.is_empty() || destination.is_empty() {
        std::process::exit(2);
    }

    let signing_key_b64 = match std::env::var("FEDERATION_SIGNING_KEY") {
        Ok(v) => v,
        Err(_) => std::process::exit(3),
    };

    let signing_key_bytes = match decode_base64_32(&signing_key_b64) {
        Some(b) => b,
        None => std::process::exit(4),
    };

    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_bytes);

    let content = match content_raw.as_deref() {
        None => None,
        Some(s) if s.trim().is_empty() => None,
        Some(s) => match serde_json::from_str::<Value>(s) {
            Ok(v) => Some(v),
            Err(_) => std::process::exit(5),
        },
    };

    let bytes =
        canonical_federation_request_bytes(&method, &uri, &origin, &destination, content.as_ref());

    let sig = signing_key.sign(&bytes);
    let sig_b64 = STANDARD_NO_PAD.encode(sig.to_bytes());
    print!("{sig_b64}");
}
