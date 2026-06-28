use crate::common::ApiError;
use crate::web::routes::{AppState, AuthenticatedUser};
use axum::{extract::State, routing::post, Json, Router};
use synapse_storage::sliding_sync::{SlidingSyncRequest, SlidingSyncResponse};

/// Sliding Sync endpoint
/// Matrix MSC3575: https://github.com/matrix-org/matrix-spec-proposals/pull/3575
///
/// 注意：不要在 `/_matrix/client/v3/sync` 上挂载这里的 POST —— 该路径的 GET
/// 已由 `sync.rs` 使用，axum 对同一路径不同 method 的 router 合并尚无法在
/// feature-flag 组合下稳定通过 ledger 校验（R2-SS-01）。SDK 和规范要求 sliding
/// sync 使用 MSC3575 unstable 路径即可。
pub fn create_sliding_sync_router(_state: AppState) -> Router<AppState> {
    Router::new()
        .route("/_matrix/client/v1/sync", post(sliding_sync))
        .route("/_matrix/client/unstable/org.matrix.msc3575/sync", post(sliding_sync))
        .route("/_matrix/client/unstable/org.matrix.simplified_msc3575/sync", post(sliding_sync))
}

pub fn sliding_sync_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::RouteEntry;
    use axum::http::Method;
    [
        (Method::POST, "/_matrix/client/v1/sync"),
        (Method::POST, "/_matrix/client/unstable/org.matrix.msc3575/sync"),
        (Method::POST, "/_matrix/client/unstable/org.matrix.simplified_msc3575/sync"),
    ]
    .into_iter()
    .map(|(m, p)| RouteEntry::new(m, p, "sliding_sync"))
    .collect()
}

#[axum::debug_handler]
async fn sliding_sync(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<SlidingSyncRequest>,
) -> Result<Json<SlidingSyncResponse>, ApiError> {
    tracing::debug!(
        "Sliding sync request from user: {}, pos: {:?}, lists: {:?}",
        auth_user.user_id,
        body.pos,
        body.lists
    );

    // Get device_id or use default
    let device_id = auth_user.device_id.unwrap_or_else(|| "default".to_string());

    let file_config = state.sync_rate_limit_override();
    let sync_rate_limit_enabled =
        file_config.as_ref().map_or(state.services.core.config.rate_limit.sync.enabled, |config| config.sync.enabled);

    if sync_rate_limit_enabled {
        let (per_second, burst_size): (u32, u32) =
            resolve_sliding_sync_rate_limit(&state, file_config.as_ref(), body.pos.is_none());
        let kind: &str = if body.pos.is_none() { "initial" } else { "incremental" };
        let rate_limit_key: String = format!("ratelimit:sliding_sync:{}:{}:{}", auth_user.user_id, device_id, kind);
        let decision: crate::cache::RateLimitDecision = state
            .cache
            .rate_limit_token_bucket_take(&rate_limit_key, per_second, burst_size)
            .await
            .map_err(|e| ApiError::internal_with_log("Sliding sync rate limit failed", &e))?;
        if !decision.allowed {
            let retry_after_ms: u64 = decision.retry_after_seconds.saturating_mul(1000);
            return Err(ApiError::rate_limited_with_retry(retry_after_ms));
        }
    }

    // Call the sliding sync service
    //
    // OPT-08: Performance gate for sliding sync. Records request duration
    // in a histogram and logs a warning when the response exceeds the
    // configured latency threshold. This provides p50/p95/p99 visibility
    // and slow-request alerting, acting as a performance rollback gate
    // inspired by Synapse v1.153.0rc3 which reverted a sliding-sync
    // optimisation after performance regressions went unnoticed.
    let sync_start = std::time::Instant::now();
    let response: SlidingSyncResponse =
        state.services.rooms.sliding_sync_service.sync(&auth_user.user_id, &device_id, body).await?;
    let elapsed_ms = sync_start.elapsed().as_millis() as u64;

    // Record duration in histogram for p50/p95/p99 observability.
    let histogram = state
        .services
        .core
        .metrics
        .get_histogram("sliding_sync_duration_ms")
        .unwrap_or_else(|| state.services.core.metrics.register_histogram("sliding_sync_duration_ms".to_string()));
    histogram.observe(elapsed_ms as f64);

    // Increment total request counter.
    let total_counter = state
        .services
        .core
        .metrics
        .get_counter("sliding_sync_requests_total")
        .unwrap_or_else(|| state.services.core.metrics.register_counter("sliding_sync_requests_total".to_string()));
    total_counter.inc();

    // Slow-request gate: warn + counter when threshold exceeded.
    let threshold_ms = state.services.core.config.performance.sliding_sync_latency_threshold_ms;
    if elapsed_ms > threshold_ms {
        ::tracing::warn!(
            user_id = %auth_user.user_id,
            device_id = %device_id,
            elapsed_ms = elapsed_ms,
            threshold_ms = threshold_ms,
            "Sliding sync response exceeded latency threshold"
        );
        let slow_counter =
            state.services.core.metrics.get_counter("sliding_sync_slow_requests_total").unwrap_or_else(|| {
                state.services.core.metrics.register_counter("sliding_sync_slow_requests_total".to_string())
            });
        slow_counter.inc();
    }

    Ok(Json(response))
}

fn resolve_sliding_sync_rate_limit(
    state: &AppState,
    file_config: Option<&crate::web::routes::state::SyncRateLimitOverride>,
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
            let config = &state.services.core.config.rate_limit.sync;
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
    #[cfg(feature = "test-utils")]
    use super::resolve_sliding_sync_rate_limit;
    #[cfg(feature = "test-utils")]
    use crate::cache::CacheConfig;
    #[cfg(feature = "test-utils")]
    use crate::common::rate_limit_config::RateLimitConfigFile;
    #[cfg(feature = "test-utils")]
    use crate::web::routes::state::SyncRateLimitOverride;
    #[cfg(feature = "test-utils")]
    use crate::web::routes::AppState;
    #[cfg(feature = "test-utils")]
    use std::sync::Arc;

    #[cfg(feature = "test-utils")]
    #[tokio::test]
    async fn test_resolve_sliding_sync_rate_limit_prefers_file_config_when_enabled() {
        let mut services = crate::services::ServiceContainer::new_test().await;
        services.core.config.rate_limit.sync.enabled = true;
        services.core.config.rate_limit.sync.initial.per_second = 3;
        services.core.config.rate_limit.sync.initial.burst_size = 7;
        services.core.config.rate_limit.sync.incremental.per_second = 4;
        services.core.config.rate_limit.sync.incremental.burst_size = 8;

        let state = AppState::new(services, Arc::new(crate::cache::CacheManager::new(&CacheConfig::default())));

        let mut file_config = RateLimitConfigFile::default();
        file_config.sync.enabled = true;
        file_config.sync.initial.per_second = 11;
        file_config.sync.initial.burst_size = 22;
        file_config.sync.incremental.per_second = 33;
        file_config.sync.incremental.burst_size = 44;
        let sync_override =
            SyncRateLimitOverride { fail_open_on_error: file_config.fail_open_on_error, sync: file_config.sync };

        assert_eq!(resolve_sliding_sync_rate_limit(&state, Some(&sync_override), true), (11, 22));
        assert_eq!(resolve_sliding_sync_rate_limit(&state, Some(&sync_override), false), (33, 44));
    }

    #[cfg(feature = "test-utils")]
    #[tokio::test]
    async fn test_resolve_sliding_sync_rate_limit_falls_back_to_runtime_config() {
        let mut services = crate::services::ServiceContainer::new_test().await;
        services.core.config.rate_limit.sync.enabled = true;
        services.core.config.rate_limit.sync.initial.per_second = 5;
        services.core.config.rate_limit.sync.initial.burst_size = 50;
        services.core.config.rate_limit.sync.incremental.per_second = 6;
        services.core.config.rate_limit.sync.incremental.burst_size = 60;

        let state = AppState::new(services, Arc::new(crate::cache::CacheManager::new(&CacheConfig::default())));

        let mut file_config = RateLimitConfigFile::default();
        file_config.sync.enabled = false;
        file_config.sync.initial.per_second = 99;
        file_config.sync.initial.burst_size = 99;
        let sync_override =
            SyncRateLimitOverride { fail_open_on_error: file_config.fail_open_on_error, sync: file_config.sync };

        assert_eq!(resolve_sliding_sync_rate_limit(&state, Some(&sync_override), true), (5, 50));
        assert_eq!(resolve_sliding_sync_rate_limit(&state, None, false), (6, 60));
    }
}
