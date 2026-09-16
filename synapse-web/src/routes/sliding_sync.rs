use crate::routes::context::SyncContext;
use crate::routes::{AppState, AuthenticatedUser, MatrixJson};
use axum::{
    extract::{Query, State},
    routing::post,
    Json, Router,
};
use serde::Deserialize;
use synapse_common::ApiError;
use synapse_services::sliding_sync_service::{SlidingSyncRequest, SlidingSyncResponse};

/// Query parameters for sliding sync requests.
///
/// MSC3575 / Simplified MSC3575: `pos` and `timeout` are sent as query
/// parameters by the SDK (see RoomManager.slidingSync), not in the JSON body.
/// We extract them here and merge into the body to ensure `SlidingSyncRequest.pos`
/// is populated for incremental sync.
#[derive(Debug, Deserialize, Default)]
struct SlidingSyncQuery {
    pos: Option<String>,
    timeout: Option<u32>,
    txn_id: Option<String>,
}

/// Sliding Sync endpoint
/// Matrix MSC3575: https://github.com/matrix-org/matrix-spec-proposals/pull/3575
/// MSC4186 (Simplified Sliding Sync): stable v4 path
///
/// 注意：不要在 `/_matrix/client/v3/sync` 上挂载这里的 POST —— 该路径的 GET
/// 已由 `sync.rs` 使用，axum 对同一路径不同 method 的 router 合并尚无法在
/// feature-flag 组合下稳定通过 ledger 校验（R2-SS-01）。SDK 和规范要求 sliding
/// sync 使用 MSC3575 unstable 路径即可。
pub fn create_sliding_sync_router(_state: AppState) -> Router<AppState> {
    Router::new()
        .route("/_matrix/client/v1/sync", post(sliding_sync))
        .route("/_matrix/client/v4/sync", post(sliding_sync))
        .route("/_matrix/client/unstable/org.matrix.msc3575/sync", post(sliding_sync))
        .route("/_matrix/client/unstable/org.matrix.simplified_msc3575/sync", post(sliding_sync))
}

/// S15 / B-3: 限流后端（Redis）故障时的决策，与 `/sync` 处理器（handlers/sync.rs）
/// 保持同一 fail-open 语义：fail_open_on_error=true 放行，否则返回 500。
fn rate_limit_decision_on_error(
    fail_open_on_error: bool,
    burst_size: u32,
) -> Result<synapse_cache::RateLimitDecision, ApiError> {
    if fail_open_on_error {
        Ok(synapse_cache::RateLimitDecision { allowed: true, retry_after_seconds: 0, remaining: burst_size })
    } else {
        Err(ApiError::internal("Sliding sync rate limit backend unavailable".to_string()))
    }
}

/// S26 / B-2: 独立限流计数器名。429 与长轮询互为掩护 —— 限流只在长轮询
/// 失效时才触发，因此 `sliding_sync_rate_limited_total > 0` 可直接作为
/// 「长轮询失效」的告警信号，必须独立于慢请求指标计数。
const SLIDING_SYNC_RATE_LIMITED_COUNTER: &str = "sliding_sync_rate_limited_total";

/// S26: 记录一次 sliding sync 限流拒绝（429）。
fn record_rate_limited(metrics: &synapse_common::metrics::MetricsCollector) {
    let counter = metrics
        .get_counter(SLIDING_SYNC_RATE_LIMITED_COUNTER)
        .unwrap_or_else(|| metrics.register_counter(SLIDING_SYNC_RATE_LIMITED_COUNTER.to_string()));
    counter.inc();
}

#[axum::debug_handler]
async fn sliding_sync(
    State(ctx): State<SyncContext>,
    auth_user: AuthenticatedUser,
    Query(query): Query<SlidingSyncQuery>,
    MatrixJson(mut body): MatrixJson<SlidingSyncRequest>,
) -> Result<Json<SlidingSyncResponse>, ApiError> {
    // MSC3575: SDK sends pos/timeout/txn_id as query parameters, not in body.
    // Merge query params into body so downstream logic sees them uniformly.
    if body.pos.is_none() {
        body.pos = query.pos;
    }
    if body.timeout.is_none() {
        body.timeout = query.timeout;
    }
    if body.txn_id.is_none() {
        body.txn_id = query.txn_id;
    }
    tracing::debug!(
        "Sliding sync request from user: {}, pos: {:?}, lists: {:?}",
        auth_user.user_id,
        body.pos,
        body.lists
    );

    // Get device_id or use default
    let device_id = auth_user.device_id.unwrap_or_else(|| "default".to_string());

    let file_config = ctx.sync_rate_limit_override();
    let sync_rate_limit_enabled =
        file_config.as_ref().map_or(ctx.config.rate_limit.sync.enabled, |config| config.sync.enabled);

    if sync_rate_limit_enabled {
        let (per_second, burst_size): (u32, u32) =
            resolve_sliding_sync_rate_limit(&ctx, file_config.as_ref(), body.pos.is_none());
        let fail_open_on_error =
            file_config.as_ref().map_or(ctx.config.rate_limit.fail_open_on_error, |config| config.fail_open_on_error);
        let kind: &str = if body.pos.is_none() { "initial" } else { "incremental" };
        let rate_limit_key: String = format!("ratelimit:sliding_sync:{}:{}:{}", auth_user.user_id, device_id, kind);
        // S15: Redis 故障时按 fail_open_on_error 放行，与 /sync 处理器语义一致
        let decision: synapse_cache::RateLimitDecision =
            match ctx.cache.rate_limit_token_bucket_take(&rate_limit_key, per_second, burst_size).await {
                Ok(decision) => decision,
                Err(error) => {
                    tracing::warn!(
                        user_id = %auth_user.user_id,
                        device_id = %device_id,
                        kind,
                        fail_open = fail_open_on_error,
                        error = %error,
                        "Sliding sync rate limiter failed"
                    );
                    rate_limit_decision_on_error(fail_open_on_error, burst_size)?
                }
            };
        if !decision.allowed {
            let retry_after_ms: u64 = decision.retry_after_seconds.saturating_mul(1000);
            record_rate_limited(&ctx.metrics);
            return Err(ApiError::rate_limited_with_retry(retry_after_ms));
        }
    }

    // Call the sliding sync service.
    //
    // S11 / A-4 / SS-02: 延迟直方图与慢请求判定统一由 service 层上报
    // （sliding_sync_request_duration_ms / 慢请求计数器），因为只有 service
    // 层能从 wall-clock 中扣除 idle_wait_ms —— 路由层自测会把健康的 30s
    // 长轮询全部误记为慢请求并重复计数。此处仅保留请求总量计数器（QPS 观测，
    // service 层无等价物）。
    let total_counter = ctx
        .metrics
        .get_counter("sliding_sync_requests_total")
        .unwrap_or_else(|| ctx.metrics.register_counter("sliding_sync_requests_total".to_string()));
    total_counter.inc();

    let response: SlidingSyncResponse = ctx.sliding_sync_service.sync(&auth_user.user_id, &device_id, body).await?;

    Ok(Json(response))
}

fn resolve_sliding_sync_rate_limit(
    ctx: &SyncContext,
    file_config: Option<&crate::routes::state::SyncRateLimitOverride>,
    is_initial: bool,
) -> (u32, u32) {
    match file_config {
        Some(config) if config.sync.enabled => {
            if is_initial {
                (config.sync.initial.per_second, config.sync.initial.burst_size)
            } else {
                (config.sync.incremental.per_second, config.sync.incremental.burst_size)
            }
        }
        _ => {
            let config = &ctx.config.rate_limit.sync;
            if is_initial {
                (config.initial.per_second, config.initial.burst_size)
            } else {
                (config.incremental.per_second, config.incremental.burst_size)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    // ------------------------------------------------------------------
    // S26 / B-2: 429 与长轮询互为掩护 —— 限流触发必须独立计数，
    // sliding_sync_rate_limited_total > 0 即长轮询失效的告警信号
    // ------------------------------------------------------------------

    #[test]
    fn test_record_rate_limited_increments_dedicated_counter() {
        let metrics = synapse_common::metrics::MetricsCollector::new();
        super::record_rate_limited(&metrics);
        super::record_rate_limited(&metrics);
        let counter =
            metrics.get_counter("sliding_sync_rate_limited_total").expect("rate-limited counter must be registered");
        assert_eq!(counter.get(), 2, "每次 429 拒绝都必须独立计数");
    }

    #[test]
    fn test_route_layer_does_not_double_count_slow_requests() {
        // S11: 慢请求判定与计数只属 service 层（其扣除了 idle_wait_ms）。
        // 路由层不得再注册/递增慢请求计数器 —— 该守卫通过源码扫描维持：
        // 本文件中该计数器名只允许出现在本测试的 matches() 调用里。
        let source = include_str!("sliding_sync.rs");
        let occurrences = source.matches("sliding_sync_slow_requests_total").count();
        assert!(occurrences == 1, "路由层不得直接计数慢请求, found {occurrences} occurrences");
    }

    #[cfg(feature = "test-utils")]
    use super::resolve_sliding_sync_rate_limit;
    #[cfg(feature = "test-utils")]
    use crate::routes::context::SyncContext;
    #[cfg(feature = "test-utils")]
    use crate::routes::state::SyncRateLimitOverride;
    #[cfg(feature = "test-utils")]
    use crate::routes::AppState;
    #[cfg(feature = "test-utils")]
    use axum::extract::FromRef;
    #[cfg(feature = "test-utils")]
    use std::sync::Arc;
    #[cfg(feature = "test-utils")]
    use synapse_cache::CacheConfig;
    #[cfg(feature = "test-utils")]
    use synapse_common::RateLimitConfigFile;

    // ------------------------------------------------------------------
    // S15 / B-3 / SS-08: Redis 故障时的 fail-open 语义必须与 /sync 一致
    // ------------------------------------------------------------------

    #[test]
    fn test_rate_limit_error_fail_open_allows_request() {
        let decision = super::rate_limit_decision_on_error(true, 50).expect("fail-open must allow the request");
        assert!(decision.allowed);
        assert_eq!(decision.retry_after_seconds, 0);
        assert_eq!(decision.remaining, 50);
    }

    #[test]
    fn test_rate_limit_error_fail_closed_returns_error() {
        let result = super::rate_limit_decision_on_error(false, 50);
        assert!(result.is_err(), "fail-closed must surface the error");
    }

    #[cfg(feature = "test-utils")]
    #[tokio::test]
    async fn test_resolve_sliding_sync_rate_limit_prefers_file_config_when_enabled() {
        let mut services = synapse_services::ServiceContainer::new_test().await;
        {
            let cfg = services.core.config_mut();
            cfg.rate_limit.sync.enabled = true;
            cfg.rate_limit.sync.initial.per_second = 3;
            cfg.rate_limit.sync.initial.burst_size = 7;
            cfg.rate_limit.sync.incremental.per_second = 4;
            cfg.rate_limit.sync.incremental.burst_size = 8;
        }

        let state = AppState::new(services, Arc::new(synapse_cache::CacheManager::new(&CacheConfig::default())));

        let mut file_config = RateLimitConfigFile::default();
        file_config.sync.enabled = true;
        file_config.sync.initial.per_second = 11;
        file_config.sync.initial.burst_size = 22;
        file_config.sync.incremental.per_second = 33;
        file_config.sync.incremental.burst_size = 44;
        let sync_override =
            SyncRateLimitOverride { fail_open_on_error: file_config.fail_open_on_error, sync: file_config.sync };

        let ctx = SyncContext::from_ref(&state);
        assert_eq!(resolve_sliding_sync_rate_limit(&ctx, Some(&sync_override), true), (11, 22));
        assert_eq!(resolve_sliding_sync_rate_limit(&ctx, Some(&sync_override), false), (33, 44));
    }

    #[cfg(feature = "test-utils")]
    #[tokio::test]
    async fn test_resolve_sliding_sync_rate_limit_falls_back_to_runtime_config() {
        let mut services = synapse_services::ServiceContainer::new_test().await;
        {
            let cfg = services.core.config_mut();
            cfg.rate_limit.sync.enabled = true;
            cfg.rate_limit.sync.initial.per_second = 5;
            cfg.rate_limit.sync.initial.burst_size = 50;
            cfg.rate_limit.sync.incremental.per_second = 6;
            cfg.rate_limit.sync.incremental.burst_size = 60;
        }

        let state = AppState::new(services, Arc::new(synapse_cache::CacheManager::new(&CacheConfig::default())));

        let mut file_config = RateLimitConfigFile::default();
        file_config.sync.enabled = false;
        file_config.sync.initial.per_second = 99;
        file_config.sync.initial.burst_size = 99;
        let sync_override =
            SyncRateLimitOverride { fail_open_on_error: file_config.fail_open_on_error, sync: file_config.sync };

        let ctx = SyncContext::from_ref(&state);
        assert_eq!(resolve_sliding_sync_rate_limit(&ctx, Some(&sync_override), true), (5, 50));
        assert_eq!(resolve_sliding_sync_rate_limit(&ctx, None, false), (6, 60));
    }
}
