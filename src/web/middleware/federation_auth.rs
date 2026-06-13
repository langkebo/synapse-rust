use crate::common::ApiError;
use crate::web::routes::AppState;
use crate::web::utils::encoding::decode_base64_32;
use axum::extract::State;
use axum::http::Request;
use axum::response::IntoResponse;
use axum::{body::Body, middleware::Next, response::Response};
use base64::Engine;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Semaphore;

#[derive(Clone, Debug)]
pub struct FederationRequestAuth {
    pub origin: String,
    pub key_id: String,
}

pub async fn federation_auth_middleware(State(state): State<AppState>, request: Request<Body>, next: Next) -> Response {
    if !state.services.core.config.federation.enabled || !state.services.core.config.federation.allow_ingress {
        return axum::http::StatusCode::NOT_FOUND.into_response();
    }

    let (parts, body) = request.into_parts();

    let auth_header =
        parts.headers.get("authorization").or(parts.headers.get("Authorization")).and_then(|h| h.to_str().ok());

    let auth_header = match auth_header {
        Some(v) => v,
        None => return ApiError::unauthorized("Missing federation signature".to_string()).into_response(),
    };

    let params = match parse_x_matrix_authorization(auth_header) {
        Some(p) => p,
        None => return ApiError::unauthorized("Missing federation signature".to_string()).into_response(),
    };

    if let Some(ref dest) = params.destination {
        if !is_local_federation_destination(&state, dest) {
            ::tracing::warn!(
                target: "security_audit",
                event = "federation_destination_mismatch",
                claimed_destination = dest,
                local_server = state.services.core.server_name,
                origin = params.origin,
                "Federation request destination does not match local server - possible replay attack"
            );
            return ApiError::unauthorized("Federation request destination does not match this server".to_string())
                .into_response();
        }
    }

    let destination = state.services.core.server_name.as_str();

    let body_limit = state.services.core.config.federation.max_transaction_payload.max(64 * 1024) as usize;

    let body_bytes = match axum::body::to_bytes(body, body_limit).await {
        Ok(b) => b,
        Err(_) => return ApiError::unauthorized("Invalid request body".to_string()).into_response(),
    };

    let content = if body_bytes.is_empty() {
        None
    } else {
        match serde_json::from_slice::<Value>(&body_bytes) {
            Ok(v) => Some(v),
            Err(_) => return ApiError::unauthorized("Invalid JSON body".to_string()).into_response(),
        }
    };

    let request_target =
        parts.uri.path_and_query().map_or_else(|| parts.uri.path().to_string(), |p| p.as_str().to_string());
    let key_fetch_priority = request_target.contains("/_matrix/federation/v1/make_join/")
        || request_target.contains("/_matrix/federation/v1/send_join/")
        || request_target.contains("/_matrix/federation/v1/invite/")
        || request_target.contains("/_matrix/federation/v1/make_leave/")
        || request_target.contains("/_matrix/federation/v1/send_leave/");

    let signed_bytes = canonical_federation_request_bytes(
        parts.method.as_str(),
        &request_target,
        &params.origin,
        destination,
        content.as_ref(),
    );

    let signature_valid = verify_federation_signature_with_cache(
        &state,
        &params.origin,
        &params.key,
        &params.sig,
        &signed_bytes,
        key_fetch_priority,
    )
    .await;

    if let Err(e) = signature_valid {
        tracing::warn!(
            "Unauthorized federation request from {:?}. Server name: {}. Error: {}",
            parts.headers.get("x-forwarded-for").or(parts.headers.get("host")),
            state.services.core.server_name,
            e
        );
        return ApiError::unauthorized("Invalid federation signature".to_string()).into_response();
    }

    let origin_server = &params.origin;

    if state.services.core.config.federation.admission_mode {
        let server_status =
            sqlx::query_scalar!("SELECT status FROM federation_servers WHERE server_name = $1", origin_server)
                .fetch_optional(&*state.services.account.user_storage.pool)
                .await
                .ok()
                .flatten();

        match server_status {
            Some(status) if status != "active" => {
                tracing::warn!("Federation request rejected from server '{}' with status '{}'", origin_server, status);
                return ApiError::forbidden(format!(
                    "Server '{origin_server}' is not authorized for federation (status: {status})"
                ))
                .into_response();
            }
            None => {
                let now = chrono::Utc::now().timestamp_millis();
                let _ = sqlx::query!(
                    "INSERT INTO federation_servers (server_name, status, updated_ts) \
                     VALUES ($1, 'pending', $2) \
                     ON CONFLICT (server_name) DO NOTHING",
                    origin_server,
                    now
                )
                .execute(&*state.services.account.user_storage.pool)
                .await;

                tracing::info!("New federation server '{}' registered as pending", origin_server);
                return ApiError::forbidden(format!(
                    "Server '{origin_server}' is pending federation admission approval"
                ))
                .into_response();
            }
            _ => {}
        }
    }

    let mut parts = parts;
    parts.extensions.insert(FederationRequestAuth { origin: params.origin, key_id: params.key });

    let request = Request::from_parts(parts, Body::from(body_bytes));
    next.run(request).await
}

fn is_local_federation_destination(state: &AppState, destination: &str) -> bool {
    let server_config = &state.services.core.config.server;
    [
        state.services.core.server_name.as_str(),
        server_config.name.as_str(),
        server_config.get_server_name(),
        state.services.core.config.federation.server_name.as_str(),
    ]
    .into_iter()
    .any(|local_name| !local_name.is_empty() && local_name == destination)
}

pub async fn replication_http_auth_middleware(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    if !state.services.core.config.worker.replication.http.enabled {
        return next.run(request).await;
    }
    let secret = if let Some(s) = &state.services.core.config.worker.replication.http.secret {
        s.clone()
    } else if let Some(p) = &state.services.core.config.worker.replication.http.secret_path {
        match fs::read_to_string(PathBuf::from(p)) {
            Ok(s) => s.trim().to_string(),
            Err(_) => return ApiError::unauthorized("Replication secret not available".to_string()).into_response(),
        }
    } else {
        return ApiError::unauthorized("Replication secret not configured".to_string()).into_response();
    };
    let token = request.headers().get("x-synapse-worker-secret").and_then(|h| h.to_str().ok()).unwrap_or_default();
    if !crate::common::crypto::secure_compare(token, &secret) {
        return ApiError::unauthorized("Invalid replication secret".to_string()).into_response();
    }
    next.run(request).await
}

#[derive(Debug, Clone)]
struct XMatrixAuthParams {
    origin: String,
    key: String,
    sig: String,
    destination: Option<String>,
}

fn parse_x_matrix_authorization(header_value: &str) -> Option<XMatrixAuthParams> {
    let header_value = header_value.trim();
    if !header_value.to_ascii_lowercase().starts_with("x-matrix") {
        return None;
    }
    let header_value = header_value["x-matrix".len()..].trim();

    let mut origin: Option<String> = None;
    let mut key: Option<String> = None;
    let mut sig: Option<String> = None;
    let mut destination: Option<String> = None;

    for part in header_value.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let Some((k, v)) = part.split_once('=') else {
            continue;
        };
        let k = k.trim().to_ascii_lowercase();
        let mut v = v.trim();
        if v.starts_with('"') && v.ends_with('"') && v.len() >= 2 {
            v = &v[1..v.len() - 1];
        }

        match k.as_str() {
            "origin" => origin = Some(v.to_string()),
            "key" => key = Some(v.to_string()),
            "sig" => sig = Some(v.to_string()),
            "destination" => destination = Some(v.to_string()),
            _ => {}
        }
    }

    Some(XMatrixAuthParams { origin: origin?, key: key?, sig: sig?, destination })
}

fn canonical_federation_request_bytes(
    method: &str,
    uri: &str,
    origin: &str,
    destination: &str,
    content: Option<&Value>,
) -> Vec<u8> {
    let result =
        crate::federation::signing::canonical_federation_request_bytes(method, uri, origin, destination, content);
    tracing::debug!("Canonical request bytes: {}", String::from_utf8_lossy(&result));
    result
}

pub(crate) async fn verify_federation_signature_with_cache(
    state: &AppState,
    origin: &str,
    key_id: &str,
    signature: &str,
    signed_bytes: &[u8],
    key_fetch_priority: bool,
) -> Result<(), ApiError> {
    use crate::cache::CacheEntryKey;

    let content_hash = compute_signature_content_hash(signed_bytes);
    let cache_key = CacheEntryKey::new(origin, key_id, &content_hash);

    if let Some(entry) = state.federation_signature_cache.get_signature(&cache_key) {
        if !entry.is_expired() {
            tracing::debug!("Signature cache hit for {}:{}", origin, key_id);
            if entry.verified {
                return Ok(());
            }
            return Err(ApiError::unauthorized("Cached signature verification failed".to_string()));
        }
    }

    let result = verify_federation_signature(state, origin, key_id, signature, signed_bytes, key_fetch_priority).await;

    state.federation_signature_cache.set_signature(&cache_key, result.is_ok());

    result
}

fn compute_signature_content_hash(content: &[u8]) -> String {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    hasher.update(content);
    let result = hasher.finalize();
    base64::engine::general_purpose::STANDARD_NO_PAD.encode(result)
}

async fn verify_federation_signature(
    state: &AppState,
    origin: &str,
    key_id: &str,
    signature: &str,
    signed_bytes: &[u8],
    key_fetch_priority: bool,
) -> Result<(), ApiError> {
    let public_key = get_federation_verify_key(state, origin, key_id, key_fetch_priority).await?;

    let signature_bytes = match decode_ed25519_signature(signature) {
        Ok(sig) => sig,
        Err(_) => return Err(ApiError::unauthorized("Invalid signature format".to_string())),
    };

    let verifying_key = match ed25519_dalek::VerifyingKey::from_bytes(&public_key) {
        Ok(k) => k,
        Err(_) => return Err(ApiError::unauthorized("Invalid public key".to_string())),
    };

    tracing::debug!(
        "Verifying signature for origin={}, key_id={}, signed_bytes={}",
        origin,
        key_id,
        String::from_utf8_lossy(signed_bytes)
    );

    match verifying_key.verify_strict(signed_bytes, &signature_bytes) {
        Ok(()) => Ok(()),
        Err(e) => {
            tracing::debug!("Signature verification failed: {:?}", e);
            Err(ApiError::unauthorized("Signature verification failed".to_string()))
        }
    }
}

async fn get_federation_verify_key(
    state: &AppState,
    origin: &str,
    key_id: &str,
    key_fetch_priority: bool,
) -> Result<[u8; 32], ApiError> {
    let cache_key = format!("federation:verify_key:{origin}:{key_id}");
    if let Ok(Some(cached)) = state.cache.get::<String>(&cache_key).await {
        if let Ok(key) = decode_ed25519_public_key(&cached) {
            return Ok(key);
        }
    }

    if origin == state.services.core.server_name || origin == state.services.core.config.federation.server_name {
        if let Some(key) = get_local_verify_key(state, key_id).await {
            let key_str = base64::engine::general_purpose::STANDARD_NO_PAD.encode(key);
            let ttl = 3600u64;
            let _ = state.cache.set(&cache_key, &key_str, ttl).await;
            return Ok(key);
        }
    }

    let fetched = fetch_federation_verify_key(state, origin, key_id, key_fetch_priority).await?;
    let ttl = 3600u64;
    let _ = state.cache.set(&cache_key, &fetched, ttl).await;
    decode_ed25519_public_key(&fetched).map_err(|_| ApiError::unauthorized("Invalid public key".to_string()))
}

async fn get_local_verify_key(state: &AppState, key_id: &str) -> Option<[u8; 32]> {
    let config = &state.services.core.config.federation;

    if !config.enabled {
        return None;
    }

    let config_key_id = config.key_id.as_deref().unwrap_or("ed25519:1");
    if key_id != config_key_id {
        if state.services.federation.key_rotation_manager.load_or_create_key().await.is_err() {
            return None;
        }

        let current_key = state.services.federation.key_rotation_manager.get_current_key().await.ok().flatten()?;

        if current_key.key_id != key_id {
            return None;
        }

        return decode_ed25519_public_key(&current_key.public_key).ok();
    }

    if let Some(signing_key) = config.signing_key.as_deref() {
        let signing_key_bytes = decode_base64_32(signing_key)?;
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_bytes);
        let verifying_key = signing_key.verifying_key();
        return Some(*verifying_key.as_bytes());
    }

    if state.services.federation.key_rotation_manager.load_or_create_key().await.is_err() {
        return None;
    }

    let current_key = state.services.federation.key_rotation_manager.get_current_key().await.ok().flatten()?;

    if current_key.key_id != key_id {
        return None;
    }

    decode_ed25519_public_key(&current_key.public_key).ok()
}

async fn fetch_federation_verify_key(
    state: &AppState,
    origin: &str,
    key_id: &str,
    key_fetch_priority: bool,
) -> Result<String, ApiError> {
    let backoff_key = format!("federation:key_fetch_backoff:{origin}:{key_id}");
    if let Ok(Some(true)) = state.cache.get::<bool>(&backoff_key).await {
        return Err(ApiError::unauthorized("Public key not found".to_string()));
    }

    let semaphore: &Arc<Semaphore> = if key_fetch_priority {
        &state.federation_key_fetch_priority_semaphore
    } else {
        &state.federation_key_fetch_general_semaphore
    };
    let _permit = semaphore
        .clone()
        .acquire_owned()
        .await
        .map_err(|_| ApiError::internal("Rate limit semaphore closed".to_string()))?;

    let timeout_ms = state.services.core.config.federation.key_fetch_timeout_ms.max(1);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(timeout_ms))
        .build()
        .map_err(|e| ApiError::internal(e.to_string()))?;

    let urls = [
        format!("https://{origin}/_matrix/key/v2/server"),
        format!("http://{origin}/_matrix/key/v2/server"),
        format!("https://{origin}/_matrix/key/v2/query/{origin}/{key_id}"),
        format!("http://{origin}/_matrix/key/v2/query/{origin}/{key_id}"),
    ];

    for url in urls {
        let resp = match client.get(&url).send().await {
            Ok(r) => r,
            Err(_) => continue,
        };
        if !resp.status().is_success() {
            continue;
        }
        let json = match resp.json::<Value>().await {
            Ok(v) => v,
            Err(_) => continue,
        };
        if let Some(key) = extract_verify_key_from_server_keys(&json, origin, key_id) {
            if verify_server_keys_signature(&json, origin, key_id, &key) {
                return Ok(key);
            }
            tracing::warn!("Server keys signature verification failed for {} key_id={}", origin, key_id);
        }
    }

    let _ = state.cache.set(&backoff_key, true, 30).await;
    Err(ApiError::unauthorized("Public key not found".to_string()))
}

fn extract_verify_key_from_server_keys(body: &Value, origin: &str, key_id: &str) -> Option<String> {
    if let Some(key) = extract_verify_key_from_server_keys_object(body, key_id) {
        return Some(key);
    }

    let server_keys = body.get("server_keys")?.as_array()?;
    for entry in server_keys {
        if entry.get("server_name").and_then(|v| v.as_str()).is_some_and(|v| v != origin) {
            continue;
        }

        if let Some(key) = extract_verify_key_from_server_keys_object(entry, key_id) {
            return Some(key);
        }
    }

    None
}

fn extract_verify_key_from_server_keys_object(body: &Value, key_id: &str) -> Option<String> {
    let verify_keys = body.get("verify_keys")?.as_object()?;
    if let Some(entry) = verify_keys.get(key_id) {
        if let Some(key) = entry.get("key").and_then(|v| v.as_str()) {
            return Some(key.to_string());
        }
    }
    None
}

fn verify_server_keys_signature(body: &Value, origin: &str, key_id: &str, verify_key: &str) -> bool {
    let signature =
        match body.get("signatures").and_then(|s| s.get(origin)).and_then(|s| s.get(key_id)).and_then(|s| s.as_str()) {
            Some(sig) => sig,
            None => {
                tracing::warn!("No signature found in server keys response for {} key_id={}", origin, key_id);
                return false;
            }
        };

    let pub_key_bytes = match decode_ed25519_public_key(verify_key) {
        Ok(bytes) => bytes,
        Err(()) => return false,
    };

    let verifying_key = match ed25519_dalek::VerifyingKey::from_bytes(&pub_key_bytes) {
        Ok(key) => key,
        Err(_) => return false,
    };

    let sig = match decode_ed25519_signature(signature) {
        Ok(s) => s,
        Err(()) => return false,
    };

    let mut unsigned = body.clone();
    if let Some(obj) = unsigned.as_object_mut() {
        obj.remove("signatures");
        obj.remove("unsigned");
    }
    let canonical = crate::federation::signing::canonical_json_string(&unsigned);

    use ed25519_dalek::Verifier;
    verifying_key.verify(canonical.as_bytes(), &sig).is_ok()
}

fn decode_ed25519_public_key(key: &str) -> Result<[u8; 32], ()> {
    let engines = [base64::engine::general_purpose::STANDARD, base64::engine::general_purpose::STANDARD_NO_PAD];

    for engine in engines {
        if let Ok(bytes) = engine.decode(key) {
            if bytes.len() == 32 {
                let mut out = [0u8; 32];
                out.copy_from_slice(&bytes);
                return Ok(out);
            }
        }
    }
    Err(())
}

fn decode_ed25519_signature(sig: &str) -> Result<ed25519_dalek::Signature, ()> {
    let engines = [
        base64::engine::general_purpose::STANDARD,
        base64::engine::general_purpose::STANDARD_NO_PAD,
        base64::engine::general_purpose::URL_SAFE,
        base64::engine::general_purpose::URL_SAFE_NO_PAD,
    ];

    for engine in engines {
        if let Ok(bytes) = engine.decode(sig) {
            if bytes.len() == 64 {
                if let Ok(sig) = ed25519_dalek::Signature::try_from(&bytes[..]) {
                    return Ok(sig);
                }
            }
        }
    }
    Err(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::{CacheConfig, CacheManager};
    use crate::services::ServiceContainer;
    use crate::web::routes::AppState;
    use ed25519_dalek::Signer;
    use std::sync::Arc;

    #[test]
    fn test_extract_verify_key_from_server_key_response() {
        let body = serde_json::json!({
            "server_name": "example.org",
            "verify_keys": {
                "ed25519:abc": { "key": "SGVsbG9Xb3JsZA" }
            }
        });

        let key = extract_verify_key_from_server_keys(&body, "example.org", "ed25519:abc");
        assert_eq!(key, Some("SGVsbG9Xb3JsZA".to_string()));
    }

    #[test]
    fn test_extract_verify_key_from_query_response() {
        let body = serde_json::json!({
            "server_keys": [
                {
                    "server_name": "example.org",
                    "verify_keys": {
                        "ed25519:abc": { "key": "SGVsbG9Xb3JsZA" }
                    }
                }
            ]
        });

        let key = extract_verify_key_from_server_keys(&body, "example.org", "ed25519:abc");
        assert_eq!(key, Some("SGVsbG9Xb3JsZA".to_string()));
    }

    #[test]
    fn test_parse_x_matrix_authorization_header() {
        let params =
            parse_x_matrix_authorization(r#"X-Matrix origin="test.example.com", key="ed25519:test", sig="abc123""#)
                .expect("header should parse");

        assert_eq!(params.origin, "test.example.com");
        assert_eq!(params.key, "ed25519:test");
        assert_eq!(params.sig, "abc123");
    }

    #[test]
    fn test_parse_x_matrix_authorization_parameter_names_case_insensitive() {
        let params = parse_x_matrix_authorization(
            r#"X-Matrix Origin="test.example.com", Destination="dest.example.com", Key="ed25519:test", Sig="abc123""#,
        )
        .expect("header should parse");

        assert_eq!(params.origin, "test.example.com");
        assert_eq!(params.destination.as_deref(), Some("dest.example.com"));
        assert_eq!(params.key, "ed25519:test");
        assert_eq!(params.sig, "abc123");
    }

    #[tokio::test]
    async fn test_verify_federation_signature_with_local_config_key() {
        let signing_key_bytes = [7u8; 32];
        let signing_key_b64 = base64::engine::general_purpose::STANDARD_NO_PAD.encode(signing_key_bytes);
        let key_id = "ed25519:test".to_string();
        let origin = "test.example.com".to_string();
        let body = serde_json::json!({
            "invite": {
                "display_name": "Bridge Invite"
            }
        });
        let uri = "/_matrix/federation/v1/exchange_third_party_invite/!room:test.example.com";

        let mut services = ServiceContainer::new_test().await;
        services.core.config.federation.enabled = true;
        services.core.config.federation.allow_ingress = true;
        services.core.config.federation.server_name = origin.clone();
        services.core.config.federation.key_id = Some(key_id.clone());
        services.core.config.federation.signing_key = Some(signing_key_b64);
        services.core.server_name = origin.clone();

        let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
        let state = AppState::new(services, cache);

        let signed_bytes = canonical_federation_request_bytes("PUT", uri, &origin, &origin, Some(&body));
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_bytes);
        let signature = signing_key.sign(&signed_bytes);
        let signature_b64 = base64::engine::general_purpose::STANDARD_NO_PAD.encode(signature.to_bytes());

        let header = format!("X-Matrix origin=\"{origin}\", key=\"{key_id}\", sig=\"{signature_b64}\"");
        let params = parse_x_matrix_authorization(&header).expect("header should parse");

        verify_federation_signature_with_cache(&state, &params.origin, &params.key, &params.sig, &signed_bytes, false)
            .await
            .expect("signature should verify against local config key");
    }

    #[test]
    fn test_compute_signature_content_hash_deterministic() {
        let content1 = b"test content for hashing with more data";
        let content2 = b"test content for hashing with more data";
        let content3 = b"different content";

        let hash1 = compute_signature_content_hash(content1);
        let hash2 = compute_signature_content_hash(content2);
        let hash3 = compute_signature_content_hash(content3);

        assert_eq!(hash1, hash2, "Same content should produce same hash");
        assert_ne!(hash1, hash3, "Different content should produce different hash");
        assert_eq!(hash1.len(), 43, "SHA256 Base64 output should be 43 characters");
    }

    #[test]
    fn test_compute_signature_content_hash_empty() {
        let empty_content = b"";
        let hash = compute_signature_content_hash(empty_content);

        assert_eq!(hash.len(), 43);
        assert_ne!(hash, "");
    }

    #[test]
    fn test_compute_signature_content_hash_binary_data() {
        let binary_data: [u8; 16] =
            [0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f];
        let hash = compute_signature_content_hash(&binary_data);

        assert_eq!(hash.len(), 43);
        assert!(hash.chars().all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '='));
    }
}
