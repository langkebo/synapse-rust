use crate::common::security::check_url_against_blacklist;
use crate::common::*;
use crate::web::middleware::FederationRequestAuth;
use crate::web::routes::AppState;
use crate::web::utils::encoding::decode_base64_32;
use axum::extract::{Extension, Json, Path, State};
use base64::Engine;
use serde_json::{json, Value};

pub(super) async fn server_key(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    if state.services.core.config.federation.signing_key.is_none() {
        state
            .services
            .federation
            .key_rotation_manager
            .load_or_create_key()
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to initialize federation signing key", &e))?;
    }

    Ok(Json(resolve_server_keys(&state).await?))
}

pub(super) async fn key_query(
    State(state): State<AppState>,
    Path((server_name, key_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    if server_name == state.services.core.server_name
        || server_name == state.services.core.config.federation.server_name
    {
        return server_key(State(state)).await;
    }

    let response = fetch_remote_server_keys_response(&state, &server_name, &key_id).await?;

    // P2-15: Defensive validation of the response before returning it to the
    // client. Invalid responses are never cached (see
    // `fetch_remote_server_keys_response`); this check only logs a warning so
    // we can observe stale/malformed keys slipping through (e.g. an expired
    // cached entry). We still return the response rather than erroring.
    if validate_server_key_response(&response, &server_name).is_none() {
        ::tracing::warn!(
            server_name = %server_name,
            key_id = %key_id,
            "Federation key query response failed validation; returning without caching"
        );
    }

    Ok(Json(response))
}

pub(super) async fn key_clone(
    State(state): State<AppState>,
    Json(_body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    server_key(State(state)).await
}

pub(super) async fn keys_claim(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let mut request: crate::e2ee::device_keys::KeyClaimRequest =
        serde_json::from_value(body).map_err(|e| ApiError::bad_request(format!("Invalid claim request: {e}")))?;

    if let Some(one_time_keys) = request.one_time_keys.as_object_mut() {
        // Batch federation origin validation: collect local users, then issue
        // a single query to filter those who share a joined room with a
        // member from the requesting server. Replaces the previous
        // M × (1 + N) nested N+1 pattern. See NEW-P1-03.
        let local_users: Vec<String> = one_time_keys
            .keys()
            .filter(|user_id| super::user_matches_origin(user_id, &state.services.core.server_name))
            .cloned()
            .collect();

        let allowed_local_users =
            state.services.rooms.room_service.filter_users_sharing_room_with_server(&local_users, &auth.origin).await?;

        one_time_keys.retain(|user_id, _| allowed_local_users.contains(user_id));
    }

    ::tracing::info!(
        target: "security_audit",
        event = "federation_keys_claim",
        origin = ?auth.origin,
        "Federation keys claim request"
    );

    let response = state
        .services
        .e2ee
        .device_keys_service
        .claim_keys_for_federation(request, &state.services.core.server_name)
        .await?;

    Ok(Json(json!({
        "one_time_keys": response.one_time_keys,
        "failures": response.failures
    })))
}

pub(super) async fn keys_query(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let mut request: crate::e2ee::device_keys::KeyQueryRequest =
        serde_json::from_value(body).map_err(|e| ApiError::bad_request(format!("Invalid query request: {e}")))?;

    if let Some(device_keys) = request.device_keys.as_object_mut() {
        // Batch federation origin validation: collect local users, then issue
        // a single query to filter those who share a joined room with a
        // member from the requesting server. Replaces the previous
        // M × (1 + N) nested N+1 pattern. See NEW-P1-03.
        let local_users: Vec<String> = device_keys
            .keys()
            .filter(|user_id| super::user_matches_origin(user_id, &state.services.core.server_name))
            .cloned()
            .collect();

        let allowed_local_users =
            state.services.rooms.room_service.filter_users_sharing_room_with_server(&local_users, &auth.origin).await?;

        device_keys.retain(|user_id, _| allowed_local_users.contains(user_id));
    }

    ::tracing::info!(
        target: "security_audit",
        event = "federation_keys_query",
        origin = ?auth.origin,
        "Federation keys query request"
    );

    let response = state
        .services
        .e2ee
        .device_keys_service
        .query_keys_for_federation(request, &state.services.core.server_name)
        .await?;

    Ok(Json(json!({
        "device_keys": response.device_keys,
        "failures": response.failures
    })))
}

pub(super) async fn keys_upload(
    State(_state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Json(_body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    ::tracing::info!(
        target: "security_audit",
        event = "federation_keys_upload",
        origin = ?auth.origin,
        "Federation keys upload request rejected: use user/keys endpoints instead"
    );

    Err(ApiError::unrecognized("Federation keys upload is not supported. Use client-side user/keys endpoints instead."))
}

pub(super) async fn legacy_keys_claim(
    State(_state): State<AppState>,
    Extension(_auth): Extension<FederationRequestAuth>,
    Json(_body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    Err(ApiError::unrecognized(
        "Legacy federation keys claim endpoint is not supported. Please use /_matrix/federation/v1/user/keys/claim instead.",
    ))
}

pub(super) async fn legacy_keys_query(
    State(_state): State<AppState>,
    Extension(_auth): Extension<FederationRequestAuth>,
    Json(_body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    Err(ApiError::unrecognized(
        "Legacy federation keys query endpoint is not supported. Please use /_matrix/federation/v1/user/keys/query instead.",
    ))
}

pub(super) async fn query_auth(State(_state): State<AppState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({
        "auth_chain": []
    })))
}

pub(super) async fn event_auth(State(_state): State<AppState>) -> Result<Json<Value>, ApiError> {
    Err(ApiError::not_found("Federation event_auth is not implemented; use supported auth-chain endpoints".to_string()))
}

async fn resolve_server_keys(state: &AppState) -> Result<Value, ApiError> {
    let config = &state.services.core.config.federation;
    if !config.enabled {
        return Err(ApiError::not_found("Federation disabled".to_string()));
    }

    if let Some(current_key) = state
        .services
        .federation
        .key_rotation_manager
        .get_current_key()
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to load federation signing key", &e))?
    {
        return state
            .services
            .federation
            .key_rotation_manager
            .get_server_keys_response()
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to build server key response", &e))
            .or_else(|_| {
                let key_id_for_sign = current_key.key_id.clone();
                let secret_key_for_sign = current_key.secret_key.clone();
                let mut response = json!({
                    "server_name": config.server_name,
                    "verify_keys": {
                        current_key.key_id: { "key": current_key.public_key }
                    },
                    "old_verify_keys": {},
                    "valid_until_ts": current_key.expires_at
                });
                if let Err(e) = crate::federation::signing::sign_json(
                    &config.server_name,
                    &key_id_for_sign,
                    &secret_key_for_sign,
                    &mut response,
                ) {
                    ::tracing::warn!("Failed to sign server keys response (fallback): {}", e);
                }
                Ok(response)
            });
    }

    let key_id = config.key_id.clone().unwrap_or_else(|| "ed25519:1".to_string());

    let verify_key = match config.signing_key.as_deref().and_then(|k| {
        let res = derive_ed25519_verify_key_base64(k);
        if res.is_none() {
            ::tracing::error!("Failed to derive verify key from signing_key: {}", k);
        }
        res
    }) {
        Some(k) => k,
        None => {
            ::tracing::error!("Federation signing key missing or invalid in config");
            return Err(ApiError::internal("Missing or invalid federation signing key".to_string()));
        }
    };

    let valid_until = chrono::Utc::now().timestamp_millis() + 3600 * 1000;

    let key_id_for_sign = key_id.clone();
    let mut response = json!({
        "server_name": config.server_name,
        "verify_keys": {
            key_id: { "key": verify_key }
        },
        "old_verify_keys": {},
        "valid_until_ts": valid_until
    });

    if let Some(secret_key) = config.signing_key.as_deref() {
        if let Err(e) =
            crate::federation::signing::sign_json(&config.server_name, &key_id_for_sign, secret_key, &mut response)
        {
            ::tracing::warn!("Failed to sign server keys response (config fallback): {}", e);
        }
    }

    Ok(response)
}

fn derive_ed25519_verify_key_base64(signing_key: &str) -> Option<String> {
    let signing_key = decode_base64_32(signing_key)?;
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key);
    let verifying_key = signing_key.verifying_key();
    Some(base64::engine::general_purpose::STANDARD_NO_PAD.encode(verifying_key.as_bytes()))
}

async fn fetch_remote_server_keys_response(
    state: &AppState,
    server_name: &str,
    key_id: &str,
) -> Result<Value, ApiError> {
    let backoff_key = format!("federation:key_fetch_backoff:{server_name}:{key_id}");
    if let Ok(Some(true)) = state.cache.get::<bool>(&backoff_key).await {
        return Err(ApiError::not_found(format!("Remote server key '{key_id}' for '{server_name}' not found")));
    }

    let cache_key = format!("federation:server_keys:{server_name}:{key_id}");
    if let Ok(Some(cached)) = state.cache.get::<Value>(&cache_key).await {
        return Ok(cached);
    }

    let _permit = state
        .federation_key_fetch_general_semaphore
        .clone()
        .acquire_owned()
        .await
        .map_err(|e| ApiError::internal_with_log("Federation key fetch semaphore closed", &e))?;

    let timeout_ms = state.services.core.config.federation.key_fetch_timeout_ms.max(1);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(timeout_ms))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| ApiError::internal_with_log("Failed to build federation HTTP client", &e))?;

    // SSRF protection: reuse the URL preview IP blacklist to block private/loopback addresses.
    let ip_blacklist = &state.services.core.config.url_preview.ip_range_blacklist;

    // HTTPS only — Matrix federation requires TLS. HTTP fallback is removed to prevent
    // MITM attacks on server key retrieval.
    let urls = [
        format!("https://{server_name}/_matrix/key/v2/server"),
        format!("https://{server_name}/_matrix/key/v2/query/{server_name}/{key_id}"),
    ];

    for url in &urls {
        // Block requests to private/loopback/link-local IPs to prevent SSRF.
        if let Err(reason) = check_url_against_blacklist(url, ip_blacklist) {
            ::tracing::warn!(server_name = %server_name, url = %url, reason = %reason, "Blocked federation key fetch to blacklisted address");
            continue;
        }

        let response = match client.get(url).send().await {
            Ok(response) if response.status().is_success() => response,
            _ => continue,
        };

        let body = match response.json::<Value>().await {
            Ok(body) => body,
            Err(_) => continue,
        };

        let Some(key) = extract_remote_verify_key(&body, server_name, key_id) else {
            continue;
        };

        // P2-15: Fully validate the remote server key response before caching.
        // Rejects mismatched server_name, missing/expired valid_until_ts,
        // empty/malformed verify_keys, and responses lacking a self-signature.
        // On failure we log a warning and skip caching (continue to the next
        // URL) rather than erroring the client request.
        let Some(valid_until_ts) = validate_server_key_response(&body, server_name) else {
            continue;
        };
        let now_ms = chrono::Utc::now().timestamp_millis();

        let canonical_response = json!({
            "server_name": body
                .get("server_name")
                .and_then(|v| v.as_str())
                .unwrap_or(server_name),
            "valid_until_ts": valid_until_ts,
            "verify_keys": {
                key_id: {
                    "key": key
                }
            },
            "old_verify_keys": body
                .get("old_verify_keys")
                .cloned()
                .unwrap_or_else(|| json!({})),
            "signatures": body
                .get("signatures")
                .cloned()
                .unwrap_or_else(|| json!({}))
        });

        // Cache TTL = min(configured_ttl, remaining lifetime of the key).
        // `valid_until_ts` is in ms; cache TTL is in seconds.
        let configured_ttl = state.services.core.config.federation.key_cache_ttl.max(60);
        let remaining_secs = ((valid_until_ts - now_ms) / 1000).max(1) as u64;
        let ttl = configured_ttl.min(remaining_secs);
        if let Err(e) = state.cache.set(&cache_key, &canonical_response, ttl).await {
            ::tracing::debug!("Failed to cache federation key response: {}", e);
        }
        return Ok(canonical_response);
    }

    if let Err(e) = state.cache.set(&backoff_key, true, 30).await {
        ::tracing::debug!("Failed to set federation backoff cache: {}", e);
    }
    Err(ApiError::not_found(format!("Remote server key '{key_id}' for '{server_name}' not found")))
}

/// Validate a federation server key response before caching or returning it.
///
/// Returns `Some(valid_until_ts)` when the response is structurally valid and
/// safe to cache, or `None` (after logging a `warn!`) when validation fails.
///
/// This is defensive validation that checks:
/// - `server_name` (if present) matches the requested server
/// - `valid_until_ts` is present, parseable, and in the future
/// - `verify_keys` is present, is an object, and is non-empty
/// - each verify key has a `key` field that is a base64-encoded 32-byte value
/// - `old_verify_keys` (if present) is an object where each entry has a
///   `key` field (base64 32-byte) and an integer `expired_ts` field
/// - `signatures` is present, is an object, and contains at least one
///   self-signature (an entry keyed by `server_name` with a non-empty object)
/// - at least one self-signature cryptographically verifies against the
///   corresponding verify key over the canonical JSON of the response with
///   `signatures` removed
fn validate_server_key_response(body: &Value, server_name: &str) -> Option<i64> {
    // `server_name` (if present) must match the requested server.
    if let Some(actual) = body.get("server_name").and_then(|v| v.as_str()) {
        if actual != server_name {
            ::tracing::warn!(
                server_name = %server_name,
                actual_server_name = %actual,
                "Remote server key response server_name mismatch; refusing to cache"
            );
            return None;
        }
    }

    // `valid_until_ts` must be present and in the future.
    let now_ms = chrono::Utc::now().timestamp_millis();
    let valid_until_ts = match body.get("valid_until_ts").and_then(|v| v.as_i64()) {
        Some(ts) => ts,
        None => {
            ::tracing::warn!(
                server_name = %server_name,
                "Remote server key response missing valid_until_ts; refusing to cache"
            );
            return None;
        }
    };
    if valid_until_ts <= now_ms {
        ::tracing::warn!(
            server_name = %server_name,
            valid_until_ts = valid_until_ts,
            now_ms = now_ms,
            "Remote server key response has expired valid_until_ts; refusing to cache"
        );
        return None;
    }

    // `verify_keys` must be present, be an object, and be non-empty.
    let verify_keys = match body.get("verify_keys").and_then(|v| v.as_object()) {
        Some(obj) if !obj.is_empty() => obj,
        _ => {
            ::tracing::warn!(
                server_name = %server_name,
                "Remote server key response missing or empty verify_keys; refusing to cache"
            );
            return None;
        }
    };

    // Each verify key must have a `key` field that is a base64-encoded 32-byte
    // Ed25519 public key.
    for (kid, entry) in verify_keys {
        let Some(key_str) = entry.get("key").and_then(|v| v.as_str()) else {
            ::tracing::warn!(
                server_name = %server_name,
                key_id = %kid,
                "Remote verify key entry missing 'key' field; refusing to cache"
            );
            return None;
        };
        if decode_base64_32(key_str).is_none() {
            ::tracing::warn!(
                server_name = %server_name,
                key_id = %kid,
                "Remote verify key has invalid base64 32-byte key; refusing to cache"
            );
            return None;
        }
    }

    // P2-15: `old_verify_keys` (if present) must be an object where each entry
    // has a `key` field (base64 32-byte) and an integer `expired_ts` field.
    if let Some(old_verify_keys) = body.get("old_verify_keys") {
        if !old_verify_keys.is_null() {
            let old_obj = match old_verify_keys.as_object() {
                Some(obj) => obj,
                None => {
                    ::tracing::warn!(
                        server_name = %server_name,
                        "Remote server key response has non-object old_verify_keys; refusing to cache"
                    );
                    return None;
                }
            };
            for (kid, entry) in old_obj {
                let Some(key_str) = entry.get("key").and_then(|v| v.as_str()) else {
                    ::tracing::warn!(
                        server_name = %server_name,
                        key_id = %kid,
                        "Remote old_verify_keys entry missing 'key' field; refusing to cache"
                    );
                    return None;
                };
                if decode_base64_32(key_str).is_none() {
                    ::tracing::warn!(
                        server_name = %server_name,
                        key_id = %kid,
                        "Remote old_verify_keys entry has invalid base64 32-byte key; refusing to cache"
                    );
                    return None;
                };
                if entry.get("expired_ts").and_then(|v| v.as_i64()).is_none() {
                    ::tracing::warn!(
                        server_name = %server_name,
                        key_id = %kid,
                        "Remote old_verify_keys entry missing or non-integer expired_ts; refusing to cache"
                    );
                    return None;
                }
            }
        }
    }

    // `signatures` must be present, be an object, and contain at least one
    // self-signature (an entry keyed by `server_name` with a non-empty object).
    let signatures = match body.get("signatures").and_then(|v| v.as_object()) {
        Some(obj) => obj,
        None => {
            ::tracing::warn!(
                server_name = %server_name,
                "Remote server key response missing signatures; refusing to cache"
            );
            return None;
        }
    };
    // P2-15: Cryptographically verify at least one self-signature against the
    // corresponding verify key over the canonical JSON of the response with
    // `signatures` removed. The `None` arm emits a diagnostic before bailing so
    // clippy does not suggest rewriting this as `?`.
    let self_sig = match signatures.get(server_name).and_then(|v| v.as_object()).filter(|sig| !sig.is_empty()) {
        Some(s) => s,
        None => {
            ::tracing::warn!(
                server_name = %server_name,
                "Remote server key response missing self-signature; refusing to cache"
            );
            return None;
        }
    };
    let mut verified = false;
    for (kid, sig_val) in self_sig {
        let Some(sig_str) = sig_val.as_str() else {
            continue;
        };
        let Some(key_str) = verify_keys.get(kid).and_then(|e| e.get("key")).and_then(|v| v.as_str()) else {
            continue;
        };
        if verify_ed25519_signature(key_str, sig_str, server_name, body) {
            verified = true;
            break;
        }
    }
    if !verified {
        ::tracing::warn!(
            server_name = %server_name,
            "Remote server key response self-signature verification failed; refusing to cache"
        );
        return None;
    }

    Some(valid_until_ts)
}

/// Verify an Ed25519 self-signature on a server key response.
///
/// Builds the canonical JSON of `body` with the `signatures` field removed,
/// then verifies the signature using the public key from `verify_keys`.
fn verify_ed25519_signature(public_key_b64: &str, signature_b64: &str, server_name: &str, body: &Value) -> bool {
    use synapse_common::canonical_json;

    // Build the canonical JSON of the response with `signatures` removed.
    let mut body_without_sigs = body.clone();
    if let Some(obj) = body_without_sigs.as_object_mut() {
        obj.remove("signatures");
    } else {
        return false;
    }

    let canonical = match canonical_json::canonical_json(&body_without_sigs) {
        Ok(s) => s,
        Err(e) => {
            ::tracing::debug!(
                server_name = %server_name,
                error = %e,
                "Failed to canonicalize server key response for signature verification"
            );
            return false;
        }
    };

    let pub_key_bytes = match base64::engine::general_purpose::STANDARD_NO_PAD.decode(public_key_b64) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };
    let pub_key_array: [u8; 32] = match pub_key_bytes.as_slice().try_into() {
        Ok(arr) => arr,
        Err(_) => return false,
    };
    let verifying_key = match ed25519_dalek::VerifyingKey::from_bytes(&pub_key_array) {
        Ok(key) => key,
        Err(_) => return false,
    };

    let sig_bytes = match base64::engine::general_purpose::STANDARD.decode(signature_b64) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };
    let signature = match ed25519_dalek::Signature::from_slice(&sig_bytes) {
        Ok(sig) => sig,
        Err(_) => return false,
    };

    verifying_key.verify_strict(canonical.as_bytes(), &signature).is_ok()
}

fn extract_remote_verify_key(body: &Value, server_name: &str, key_id: &str) -> Option<String> {
    if let Some(key) = extract_remote_verify_key_from_object(body, key_id) {
        return Some(key);
    }

    let server_keys = body.get("server_keys")?.as_array()?;
    for entry in server_keys {
        if entry.get("server_name").and_then(|value| value.as_str()).is_some_and(|value| value != server_name) {
            continue;
        }

        if let Some(key) = extract_remote_verify_key_from_object(entry, key_id) {
            return Some(key);
        }
    }

    None
}

fn extract_remote_verify_key_from_object(body: &Value, key_id: &str) -> Option<String> {
    let verify_keys = body.get("verify_keys")?.as_object()?;
    let entry = verify_keys.get(key_id)?;
    entry.get("key")?.as_str().map(str::to_string)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use rand::RngCore;
    use serde_json::json;

    /// Generate an Ed25519 keypair and return (key_id, public_key_b64, signing_key).
    fn gen_keypair() -> (String, String, SigningKey) {
        let mut rng = rand::rng();
        let mut secret_bytes = [0u8; 32];
        rng.fill_bytes(&mut secret_bytes);
        let signing_key = SigningKey::from_bytes(&secret_bytes);
        let pub_key_b64 =
            base64::engine::general_purpose::STANDARD_NO_PAD.encode(signing_key.verifying_key().to_bytes());
        let key_id = format!("ed25519:{}", &pub_key_b64[..8]);
        (key_id, pub_key_b64, signing_key)
    }

    /// Build a server key response body and sign it with the given signing key.
    fn sign_response(
        server_name: &str,
        key_id: &str,
        pub_key_b64: &str,
        signing_key: &SigningKey,
        valid_until_ts: i64,
    ) -> Value {
        let mut body = json!({
            "server_name": server_name,
            "valid_until_ts": valid_until_ts,
            "verify_keys": {
                key_id: { "key": pub_key_b64 }
            },
            "old_verify_keys": {}
        });

        // Sign: canonical JSON of body without signatures, then add signatures.
        let canonical = synapse_common::canonical_json::canonical_json(&body).unwrap();
        let sig = signing_key.sign(canonical.as_bytes());
        let sig_b64 = base64::engine::general_purpose::STANDARD.encode(sig.to_bytes());

        if let Some(obj) = body.as_object_mut() {
            obj.insert("signatures".to_string(), json!({ server_name: { key_id: sig_b64 } }));
        }
        body
    }

    /// Re-sign an existing response body after modifications.
    fn resign_response(body: &Value, server_name: &str, key_id: &str, signing_key: &SigningKey) -> Value {
        let mut body = body.clone();
        if let Some(obj) = body.as_object_mut() {
            obj.remove("signatures");
        }
        let canonical = synapse_common::canonical_json::canonical_json(&body).unwrap();
        let sig = signing_key.sign(canonical.as_bytes());
        let sig_b64 = base64::engine::general_purpose::STANDARD.encode(sig.to_bytes());
        if let Some(obj) = body.as_object_mut() {
            obj.insert("signatures".to_string(), json!({ server_name: { key_id: sig_b64 } }));
        }
        body
    }

    fn future_ts() -> i64 {
        chrono::Utc::now().timestamp_millis() + 86_400_000
    }

    #[test]
    fn test_validate_accepts_well_formed_response() {
        let (key_id, pub_key_b64, signing_key) = gen_keypair();
        let body = sign_response("example.com", &key_id, &pub_key_b64, &signing_key, future_ts());
        let result = validate_server_key_response(&body, "example.com");
        assert!(result.is_some());
    }

    #[test]
    fn test_validate_rejects_server_name_mismatch() {
        let (key_id, pub_key_b64, signing_key) = gen_keypair();
        let body = sign_response("example.com", &key_id, &pub_key_b64, &signing_key, future_ts());
        assert!(validate_server_key_response(&body, "other.com").is_none());
    }

    #[test]
    fn test_validate_rejects_missing_valid_until_ts() {
        let (key_id, pub_key_b64, signing_key) = gen_keypair();
        let mut body = sign_response("example.com", &key_id, &pub_key_b64, &signing_key, future_ts());
        if let Some(obj) = body.as_object_mut() {
            obj.remove("valid_until_ts");
        }
        let body = resign_response(&body, "example.com", &key_id, &signing_key);
        assert!(validate_server_key_response(&body, "example.com").is_none());
    }

    #[test]
    fn test_validate_rejects_expired_valid_until_ts() {
        let (key_id, pub_key_b64, signing_key) = gen_keypair();
        let past_ts = chrono::Utc::now().timestamp_millis() - 1000;
        let body = sign_response("example.com", &key_id, &pub_key_b64, &signing_key, past_ts);
        assert!(validate_server_key_response(&body, "example.com").is_none());
    }

    #[test]
    fn test_validate_rejects_empty_verify_keys() {
        let (key_id, pub_key_b64, signing_key) = gen_keypair();
        let mut body = sign_response("example.com", &key_id, &pub_key_b64, &signing_key, future_ts());
        if let Some(obj) = body.as_object_mut() {
            obj.insert("verify_keys".to_string(), json!({}));
        }
        let body = resign_response(&body, "example.com", &key_id, &signing_key);
        assert!(validate_server_key_response(&body, "example.com").is_none());
    }

    #[test]
    fn test_validate_rejects_verify_key_missing_key_field() {
        let (key_id, pub_key_b64, signing_key) = gen_keypair();
        let mut body = sign_response("example.com", &key_id, &pub_key_b64, &signing_key, future_ts());
        if let Some(obj) = body.as_object_mut() {
            obj.insert("verify_keys".to_string(), json!({ "ed25519:bad": { "not_key": "x" } }));
        }
        let body = resign_response(&body, "example.com", &key_id, &signing_key);
        assert!(validate_server_key_response(&body, "example.com").is_none());
    }

    #[test]
    fn test_validate_rejects_verify_key_invalid_base64() {
        let (key_id, pub_key_b64, signing_key) = gen_keypair();
        let mut body = sign_response("example.com", &key_id, &pub_key_b64, &signing_key, future_ts());
        if let Some(obj) = body.as_object_mut() {
            obj.insert("verify_keys".to_string(), json!({ "ed25519:bad": { "key": "not_base64!!!" } }));
        }
        let body = resign_response(&body, "example.com", &key_id, &signing_key);
        assert!(validate_server_key_response(&body, "example.com").is_none());
    }

    #[test]
    fn test_validate_accepts_null_old_verify_keys() {
        let (key_id, pub_key_b64, signing_key) = gen_keypair();
        let mut body = sign_response("example.com", &key_id, &pub_key_b64, &signing_key, future_ts());
        if let Some(obj) = body.as_object_mut() {
            obj.insert("old_verify_keys".to_string(), Value::Null);
        }
        let body = resign_response(&body, "example.com", &key_id, &signing_key);
        assert!(validate_server_key_response(&body, "example.com").is_some());
    }

    #[test]
    fn test_validate_accepts_missing_old_verify_keys() {
        let (key_id, pub_key_b64, signing_key) = gen_keypair();
        let mut body = sign_response("example.com", &key_id, &pub_key_b64, &signing_key, future_ts());
        if let Some(obj) = body.as_object_mut() {
            obj.remove("old_verify_keys");
        }
        let body = resign_response(&body, "example.com", &key_id, &signing_key);
        assert!(validate_server_key_response(&body, "example.com").is_some());
    }

    #[test]
    fn test_validate_accepts_well_formed_old_verify_keys() {
        let (key_id, pub_key_b64, signing_key) = gen_keypair();
        let (old_key_id, old_pub_key_b64, _) = gen_keypair();
        let mut body = sign_response("example.com", &key_id, &pub_key_b64, &signing_key, future_ts());
        if let Some(obj) = body.as_object_mut() {
            obj.insert(
                "old_verify_keys".to_string(),
                json!({
                    old_key_id: {
                        "key": old_pub_key_b64,
                        "expired_ts": chrono::Utc::now().timestamp_millis() - 1000
                    }
                }),
            );
        }
        let body = resign_response(&body, "example.com", &key_id, &signing_key);
        assert!(validate_server_key_response(&body, "example.com").is_some());
    }

    #[test]
    fn test_validate_rejects_old_verify_keys_non_object() {
        let (key_id, pub_key_b64, signing_key) = gen_keypair();
        let mut body = sign_response("example.com", &key_id, &pub_key_b64, &signing_key, future_ts());
        if let Some(obj) = body.as_object_mut() {
            obj.insert("old_verify_keys".to_string(), json!("not_an_object"));
        }
        let body = resign_response(&body, "example.com", &key_id, &signing_key);
        assert!(validate_server_key_response(&body, "example.com").is_none());
    }

    #[test]
    fn test_validate_rejects_old_verify_keys_missing_key_field() {
        let (key_id, pub_key_b64, signing_key) = gen_keypair();
        let mut body = sign_response("example.com", &key_id, &pub_key_b64, &signing_key, future_ts());
        if let Some(obj) = body.as_object_mut() {
            obj.insert("old_verify_keys".to_string(), json!({ "ed25519:old": { "expired_ts": 123 } }));
        }
        let body = resign_response(&body, "example.com", &key_id, &signing_key);
        assert!(validate_server_key_response(&body, "example.com").is_none());
    }

    #[test]
    fn test_validate_rejects_old_verify_keys_invalid_base64() {
        let (key_id, pub_key_b64, signing_key) = gen_keypair();
        let mut body = sign_response("example.com", &key_id, &pub_key_b64, &signing_key, future_ts());
        if let Some(obj) = body.as_object_mut() {
            obj.insert(
                "old_verify_keys".to_string(),
                json!({ "ed25519:old": { "key": "too_short", "expired_ts": 123 } }),
            );
        }
        let body = resign_response(&body, "example.com", &key_id, &signing_key);
        assert!(validate_server_key_response(&body, "example.com").is_none());
    }

    #[test]
    fn test_validate_rejects_old_verify_keys_missing_expired_ts() {
        let (key_id, pub_key_b64, signing_key) = gen_keypair();
        let (_, old_pub_key_b64, _) = gen_keypair();
        let mut body = sign_response("example.com", &key_id, &pub_key_b64, &signing_key, future_ts());
        if let Some(obj) = body.as_object_mut() {
            obj.insert("old_verify_keys".to_string(), json!({ "ed25519:old": { "key": old_pub_key_b64 } }));
        }
        let body = resign_response(&body, "example.com", &key_id, &signing_key);
        assert!(validate_server_key_response(&body, "example.com").is_none());
    }

    #[test]
    fn test_validate_rejects_missing_signatures() {
        let (key_id, pub_key_b64, signing_key) = gen_keypair();
        let mut body = sign_response("example.com", &key_id, &pub_key_b64, &signing_key, future_ts());
        if let Some(obj) = body.as_object_mut() {
            obj.remove("signatures");
        }
        assert!(validate_server_key_response(&body, "example.com").is_none());
    }

    #[test]
    fn test_validate_rejects_missing_self_signature() {
        let (key_id, pub_key_b64, signing_key) = gen_keypair();
        let mut body = sign_response("example.com", &key_id, &pub_key_b64, &signing_key, future_ts());
        // Replace self-signature with a signature from a different server.
        if let Some(obj) = body.as_object_mut() {
            if let Some(sigs) = obj.get_mut("signatures").and_then(|v| v.as_object_mut()) {
                sigs.remove("example.com");
                sigs.insert("other.com".to_string(), json!({}));
            }
        }
        assert!(validate_server_key_response(&body, "example.com").is_none());
    }

    #[test]
    fn test_validate_rejects_empty_self_signature() {
        let (key_id, pub_key_b64, signing_key) = gen_keypair();
        let mut body = sign_response("example.com", &key_id, &pub_key_b64, &signing_key, future_ts());
        if let Some(obj) = body.as_object_mut() {
            if let Some(sigs) = obj.get_mut("signatures").and_then(|v| v.as_object_mut()) {
                sigs.insert("example.com".to_string(), json!({}));
            }
        }
        assert!(validate_server_key_response(&body, "example.com").is_none());
    }

    #[test]
    fn test_validate_rejects_tampered_response() {
        let (key_id, pub_key_b64, signing_key) = gen_keypair();
        let mut body = sign_response("example.com", &key_id, &pub_key_b64, &signing_key, future_ts());
        // Tamper with valid_until_ts after signing — signature won't match.
        if let Some(obj) = body.as_object_mut() {
            obj.insert("valid_until_ts".to_string(), json!(future_ts() + 999999));
        }
        assert!(validate_server_key_response(&body, "example.com").is_none());
    }

    #[test]
    fn test_validate_rejects_tampered_old_verify_keys_chain() {
        let (key_id, pub_key_b64, signing_key) = gen_keypair();
        let (old_key_id, old_pub_key_b64, _) = gen_keypair();
        let mut body = sign_response("example.com", &key_id, &pub_key_b64, &signing_key, future_ts());
        if let Some(obj) = body.as_object_mut() {
            obj.insert(
                "old_verify_keys".to_string(),
                json!({
                    old_key_id.clone(): {
                        "key": old_pub_key_b64,
                        "expired_ts": chrono::Utc::now().timestamp_millis() - 1000
                    }
                }),
            );
        }
        let mut body = resign_response(&body, "example.com", &key_id, &signing_key);
        if let Some(obj) = body.as_object_mut() {
            obj.insert(
                "old_verify_keys".to_string(),
                json!({
                    old_key_id: {
                        "key": pub_key_b64,
                        "expired_ts": chrono::Utc::now().timestamp_millis() - 1000
                    }
                }),
            );
        }
        assert!(validate_server_key_response(&body, "example.com").is_none());
    }

    #[test]
    fn test_validate_rejects_wrong_signature_key() {
        let (key_id, pub_key_b64, signing_key) = gen_keypair();
        let (_, _other_pub_key_b64, other_signing_key) = gen_keypair();
        let mut body = sign_response("example.com", &key_id, &pub_key_b64, &signing_key, future_ts());
        // Replace the signature with one from a different key.
        let canonical = {
            let mut b = body.clone();
            if let Some(obj) = b.as_object_mut() {
                obj.remove("signatures");
            }
            synapse_common::canonical_json::canonical_json(&b).unwrap()
        };
        let sig = other_signing_key.sign(canonical.as_bytes());
        let sig_b64 = base64::engine::general_purpose::STANDARD.encode(sig.to_bytes());
        if let Some(obj) = body.as_object_mut() {
            obj.insert("signatures".to_string(), json!({ "example.com": { key_id: sig_b64 } }));
        }
        // The signature won't verify against the original public key.
        assert!(validate_server_key_response(&body, "example.com").is_none());
    }

    #[test]
    fn test_validate_accepts_multiple_verify_keys() {
        let (key_id1, pub_key_b641, signing_key1) = gen_keypair();
        let (key_id2, pub_key_b642, _) = gen_keypair();
        let mut body = sign_response("example.com", &key_id1, &pub_key_b641, &signing_key1, future_ts());
        // Add a second verify key (unsigned, but the first key's signature should verify).
        if let Some(obj) = body.as_object_mut() {
            if let Some(vk) = obj.get_mut("verify_keys").and_then(|v| v.as_object_mut()) {
                vk.insert(key_id2, json!({ "key": pub_key_b642 }));
            }
        }
        let body = resign_response(&body, "example.com", &key_id1, &signing_key1);
        assert!(validate_server_key_response(&body, "example.com").is_some());
    }

    #[test]
    fn test_validate_rejects_signature_with_wrong_key_id() {
        let (key_id, pub_key_b64, signing_key) = gen_keypair();
        let (other_key_id, _, _) = gen_keypair();
        let mut body = sign_response("example.com", &key_id, &pub_key_b64, &signing_key, future_ts());
        // Rename the signature key to a non-existent key_id.
        if let Some(obj) = body.as_object_mut() {
            if let Some(sigs) = obj.get_mut("signatures").and_then(|v| v.as_object_mut()) {
                if let Some(self_sigs) = sigs.get_mut("example.com").and_then(|v| v.as_object_mut()) {
                    if let Some(sig_val) = self_sigs.remove(&key_id) {
                        self_sigs.insert(other_key_id, sig_val);
                    }
                }
            }
        }
        // The signature key_id doesn't match any verify_keys entry.
        assert!(validate_server_key_response(&body, "example.com").is_none());
    }
}
