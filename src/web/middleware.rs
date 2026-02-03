use crate::cache::*;
use crate::common::ApiError;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, Request, StatusCode};
use axum::response::IntoResponse;
use axum::{body::Body, middleware::Next, response::Response};
use base64::Engine;
use serde_json::json;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

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

pub async fn cors_middleware(request: Request<Body>, next: axum::middleware::Next) -> Response {
    let mut response = next.run(request).await;

    let headers = response.headers_mut();
    headers.insert("Access-Control-Allow-Origin", HeaderValue::from_static("*"));
    headers.insert(
        "Access-Control-Allow-Methods",
        HeaderValue::from_static("GET, POST, PUT, DELETE, OPTIONS"),
    );
    headers.insert(
        "Access-Control-Allow-Headers",
        HeaderValue::from_static("Content-Type, Authorization, X-Requested-With"),
    );
    headers.insert("Access-Control-Max-Age", HeaderValue::from_static("86400"));

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

    let public_key = match get_federation_verify_key(&state, &params.origin, &params.key).await {
        Ok(k) => k,
        Err(e) => return e.into_response(),
    };

    let signature = match decode_ed25519_signature(&params.sig) {
        Ok(sig) => sig,
        Err(_) => return ApiError::unauthorized("Invalid signature".to_string()).into_response(),
    };

    let verifying_key = match ed25519_dalek::VerifyingKey::from_bytes(&public_key) {
        Ok(k) => k,
        Err(_) => return ApiError::unauthorized("Invalid public key".to_string()).into_response(),
    };

    if verifying_key
        .verify_strict(&signed_bytes, &signature)
        .is_err()
    {
        tracing::warn!(
            "Unauthorized federation request from {:?}. Server name: {}",
            parts
                .headers
                .get("x-forwarded-for")
                .or(parts.headers.get("host")),
            state.services.server_name
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
}
