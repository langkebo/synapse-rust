use crate::cache::*;
use axum::extract::State;
use axum::http::{HeaderMap, Request, StatusCode};
use axum::{body::Body, middleware::Next, response::Response};
use serde_json::json;
use std::sync::Arc;
use std::time::Instant;

pub async fn logging_middleware(request: Request<Body>, next: axum::middleware::Next) -> Response {
    let start = Instant::now();
    let method = request.method().clone();
    let uri = request.uri().clone();
    let headers = request.headers().clone();

    let user_id = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.to_string());

    let response = next.run(request).await;

    let duration = start.elapsed();
    let status = response.status();

    tracing::info!(
        "Request: {} {} {} {} {:?} {}ms",
        user_id.unwrap_or_else(|| "anonymous".to_string()),
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
    headers.insert("Access-Control-Allow-Origin", "*".parse().unwrap());
    headers.insert(
        "Access-Control-Allow-Methods",
        "GET, POST, PUT, DELETE, OPTIONS".parse().unwrap(),
    );
    headers.insert(
        "Access-Control-Allow-Headers",
        "Content-Type, Authorization, X-Requested-With"
            .parse()
            .unwrap(),
    );
    headers.insert("Access-Control-Max-Age", "86400".parse().unwrap());

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
            tracing::warn!("Rate limiter error, allowing request: {}", e);
            return next.run(request).await;
        }
    };

    if !decision.allowed {
        let retry_after_ms = decision.retry_after_seconds.saturating_mul(1000);
        let mut response = Response::builder()
            .status(StatusCode::TOO_MANY_REQUESTS)
            .header("content-type", "application/json")
            .header("retry-after", decision.retry_after_seconds.to_string())
            .body(Body::from(
                json!({
                    "errcode": "M_LIMIT_EXCEEDED",
                    "error": "Rate limited",
                    "retry_after_ms": retry_after_ms
                })
                .to_string(),
            ))
            .unwrap();

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
    request: Request<Body>,
    next: axum::middleware::Next,
    _state: Arc<crate::web::routes::AppState>,
) -> Result<Response, StatusCode> {
    let token = extract_token(request.headers());

    if token.is_none() {
        return Ok(Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .body(Body::from(
                r#"{"errcode": "UNAUTHORIZED", "error": "Missing access token"}"#,
            ))
            .unwrap());
    }

    Ok(next.run(request).await)
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
                return Some(original.split(':').next().unwrap().to_string());
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
}
