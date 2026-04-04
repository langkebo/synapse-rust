use base64::engine::general_purpose::STANDARD_NO_PAD;
use base64::Engine;
use ed25519_dalek::Signer;
use serde_json::Value;
use synapse_rust::federation::signing::canonical_federation_request_bytes;

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
