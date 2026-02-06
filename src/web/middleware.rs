use crate::cache::*;
use crate::common::ApiError;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, Request, StatusCode};
use axum::response::IntoResponse;
use axum::{body::Body, middleware::Next, response::Response};
use base64::Engine;
use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::json;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

static CORS_ORIGINS_REGEX: Lazy<Option<Regex>> = Lazy::new(|| {
    std::env::var("CORS_ORIGIN_PATTERN")
        .ok()
        .map(|pattern| Regex::new(&pattern).expect("Invalid CORS_ORIGIN_PATTERN regex"))
});

const FEDERATION_SIGNATURE_TTL_MS: u64 = 300 * 1000; // 5分钟容忍度
const FEDERATION_KEY_CACHE_TTL: u64 = 3600;
#[allow(dead_code)]
const FEDERATION_SIGNATURE_CACHE_TTL: u64 = 300;
#[allow(dead_code)]
const FEDERATION_KEY_ROTATION_GRACE_PERIOD_MS: u64 = 600 * 1000; // 10分钟宽限期

#[allow(dead_code)]
fn verify_signature_timestamp(signature_ts: i64) -> Result<(), ApiError> {
    let now = chrono::Utc::now().timestamp_millis();
    let tolerance = FEDERATION_SIGNATURE_TTL_MS as i64;

    if (signature_ts - now).abs() > tolerance {
        tracing::warn!(
            "Signature timestamp out of tolerance: {}ms (tolerance: {}ms)",
            (signature_ts - now).abs(),
            tolerance
        );
        Err(ApiError::unauthorized(
            "Signature timestamp out of tolerance".to_string(),
        ))
    } else {
        Ok(())
    }
}

#[allow(dead_code)]
async fn verify_federation_signature_with_timestamp(
    state: &crate::web::routes::AppState,
    origin: &str,
    key_id: &str,
    signature: &str,
    signature_ts: i64,
    signed_bytes: &[u8],
) -> Result<(), ApiError> {
    verify_signature_timestamp(signature_ts)?;

    verify_federation_signature(state, origin, key_id, signature, signed_bytes).await
}

#[allow(dead_code)]
async fn verify_with_key_rotation(
    state: &crate::web::routes::AppState,
    origin: &str,
    key_id: &str,
    signature: &str,
    signed_bytes: &[u8],
) -> Result<(), ApiError> {
    match verify_federation_signature_with_cache(state, origin, key_id, signature, signed_bytes)
        .await
    {
        Ok(()) => {
            tracing::debug!(
                "Signature verified with current key for {}:{}",
                origin,
                key_id
            );
            return Ok(());
        }
        Err(e) => {
            tracing::debug!(
                "Current key verification failed, trying historical keys: {}",
                e
            );
        }
    }

    let historical_key = get_historical_key(state, origin, key_id).await?;
    if let Some(public_key) = historical_key {
        let signature_bytes = decode_ed25519_signature(signature)
            .map_err(|_| ApiError::unauthorized("Invalid signature format".to_string()))?;

        let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&public_key)
            .map_err(|_| ApiError::unauthorized("Invalid public key".to_string()))?;

        match verifying_key.verify_strict(signed_bytes, &signature_bytes) {
            Ok(()) => {
                tracing::info!(
                    "Signature verified with historical key for {}:{} (key rotation detected)",
                    origin,
                    key_id
                );
                return Ok(());
            }
            Err(e) => {
                tracing::debug!("Historical key verification failed: {:?}", e);
            }
        }
    }

    Err(ApiError::unauthorized(
        "Signature verification failed with all available keys".to_string(),
    ))
}

async fn get_historical_key(
    state: &crate::web::routes::AppState,
    origin: &str,
    key_id: &str,
) -> Result<Option<[u8; 32]>, ApiError> {
    let cache_key = format!("federation:historical_key:{}:{}", origin, key_id);
    if let Ok(Some(cached)) = state.cache.get::<String>(&cache_key).await {
        if let Ok(key) = decode_ed25519_public_key(&cached) {
            return Ok(Some(key));
        }
    }

    #[derive(sqlx::FromRow)]
    struct HistoricalKeyRow {
        public_key: String,
    }

    let row = sqlx::query_as::<_, HistoricalKeyRow>(
        r#"
        SELECT public_key FROM federation_signing_keys
        WHERE key_id = $1 AND expires_at < $2
        ORDER BY created_at DESC LIMIT 1
        "#,
    )
    .bind(key_id)
    .bind(chrono::Utc::now().timestamp_millis())
    .fetch_optional(state.services.user_storage.pool.as_ref())
    .await
    .map_err(|e| ApiError::internal(format!("Failed to query historical key: {}", e)))?;

    if let Some(key_row) = row {
        if let Ok(key_bytes) = decode_ed25519_public_key(&key_row.public_key) {
            let ttl = FEDERATION_KEY_CACHE_TTL;
            let _ = state.cache.set(&cache_key, &key_row.public_key, ttl).await;
            return Ok(Some(key_bytes));
        }
    }

    Ok(None)
}

#[allow(dead_code)]
async fn prewarm_federation_keys(state: &crate::web::routes::AppState, origins: &[&str]) {
    for origin in origins {
        if let Err(e) = prewarm_keys_for_origin(state, origin).await {
            tracing::warn!("Failed to prewarm keys for {}: {}", origin, e);
        }
    }
}

#[allow(dead_code)]
async fn prewarm_keys_for_origin(
    state: &crate::web::routes::AppState,
    origin: &str,
) -> Result<(), ApiError> {
    let cache_key = format!("federation:server_keys:{}", origin);

    if let Ok(Some(_)) = state.cache.get::<String>(&cache_key).await {
        tracing::debug!("Server keys already cached for {}", origin);
        return Ok(());
    }

    let urls = [
        format!("https://{}/_matrix/key/v2/server", origin),
        format!("http://{}:8448/_matrix/key/v2/server", origin),
    ];

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .map_err(|e| ApiError::internal(e.to_string()))?;

    for url in urls {
        match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => match resp.json::<Value>().await {
                Ok(json) => {
                    let keys_json = serde_json::to_string(&json).unwrap_or_default();
                    let ttl = FEDERATION_KEY_CACHE_TTL;
                    let _ = state.cache.set(&cache_key, keys_json, ttl).await;
                    tracing::info!("Successfully prewarmed keys for {}", origin);
                    return Ok(());
                }
                Err(e) => {
                    tracing::debug!("Failed to parse response from {}: {}", url, e);
                }
            },
            Err(e) => {
                tracing::debug!("Failed to fetch keys from {}: {}", url, e);
            }
            _ => {
                tracing::debug!("Non-success status from {}", url);
            }
        }
    }

    Err(ApiError::not_found(format!(
        "Failed to prewarm keys for {}: no valid response",
        origin
    )))
}

pub async fn logging_middleware(request: Request<Body>, next: axum::middleware::Next) -> Response {
    let start = Instant::now();
    let method = request.method().clone();
    let uri = request.uri().clone();
    let authenticated = request
        .headers()
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .is_some();

    let mut headers = request.headers().clone();
    headers.remove("authorization");
    headers.remove("cookie");

    let response = next.run(request).await;

    let duration = start.elapsed();
    let status = response.status();

    tracing::info!(
        "Request: {} {} {} {} {:?} {}ms",
        if authenticated {
            "authenticated"
        } else {
            "anonymous"
        },
        method,
        uri,
        status.as_u16(),
        headers,
        duration.as_millis()
    );

    response
}

fn is_dev_mode() -> bool {
    std::env::var("RUST_ENV")
        .unwrap_or_else(|_| "production".to_string())
        .to_lowercase()
        == "development"
}

fn get_allowed_origins() -> Vec<String> {
    std::env::var("ALLOWED_ORIGINS")
        .ok()
        .map(|s| s.split(',').map(|v| v.trim().to_string()).collect())
        .unwrap_or_default()
}

fn is_origin_allowed(origin: &str) -> bool {
    if is_dev_mode() {
        return true;
    }

    let allowed_origins = get_allowed_origins();
    if allowed_origins.is_empty() {
        if let Some(ref pattern) = *CORS_ORIGINS_REGEX {
            return pattern.is_match(origin);
        }
        return false;
    }

    allowed_origins.iter().any(|o| {
        if o == "*" {
            true
        } else {
            normalize_origin(o) == normalize_origin(origin)
        }
    })
}

fn normalize_origin(origin: &str) -> String {
    let normalized = origin.trim_end_matches('/').to_lowercase();
    let parts: Vec<&str> = normalized.split("://").collect();
    if parts.len() == 2 {
        format!("{}://{}", parts[0], parts[1])
    } else {
        normalized
    }
}

pub async fn cors_middleware(request: Request<Body>, next: axum::middleware::Next) -> Response {
    let origin = request
        .headers()
        .get("origin")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let is_options = request.method() == "OPTIONS";

    let mut response = next.run(request).await;

    let allow_origin = if is_dev_mode() {
        origin.as_deref().or(Some("*"))
    } else if let Some(ref req_origin) = origin {
        if is_origin_allowed(req_origin) {
            Some(req_origin.as_str())
        } else {
            tracing::warn!("CORS origin rejected: {}", req_origin);
            None
        }
    } else {
        None
    };

    if let Some(allowed) = allow_origin {
        if let Ok(value) = HeaderValue::from_str(allowed) {
            response
                .headers_mut()
                .insert("Access-Control-Allow-Origin", value);
        }
    }

    response.headers_mut().insert(
        "Access-Control-Allow-Methods",
        HeaderValue::from_static("GET, POST, PUT, DELETE, OPTIONS, PATCH"),
    );

    response.headers_mut().insert(
        "Access-Control-Allow-Headers",
        HeaderValue::from_static("Content-Type, Authorization, X-Requested-With, X-Request-ID"),
    );

    response.headers_mut().insert(
        "Access-Control-Expose-Headers",
        HeaderValue::from_static("X-Request-ID"),
    );

    response.headers_mut().insert(
        "Access-Control-Allow-Credentials",
        HeaderValue::from_static("true"),
    );

    if let Some(ref origin) = origin {
        response.headers_mut().insert(
            "Vary",
            HeaderValue::from_str(&format!("Origin, {}", origin))
                .unwrap_or_else(|_| HeaderValue::from_static("Origin")),
        );
    } else {
        response
            .headers_mut()
            .insert("Vary", HeaderValue::from_static("Origin"));
    }

    if is_options {
        let max_age = std::env::var("CORS_MAX_AGE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(86400);

        response
            .headers_mut()
            .insert("Access-Control-Max-Age", HeaderValue::from(max_age));

        *response.status_mut() = StatusCode::NO_CONTENT;
    }

    response
}

pub async fn metrics_middleware(request: Request<Body>, next: axum::middleware::Next) -> Response {
    let start = Instant::now();
    let method = request.method().clone();
    let path = request.uri().path().to_string();

    let response = next.run(request).await;
    let duration = start.elapsed();
    let status = response.status().as_u16();

    tracing::debug!("{} {} {} {}ms", method, path, status, duration.as_millis());

    response
}

pub fn extract_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

pub async fn rate_limit_middleware(
    State(state): State<crate::web::routes::AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let config = &state.services.config.rate_limit;
    if !config.enabled {
        return next.run(request).await;
    }

    let path = request.uri().path();
    if config.exempt_paths.iter().any(|p| p == path)
        || config
            .exempt_path_prefixes
            .iter()
            .any(|p| !p.is_empty() && path.starts_with(p))
    {
        return next.run(request).await;
    }

    let ip = extract_client_ip(request.headers(), config.ip_header_priority.as_slice())
        .unwrap_or_else(|| "unknown".to_string());

    let (endpoint_id, rule) = select_endpoint_rule(config, path);
    let endpoint_id = config
        .endpoint_aliases
        .get(&endpoint_id)
        .cloned()
        .unwrap_or(endpoint_id);

    let redis_prefix = state.services.config.redis.key_prefix.as_str();
    let cache_key = format!(
        "{}{}",
        redis_prefix,
        CacheKeyBuilder::ip_rate_limit(&ip, endpoint_id.as_str())
    );

    let decision = match state
        .cache
        .rate_limit_token_bucket_take(&cache_key, rule.per_second, rule.burst_size)
        .await
    {
        Ok(d) => d,
        Err(e) => {
            if state.services.config.rate_limit.fail_open_on_error {
                tracing::warn!("Rate limiter error, allowing request: {}", e);
                return next.run(request).await;
            } else {
                let mut response = Response::new(Body::from(
                    json!({
                        "errcode": "M_LIMIT_EXCEEDED",
                        "error": "Rate limiter unavailable"
                    })
                    .to_string(),
                ));
                *response.status_mut() = StatusCode::TOO_MANY_REQUESTS;
                response
                    .headers_mut()
                    .insert("content-type", HeaderValue::from_static("application/json"));
                return response;
            }
        }
    };

    if !decision.allowed {
        let retry_after_ms = decision.retry_after_seconds.saturating_mul(1000);
        let mut response = Response::new(Body::from(
            json!({
                "errcode": "M_LIMIT_EXCEEDED",
                "error": "Rate limited",
                "retry_after_ms": retry_after_ms
            })
            .to_string(),
        ));
        *response.status_mut() = StatusCode::TOO_MANY_REQUESTS;
        response
            .headers_mut()
            .insert("content-type", HeaderValue::from_static("application/json"));
        if let Ok(v) = decision.retry_after_seconds.to_string().parse() {
            response.headers_mut().insert("retry-after", v);
        }

        if config.include_headers {
            if let Ok(v) = decision.remaining.to_string().parse() {
                response.headers_mut().insert("x-ratelimit-remaining", v);
            }
            if let Ok(v) = rule.burst_size.to_string().parse() {
                response.headers_mut().insert("x-ratelimit-limit", v);
            }
            if let Ok(v) = retry_after_ms.to_string().parse() {
                response.headers_mut().insert("x-ratelimit-after", v);
            }
        }

        return response;
    }

    let mut response = next.run(request).await;
    if config.include_headers {
        if let Ok(v) = decision.remaining.to_string().parse() {
            response.headers_mut().insert("x-ratelimit-remaining", v);
        }
        if let Ok(v) = rule.burst_size.to_string().parse() {
            response.headers_mut().insert("x-ratelimit-limit", v);
        }
    }
    response
}

pub async fn auth_middleware(
    State(state): State<crate::web::routes::AppState>,
    request: Request<Body>,
    next: axum::middleware::Next,
) -> Response {
    let token = match extract_token(request.headers()) {
        Some(token) => token,
        None => return ApiError::unauthorized("Missing access token".to_string()).into_response(),
    };

    if let Err(err) = state.services.auth_service.validate_token(&token).await {
        return err.into_response();
    }

    next.run(request).await
}

pub async fn federation_auth_middleware(
    State(state): State<crate::web::routes::AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    if !state.services.config.federation.enabled || !state.services.config.federation.allow_ingress
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    let (parts, body) = request.into_parts();

    let auth_header = parts
        .headers
        .get("authorization")
        .or(parts.headers.get("Authorization"))
        .and_then(|h| h.to_str().ok());

    let auth_header = match auth_header {
        Some(v) => v,
        None => {
            return ApiError::unauthorized("Missing federation signature".to_string())
                .into_response()
        }
    };

    let params = match parse_x_matrix_authorization(auth_header) {
        Some(p) => p,
        None => {
            return ApiError::unauthorized("Missing federation signature".to_string())
                .into_response()
        }
    };

    let destination = state.services.server_name.as_str();

    let body_limit = state
        .services
        .config
        .federation
        .max_transaction_payload
        .max(64 * 1024) as usize;

    let body_bytes = match axum::body::to_bytes(body, body_limit).await {
        Ok(b) => b,
        Err(_) => {
            return ApiError::unauthorized("Invalid request body".to_string()).into_response()
        }
    };

    let content = if body_bytes.is_empty() {
        None
    } else {
        match serde_json::from_slice::<Value>(&body_bytes) {
            Ok(v) => Some(v),
            Err(_) => {
                return ApiError::unauthorized("Invalid JSON body".to_string()).into_response()
            }
        }
    };

    let request_target = parts
        .uri
        .path_and_query()
        .map(|p| p.as_str().to_string())
        .unwrap_or_else(|| parts.uri.path().to_string());

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
    )
    .await;

    if let Err(e) = signature_valid {
        tracing::warn!(
            "Unauthorized federation request from {:?}. Server name: {}. Error: {}",
            parts
                .headers
                .get("x-forwarded-for")
                .or(parts.headers.get("host")),
            state.services.server_name,
            e
        );
        return ApiError::unauthorized("Invalid federation signature".to_string()).into_response();
    }

    let request = Request::from_parts(parts, Body::from(body_bytes));
    next.run(request).await
}

pub async fn replication_http_auth_middleware(
    State(state): State<crate::web::routes::AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    if !state.services.config.worker.replication.http.enabled {
        return StatusCode::NOT_FOUND.into_response();
    }
    let secret = if let Some(s) = &state.services.config.worker.replication.http.secret {
        s.clone()
    } else if let Some(p) = &state.services.config.worker.replication.http.secret_path {
        match fs::read_to_string(PathBuf::from(p)) {
            Ok(s) => s.trim().to_string(),
            Err(_) => {
                return ApiError::unauthorized("Replication secret not available".to_string())
                    .into_response()
            }
        }
    } else {
        return ApiError::unauthorized("Replication secret not configured".to_string())
            .into_response();
    };
    let token = extract_token(request.headers()).unwrap_or_default();
    if token != secret {
        return ApiError::unauthorized("Invalid replication secret".to_string()).into_response();
    }
    next.run(request).await
}

#[derive(Debug, Clone)]
struct XMatrixAuthParams {
    origin: String,
    key: String,
    sig: String,
}

fn parse_x_matrix_authorization(header_value: &str) -> Option<XMatrixAuthParams> {
    let header_value = header_value.trim();
    let header_value = header_value.strip_prefix("X-Matrix ")?;

    let mut origin: Option<String> = None;
    let mut key: Option<String> = None;
    let mut sig: Option<String> = None;

    for part in header_value.split(',') {
        let part = part.trim();
        let (k, v) = part.split_once('=')?;
        let k = k.trim();
        let mut v = v.trim();
        if v.starts_with('"') && v.ends_with('"') && v.len() >= 2 {
            v = &v[1..v.len() - 1];
        }

        match k {
            "origin" => origin = Some(v.to_string()),
            "key" => key = Some(v.to_string()),
            "sig" => sig = Some(v.to_string()),
            _ => {}
        }
    }

    Some(XMatrixAuthParams {
        origin: origin?,
        key: key?,
        sig: sig?,
    })
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
    canonical_json_bytes(&Value::Object(obj))
}

fn canonical_json_bytes(value: &Value) -> Vec<u8> {
    canonical_json_string(value).into_bytes()
}

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

async fn verify_federation_signature_with_cache(
    state: &crate::web::routes::AppState,
    origin: &str,
    key_id: &str,
    signature: &str,
    signed_bytes: &[u8],
) -> Result<(), ApiError> {
    let cache_key = format!(
        "federation:signature_cache:{}:{}:{}",
        origin,
        key_id,
        compute_signature_content_hash(signed_bytes)
    );

    if let Ok(Some(_)) = state.cache.get::<String>(&cache_key).await {
        tracing::debug!("Signature cache hit for {}:{}", origin, key_id);
        return Ok(());
    }

    let result = verify_federation_signature(state, origin, key_id, signature, signed_bytes).await;

    if result.is_ok() {
        let ttl = 300u64;
        let _ = state.cache.set(&cache_key, "valid", ttl).await;
    }

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
    state: &crate::web::routes::AppState,
    origin: &str,
    key_id: &str,
    signature: &str,
    signed_bytes: &[u8],
) -> Result<(), ApiError> {
    let public_key = get_federation_verify_key(state, origin, key_id).await?;

    let signature_bytes = match decode_ed25519_signature(signature) {
        Ok(sig) => sig,
        Err(_) => {
            return Err(ApiError::unauthorized(
                "Invalid signature format".to_string(),
            ))
        }
    };

    let verifying_key = match ed25519_dalek::VerifyingKey::from_bytes(&public_key) {
        Ok(k) => k,
        Err(_) => return Err(ApiError::unauthorized("Invalid public key".to_string())),
    };

    match verifying_key.verify_strict(signed_bytes, &signature_bytes) {
        Ok(()) => Ok(()),
        Err(e) => {
            tracing::debug!("Signature verification failed: {:?}", e);
            Err(ApiError::unauthorized(
                "Signature verification failed".to_string(),
            ))
        }
    }
}

#[allow(dead_code)]
async fn verify_batch_signatures(
    state: &crate::web::routes::AppState,
    signatures: &HashMap<String, HashMap<String, String>>,
    _origin: &str,
    signed_bytes: &[u8],
) -> Result<(), ApiError> {
    if signatures.is_empty() {
        return Err(ApiError::unauthorized("No signatures provided".to_string()));
    }

    let mut first_error = None;

    for (sig_origin, key_signatures) in signatures {
        for (key_id, signature) in key_signatures {
            match verify_federation_signature_with_cache(
                state,
                sig_origin,
                key_id,
                signature,
                signed_bytes,
            )
            .await
            {
                Ok(()) => {
                    tracing::debug!(
                        "Signature verified successfully for {}:{}",
                        sig_origin,
                        key_id
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        "Signature verification failed for {}:{}: {}",
                        sig_origin,
                        key_id,
                        e
                    );
                    if first_error.is_none() {
                        first_error = Some(e);
                    }
                }
            }
        }
    }

    if let Some(error) = first_error {
        Err(error)
    } else {
        Err(ApiError::unauthorized(
            "No valid signatures found".to_string(),
        ))
    }
}

async fn get_federation_verify_key(
    state: &crate::web::routes::AppState,
    origin: &str,
    key_id: &str,
) -> Result<[u8; 32], ApiError> {
    let cache_key = format!("federation:verify_key:{}:{}", origin, key_id);
    if let Ok(Some(cached)) = state.cache.get::<String>(&cache_key).await {
        if let Ok(key) = decode_ed25519_public_key(&cached) {
            return Ok(key);
        }
    }

    let fetched = fetch_federation_verify_key(origin, key_id).await?;
    let ttl = 3600u64;
    let _ = state.cache.set(&cache_key, &fetched, ttl).await;
    decode_ed25519_public_key(&fetched)
        .map_err(|_| ApiError::unauthorized("Invalid public key".to_string()))
}

async fn fetch_federation_verify_key(origin: &str, key_id: &str) -> Result<String, ApiError> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .map_err(|e| ApiError::internal(e.to_string()))?;

    let urls = [
        format!("https://{}/_matrix/key/v2/server", origin),
        format!("http://{}/_matrix/key/v2/server", origin),
        format!(
            "https://{}/_matrix/key/v2/query/{}/{}",
            origin, origin, key_id
        ),
        format!(
            "http://{}/_matrix/key/v2/query/{}/{}",
            origin, origin, key_id
        ),
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
            return Ok(key);
        }
    }

    Err(ApiError::unauthorized("Public key not found".to_string()))
}

fn extract_verify_key_from_server_keys(body: &Value, origin: &str, key_id: &str) -> Option<String> {
    if let Some(key) = extract_verify_key_from_server_keys_object(body, key_id) {
        return Some(key);
    }

    let server_keys = body.get("server_keys")?.as_array()?;
    for entry in server_keys {
        if entry
            .get("server_name")
            .and_then(|v| v.as_str())
            .is_some_and(|v| v != origin)
        {
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

fn decode_ed25519_public_key(key: &str) -> Result<[u8; 32], ()> {
    let engines = [
        base64::engine::general_purpose::STANDARD,
        base64::engine::general_purpose::STANDARD_NO_PAD,
    ];

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

fn select_endpoint_rule<'a>(
    config: &'a crate::common::config::RateLimitConfig,
    path: &str,
) -> (String, &'a crate::common::config::RateLimitRule) {
    let mut best: Option<(
        usize,
        bool,
        &'a crate::common::config::RateLimitEndpointRule,
    )> = None;
    for rule in &config.endpoints {
        let matches = match rule.match_type {
            crate::common::config::RateLimitMatchType::Exact => path == rule.path,
            crate::common::config::RateLimitMatchType::Prefix => path.starts_with(&rule.path),
        };
        if !matches {
            continue;
        }
        let score = rule.path.len();
        let is_exact = matches
            && matches!(
                rule.match_type,
                crate::common::config::RateLimitMatchType::Exact
            );
        match best {
            None => best = Some((score, is_exact, rule)),
            Some((best_score, best_exact, _)) => {
                if (is_exact && !best_exact) || (is_exact == best_exact && score > best_score) {
                    best = Some((score, is_exact, rule));
                }
            }
        }
    }

    if let Some((_, _, endpoint_rule)) = best {
        (endpoint_rule.path.clone(), &endpoint_rule.rule)
    } else {
        (path.to_string(), &config.default)
    }
}

fn extract_client_ip(headers: &HeaderMap, priority: &[String]) -> Option<String> {
    for name in priority {
        let lower = name.to_ascii_lowercase();
        if lower == "x-forwarded-for" {
            if let Some(ip) = headers
                .get("x-forwarded-for")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.split(',').next())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
            {
                return Some(ip);
            }
            continue;
        }

        if lower == "x-real-ip" {
            if let Some(ip) = headers
                .get("x-real-ip")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
            {
                return Some(ip);
            }
            continue;
        }

        if lower == "forwarded" {
            if let Some(ip) = headers
                .get("forwarded")
                .and_then(|v| v.to_str().ok())
                .and_then(parse_forwarded_for)
            {
                return Some(ip);
            }
            continue;
        }

        if let Some(ip) = headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
        {
            return Some(ip);
        }
    }
    None
}

fn parse_forwarded_for(value: &str) -> Option<String> {
    let first = value.split(',').next()?.trim();
    for part in first.split(';') {
        let part = part.trim();
        let lower = part.to_ascii_lowercase();
        if lower.starts_with("for=") {
            let mut original = part[4..].trim();
            if original.starts_with('"') && original.ends_with('"') {
                original = &original[1..original.len() - 1];
            }

            if original.starts_with('[') {
                if let Some(end) = original.find(']') {
                    return Some(original[1..end].to_string());
                }
            }

            let colons = original.chars().filter(|c| *c == ':').count();
            if colons == 1 {
                return original.split(':').next().map(|s| s.to_string());
            }

            if !original.is_empty() {
                return Some(original.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::config::{
        RateLimitConfig, RateLimitEndpointRule, RateLimitMatchType, RateLimitRule,
    };

    #[test]
    fn test_extract_client_ip() {
        let mut headers = HeaderMap::new();
        let priority = vec!["x-forwarded-for".to_string(), "x-real-ip".to_string()];

        // Test X-Forwarded-For
        headers.insert("x-forwarded-for", "1.2.3.4, 5.6.7.8".parse().unwrap());
        assert_eq!(
            extract_client_ip(&headers, &priority),
            Some("1.2.3.4".to_string())
        );

        // Test X-Real-IP
        headers = HeaderMap::new();
        headers.insert("x-real-ip", "10.0.0.1".parse().unwrap());
        assert_eq!(
            extract_client_ip(&headers, &priority),
            Some("10.0.0.1".to_string())
        );

        // Test Priority (X-Forwarded-For > X-Real-IP)
        headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "1.2.3.4".parse().unwrap());
        headers.insert("x-real-ip", "10.0.0.1".parse().unwrap());
        assert_eq!(
            extract_client_ip(&headers, &priority),
            Some("1.2.3.4".to_string())
        );
    }

    #[test]
    fn test_extract_client_ip_forwarded() {
        let mut headers = HeaderMap::new();
        let priority = vec!["forwarded".to_string()];

        headers.insert(
            "forwarded",
            "for=192.0.2.60;proto=http;by=203.0.113.43".parse().unwrap(),
        );
        assert_eq!(
            extract_client_ip(&headers, &priority),
            Some("192.0.2.60".to_string())
        );

        headers = HeaderMap::new();
        headers.insert(
            "forwarded",
            "for=\"[2001:db8:cafe::17]:4711\"".parse().unwrap(),
        );
        assert_eq!(
            extract_client_ip(&headers, &priority),
            Some("2001:db8:cafe::17".to_string())
        );
    }

    #[test]
    fn test_select_endpoint_rule() {
        let mut config = RateLimitConfig::default();
        config.endpoints.push(RateLimitEndpointRule {
            path: "/_matrix/client/r0/login".to_string(),
            match_type: RateLimitMatchType::Exact,
            rule: RateLimitRule {
                per_second: 5,
                burst_size: 10,
            },
        });
        config.endpoints.push(RateLimitEndpointRule {
            path: "/_matrix/client".to_string(),
            match_type: RateLimitMatchType::Prefix,
            rule: RateLimitRule {
                per_second: 50,
                burst_size: 100,
            },
        });
        config.endpoints.push(RateLimitEndpointRule {
            path: "/_matrix/client/r0/sync".to_string(),
            match_type: RateLimitMatchType::Prefix,
            rule: RateLimitRule {
                per_second: 20,
                burst_size: 40,
            },
        });

        // Exact match
        let (id, rule) = select_endpoint_rule(&config, "/_matrix/client/r0/login");
        assert_eq!(id, "/_matrix/client/r0/login");
        assert_eq!(rule.per_second, 5);

        // Longest prefix match
        let (id, rule) = select_endpoint_rule(&config, "/_matrix/client/r0/sync?since=123");
        assert_eq!(id, "/_matrix/client/r0/sync");
        assert_eq!(rule.per_second, 20);

        // Shorter prefix match
        let (id, rule) = select_endpoint_rule(&config, "/_matrix/client/versions");
        assert_eq!(id, "/_matrix/client");
        assert_eq!(rule.per_second, 50);

        // Default fallback
        let (id, rule) = select_endpoint_rule(&config, "/other/path");
        assert_eq!(id, "/other/path");
        assert_eq!(rule.per_second, config.default.per_second);
    }

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
    fn test_compute_signature_content_hash_deterministic() {
        let content1 = b"test content for hashing with more data";
        let content2 = b"test content for hashing with more data";
        let content3 = b"different content";

        let hash1 = compute_signature_content_hash(content1);
        let hash2 = compute_signature_content_hash(content2);
        let hash3 = compute_signature_content_hash(content3);

        assert_eq!(hash1, hash2, "Same content should produce same hash");
        assert_ne!(
            hash1, hash3,
            "Different content should produce different hash"
        );
        assert_eq!(
            hash1.len(),
            43,
            "SHA256 Base64 output should be 43 characters"
        );
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
        let binary_data: [u8; 16] = [
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d,
            0x0e, 0x0f,
        ];
        let hash = compute_signature_content_hash(&binary_data);

        assert_eq!(hash.len(), 43);
        assert!(hash
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '='));
    }
}
