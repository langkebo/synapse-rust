use crate::routes::context::CoreContext;
use crate::utils::ip::effective_client_ip;
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderValue, Request};
use axum::response::{IntoResponse, Response};
use axum::{body::Body, middleware::Next};
use std::net::SocketAddr;
use synapse_cache::*;
use synapse_common::error::ApiError;
use synapse_common::RateLimitBackend;

/// See [`rate_limit_middleware`].
pub async fn rate_limit_middleware(State(ctx): State<CoreContext>, request: Request<Body>, next: Next) -> Response {
    // 配置启动后只读，无理由 clone：原版每次请求深拷 5 个堆分配字段
    // （Vec<Rule>、Vec<String>、Vec<String>、Vec<String>、HashMap<String,String>）。
    // 改为引用后，仅末尾少量需要 owned 的字段（endpoints）走引用或 clone。
    let config = &ctx.config.rate_limit;
    let file_config = ctx.rate_limit_config();

    let enabled = file_config.as_ref().map_or(config.enabled, |c| c.enabled);
    if !enabled {
        return next.run(request).await;
    }

    let path = request.uri().path();
    let exempt_paths = file_config.as_ref().map_or(&config.exempt_paths, |c| &c.exempt_paths);
    let exempt_path_prefixes = file_config.as_ref().map_or(&config.exempt_path_prefixes, |c| &c.exempt_path_prefixes);

    // W7+: 指标句柄。由 CacheManager 的 OnceLock 缓存，get_or_init 之后
    // 只是原子读；后续 `inc()` 是 AtomicU64::fetch_add(Relaxed)，热路径无锁。
    let rl_metrics = ctx.cache.rate_limit_metrics(&ctx.metrics);

    // B-4: Check the auto-derived exempt list from the route ledger first,
    // then fall back to config-based exempt_paths and exempt_path_prefixes.
    if ctx.rate_limit_exempt_paths.contains(&path)
        || exempt_paths.iter().any(|p: &String| p == path)
        || exempt_path_prefixes.iter().any(|p: &String| !p.is_empty() && path.starts_with(p))
    {
        // W7+: 豁免路径连判定都没做，单独计数且不进 `requests_total`
        // 分母——否则 exempt 流量会稀释限流率，让「限流是否生效」看起来
        // 比实际更宽松。总请求数 = requests_total + exempt_total。
        rl_metrics.exempt_total.inc();
        return next.run(request).await;
    }

    rl_metrics.requests_total.inc();

    let ip_header_priority = file_config.as_ref().map_or(&config.ip_header_priority, |c| &c.ip_header_priority);
    let peer_addr = request.extensions().get::<ConnectInfo<SocketAddr>>().map(|c| c.0);
    let trusted_proxies = file_config.as_ref().map_or(&config.trusted_proxies, |c| &c.trusted_proxies);
    let trust_forwarded = file_config.as_ref().map_or(config.trust_forwarded, |c| c.trust_forwarded);
    // Shared with the login lockout (`routes::auth_compat`): both must attribute a
    // request to the same address, or one of them can be steered by a spoofed header.
    let ip = effective_client_ip(request.headers(), peer_addr, trust_forwarded, ip_header_priority, trusted_proxies);

    let (endpoint_id, per_second, burst_size) = match &file_config {
        Some(fc) => {
            let (id, r) = synapse_common::select_endpoint_rule(fc, path);
            (id, r.per_second, r.burst_size)
        }
        None => {
            let (id, r) = synapse_common::select_endpoint_rule_runtime(config, path);
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
            target: "rate_limit",
            backend = "redis",
            redis_available = false,
            fail_open = fail_open,
            "Rate limit backend is set to 'redis' but Redis is not available"
        );
        if fail_open {
            // W7+: 放行 = 限流此刻形同虚设，单独计数（应告警）
            rl_metrics.fail_open_total.inc();
            return next.run(request).await;
        }
        // W7+: 硬拒绝 = Redis 一挂全站 429，单独计数（应告警）
        rl_metrics.fail_closed_total.inc();
        // OBS-05 (P2): fail_closed 仅 inc counter 不够，必须留痕 error 日志，
        // 否则运维侧无法在告警系统未配置 metrics scraping 时感知 Redis 故障。
        tracing::error!(
            target: "rate_limit",
            event = "rate_limit_fail_closed",
            backend = "redis",
            "Rejecting request because rate limit Redis backend is unavailable"
        );
        return ApiError::rate_limited("").into_response();
    }

    let decision = match ctx.cache.rate_limit_token_bucket_take(&cache_key, per_second, burst_size).await {
        Ok(d) => d,
        Err(e) => {
            if fail_open {
                // A4: 加攻击特征字段（client_ip + path + endpoint + retry_after）。
                // rate_limit middleware 早于 auth middleware 执行，无法在此处
                // 解析 user_id；用 is_authenticated（Authorization header 存在性）
                // 替代，运维可在 production 日志里定位"未认证刷量"vs"已认证刷量"。
                let is_authenticated = request.headers().contains_key(axum::http::header::AUTHORIZATION);
                tracing::warn!(
                    target: "rate_limit",
                    event = "rate_limit_fail_open",
                    client_ip = %ip,
                    request_path = %request.uri().path(),
                    endpoint = %endpoint_id,
                    is_authenticated,
                    error = %e,
                    "Rate limiter error, allowing request"
                );
                rl_metrics.fail_open_total.inc();
                return next.run(request).await;
            }
            rl_metrics.fail_closed_total.inc();
            return ApiError::rate_limited("").into_response();
        }
    };

    if !decision.allowed {
        // W7+: 429 拒绝（token bucket 耗尽）——与 fail_closed 分开计：
        // 前者是限流在正常工作，后者是限流后端自身故障。混在一起会让
        // 「限流生效了」和「限流挂了」看起来一样。
        rl_metrics.rejected_total.inc();
        // A4: 升级到 warn + 攻击特征字段。429 在 attack 场景（登录爆破、CC）
        // 是真实安全事件，运维需要能在默认 RUST_LOG=info 下看到。生产
        // 高峰正常限流可用 `RUST_LOG=rate_limit=info` 关闭此告警。
        //
        // user_id 不可用：auth middleware 在 rate_limit 之后执行。改用
        // is_authenticated（Authorization header 存在性）嗅探未认证/已认证
        // 两种攻击模式。
        let is_authenticated = request.headers().contains_key(axum::http::header::AUTHORIZATION);
        tracing::warn!(
            target: "rate_limit",
            event = "rate_limit_rejected",
            client_ip = %ip,
            request_path = %request.uri().path(),
            endpoint = %endpoint_id,
            is_authenticated,
            per_second,
            burst_size,
            retry_after_seconds = decision.retry_after_seconds,
            "rate limit rejected request"
        );
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
                response.headers_mut().insert("x-ratelimit-retry-after-ms", v.clone());
                response.headers_mut().insert("x-ratelimit-after", v);
            }
        }

        return response;
    }

    rl_metrics.allowed_total.inc();

    let mut response = next.run(request).await;
    if include_headers {
        if let Ok(v) = decision.remaining.to_string().parse() {
            response.headers_mut().insert("x-ratelimit-remaining", v);
        }
        if let Ok(v) = burst_size.to_string().parse() {
            response.headers_mut().insert("x-ratelimit-limit", v);
        }
        response.headers_mut().insert("x-ratelimit-retry-after-ms", HeaderValue::from_static("0"));
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    // The tests below pin `extract_client_ip`'s hop-walking directly; the middleware
    // itself now goes through `effective_client_ip`.
    #[cfg(feature = "test-utils")]
    use crate::routes::AppState;
    use crate::utils::ip::extract_client_ip;
    #[cfg(feature = "test-utils")]
    use axum::http::StatusCode;
    #[cfg(feature = "test-utils")]
    use axum::{middleware, routing::get, Router};
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    #[cfg(feature = "test-utils")]
    use std::sync::Arc;
    #[cfg(feature = "test-utils")]
    use synapse_cache::{CacheConfig, CacheManager};
    use synapse_common::config::{RateLimitConfig, RateLimitEndpointRule, RateLimitMatchType, RateLimitRule};
    #[cfg(feature = "test-utils")]
    use synapse_services::ServiceContainer;
    #[cfg(feature = "test-utils")]
    use tower::ServiceExt;

    #[test]
    fn test_extract_client_ip() {
        let mut headers = axum::http::HeaderMap::new();
        let priority = vec!["x-forwarded-for".to_string(), "x-real-ip".to_string()];
        let peer = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)), 12345);
        let trusted: Vec<String> = vec!["10.0.0.0/8".to_string()];

        headers.insert("x-forwarded-for", "1.2.3.4, 5.6.7.8".parse().expect("valid header value"));
        // SEC-01: 取最右第一个不可信跳（5.6.7.8 是最右且不在 trusted_proxies 内），
        // 不再取可伪造的最左元素
        assert_eq!(extract_client_ip(&headers, &priority, Some(peer), &trusted), Some("5.6.7.8".to_string()));

        headers = axum::http::HeaderMap::new();
        headers.insert("x-real-ip", "10.0.0.1".parse().expect("valid header value"));
        assert_eq!(extract_client_ip(&headers, &priority, Some(peer), &trusted), Some("10.0.0.1".to_string()));

        headers = axum::http::HeaderMap::new();
        headers.insert("x-forwarded-for", "1.2.3.4".parse().expect("valid header value"));
        headers.insert("x-real-ip", "10.0.0.1".parse().expect("valid header value"));
        assert_eq!(extract_client_ip(&headers, &priority, Some(peer), &trusted), Some("1.2.3.4".to_string()));
    }

    #[test]
    fn test_extract_client_ip_forwarded() {
        let mut headers = axum::http::HeaderMap::new();
        let priority = vec!["forwarded".to_string()];
        let peer = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)), 12345);
        let trusted: Vec<String> = vec!["10.0.0.0/8".to_string()];

        headers.insert("forwarded", "for=192.0.2.60;proto=http;by=203.0.113.43".parse().expect("valid header value"));
        assert_eq!(extract_client_ip(&headers, &priority, Some(peer), &trusted), Some("192.0.2.60".to_string()));

        headers = axum::http::HeaderMap::new();
        headers.insert("forwarded", "for=\"[2001:db8:cafe::17]:4711\"".parse().expect("valid header value"));
        assert_eq!(extract_client_ip(&headers, &priority, Some(peer), &trusted), Some("2001:db8:cafe::17".to_string()));
    }

    #[test]
    fn test_select_endpoint_rule() {
        let mut config = RateLimitConfig::default();
        config.endpoints.push(RateLimitEndpointRule {
            path: "/_matrix/client/v3/login".to_string(),
            match_type: RateLimitMatchType::Exact,
            rule: RateLimitRule { per_second: 5, burst_size: 10 },
        });
        config.endpoints.push(RateLimitEndpointRule {
            path: "/_matrix/client".to_string(),
            match_type: RateLimitMatchType::Prefix,
            rule: RateLimitRule { per_second: 50, burst_size: 100 },
        });
        config.endpoints.push(RateLimitEndpointRule {
            path: "/_matrix/client/v3/sync".to_string(),
            match_type: RateLimitMatchType::Prefix,
            rule: RateLimitRule { per_second: 20, burst_size: 40 },
        });

        let (id, rule) = synapse_common::select_endpoint_rule_runtime(&config, "/_matrix/client/v3/login");
        assert_eq!(id, "/_matrix/client/v3/login");
        assert_eq!(rule.per_second, 5);

        let (id, rule) = synapse_common::select_endpoint_rule_runtime(&config, "/_matrix/client/v3/sync?since=123");
        assert_eq!(id, "/_matrix/client/v3/sync");
        assert_eq!(rule.per_second, 20);

        let (id, rule) = synapse_common::select_endpoint_rule_runtime(&config, "/_matrix/client/versions");
        assert_eq!(id, "/_matrix/client");
        assert_eq!(rule.per_second, 50);

        let (id, rule) = synapse_common::select_endpoint_rule_runtime(&config, "/other/path");
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
        services.core.config_mut().rate_limit = RateLimitConfig {
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

        // B-4: Auto-derive exempt paths from route metadata, mirroring what
        // `create_router` does at startup.
        let exempt_paths: Vec<&'static str> =
            crate::declared_ledger_all().iter().filter(|e| e.rate_limit_exempt).map(|e| e.path).collect();
        let state = AppState::new(services, cache).with_rate_limit_exempt_paths(exempt_paths);

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
        services.core.config_mut().rate_limit = RateLimitConfig {
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
        assert_eq!(first.headers().get("x-ratelimit-retry-after-ms").unwrap(), "0");

        let second = app.oneshot(request()).await.expect("second request should return a response");
        assert_eq!(second.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(second.headers().get("retry-after").is_some());
        assert!(second.headers().get("x-ratelimit-retry-after-ms").is_some());
        assert!(second.headers().get("x-ratelimit-after").is_some());
    }

    // ── W7+: 限流指标化 ────────────────────────────────────────────
    //
    // 验证 6 个 counter 中能在单测里触达的 4 个：
    //   requests / allowed / rejected / exempt
    // fail_open + fail_closed 需要后端不可用才能触发，属于集成测试范围
    // （此处不 mock Redis 故障，避免为测试引入故障注入点）。

    #[cfg(feature = "test-utils")]
    #[tokio::test]
    async fn test_rate_limit_middleware_emits_metrics() {
        async fn ok_handler() -> StatusCode {
            StatusCode::OK
        }

        let mut services = ServiceContainer::new_test().await;
        // 每个 AppState / CacheManager 都是本测试独立 new 的，OnceLock 缓存的
        // counter 句柄只绑定到本测试的 collector，不与其它测试共享。
        services.core.config_mut().rate_limit = RateLimitConfig {
            enabled: true,
            exempt_paths: vec!["/exempt".to_string()],
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
        // `state` 随后要 move 进 `with_state`，故在此先取出 collector 句柄。
        let collector = state.services.core.metrics.clone();

        let app = Router::new()
            .route("/limited", get(ok_handler))
            .route("/exempt", get(ok_handler))
            .layer(middleware::from_fn_with_state(state.clone(), rate_limit_middleware))
            .with_state(state);

        let request = |uri: &str| {
            Request::builder()
                .method(axum::http::Method::GET)
                .uri(uri)
                .header("x-forwarded-for", "9.9.9.9")
                .body(Body::empty())
                .expect("request should build")
        };

        // /limited 第 1 次：放行（burst=1 的 token 被消耗）
        let first = app.clone().oneshot(request("/limited")).await.expect("first request should succeed");
        assert_eq!(first.status(), StatusCode::OK);

        // /limited 第 2 次：token 耗尽 → 429
        let second = app.clone().oneshot(request("/limited")).await.expect("second request should return a response");
        assert_eq!(second.status(), StatusCode::TOO_MANY_REQUESTS);

        // /exempt：命中豁免，连判定都不做
        let exempt = app.clone().oneshot(request("/exempt")).await.expect("exempt request should succeed");
        assert_eq!(exempt.status(), StatusCode::OK);

        let all = collector.collect_metrics();
        let value = |name: &str| -> u64 { all.iter().find(|m| m.name == name).map_or(0, |m| m.value as u64) };

        assert_eq!(value("rate_limit_requests_total"), 2, "/limited 两次进判定；/exempt 不计数");
        assert_eq!(value("rate_limit_requests_allowed_total"), 1);
        assert_eq!(value("rate_limit_requests_rejected_total"), 1);
        assert_eq!(value("rate_limit_requests_exempt_total"), 1);
        assert_eq!(value("rate_limit_fail_open_total"), 0, "后端正常，不应有 fail-open");
        assert_eq!(value("rate_limit_fail_closed_total"), 0, "后端正常，不应有 fail-closed");
    }

    /// 句柄缓存的关键回归：`rate_limit_metrics()` 必须每次返回**同一个**
    /// counter 对象。若误改成每次都 `register_counter*`，registry 里的条目
    /// 会被后注册的覆盖，计数永久分叉（registry 值 < 实际累计值）。
    #[cfg(feature = "test-utils")]
    #[tokio::test]
    async fn test_rate_limit_metrics_handle_is_stable_across_calls() {
        let services = ServiceContainer::new_test().await;
        let collector = &services.core.metrics;
        let cache = CacheManager::new(&CacheConfig::default());

        // 同一 CacheManager 连续取 4 次句柄，逐次 inc
        for _ in 0..4 {
            cache.rate_limit_metrics(collector).rejected_total.inc();
        }

        let all = collector.collect_metrics();
        let rejected =
            all.iter().find(|m| m.name == "rate_limit_requests_rejected_total").map_or(0, |m| m.value as u64);
        assert_eq!(rejected, 4, "4 次调用必须累到同一个 counter 上");
    }
}
