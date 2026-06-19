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
        let requested_users = one_time_keys.keys().cloned().collect::<Vec<_>>();
        let mut allowed_local_users = std::collections::HashSet::new();

        for user_id in requested_users {
            if !super::user_matches_origin(&user_id, &state.services.core.server_name) {
                continue;
            }

            if super::validate_federation_origin_shares_user_room(&state, &user_id, &auth.origin).await.is_ok() {
                allowed_local_users.insert(user_id);
            }
        }

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
        let requested_users = device_keys.keys().cloned().collect::<Vec<_>>();
        let mut allowed_local_users = std::collections::HashSet::new();

        for user_id in requested_users {
            if !super::user_matches_origin(&user_id, &state.services.core.server_name) {
                continue;
            }

            if super::validate_federation_origin_shares_user_room(&state, &user_id, &auth.origin).await.is_ok() {
                allowed_local_users.insert(user_id);
            }
        }

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

        // P2-16: Validate `valid_until_ts` before caching. A missing or
        // already-expired value lets an attacker inject stale but
        // correctly-signed key responses. Reject both cases.
        let now_ms = chrono::Utc::now().timestamp_millis();
        let Some(valid_until_ts) = body.get("valid_until_ts").and_then(|v| v.as_i64()) else {
            ::tracing::warn!(
                server_name = %server_name,
                key_id = %key_id,
                "Remote server key response missing valid_until_ts; refusing to cache"
            );
            continue;
        };
        if valid_until_ts <= now_ms {
            ::tracing::warn!(
                server_name = %server_name,
                key_id = %key_id,
                valid_until_ts = valid_until_ts,
                now_ms = now_ms,
                "Remote server key response has expired valid_until_ts; refusing to cache"
            );
            continue;
        }

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
