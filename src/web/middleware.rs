use crate::cache::*;
use crate::common::ApiError;
use crate::web::utils::{encoding::decode_base64_32, ip::extract_client_ip};
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, Method, Request, StatusCode};
use axum::response::IntoResponse;
use axum::{body::Body, middleware::Next, response::Response, Json};
use base64::Engine; // Required for decode in module-level functions
use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::json;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::Semaphore;
use url::Url;

static CORS_ORIGINS_REGEX: Lazy<Option<Regex>> = Lazy::new(|| {
    std::env::var("CORS_ORIGIN_PATTERN")
        .ok()
        .and_then(|pattern| match Regex::new(&pattern) {
            Ok(regex) => Some(regex),
            Err(e) => {
                tracing::error!("Invalid CORS_ORIGIN_PATTERN regex '{}': {}", pattern, e);
                None
            }
        })
});

const FEDERATION_SIGNATURE_TTL_MS: u64 = 300 * 1000;

// ============================================================================
// CSRF Protection Middleware
// ============================================================================

/// CSRF token manager for generating and validating tokens
///
/// In production, this should be used with axum's middleware system
pub struct CsrfTokenManager {
    secret: String,
    #[allow(dead_code)]
    token_ttl: std::time::Duration,
}

impl CsrfTokenManager {
    pub fn new(secret: String) -> Self {
        Self {
            secret,
            token_ttl: std::time::Duration::from_secs(24 * 3600), // 24 hours
        }
    }

    /// Generate a CSRF token for a session
    #[allow(dead_code)]
    pub fn generate_token(&self, session_id: &str) -> String {
        let issued_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or_default();
        let payload = format!("{}:{}", session_id, issued_at);
        let signature = crate::common::crypto::compute_hash(format!("{}{}", payload, self.secret));
        format!("{}:{}", payload, &signature[..16])
    }

    /// Validate a CSRF token
    #[allow(dead_code)]
    pub fn validate_token(&self, token: &str, session_id: &str) -> bool {
        let parts: Vec<&str> = token.split(':').collect();
        if parts.len() != 3 {
            return false;
        }

        if parts[0] != session_id {
            return false;
        }

        let issued_at = match parts[1].parse::<u64>() {
            Ok(issued_at) => issued_at,
            Err(_) => return false,
        };
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or_default();
        if now.saturating_sub(issued_at) > self.token_ttl.as_secs() {
            return false;
        }

        let expected_signature = crate::common::crypto::compute_hash(format!(
            "{}:{}{}",
            parts[0], parts[1], self.secret
        ));

        expected_signature.starts_with(parts[2])
    }
}

#[derive(Debug, Clone)]
pub struct CorsSecurityReport {
    pub is_development_mode: bool,
    pub allows_any_origin: bool,
    pub allowed_origins: Vec<String>,
    pub has_pattern: bool,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

impl CorsSecurityReport {
    pub fn has_issues(&self) -> bool {
        !self.errors.is_empty() || !self.warnings.is_empty()
    }
}

pub fn check_cors_security() -> CorsSecurityReport {
    let is_dev = is_dev_mode();
    let allowed_origins = get_allowed_origins();
    let allows_any_origin = allowed_origins.iter().any(|o| o == "*");
    let has_pattern = CORS_ORIGINS_REGEX.is_some();

    let mut warnings = Vec::new();
    let mut errors = Vec::new();

    if is_dev {
        warnings.push(
            "⚠️  DEVELOPMENT MODE ENABLED - CORS allows all origins. DO NOT use in production!"
                .to_string(),
        );
    }

    if !is_dev && allows_any_origin {
        errors.push(
            "🚨 SECURITY ERROR: Production environment cannot use '*' as CORS origin. \
             Please configure ALLOWED_ORIGINS environment variable with specific domains."
                .to_string(),
        );
    }

    if !is_dev && allowed_origins.is_empty() && !has_pattern {
        errors.push(
            "🚨 SECURITY ERROR: No CORS origins configured in production. \
             Set ALLOWED_ORIGINS or CORS_ORIGIN_PATTERN environment variable."
                .to_string(),
        );
    }

    if !is_dev && allows_any_origin {
        warnings.push(
            "⚠️  CORS wildcard origin detected in production configuration. \
             This is a security risk and may expose your server to CSRF attacks."
                .to_string(),
        );
    }

    CorsSecurityReport {
        is_development_mode: is_dev,
        allows_any_origin,
        allowed_origins,
        has_pattern,
        warnings,
        errors,
    }
}

pub fn log_cors_security_report(report: &CorsSecurityReport) {
    println!();
    println!("╔════════════════════════════════════════════════════════════════╗");
    println!("║              CORS Security Configuration Check                 ║");
    println!("╠════════════════════════════════════════════════════════════════╣");

    if report.is_development_mode {
        println!("║  🔧 MODE: DEVELOPMENT                                          ║");
        println!("║  ⚠️  WARNING: Development mode is ACTIVE                        ║");
        println!("║  ⚠️  All CORS origins are permitted - NOT SAFE FOR PRODUCTION  ║");
    } else {
        println!("║  🏭 MODE: PRODUCTION                                           ║");
    }

    println!("╠════════════════════════════════════════════════════════════════╣");

    if report.allows_any_origin {
        println!("║  🌐 CORS Origin: * (ANY ORIGIN)                                ║");
    } else if report.has_pattern {
        println!("║  🌐 CORS Origin: Pattern-based matching                        ║");
    } else if report.allowed_origins.is_empty() {
        println!("║  🌐 CORS Origin: NOT CONFIGURED                                ║");
    } else {
        println!("║  🌐 CORS Origins:                                              ║");
        for origin in &report.allowed_origins {
            let truncated = if origin.len() > 50 {
                format!("{}...", &origin[..47])
            } else {
                origin.clone()
            };
            println!("║    - {:<58}║", truncated);
        }
    }

    println!("╠════════════════════════════════════════════════════════════════╣");

    if !report.errors.is_empty() {
        println!("║  🚨 ERRORS:                                                    ║");
        for error in &report.errors {
            for line in textwrap::wrap(error, 60) {
                let padding = if line.len() < 60 { 60 - line.len() } else { 0 };
                println!("║    {}{}", line, " ".repeat(padding));
            }
        }
    }

    if !report.warnings.is_empty() {
        println!("║  ⚠️  WARNINGS:                                                  ║");
        for warning in &report.warnings {
            for line in textwrap::wrap(warning, 60) {
                let padding = if line.len() < 60 { 60 - line.len() } else { 0 };
                println!("║    {}{}", line, " ".repeat(padding));
            }
        }
    }

    if !report.has_issues() {
        println!("║  ✅ CORS configuration looks secure                            ║");
    }

    println!("╚════════════════════════════════════════════════════════════════╝");
    println!();

    for error in &report.errors {
        tracing::error!("{}", error);
    }
    for warning in &report.warnings {
        tracing::warn!("{}", warning);
    }
}

pub fn validate_cors_config_for_production() -> Result<(), String> {
    let report = check_cors_security();

    if !report.errors.is_empty() {
        return Err(report.errors.join("; "));
    }

    Ok(())
}

pub fn validate_bind_address_for_dev_mode(host: &str) -> Result<(), String> {
    if !is_dev_mode() {
        return Ok(());
    }

    let local_addresses = ["127.0.0.1", "localhost", "::1", "0.0.0.0", "::", "[::]"];

    let is_local = local_addresses.iter().any(|&local| {
        host.eq_ignore_ascii_case(local) || host.starts_with("127.") || host.starts_with("::1")
    });

    if !is_local {
        return Err(format!(
            "Development mode should only bind to local addresses. \
             Current bind address '{}' is not local. \
             For development, use '127.0.0.1' or 'localhost'.",
            host
        ));
    }

    Ok(())
}
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

    verify_federation_signature(state, origin, key_id, signature, signed_bytes, false).await
}

#[allow(dead_code)]
async fn verify_with_key_rotation(
    state: &crate::web::routes::AppState,
    origin: &str,
    key_id: &str,
    signature: &str,
    signed_bytes: &[u8],
) -> Result<(), ApiError> {
    match verify_federation_signature_with_cache(
        state,
        origin,
        key_id,
        signature,
        signed_bytes,
        false,
    )
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
        WHERE server_name = $1 AND key_id = $2 AND expires_at < $3
        ORDER BY created_ts DESC LIMIT 1
        "#,
    )
    .bind(origin)
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

// ============================================================================
// SECTION: Utility Functions
// ============================================================================

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

fn is_safe_http_method(method: &Method) -> bool {
    matches!(
        *method,
        Method::GET | Method::HEAD | Method::OPTIONS | Method::TRACE
    )
}

fn extract_session_id_for_csrf(headers: &HeaderMap) -> Option<String> {
    headers
        .get("cookie")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string())
        .or_else(|| {
            headers
                .get("authorization")
                .and_then(|value| value.to_str().ok())
                .map(|value| value.to_string())
        })
}

fn extract_origin_candidate(headers: &HeaderMap) -> Option<String> {
    headers
        .get("origin")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string())
        .or_else(|| {
            headers
                .get("referer")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| Url::parse(value).ok())
                .map(|value| value.origin().ascii_serialization())
        })
}

fn extract_request_origin(headers: &HeaderMap) -> Option<String> {
    let host = headers
        .get("x-forwarded-host")
        .or_else(|| headers.get("host"))
        .and_then(|value| value.to_str().ok())?;
    let scheme = headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("https");

    Some(normalize_origin(&format!("{scheme}://{host}")))
}

fn same_origin(request_origin: &str, headers: &HeaderMap) -> bool {
    extract_request_origin(headers)
        .map(|server_origin| normalize_origin(request_origin) == server_origin)
        .unwrap_or(false)
}

fn csrf_forbidden_response(message: &str) -> Response {
    let mut response = Response::new(Body::from(
        json!({
            "errcode": "M_FORBIDDEN",
            "error": message
        })
        .to_string(),
    ));
    *response.status_mut() = StatusCode::FORBIDDEN;
    response
        .headers_mut()
        .insert("content-type", HeaderValue::from_static("application/json"));
    response
}

pub async fn csrf_middleware(
    State(state): State<crate::web::routes::AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let csrf_manager = CsrfTokenManager::new(state.services.server_name.clone());
    let method = request.method().clone();
    let headers = request.headers().clone();
    let session_id = extract_session_id_for_csrf(&headers);
    let request_origin = extract_origin_candidate(&headers);
    let browser_authenticated_request = session_id.is_some() && request_origin.is_some();

    if !is_safe_http_method(&method) && browser_authenticated_request {
        let origin = request_origin.unwrap_or_default();
        if !same_origin(&origin, &headers) && !is_origin_allowed(&origin) {
            tracing::warn!("CSRF origin rejected: {}", origin);
            return csrf_forbidden_response("Cross-site requests are not allowed");
        }

        let csrf_token = headers
            .get("x-csrf-token")
            .and_then(|value| value.to_str().ok());

        match (session_id.as_deref(), csrf_token) {
            (Some(session), Some(token)) if csrf_manager.validate_token(token, session) => {}
            _ => return csrf_forbidden_response("Missing or invalid CSRF token"),
        }
    }

    let mut response = next.run(request).await;

    if is_safe_http_method(&method) {
        if let Some(session_id) = session_id {
            if let Ok(value) = HeaderValue::from_str(&csrf_manager.generate_token(&session_id)) {
                response.headers_mut().insert("x-csrf-token", value);
            }
        }
    }

    response
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
        HeaderValue::from_static(
            "Content-Type, Authorization, X-Requested-With, X-Request-ID, X-CSRF-Token",
        ),
    );

    response.headers_mut().insert(
        "Access-Control-Expose-Headers",
        HeaderValue::from_static("X-Request-ID, X-CSRF-Token"),
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

pub async fn security_headers_middleware(
    request: Request<Body>,
    next: axum::middleware::Next,
) -> Response {
    let mut response = next.run(request).await;

    response.headers_mut().insert(
        "X-Content-Type-Options",
        HeaderValue::from_static("nosniff"),
    );

    response
        .headers_mut()
        .insert("X-Frame-Options", HeaderValue::from_static("DENY"));

    response.headers_mut().insert(
        "X-XSS-Protection",
        HeaderValue::from_static("1; mode=block"),
    );

    if std::env::var("FORCE_HTTPS")
        .unwrap_or_default()
        .to_lowercase()
        == "true"
    {
        response.headers_mut().insert(
            "Strict-Transport-Security",
            HeaderValue::from_static("max-age=31536000; includeSubDomains"),
        );
    }

    response.headers_mut().insert(
        "Referrer-Policy",
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    );

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
    crate::web::utils::auth::bearer_token_opt(headers)
}

pub async fn rate_limit_middleware(
    State(state): State<crate::web::routes::AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let config = state.services.config.rate_limit.clone();
    let file_config = state
        .rate_limit_config_manager
        .as_ref()
        .map(|m| m.get_config());

    let enabled = file_config
        .as_ref()
        .map(|c| c.enabled)
        .unwrap_or(config.enabled);
    if !enabled {
        return next.run(request).await;
    }

    let path = request.uri().path();
    let exempt_paths = file_config
        .as_ref()
        .map(|c| c.exempt_paths.as_slice())
        .unwrap_or(&config.exempt_paths);
    let exempt_path_prefixes = file_config
        .as_ref()
        .map(|c| c.exempt_path_prefixes.as_slice())
        .unwrap_or(&config.exempt_path_prefixes);

    if exempt_paths.iter().any(|p: &String| p == path)
        || exempt_path_prefixes
            .iter()
            .any(|p: &String| !p.is_empty() && path.starts_with(p))
    {
        return next.run(request).await;
    }

    let ip_header_priority = file_config
        .as_ref()
        .map(|c| c.ip_header_priority.as_slice())
        .unwrap_or(&config.ip_header_priority);
    let ip = extract_client_ip(request.headers(), ip_header_priority)
        .unwrap_or_else(|| "unknown".to_string());

    let (endpoint_id, per_second, burst_size) = match &file_config {
        Some(fc) => {
            let (id, r) = crate::common::rate_limit_config::select_endpoint_rule(fc, path);
            (id, r.per_second, r.burst_size)
        }
        None => {
            let (id, r) =
                crate::common::rate_limit_config::select_endpoint_rule_runtime(&config, path);
            (id, r.per_second, r.burst_size)
        }
    };

    let redis_prefix = state.services.config.redis.key_prefix.as_str();
    let cache_key = format!(
        "{}{}",
        redis_prefix,
        CacheKeyBuilder::ip_rate_limit(&ip, endpoint_id.as_str())
    );

    let fail_open = file_config
        .as_ref()
        .map(|c| c.fail_open_on_error)
        .unwrap_or(config.fail_open_on_error);
    let include_headers = file_config
        .as_ref()
        .map(|c| c.include_headers)
        .unwrap_or(config.include_headers);

    let decision = match state
        .cache
        .rate_limit_token_bucket_take(&cache_key, per_second, burst_size)
        .await
    {
        Ok(d) => d,
        Err(e) => {
            if fail_open {
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

        if include_headers {
            if let Ok(v) = decision.remaining.to_string().parse() {
                response.headers_mut().insert("x-ratelimit-remaining", v);
            }
            if let Ok(v) = burst_size.to_string().parse() {
                response.headers_mut().insert("x-ratelimit-limit", v);
            }
            if let Ok(v) = retry_after_ms.to_string().parse() {
                response.headers_mut().insert("x-ratelimit-after", v);
            }
        }

        return response;
    }

    let mut response = next.run(request).await;
    if include_headers {
        if let Ok(v) = decision.remaining.to_string().parse() {
            response.headers_mut().insert("x-ratelimit-remaining", v);
        }
        if let Ok(v) = burst_size.to_string().parse() {
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
        None => return ApiError::missing_token().into_response(),
    };

    if let Err(err) = state.services.auth_service.validate_token(&token).await {
        return err.into_response();
    }

    next.run(request).await
}

// ============================================================================
// SECTION: Federation Authentication
// ============================================================================

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
        return next.run(request).await;
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
    let token = request
        .headers()
        .get("x-synapse-worker-secret")
        .and_then(|h| h.to_str().ok())
        .unwrap_or_default();
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

// ============================================================================
// SECTION: Federation Request Parsing
// ============================================================================

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

// ============================================================================
// SECTION: Federation Signature Computation
// ============================================================================

fn canonical_federation_request_bytes(
    method: &str,
    uri: &str,
    origin: &str,
    destination: &str,
    content: Option<&Value>,
) -> Vec<u8> {
    let result = crate::federation::signing::canonical_federation_request_bytes(
        method,
        uri,
        origin,
        destination,
        content,
    );
    tracing::debug!(
        "Canonical request bytes: {}",
        String::from_utf8_lossy(&result)
    );
    result
}

async fn verify_federation_signature_with_cache(
    state: &crate::web::routes::AppState,
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
            } else {
                return Err(ApiError::unauthorized(
                    "Cached signature verification failed".to_string(),
                ));
            }
        }
    }

    let result = verify_federation_signature(
        state,
        origin,
        key_id,
        signature,
        signed_bytes,
        key_fetch_priority,
    )
    .await;

    state
        .federation_signature_cache
        .set_signature(&cache_key, result.is_ok());

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
    key_fetch_priority: bool,
) -> Result<(), ApiError> {
    let public_key = get_federation_verify_key(state, origin, key_id, key_fetch_priority).await?;

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
                false,
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
    key_fetch_priority: bool,
) -> Result<[u8; 32], ApiError> {
    let cache_key = format!("federation:verify_key:{}:{}", origin, key_id);
    if let Ok(Some(cached)) = state.cache.get::<String>(&cache_key).await {
        if let Ok(key) = decode_ed25519_public_key(&cached) {
            return Ok(key);
        }
    }

    if origin == state.services.server_name
        || origin == state.services.config.federation.server_name
    {
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
    decode_ed25519_public_key(&fetched)
        .map_err(|_| ApiError::unauthorized("Invalid public key".to_string()))
}

async fn get_local_verify_key(
    state: &crate::web::routes::AppState,
    key_id: &str,
) -> Option<[u8; 32]> {
    let config = &state.services.config.federation;

    if !config.enabled {
        return None;
    }

    let config_key_id = config.key_id.as_deref().unwrap_or("ed25519:1");
    if key_id != config_key_id {
        if state
            .services
            .key_rotation_manager
            .load_or_create_key()
            .await
            .is_err()
        {
            return None;
        }

        let current_key = state
            .services
            .key_rotation_manager
            .get_current_key()
            .await
            .ok()
            .flatten()?;

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

    if state
        .services
        .key_rotation_manager
        .load_or_create_key()
        .await
        .is_err()
    {
        return None;
    }

    let current_key = state
        .services
        .key_rotation_manager
        .get_current_key()
        .await
        .ok()
        .flatten()?;

    if current_key.key_id != key_id {
        return None;
    }

    decode_ed25519_public_key(&current_key.public_key).ok()
}

async fn fetch_federation_verify_key(
    state: &crate::web::routes::AppState,
    origin: &str,
    key_id: &str,
    key_fetch_priority: bool,
) -> Result<String, ApiError> {
    let backoff_key = format!("federation:key_fetch_backoff:{}:{}", origin, key_id);
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
        .expect("semaphore closed");

    let timeout_ms = state.services.config.federation.key_fetch_timeout_ms.max(1);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(timeout_ms))
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

    let _ = state.cache.set(&backoff_key, true, 30).await;
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

// ============================================================================
// SECTION: Rate Limiting Helpers
// ============================================================================

pub async fn panic_catcher_middleware(request: Request<Body>, next: Next) -> Response {
    let method = request.method().to_string();
    let path = request.uri().path().to_string();

    tracing::debug!("Processing request: {} {}", method, path);

    let response = next.run(request).await;

    tracing::debug!(
        "Completed request: {} {} - {}",
        method,
        path,
        response.status()
    );

    response
}

pub async fn request_timeout_middleware(request: Request<Body>, next: Next) -> Response {
    let timeout_secs: u64 = std::env::var("REQUEST_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(30);

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(timeout_secs),
        next.run(request),
    )
    .await;

    match result {
        Ok(response) => response,
        Err(_) => {
            tracing::warn!("Request timeout after {}s", timeout_secs);
            (
                StatusCode::REQUEST_TIMEOUT,
                Json(json!({
                    "errcode": "M_LIMIT_EXCEEDED",
                    "error": "Request timeout",
                    "retry_after_ms": timeout_secs * 1000
                })),
            )
                .into_response()
        }
    }
}

pub async fn request_id_middleware(request: Request<Body>, next: Next) -> Response {
    let request_id = request
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("req-{}", uuid::Uuid::new_v4()));

    let mut response = next.run(request).await;

    if let Ok(v) = HeaderValue::from_str(&request_id) {
        response.headers_mut().insert("x-request-id", v);
    }

    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::{CacheConfig, CacheManager};
    use crate::common::config::{
        RateLimitConfig, RateLimitEndpointRule, RateLimitMatchType, RateLimitRule,
    };
    use crate::services::ServiceContainer;
    use crate::test_utils::{env_lock, EnvGuard};
    use crate::web::routes::AppState;
    use crate::web::utils::ip::extract_client_ip;
    use ed25519_dalek::Signer;
    use std::sync::Arc;
    use std::time::Duration;

    #[test]
    fn test_extract_client_ip() {
        let mut headers = HeaderMap::new();
        let priority = vec!["x-forwarded-for".to_string(), "x-real-ip".to_string()];

        // Test X-Forwarded-For
        headers.insert(
            "x-forwarded-for",
            "1.2.3.4, 5.6.7.8".parse().expect("valid header value"),
        );
        assert_eq!(
            extract_client_ip(&headers, &priority),
            Some("1.2.3.4".to_string())
        );

        // Test X-Real-IP
        headers = HeaderMap::new();
        headers.insert("x-real-ip", "10.0.0.1".parse().expect("valid header value"));
        assert_eq!(
            extract_client_ip(&headers, &priority),
            Some("10.0.0.1".to_string())
        );

        // Test Priority (X-Forwarded-For > X-Real-IP)
        headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            "1.2.3.4".parse().expect("valid header value"),
        );
        headers.insert("x-real-ip", "10.0.0.1".parse().expect("valid header value"));
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
            "for=192.0.2.60;proto=http;by=203.0.113.43"
                .parse()
                .expect("valid header value"),
        );
        assert_eq!(
            extract_client_ip(&headers, &priority),
            Some("192.0.2.60".to_string())
        );

        headers = HeaderMap::new();
        headers.insert(
            "forwarded",
            "for=\"[2001:db8:cafe::17]:4711\""
                .parse()
                .expect("valid header value"),
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
        let (id, rule) = crate::common::rate_limit_config::select_endpoint_rule_runtime(
            &config,
            "/_matrix/client/r0/login",
        );
        assert_eq!(id, "/_matrix/client/r0/login");
        assert_eq!(rule.per_second, 5);

        // Longest prefix match
        let (id, rule) = crate::common::rate_limit_config::select_endpoint_rule_runtime(
            &config,
            "/_matrix/client/r0/sync?since=123",
        );
        assert_eq!(id, "/_matrix/client/r0/sync");
        assert_eq!(rule.per_second, 20);

        // Shorter prefix match
        let (id, rule) = crate::common::rate_limit_config::select_endpoint_rule_runtime(
            &config,
            "/_matrix/client/versions",
        );
        assert_eq!(id, "/_matrix/client");
        assert_eq!(rule.per_second, 50);

        // Default fallback
        let (id, rule) =
            crate::common::rate_limit_config::select_endpoint_rule_runtime(&config, "/other/path");
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
    fn test_parse_x_matrix_authorization_header() {
        let params = parse_x_matrix_authorization(
            r#"X-Matrix origin="test.example.com", key="ed25519:test", sig="abc123""#,
        )
        .expect("header should parse");

        assert_eq!(params.origin, "test.example.com");
        assert_eq!(params.key, "ed25519:test");
        assert_eq!(params.sig, "abc123");
    }

    #[tokio::test]
    async fn test_verify_federation_signature_with_local_config_key() {
        let signing_key_bytes = [7u8; 32];
        let signing_key_b64 =
            base64::engine::general_purpose::STANDARD_NO_PAD.encode(signing_key_bytes);
        let key_id = "ed25519:test".to_string();
        let origin = "test.example.com".to_string();
        let body = serde_json::json!({
            "invite": {
                "display_name": "Bridge Invite"
            }
        });
        let uri = "/_matrix/federation/v1/exchange_third_party_invite/!room:test.example.com";

        let mut services = ServiceContainer::new_test();
        services.config.federation.enabled = true;
        services.config.federation.allow_ingress = true;
        services.config.federation.server_name = origin.clone();
        services.config.federation.key_id = Some(key_id.clone());
        services.config.federation.signing_key = Some(signing_key_b64);
        services.server_name = origin.clone();

        let cache = Arc::new(CacheManager::new(CacheConfig::default()));
        let state = AppState::new(services, cache);

        let signed_bytes =
            canonical_federation_request_bytes("PUT", uri, &origin, &origin, Some(&body));
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_bytes);
        let signature = signing_key.sign(&signed_bytes);
        let signature_b64 =
            base64::engine::general_purpose::STANDARD_NO_PAD.encode(signature.to_bytes());

        let header = format!(
            "X-Matrix origin=\"{}\", key=\"{}\", sig=\"{}\"",
            origin, key_id, signature_b64
        );
        let params = parse_x_matrix_authorization(&header).expect("header should parse");

        verify_federation_signature_with_cache(
            &state,
            &params.origin,
            &params.key,
            &params.sig,
            &signed_bytes,
            false,
        )
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

    #[test]
    fn test_cors_security_report_development_mode() {
        let _env_lock = env_lock();
        let mut env_guard = EnvGuard::new();
        env_guard.set("RUST_ENV", "development");
        env_guard.remove("ALLOWED_ORIGINS");

        let report = check_cors_security();

        assert!(report.is_development_mode);
        assert!(report.has_issues());
        assert!(!report.warnings.is_empty());
    }

    #[test]
    fn test_cors_security_report_production_with_wildcard() {
        std::thread::spawn(|| {
            let _env_lock = env_lock();
            let mut env_guard = EnvGuard::new();
            env_guard.set("RUST_ENV", "production");
            env_guard.set("ALLOWED_ORIGINS", "*");

            let report = check_cors_security();

            assert!(!report.is_development_mode, "Should not be in dev mode");
            assert!(
                report.allows_any_origin,
                "Should allow any origin with wildcard"
            );
            assert!(
                !report.errors.is_empty(),
                "Should have errors with wildcard in production"
            );

            let validation = validate_cors_config_for_production();
            assert!(
                validation.is_err(),
                "Validation should fail with wildcard origin in production: {:?}",
                validation
            );
        })
        .join()
        .expect("Thread panicked");
    }

    #[test]
    fn test_cors_security_report_production_no_origins() {
        let _env_lock = env_lock();
        let mut env_guard = EnvGuard::new();
        env_guard.set("RUST_ENV", "production");
        env_guard.remove("ALLOWED_ORIGINS");
        env_guard.remove("CORS_ORIGIN_PATTERN");

        let report = check_cors_security();

        assert!(!report.is_development_mode);
        assert!(report.allowed_origins.is_empty());
        assert!(!report.has_pattern);
        assert!(!report.errors.is_empty());
    }

    #[test]
    fn test_cors_security_report_production_with_specific_origins() {
        let _env_lock = env_lock();
        let mut env_guard = EnvGuard::new();
        env_guard.set("RUST_ENV", "production");
        env_guard.set(
            "ALLOWED_ORIGINS",
            "https://example.com,https://app.example.com",
        );

        let report = check_cors_security();

        assert!(!report.is_development_mode);
        assert!(!report.allows_any_origin);
        assert_eq!(report.allowed_origins.len(), 2);
        assert!(report
            .allowed_origins
            .contains(&"https://example.com".to_string()));
        assert!(report
            .allowed_origins
            .contains(&"https://app.example.com".to_string()));

        let validation = validate_cors_config_for_production();
        assert!(validation.is_ok());
    }

    #[test]
    fn test_csrf_token_round_trip() {
        let manager = CsrfTokenManager::new("secret".to_string());
        let token = manager.generate_token("session-123");

        assert!(manager.validate_token(&token, "session-123"));
        assert!(!manager.validate_token(&token, "other-session"));
    }

    #[test]
    fn test_csrf_token_expiration_is_enforced() {
        let manager = CsrfTokenManager {
            secret: "secret".to_string(),
            token_ttl: Duration::from_secs(1),
        };
        let old_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_secs()
            .saturating_sub(10);
        let payload = format!("session-123:{}", old_timestamp);
        let signature =
            crate::common::crypto::compute_hash(format!("{}{}", payload, manager.secret));
        let token = format!("{}:{}", payload, &signature[..16]);

        assert!(!manager.validate_token(&token, "session-123"));
    }

    #[test]
    fn test_extract_session_id_for_csrf_prefers_cookie_over_authorization() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            "Bearer access-token".parse().expect("valid auth header"),
        );
        headers.insert(
            "cookie",
            "sid=session-cookie".parse().expect("valid cookie header"),
        );

        assert_eq!(
            extract_session_id_for_csrf(&headers),
            Some("sid=session-cookie".to_string())
        );

        let mut cookie_only_headers = HeaderMap::new();
        cookie_only_headers.insert(
            "cookie",
            "sid=session-cookie".parse().expect("valid cookie header"),
        );
        assert_eq!(
            extract_session_id_for_csrf(&cookie_only_headers),
            Some("sid=session-cookie".to_string())
        );

        let mut auth_only_headers = HeaderMap::new();
        auth_only_headers.insert(
            "authorization",
            "Bearer access-token".parse().expect("valid auth header"),
        );
        assert_eq!(
            extract_session_id_for_csrf(&auth_only_headers),
            Some("Bearer access-token".to_string())
        );
    }

    #[test]
    fn test_extract_origin_candidate_uses_origin_or_referer() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "origin",
            "https://app.example.com"
                .parse()
                .expect("valid origin header"),
        );
        assert_eq!(
            extract_origin_candidate(&headers),
            Some("https://app.example.com".to_string())
        );

        let mut referer_headers = HeaderMap::new();
        referer_headers.insert(
            "referer",
            "https://app.example.com/path?query=1"
                .parse()
                .expect("valid referer header"),
        );
        assert_eq!(
            extract_origin_candidate(&referer_headers),
            Some("https://app.example.com".to_string())
        );
    }

    #[test]
    fn test_same_origin_uses_forwarded_host_and_proto() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-host",
            "matrix.example.com".parse().expect("valid host header"),
        );
        headers.insert(
            "x-forwarded-proto",
            "https".parse().expect("valid proto header"),
        );

        assert!(same_origin("https://matrix.example.com", &headers));
        assert!(!same_origin("https://other.example.com", &headers));
    }

    #[test]
    fn test_validate_bind_address_for_dev_mode_local() {
        let _env_lock = env_lock();
        let mut env_guard = EnvGuard::new();
        env_guard.set("RUST_ENV", "development");

        assert!(validate_bind_address_for_dev_mode("127.0.0.1").is_ok());
        assert!(validate_bind_address_for_dev_mode("localhost").is_ok());
        assert!(validate_bind_address_for_dev_mode("::1").is_ok());
        assert!(validate_bind_address_for_dev_mode("0.0.0.0").is_ok());
        assert!(validate_bind_address_for_dev_mode("127.0.0.5").is_ok());
    }

    #[test]
    fn test_validate_bind_address_for_dev_mode_non_local() {
        let _env_lock = env_lock();
        let mut env_guard = EnvGuard::new();
        env_guard.set("RUST_ENV", "development");

        let result = validate_bind_address_for_dev_mode("192.168.1.1");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("Development mode should only bind to local addresses"));

        let result = validate_bind_address_for_dev_mode("example.com");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_bind_address_for_production_mode() {
        let _env_lock = env_lock();
        let mut env_guard = EnvGuard::new();
        env_guard.set("RUST_ENV", "production");

        assert!(validate_bind_address_for_dev_mode("0.0.0.0").is_ok());
        assert!(validate_bind_address_for_dev_mode("192.168.1.1").is_ok());
        assert!(validate_bind_address_for_dev_mode("example.com").is_ok());
    }

    #[test]
    fn test_cors_security_report_has_issues() {
        let report_with_errors = CorsSecurityReport {
            is_development_mode: false,
            allows_any_origin: true,
            allowed_origins: vec!["*".to_string()],
            has_pattern: false,
            warnings: vec![],
            errors: vec!["Test error".to_string()],
        };
        assert!(report_with_errors.has_issues());

        let report_with_warnings = CorsSecurityReport {
            is_development_mode: true,
            allows_any_origin: true,
            allowed_origins: vec![],
            has_pattern: false,
            warnings: vec!["Test warning".to_string()],
            errors: vec![],
        };
        assert!(report_with_warnings.has_issues());

        let report_clean = CorsSecurityReport {
            is_development_mode: false,
            allows_any_origin: false,
            allowed_origins: vec!["https://example.com".to_string()],
            has_pattern: false,
            warnings: vec![],
            errors: vec![],
        };
        assert!(!report_clean.has_issues());
    }
}
