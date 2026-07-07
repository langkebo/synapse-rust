use crate::cache::*;
use crate::common::error::ApiError;
use crate::common::RateLimitBackend;
use crate::web::routes::context::CoreContext;
use crate::web::utils::ip::extract_client_ip;
use axum::extract::State;
use axum::http::{HeaderValue, Request};
use axum::response::{IntoResponse, Response};
use axum::{body::Body, middleware::Next};

fn is_sync_rate_limit_exempt_path(path: &str) -> bool {
    matches!(
        path,
        "/_matrix/client/r0/sync"
            | "/_matrix/client/v1/sync"
            | "/_matrix/client/v3/sync"
            | "/_matrix/client/unstable/org.matrix.msc3575/sync"
            | "/_matrix/client/unstable/org.matrix.simplified_msc3575/sync"
    )
}

pub async fn rate_limit_middleware(State(ctx): State<CoreContext>, request: Request<Body>, next: Next) -> Response {
    let config = ctx.config.rate_limit.clone();
    let file_config = ctx.rate_limit_config();

    let enabled = file_config.as_ref().map_or(config.enabled, |c| c.enabled);
    if !enabled {
        return next.run(request).await;
    }

    let path = request.uri().path();
    let exempt_paths = file_config.as_ref().map_or(&config.exempt_paths, |c| &c.exempt_paths);
    let exempt_path_prefixes = file_config.as_ref().map_or(&config.exempt_path_prefixes, |c| &c.exempt_path_prefixes);

    if is_sync_rate_limit_exempt_path(path)
        || exempt_paths.iter().any(|p: &String| p == path)
        || exempt_path_prefixes.iter().any(|p: &String| !p.is_empty() && path.starts_with(p))
    {
        return next.run(request).await;
    }

    let ip_header_priority = file_config.as_ref().map_or(&config.ip_header_priority, |c| &c.ip_header_priority);
    let ip = extract_client_ip(request.headers(), ip_header_priority).unwrap_or_else(|| "unknown".to_string());

    let (endpoint_id, per_second, burst_size) = match &file_config {
        Some(fc) => {
            let (id, r) = crate::common::select_endpoint_rule(fc, path);
            (id, r.per_second, r.burst_size)
        }
        None => {
            let (id, r) = crate::common::select_endpoint_rule_runtime(&config, path);
            (id, r.per_second, r.burst_size)
        }
    };

    let redis_prefix = ctx.config.redis.key_prefix.as_str();
    let cache_key = format!("{}{}", redis_prefix, CacheKeyBuilder::ip_rate_limit(&ip, endpoint_id.as_str()));

    let fail_open = file_config.as_ref().map_or(config.fail_open_on_error, |c| c.fail_open_on_error);
    let include_headers = file_config.as_ref().map_or(config.include_headers, |c| c.include_headers);

    // Determine the configured backend and whether Redis is actually available.
    let backend = file_config.as_ref().map_or(RateLimitBackend::Auto, |c| c.backend);
    let redis_available = ctx.cache.is_redis_enabled();

    // When backend is explicitly "redis" but Redis is not available, reject
    // the request rather than silently falling back to an inconsistent
    // in-memory bucket (which would defeat the purpose of the "redis" setting).
    if matches!(backend, RateLimitBackend::Redis) && !redis_available {
        tracing::error!(
            "Rate limit backend is set to 'redis' but Redis is not available. \
             Rejecting request to avoid inconsistent multi-worker rate limiting."
        );
        if fail_open {
            return next.run(request).await;
        }
        return ApiError::rate_limited("").into_response();
    }

    let decision = match ctx.cache.rate_limit_token_bucket_take(&cache_key, per_second, burst_size).await {
        Ok(d) => d,
        Err(e) => {
            if fail_open {
                tracing::warn!("Rate limiter error, allowing request: {}", e);
                return next.run(request).await;
            }
            return ApiError::rate_limited("").into_response();
        }
    };

    if !decision.allowed {
        let retry_after_ms = decision.retry_after_seconds.saturating_mul(1000);
        let mut response = ApiError::rate_limited_with_retry(retry_after_ms).into_response();
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
            if let Ok(v) = HeaderValue::from_str(&retry_after_ms.to_string()) {
                response.headers_mut().insert("x-ratelimit-retry-after", v.clone());
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
        response.headers_mut().insert("x-ratelimit-retry-after", HeaderValue::from_static("0"));
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "test-utils")]
    use crate::cache::{CacheConfig, CacheManager};
    use crate::common::config::{RateLimitConfig, RateLimitEndpointRule, RateLimitMatchType, RateLimitRule};
    #[cfg(feature = "test-utils")]
    use crate::web::routes::AppState;
    use crate::web::utils::ip::extract_client_ip;
    #[cfg(feature = "test-utils")]
    use axum::http::StatusCode;
    #[cfg(feature = "test-utils")]
    use axum::{middleware, routing::get, Router};
    #[cfg(feature = "test-utils")]
    use std::sync::Arc;
    #[cfg(feature = "test-utils")]
    use synapse_services::ServiceContainer;
    #[cfg(feature = "test-utils")]
    use tower::ServiceExt;

    #[test]
    fn test_extract_client_ip() {
        let mut headers = axum::http::HeaderMap::new();
        let priority = vec!["x-forwarded-for".to_string(), "x-real-ip".to_string()];

        headers.insert("x-forwarded-for", "1.2.3.4, 5.6.7.8".parse().expect("valid header value"));
        assert_eq!(extract_client_ip(&headers, &priority), Some("1.2.3.4".to_string()));

        headers = axum::http::HeaderMap::new();
        headers.insert("x-real-ip", "10.0.0.1".parse().expect("valid header value"));
        assert_eq!(extract_client_ip(&headers, &priority), Some("10.0.0.1".to_string()));

        headers = axum::http::HeaderMap::new();
        headers.insert("x-forwarded-for", "1.2.3.4".parse().expect("valid header value"));
        headers.insert("x-real-ip", "10.0.0.1".parse().expect("valid header value"));
        assert_eq!(extract_client_ip(&headers, &priority), Some("1.2.3.4".to_string()));
    }

    #[test]
    fn test_is_sync_rate_limit_exempt_path() {
        assert!(is_sync_rate_limit_exempt_path("/_matrix/client/r0/sync"));
        assert!(is_sync_rate_limit_exempt_path("/_matrix/client/v1/sync"));
        assert!(is_sync_rate_limit_exempt_path("/_matrix/client/v3/sync"));
        assert!(is_sync_rate_limit_exempt_path("/_matrix/client/unstable/org.matrix.msc3575/sync"));
        assert!(is_sync_rate_limit_exempt_path("/_matrix/client/unstable/org.matrix.simplified_msc3575/sync"));
        assert!(!is_sync_rate_limit_exempt_path("/_matrix/client/v3/events"));
    }

    #[test]
    fn test_extract_client_ip_forwarded() {
        let mut headers = axum::http::HeaderMap::new();
        let priority = vec!["forwarded".to_string()];

        headers.insert("forwarded", "for=192.0.2.60;proto=http;by=203.0.113.43".parse().expect("valid header value"));
        assert_eq!(extract_client_ip(&headers, &priority), Some("192.0.2.60".to_string()));

        headers = axum::http::HeaderMap::new();
        headers.insert("forwarded", "for=\"[2001:db8:cafe::17]:4711\"".parse().expect("valid header value"));
        assert_eq!(extract_client_ip(&headers, &priority), Some("2001:db8:cafe::17".to_string()));
    }

    #[test]
    fn test_select_endpoint_rule() {
        let mut config = RateLimitConfig::default();
        config.endpoints.push(RateLimitEndpointRule {
            path: "/_matrix/client/r0/login".to_string(),
            match_type: RateLimitMatchType::Exact,
            rule: RateLimitRule { per_second: 5, burst_size: 10 },
        });
        config.endpoints.push(RateLimitEndpointRule {
            path: "/_matrix/client".to_string(),
            match_type: RateLimitMatchType::Prefix,
            rule: RateLimitRule { per_second: 50, burst_size: 100 },
        });
        config.endpoints.push(RateLimitEndpointRule {
            path: "/_matrix/client/r0/sync".to_string(),
            match_type: RateLimitMatchType::Prefix,
            rule: RateLimitRule { per_second: 20, burst_size: 40 },
        });

        let (id, rule) = crate::common::select_endpoint_rule_runtime(&config, "/_matrix/client/r0/login");
        assert_eq!(id, "/_matrix/client/r0/login");
        assert_eq!(rule.per_second, 5);

        let (id, rule) = crate::common::select_endpoint_rule_runtime(&config, "/_matrix/client/r0/sync?since=123");
        assert_eq!(id, "/_matrix/client/r0/sync");
        assert_eq!(rule.per_second, 20);

        let (id, rule) = crate::common::select_endpoint_rule_runtime(&config, "/_matrix/client/versions");
        assert_eq!(id, "/_matrix/client");
        assert_eq!(rule.per_second, 50);

        let (id, rule) = crate::common::select_endpoint_rule_runtime(&config, "/other/path");
        assert_eq!(id, "/other/path");
        assert_eq!(rule.per_second, config.default.per_second);
    }

    #[cfg(feature = "test-utils")]
    #[tokio::test]
    async fn test_rate_limit_middleware_exempts_sync_endpoints() {
        async fn ok_handler() -> StatusCode {
            StatusCode::OK
        }

        let mut services = ServiceContainer::new_test().await;
        services.core.config.rate_limit = RateLimitConfig {
            enabled: true,
            default: RateLimitRule { per_second: 1, burst_size: 1 },
            endpoints: vec![RateLimitEndpointRule {
                path: "/".to_string(),
                match_type: RateLimitMatchType::Prefix,
                rule: RateLimitRule { per_second: 1, burst_size: 1 },
            }],
            ..RateLimitConfig::default()
        };

        let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
        let state = AppState::new(services, cache);

        let app = Router::new()
            .route("/_matrix/client/v3/sync", get(ok_handler))
            .route("/rooms/test/send", get(ok_handler))
            .layer(middleware::from_fn_with_state(state.clone(), rate_limit_middleware))
            .with_state(state);

        let sync_request = || {
            Request::builder()
                .method(axum::http::Method::GET)
                .uri("/_matrix/client/v3/sync")
                .header("x-forwarded-for", "1.2.3.4")
                .body(Body::empty())
                .expect("request should build")
        };
        let normal_request = || {
            Request::builder()
                .method(axum::http::Method::GET)
                .uri("/rooms/test/send")
                .header("x-forwarded-for", "1.2.3.4")
                .body(Body::empty())
                .expect("request should build")
        };

        let sync_response_1 = app.clone().oneshot(sync_request()).await.expect("sync request should succeed");
        let sync_response_2 = app.clone().oneshot(sync_request()).await.expect("second sync request should succeed");
        let normal_response_1 = app.clone().oneshot(normal_request()).await.expect("normal request should succeed");
        let normal_response_2 =
            app.oneshot(normal_request()).await.expect("second normal request should return a response");

        assert_eq!(sync_response_1.status(), StatusCode::OK);
        assert_eq!(sync_response_2.status(), StatusCode::OK);
        assert_eq!(normal_response_1.status(), StatusCode::OK);
        assert_eq!(normal_response_2.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[cfg(feature = "test-utils")]
    #[tokio::test]
    async fn test_rate_limit_middleware_sets_retry_after_headers() {
        async fn ok_handler() -> StatusCode {
            StatusCode::OK
        }

        let mut services = ServiceContainer::new_test().await;
        services.core.config.rate_limit = RateLimitConfig {
            enabled: true,
            default: RateLimitRule { per_second: 1, burst_size: 1 },
            endpoints: vec![RateLimitEndpointRule {
                path: "/limited".to_string(),
                match_type: RateLimitMatchType::Exact,
                rule: RateLimitRule { per_second: 1, burst_size: 1 },
            }],
            ..RateLimitConfig::default()
        };

        let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
        let state = AppState::new(services, cache);

        let app = Router::new()
            .route("/limited", get(ok_handler))
            .layer(middleware::from_fn_with_state(state.clone(), rate_limit_middleware))
            .with_state(state);

        let request = || {
            Request::builder()
                .method(axum::http::Method::GET)
                .uri("/limited")
                .header("x-forwarded-for", "1.2.3.4")
                .body(Body::empty())
                .expect("request should build")
        };

        let first = app.clone().oneshot(request()).await.expect("first request should succeed");
        assert_eq!(first.status(), StatusCode::OK);
        assert_eq!(first.headers().get("x-ratelimit-retry-after").unwrap(), "0");

        let second = app.oneshot(request()).await.expect("second request should return a response");
        assert_eq!(second.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(second.headers().get("retry-after").is_some());
        assert!(second.headers().get("x-ratelimit-retry-after").is_some());
        assert!(second.headers().get("x-ratelimit-after").is_some());
    }
}
