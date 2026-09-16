use crate::routes::context::{CoreContext, FederationContext};
use crate::utils::encoding::decode_base64_32;
use axum::extract::State;
use axum::http::Request;
use axum::response::IntoResponse;
use axum::{body::Body, middleware::Next, response::Response};
use base64::Engine;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use synapse_common::check_url_and_resolve;
use synapse_common::current_timestamp_millis;
use synapse_common::ApiError;
use tokio::sync::Semaphore;

/// F-01: Mirror the 7-day server-key validity cap from Matrix SS API spec §1.2.
/// We use this when deciding the cache TTL for keys fetched from peers.
/// "Servers MUST publish a `valid_until_ts` no more than 7 days in the future."
const MAX_SERVER_KEY_VALIDITY_SECS: u64 = 7 * 24 * 60 * 60;

/// Default TTL for caching federation verify keys (1 hour).
const FEDERATION_KEY_CACHE_TTL_SECS: u64 = 3600;

/// Effective cache TTL for a federation verify key (F-01).
///
/// The TTL is the minimum of:
///   - [`FEDERATION_KEY_CACHE_TTL_SECS`] (1 hour, our refresh budget)
///   - `valid_until_ts - now` clipped to [`MAX_SERVER_KEY_VALIDITY_SECS`] (spec §1.2)
///
/// When `valid_until_ts` is `None` (malformed peer response or local key
/// without an expiry), we fall back to the default 1-hour budget so the
/// caller still gets a finite, conservative TTL.
fn compute_key_cache_ttl_secs(valid_until_ts: Option<i64>) -> u64 {
    let now_ms = current_timestamp_millis();
    let peer_secs_remaining = valid_until_ts.map_or(u64::MAX, |ts| ((ts - now_ms) / 1000).max(0) as u64);
    let spec_capped = peer_secs_remaining.min(MAX_SERVER_KEY_VALIDITY_SECS);
    FEDERATION_KEY_CACHE_TTL_SECS.min(spec_capped)
}

/// The `FederationRequestAuth` struct.
#[derive(Clone, Debug)]
pub struct FederationRequestAuth {
    /// The `origin` field.
    pub origin: String,
    /// The `key_id` field.
    pub key_id: String,
}

/// See [`federation_auth_middleware`].
pub async fn federation_auth_middleware(
    State(ctx): State<FederationContext>,
    request: Request<Body>,
    next: Next,
) -> Response {
    if !ctx.config.federation.enabled || !ctx.config.federation.allow_ingress {
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

    // S6: Validate origin format before any further processing.
    // Reject malformed origins (empty, too long, invalid characters) early.
    if let Err(reason) = synapse_common::security::SecurityValidator::validate_origin(&params.origin) {
        ::tracing::warn!(
            target: "security_audit",
            event = "federation_invalid_origin",
            origin = %params.origin,
            reason = %reason,
            "Federation request rejected: invalid origin format"
        );
        return ApiError::unauthorized("Invalid federation origin".to_string()).into_response();
    }

    if let Some(ref dest) = params.destination {
        if !is_local_federation_destination(&ctx, dest) {
            ::tracing::warn!(
                target: "security_audit",
                event = "federation_destination_mismatch",
                claimed_destination = dest,
                local_server = ctx.server_name,
                origin = params.origin,
                "Federation request destination does not match local server - possible replay attack"
            );
            return ApiError::unauthorized("Federation request destination does not match this server".to_string())
                .into_response();
        }
    }

    let destination = ctx.server_name.as_str();

    let body_limit = ctx.config.federation.max_transaction_payload.max(64 * 1024) as usize;

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
        &ctx,
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
            ctx.server_name,
            e
        );
        return ApiError::unauthorized("Invalid federation signature".to_string()).into_response();
    }

    // S1 修复：签名时间戳校验。X-Matrix 头中的 `ts` 参数指示签名时间，
    // 超出容差窗口的请求必须被拒绝，防止合法签名请求被无限重放。
    // N2：Matrix 规范中 ts 为可选参数，但为了审计可见性，当 replay_protection
    // 启用而 ts 缺失时，记录 warn 日志（Time-of-check 降级为仅靠 replay cache）。
    if let Some(ts) = params.ts {
        let tolerance_ms = ctx.config.federation.signing_ts_tolerance_ms;
        if tolerance_ms > 0 {
            if let Err(reason) =
                synapse_common::security::SecurityValidator::validate_federation_timestamp(ts, tolerance_ms)
            {
                tracing::warn!(
                    target: "security_audit",
                    event = "federation_timestamp_rejected",
                    origin = %params.origin,
                    ts = ts,
                    tolerance_ms = tolerance_ms,
                    reason = %reason,
                    "Federation request rejected: signature timestamp out of tolerance"
                );
                return ApiError::unauthorized("Federation signature timestamp out of tolerance".to_string())
                    .into_response();
            }
        }
    } else if ctx.config.federation.replay_protection_enabled {
        tracing::warn!(
            target: "security_audit",
            event = "federation_timestamp_missing",
            origin = %params.origin,
            "Federation request is missing X-Matrix `ts` parameter; timestamp check skipped, relying solely on replay-protection cache"
        );
    }

    // S1 修复：重放保护。验签通过后，将签名哈希记入 ReplayProtectionCache，
    // 窗口内重复提交同一签名即被拒绝。使用 security::compute_signature_hash
    // 计算（含 origin + key_id + signature + signed_bytes 四元组）。
    if ctx.config.federation.replay_protection_enabled {
        let sig_hash =
            synapse_common::security::compute_signature_hash(&params.origin, &params.key, &params.sig, &signed_bytes);
        if !ctx.replay_protection_cache.check_and_record(&sig_hash) {
            tracing::warn!(
                target: "security_audit",
                event = "federation_replay_detected",
                origin = %params.origin,
                "Federation request rejected: signature replay detected within protection window"
            );
            return ApiError::unauthorized("Federation request replay detected".to_string()).into_response();
        }
    }

    let origin_server = &params.origin;

    if ctx.config.federation.admission_mode {
        match ctx.admin_federation_service.check_admission(origin_server).await {
            Ok(Some(status)) if status != "active" => {
                tracing::warn!("Federation request rejected from server '{}' with status '{}'", origin_server, status);
                return ApiError::forbidden(format!(
                    "Server '{origin_server}' is not authorized for federation (status: {status})"
                ))
                .into_response();
            }
            Ok(None) => {
                return ApiError::forbidden(format!(
                    "Server '{origin_server}' is pending federation admission approval"
                ))
                .into_response();
            }
            Ok(_) => {}
            Err(e) => {
                tracing::warn!("Federation admission check failed for server '{}': {}", origin_server, e);
                return ApiError::internal("Federation admission check failed".to_string()).into_response();
            }
        }
    }

    let mut parts = parts;
    parts.extensions.insert(FederationRequestAuth { origin: params.origin, key_id: params.key });

    let request = Request::from_parts(parts, Body::from(body_bytes));
    next.run(request).await
}

fn is_local_federation_destination(ctx: &FederationContext, destination: &str) -> bool {
    let server_config = &ctx.config.server;
    [
        ctx.server_name.as_str(),
        server_config.name.as_str(),
        server_config.get_server_name(),
        ctx.config.federation.server_name.as_str(),
    ]
    .into_iter()
    .any(|local_name| !local_name.is_empty() && local_name == destination)
}

/// See [`replication_http_auth_middleware`].
pub async fn replication_http_auth_middleware(
    State(ctx): State<CoreContext>,
    request: Request<Body>,
    next: Next,
) -> Response {
    if !ctx.config.worker.replication.http.enabled {
        return next.run(request).await;
    }
    let secret = if let Some(s) = &ctx.config.worker.replication.http.secret {
        s.clone()
    } else if let Some(p) = &ctx.config.worker.replication.http.secret_path {
        match fs::read_to_string(PathBuf::from(p)) {
            Ok(s) => s.trim().to_string(),
            Err(_) => return ApiError::unauthorized("Replication secret not available".to_string()).into_response(),
        }
    } else {
        return ApiError::unauthorized("Replication secret not configured".to_string()).into_response();
    };
    let token = request.headers().get("x-synapse-worker-secret").and_then(|h| h.to_str().ok()).unwrap_or_default();
    if !synapse_common::crypto::secure_compare(token, &secret) {
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
    /// S1 修复：X-Matrix Authorization 头中的 `ts` 参数（签名时间戳，毫秒）。
    ts: Option<i64>,
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
    let mut ts: Option<i64> = None;

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
            // S1 修复：解析 X-Matrix 头中的 ts 参数（签名时间戳）
            "ts" => ts = v.parse::<i64>().ok(),
            _ => {}
        }
    }

    Some(XMatrixAuthParams { origin: origin?, key: key?, sig: sig?, destination, ts })
}

fn canonical_federation_request_bytes(
    method: &str,
    uri: &str,
    origin: &str,
    destination: &str,
    content: Option<&Value>,
) -> Vec<u8> {
    match crate::federation::signing::canonical_federation_request_bytes(method, uri, origin, destination, content) {
        Ok(result) => {
            tracing::debug!("Canonical request bytes: {}", String::from_utf8_lossy(&result));
            result
        }
        Err(e) => {
            tracing::warn!("Canonical JSON error for federation request: {e}");
            Vec::new()
        }
    }
}

/// See [`verify_federation_signature_with_cache`].
pub(crate) async fn verify_federation_signature_with_cache(
    ctx: &FederationContext,
    origin: &str,
    key_id: &str,
    signature: &str,
    signed_bytes: &[u8],
    key_fetch_priority: bool,
) -> Result<(), ApiError> {
    use synapse_cache::CacheEntryKey;

    // S5 修复：缓存键纳入签名本身。此前 cache_key 仅哈希 signed_bytes（不含
    // signature），同内容换任意签名即命中已验证缓存直接放行。现在使用
    // compute_signature_hash（含 origin + key_id + signature + signed_bytes
    // 四元组）作为 content_hash，确保不同签名产生不同缓存键。
    let content_hash = synapse_common::security::compute_signature_hash(origin, key_id, signature, signed_bytes);
    let cache_key = CacheEntryKey::new(origin, key_id, &content_hash);

    if let Some(entry) = ctx.federation_signature_cache.get_signature(&cache_key) {
        if !entry.is_expired() {
            tracing::debug!("Signature cache hit for {}:{}", origin, key_id);
            if entry.verified {
                return Ok(());
            }
            return Err(ApiError::unauthorized("Cached signature verification failed".to_string()));
        }
    }

    let result = verify_federation_signature(ctx, origin, key_id, signature, signed_bytes, key_fetch_priority).await;

    // S5 修复：只缓存验证通过的结果，不缓存失败。此前失败结果也被缓存，
    // 攻击者先发坏签名请求可使后续合法请求在 TTL 内被负缓存拒绝（DoS）。
    if result.is_ok() {
        ctx.federation_signature_cache.set_signature(&cache_key, true);
    }

    result
}

#[cfg(test)]
fn compute_signature_content_hash(content: &[u8]) -> String {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    hasher.update(content);
    let result = hasher.finalize();
    base64::engine::general_purpose::STANDARD_NO_PAD.encode(result)
}

async fn verify_federation_signature(
    ctx: &FederationContext,
    origin: &str,
    key_id: &str,
    signature: &str,
    signed_bytes: &[u8],
    key_fetch_priority: bool,
) -> Result<(), ApiError> {
    let public_key = get_federation_verify_key(ctx, origin, key_id, key_fetch_priority).await?;

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
    ctx: &FederationContext,
    origin: &str,
    key_id: &str,
    key_fetch_priority: bool,
) -> Result<[u8; 32], ApiError> {
    let cache_key = format!("federation:verify_key:{origin}:{key_id}");
    if let Ok(Some(cached)) = ctx.cache.get::<String>(&cache_key).await {
        if let Ok(key) = decode_ed25519_public_key(&cached) {
            return Ok(key);
        }
    }

    if origin == ctx.server_name || origin == ctx.config.federation.server_name {
        if let Some(key) = get_local_verify_key(ctx, key_id).await {
            let key_str = base64::engine::general_purpose::STANDARD_NO_PAD.encode(key);
            let ttl = FEDERATION_KEY_CACHE_TTL_SECS;
            if let Err(e) = ctx.cache.set(&cache_key, &key_str, ttl).await {
                tracing::warn!(origin = %origin, key_id = %key_id, "Failed to cache local federation verify key: {e}");
            }
            return Ok(key);
        }
    }

    let (fetched, valid_until_ts) = fetch_federation_verify_key(ctx, origin, key_id, key_fetch_priority).await?;
    let ttl = compute_key_cache_ttl_secs(valid_until_ts);
    if let Err(e) = ctx.cache.set(&cache_key, &fetched, ttl).await {
        tracing::warn!(origin = %origin, key_id = %key_id, "Failed to cache fetched federation verify key: {e}");
    }
    decode_ed25519_public_key(&fetched).map_err(|_| ApiError::unauthorized("Invalid public key".to_string()))
}

async fn get_local_verify_key(ctx: &FederationContext, key_id: &str) -> Option<[u8; 32]> {
    let config = &ctx.config.federation;

    if !config.enabled {
        return None;
    }

    let config_key_id = config.key_id.as_deref().unwrap_or("ed25519:1");
    if key_id != config_key_id {
        if ctx.key_rotation_manager.load_or_create_key().await.is_err() {
            return None;
        }

        let current_key = match ctx.key_rotation_manager.get_current_key().await {
            Ok(Some(key)) => key,
            Ok(None) => {
                ::tracing::error!("No current federation signing key available");
                return None;
            }
            Err(e) => {
                ::tracing::error!(error = %e, "Failed to fetch current federation signing key from DB");
                return None;
            }
        };

        if current_key.key_id != key_id {
            return None;
        }

        return match decode_ed25519_public_key(&current_key.public_key) {
            Ok(key) => Some(key),
            Err(_) => {
                ::tracing::warn!(
                    key_id = %key_id,
                    "Failed to decode stored federation signing key — federation signature verification will fail for this key"
                );
                None
            }
        };
    }

    if let Some(signing_key) = config.signing_key.as_deref() {
        let signing_key_bytes = decode_base64_32(signing_key)?;
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_bytes);
        let verifying_key = signing_key.verifying_key();
        return Some(*verifying_key.as_bytes());
    }

    if ctx.key_rotation_manager.load_or_create_key().await.is_err() {
        return None;
    }

    let current_key = ctx.key_rotation_manager.get_current_key().await.ok().flatten()?;

    if current_key.key_id != key_id {
        return None;
    }

    match decode_ed25519_public_key(&current_key.public_key) {
        Ok(key) => Some(key),
        Err(_) => {
            ::tracing::warn!(
                key_id = %key_id,
                "Failed to decode stored federation signing key — federation signature verification will fail for this key"
            );
            None
        }
    }
}

async fn fetch_federation_verify_key(
    ctx: &FederationContext,
    origin: &str,
    key_id: &str,
    key_fetch_priority: bool,
) -> Result<(String, Option<i64>), ApiError> {
    let backoff_key = format!("federation:key_fetch_backoff:{origin}:{key_id}");
    if let Ok(Some(true)) = ctx.cache.get::<bool>(&backoff_key).await {
        return Err(ApiError::unauthorized("Public key not found".to_string()));
    }

    let semaphore: &Arc<Semaphore> = if key_fetch_priority {
        &ctx.federation_key_fetch_priority_semaphore
    } else {
        &ctx.federation_key_fetch_general_semaphore
    };
    let _permit = semaphore
        .clone()
        .acquire_owned()
        .await
        .map_err(|e| ApiError::internal_with_cause("Rate limit semaphore closed", e))?;

    let timeout_ms = ctx.config.federation.key_fetch_timeout_ms.max(1);
    // S2 修复: 不再使用进程级共享 client（会在连接时重新解析 DNS，存在
    // TOCTOU 风险）。改为对每个 URL 使用 pinned_client_for_url 钉扎到
    // check_url_and_resolve 返回的已验证 IP，杜绝 DNS rebinding 攻击。
    // F-1/E-1: no-redirect policy is preserved via pinned_client_for_url's
    // no_redirect parameter.

    // SSRF protection: reuse the URL preview IP blacklist to block private/loopback addresses.
    // E-2: `allow_http_key_fetch` controls only the HTTP scheme; SSRF protection
    // is independently controlled by `skip_ssrf_check` (both default false).
    let allow_http = ctx.config.federation.allow_http_key_fetch;
    let skip_ssrf = ctx.config.federation.skip_ssrf_check;
    let ip_blacklist = if skip_ssrf { &[][..] } else { &ctx.config.url_preview.ip_range_blacklist };

    let scheme = if allow_http { "http" } else { "https" };
    let urls = [
        format!("{scheme}://{origin}/_matrix/key/v2/server"),
        format!("{scheme}://{origin}/_matrix/key/v2/query/{origin}/{key_id}"),
    ];

    for url in &urls {
        // S2: check_url_and_resolve 返回 (host, verified_ips)；
        // pinned_client_for_url 用已验证 IP 钉扎 HTTP client，杜绝 DNS 重绑定。
        let (_host, verified_ips) = match check_url_and_resolve(url, ip_blacklist) {
            Ok(result) => result,
            Err(reason) => {
                tracing::warn!(
                    origin = %origin,
                    url = %url,
                    reason = %reason,
                    "Blocked federation key fetch to blacklisted address"
                );
                continue;
            }
        };

        let pinned_client = match synapse_common::http_client::pinned_client_for_url(
            url,
            &verified_ips,
            std::time::Duration::from_millis(timeout_ms),
            true, // no_redirect — preserve SSRF protection
        ) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(
                    origin = %origin,
                    url = %url,
                    error = %e,
                    "Failed to build pinned client for federation key fetch"
                );
                continue;
            }
        };

        let resp = match pinned_client.get(url).send().await {
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
        if let Some((key, valid_until_ts)) = extract_verify_key_from_server_keys(&json, origin, key_id) {
            if verify_server_keys_signature(&json, origin, key_id, &key) {
                return Ok((key, valid_until_ts));
            }
            tracing::warn!("Server keys signature verification failed for {} key_id={}", origin, key_id);
        }
    }

    if let Err(e) = ctx.cache.set(&backoff_key, true, 30).await {
        tracing::warn!(origin = %origin, key_id = %key_id, "Failed to set federation key backoff marker: {e}");
    }
    Err(ApiError::unauthorized("Public key not found".to_string()))
}

fn extract_verify_key_from_server_keys(body: &Value, origin: &str, key_id: &str) -> Option<(String, Option<i64>)> {
    if let Some(result) = extract_verify_key_from_server_keys_object(body, key_id) {
        return Some(result);
    }

    let server_keys = body.get("server_keys")?.as_array()?;
    for entry in server_keys {
        if entry.get("server_name").and_then(|v| v.as_str()).is_some_and(|v| v != origin) {
            continue;
        }

        if let Some(result) = extract_verify_key_from_server_keys_object(entry, key_id) {
            return Some(result);
        }
    }

    None
}

fn extract_verify_key_from_server_keys_object(body: &Value, key_id: &str) -> Option<(String, Option<i64>)> {
    let verify_keys = body.get("verify_keys")?.as_object()?;
    let entry = verify_keys.get(key_id)?;
    let key = entry.get("key").and_then(|v| v.as_str())?.to_string();
    // F-01: also extract valid_until_ts so the caller can compute a TTL that
    // respects the server-key validity window (spec §1.2, capped at 7 days).
    let valid_until_ts = body.get("valid_until_ts").and_then(|v| v.as_i64());
    Some((key, valid_until_ts))
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
    let canonical = match synapse_common::canonical_json(&unsigned) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("Canonical JSON error during signature verification: {e}");
            return false;
        }
    };

    verifying_key.verify_strict(canonical.as_bytes(), &sig).is_ok()
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
    #[cfg(feature = "test-utils")]
    use crate::routes::AppState;
    #[cfg(feature = "test-utils")]
    use axum::extract::FromRef;
    #[cfg(feature = "test-utils")]
    use ed25519_dalek::Signer;
    #[cfg(feature = "test-utils")]
    use std::sync::Arc;
    #[cfg(feature = "test-utils")]
    use synapse_cache::{CacheConfig, CacheManager};
    #[cfg(feature = "test-utils")]
    use synapse_services::ServiceContainer;

    #[test]
    fn test_extract_verify_key_from_server_key_response() {
        let body = serde_json::json!({
            "server_name": "example.org",
            "verify_keys": {
                "ed25519:abc": { "key": "SGVsbG9Xb3JsZA" }
            }
        });

        let key = extract_verify_key_from_server_keys(&body, "example.org", "ed25519:abc");
        assert_eq!(key, Some(("SGVsbG9Xb3JsZA".to_string(), None)));
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
        assert_eq!(key, Some(("SGVsbG9Xb3JsZA".to_string(), None)));
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

    // ── S1 修复测试：SigningTs 解析与校验 ──────────────────────────

    #[test]
    fn test_parse_x_matrix_authorization_with_ts() {
        let params = parse_x_matrix_authorization(
            r#"X-Matrix origin="test.example.com", key="ed25519:test", sig="abc123", ts=1700000000000"#,
        )
        .expect("header with ts should parse");

        assert_eq!(params.origin, "test.example.com");
        assert_eq!(params.key, "ed25519:test");
        assert_eq!(params.sig, "abc123");
        assert_eq!(params.ts, Some(1_700_000_000_000));
    }

    #[test]
    fn test_parse_x_matrix_authorization_ts_optional() {
        // 不带 ts 的头仍应正常解析（向后兼容）
        let params =
            parse_x_matrix_authorization(r#"X-Matrix origin="test.example.com", key="ed25519:test", sig="abc123""#)
                .expect("header without ts should parse");

        assert_eq!(params.origin, "test.example.com");
        assert_eq!(params.ts, None);
    }

    #[test]
    fn test_parse_x_matrix_authorization_ts_quoted() {
        let params = parse_x_matrix_authorization(
            r#"X-Matrix origin="test.example.com", key="ed25519:test", sig="abc123", ts="1700000000000""#,
        )
        .expect("header with quoted ts should parse");

        assert_eq!(params.ts, Some(1_700_000_000_000));
    }

    #[test]
    fn test_parse_x_matrix_authorization_ts_invalid_ignored() {
        // 非数字 ts 应被忽略（解析为 None），不阻止整个头解析
        let params = parse_x_matrix_authorization(
            r#"X-Matrix origin="test.example.com", key="ed25519:test", sig="abc123", ts="not-a-number""#,
        )
        .expect("header with invalid ts should still parse");

        assert_eq!(params.ts, None);
    }

    // ── S5 修复测试：签名缓存键纳入签名本身 + 不缓存失败 ──────────

    #[test]
    fn test_compute_signature_content_hash_different_for_different_signed_bytes() {
        // S5: 不同 signed_bytes 产生不同 content_hash（这是已有行为，验证不退化）
        let hash1 = compute_signature_content_hash(b"content with signature A");
        let hash2 = compute_signature_content_hash(b"content with signature B");
        assert_ne!(hash1, hash2, "不同 signed_bytes 必须产生不同 hash");
    }

    #[cfg(feature = "test-utils")]
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
        {
            let cfg = services.core.config_mut();
            cfg.federation.enabled = true;
            cfg.federation.allow_ingress = true;
            cfg.federation.server_name = origin.clone();
            cfg.federation.key_id = Some(key_id.clone());
            cfg.federation.signing_key = Some(signing_key_b64);
        }
        services.core.server_name = origin.clone();

        let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
        let state = AppState::new(services, cache);
        let ctx = FederationContext::from_ref(&state);

        let signed_bytes = canonical_federation_request_bytes("PUT", uri, &origin, &origin, Some(&body));
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_bytes);
        let signature = signing_key.sign(&signed_bytes);
        let signature_b64 = base64::engine::general_purpose::STANDARD_NO_PAD.encode(signature.to_bytes());

        let header = format!("X-Matrix origin=\"{origin}\", key=\"{key_id}\", sig=\"{signature_b64}\"");
        let params = parse_x_matrix_authorization(&header).expect("header should parse");

        verify_federation_signature_with_cache(&ctx, &params.origin, &params.key, &params.sig, &signed_bytes, false)
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

    // ------------------------------------------------------------------
    // F-01: compute_key_cache_ttl_secs MUST cap at 7 days per Matrix spec §1.2
    // ------------------------------------------------------------------

    #[test]
    fn compute_ttl_caps_at_7_days_when_valid_until_is_1_year() {
        // F-01: a peer advertising valid_until_ts one year in the future
        // must NOT cause us to cache for a full year. The spec caps server
        // key validity at 7 days. Hard invariant: ttl ≤ 7d always.
        let one_year_ms: i64 = 365 * 24 * 60 * 60 * 1000;
        let ttl = compute_key_cache_ttl_secs(Some(current_timestamp_millis() + one_year_ms));
        assert!(
            ttl <= MAX_SERVER_KEY_VALIDITY_SECS,
            "F-01 violation: TTL must cap at 7 days ({}) but got {}",
            MAX_SERVER_KEY_VALIDITY_SECS,
            ttl
        );
        // Current default (1h) is tighter than the 7d cap, so the 1h wins.
        assert_eq!(ttl, FEDERATION_KEY_CACHE_TTL_SECS, "current default 1h is the tightest bound for 1y peer validity");
    }

    #[test]
    fn compute_ttl_uses_min_of_default_and_peer_validity() {
        // Peer validity between 1h and 7d → TTL = FEDERATION_KEY_CACHE_TTL_SECS (1h)
        let five_hours_ms: i64 = 5 * 60 * 60 * 1000;
        let ttl = compute_key_cache_ttl_secs(Some(current_timestamp_millis() + five_hours_ms));
        assert_eq!(ttl, FEDERATION_KEY_CACHE_TTL_SECS, "1h default must cap mid-range peer TTL");

        // Peer validity < 1h → TTL = peer remaining validity
        let five_minutes_ms: i64 = 5 * 60 * 1000;
        let ttl_short = compute_key_cache_ttl_secs(Some(current_timestamp_millis() + five_minutes_ms));
        assert_eq!(ttl_short, 5 * 60, "short peer validity must win");
    }

    #[test]
    fn compute_ttl_falls_back_to_default_when_valid_until_is_none() {
        // Malformed / missing valid_until_ts → use default 1h
        let ttl = compute_key_cache_ttl_secs(None);
        assert_eq!(ttl, FEDERATION_KEY_CACHE_TTL_SECS, "None valid_until_ts must use default TTL");
    }

    #[test]
    fn compute_ttl_zero_when_valid_until_is_in_past() {
        // Defensive: already-expired keys must yield TTL=0 so the caller
        // re-fetches immediately rather than serving a stale key.
        let ttl = compute_key_cache_ttl_secs(Some(current_timestamp_millis() - 60_000));
        assert_eq!(ttl, 0, "expired valid_until_ts must yield TTL=0");
    }
}
