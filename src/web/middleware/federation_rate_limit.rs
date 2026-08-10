//! Per-origin federation rate limiting.
//!
//! Unlike the generic IP-based [`rate_limit_middleware`](super::rate_limit::rate_limit_middleware),
//! this middleware keys on the authenticated Matrix `origin` (extracted from
//! the `Authorization: X-Matrix` header by [`federation_auth_middleware`]).
//! This provides meaningful per-server rate limiting for federation traffic.
//!
//! The middleware must be layered **after** `federation_auth_middleware` so
//! that the `FederationRequestAuth` extension is available.

use crate::cache::CacheKeyBuilder;
use crate::common::ApiError;
use crate::web::routes::context::FederationContext;
use axum::extract::State;
use axum::http::Request;
use axum::response::{IntoResponse, Response};
use axum::{body::Body, middleware::Next};

use super::federation_auth::FederationRequestAuth;

pub async fn federation_rate_limit_middleware(
    State(ctx): State<FederationContext>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let config = &ctx.config.federation.rate_limit;
    if !config.enabled {
        return next.run(request).await;
    }

    // The federation auth middleware must have already run and inserted the
    // FederationRequestAuth extension. If it's missing, the request is
    // unauthenticated federation traffic — let the handler return 401.
    let origin = match request.extensions().get::<FederationRequestAuth>() {
        Some(auth) => auth.origin.as_str(),
        None => return next.run(request).await,
    };

    let path = request.uri().path();
    // Use the first path segment after /_matrix/federation as the endpoint
    // bucket so that all events/transaction/membership endpoints are grouped
    // rather than rate-limiting per exact path.
    let endpoint_bucket = federation_endpoint_bucket(path);

    let redis_prefix = ctx.config.redis.key_prefix.as_str();
    let cache_key =
        format!("{}{}", redis_prefix, CacheKeyBuilder::federation_origin_rate_limit(origin, endpoint_bucket));

    let decision = match ctx.cache.rate_limit_token_bucket_take(&cache_key, config.per_second, config.burst_size).await
    {
        Ok(d) => d,
        Err(e) => {
            // S17/SEC-02: 不再无条件放行 —— 由 fail_open_on_error 配置决定，
            // 与主限流器（rate_limit.rs）语义对齐。
            if federation_rate_limit_error_should_allow(config.fail_open_on_error) {
                tracing::warn!(
                    origin = %origin,
                    endpoint = %endpoint_bucket,
                    error = %e,
                    "Federation rate limiter error, allowing request (fail_open_on_error=true)"
                );
                return next.run(request).await;
            }
            tracing::error!(
                origin = %origin,
                endpoint = %endpoint_bucket,
                error = %e,
                "Federation rate limiter backend unavailable, rejecting request (fail_open_on_error=false)"
            );
            return ApiError::internal("Federation rate limit backend unavailable".to_string()).into_response();
        }
    };

    if !decision.allowed {
        let retry_after_ms = decision.retry_after_seconds.saturating_mul(1000);
        tracing::info!(
            origin = %origin,
            endpoint = %endpoint_bucket,
            retry_after_ms = %retry_after_ms,
            "Federation rate limit exceeded for origin"
        );
        return ApiError::rate_limited_with_retry(retry_after_ms).into_response();
    }

    next.run(request).await
}

/// S17/SEC-02: 联邦限流后端故障时是否放行，由 `fail_open_on_error` 配置决定。
fn federation_rate_limit_error_should_allow(fail_open_on_error: bool) -> bool {
    fail_open_on_error
}

/// Collapse a federation path into a coarse endpoint bucket for rate limiting.
///
/// Examples:
///   `/_matrix/federation/v1/send/123` -> `send`
///   `/_matrix/federation/v2/send/123` -> `send`
///   `/_matrix/federation/v1/make_join/...` -> `make_join`
///   `/_matrix/federation/v1/event/{id}` -> `event`
fn federation_endpoint_bucket(path: &str) -> &'static str {
    // All federation paths start with /_matrix/federation/. The segment
    // after the version is the operation name.
    let stripped = path.strip_prefix("/_matrix/federation/").unwrap_or(path);
    // Skip the version segment (e.g. "v1/", "v2/")
    let after_version = match stripped.find('/') {
        Some(idx) => &stripped[idx + 1..],
        None => return "other",
    };
    // The next segment is the operation
    let op = after_version.split('/').next().unwrap_or("other");
    match op {
        "send" => "send",
        "event" => "event",
        "state" => "state",
        "state_ids" => "state_ids",
        "backfill" => "backfill",
        "make_join" => "make_join",
        "send_join" => "send_join",
        "make_leave" => "make_leave",
        "send_leave" => "send_leave",
        "invite" => "invite",
        "exchange_third_party_invite" => "exchange_third_party_invite",
        "query" => "query",
        "query_auth" => "query_auth",
        "get_missing_events" => "get_missing_events",
        "get_groups_renewed" => "get_groups_renewed",
        "user" => "user",
        "query_keys" => "query_keys",
        "claim_keys" => "claim_keys",
        "version" => "version",
        _ => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // S17 / SEC-02: Redis 故障时的决策必须受 fail_open_on_error 配置控制
    // ------------------------------------------------------------------

    #[test]
    fn test_federation_rate_limit_error_fail_open_allows() {
        assert!(federation_rate_limit_error_should_allow(true), "fail-open 必须放行");
    }

    #[test]
    fn test_federation_rate_limit_error_fail_closed_rejects() {
        assert!(!federation_rate_limit_error_should_allow(false), "fail-closed 必须拒绝");
    }

    #[test]
    fn test_federation_endpoint_bucket() {
        assert_eq!(federation_endpoint_bucket("/_matrix/federation/v1/send/123"), "send");
        assert_eq!(federation_endpoint_bucket("/_matrix/federation/v2/send/abc"), "send");
        assert_eq!(
            federation_endpoint_bucket("/_matrix/federation/v1/make_join/!room:server/@user:server"),
            "make_join"
        );
        assert_eq!(federation_endpoint_bucket("/_matrix/federation/v1/event/$abc"), "event");
        assert_eq!(federation_endpoint_bucket("/_matrix/federation/v1/query"), "query");
        assert_eq!(federation_endpoint_bucket("/_matrix/federation/v1/query/auth"), "query");
        assert_eq!(federation_endpoint_bucket("/_matrix/federation/v1/unknown_op"), "other");
        assert_eq!(federation_endpoint_bucket("/some/other/path"), "other");
    }
}
