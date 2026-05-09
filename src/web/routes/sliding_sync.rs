use crate::common::rate_limit_config::RateLimitConfigFile;
use crate::common::ApiError;
use crate::storage::sliding_sync::{SlidingSyncRequest, SlidingSyncResponse};
use crate::web::routes::{AppState, AuthenticatedUser};
use axum::{extract::State, routing::post, Json, Router};

/// Sliding Sync endpoint
/// Matrix MSC3575: https://github.com/matrix-org/matrix-spec-proposals/pull/3575
///
/// 注意：不要在 `/_matrix/client/v3/sync` 上挂载这里的 POST —— 该路径的 GET
/// 已由 `sync.rs` 使用，axum 对同一路径不同 method 的 router 合并尚无法在
/// feature-flag 组合下稳定通过 ledger 校验（R2-SS-01）。SDK 和规范要求 sliding
/// sync 使用 MSC3575 unstable 路径即可。
pub fn create_sliding_sync_router(_state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/_matrix/client/v1/sync",
            post(sliding_sync),
        )
        .route(
            "/_matrix/client/unstable/org.matrix.msc3575/sync",
            post(sliding_sync),
        )
        .route(
            "/_matrix/client/unstable/org.matrix.simplified_msc3575/sync",
            post(sliding_sync),
        )
}

pub fn sliding_sync_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::RouteEntry;
    use axum::http::Method;
    [
        (
            Method::POST,
            "/_matrix/client/v1/sync",
        ),
        (
            Method::POST,
            "/_matrix/client/unstable/org.matrix.msc3575/sync",
        ),
        (
            Method::POST,
            "/_matrix/client/unstable/org.matrix.simplified_msc3575/sync",
        ),
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

    let file_config = state
        .rate_limit_config_manager
        .as_ref()
        .map(|manager| manager.get_config());
    let sync_rate_limit_enabled = file_config
        .as_ref()
        .map(|config| config.sync.enabled)
        .unwrap_or(state.services.config.rate_limit.sync.enabled);

    if sync_rate_limit_enabled {
        let (per_second, burst_size) =
            resolve_sliding_sync_rate_limit(&state, file_config.as_ref(), body.pos.is_none());
        let kind = if body.pos.is_none() {
            "initial"
        } else {
            "incremental"
        };
        let rate_limit_key = format!(
            "ratelimit:sliding_sync:{}:{}:{}",
            auth_user.user_id, device_id, kind
        );
        let decision = state
            .cache
            .rate_limit_token_bucket_take(&rate_limit_key, per_second, burst_size)
            .await
            .map_err(|e| ApiError::internal(format!("Sliding sync rate limit failed: {}", e)))?;
        if !decision.allowed {
            let retry_after_ms = decision.retry_after_seconds.saturating_mul(1000);
            return Err(ApiError::rate_limited_with_retry(retry_after_ms));
        }
    }

    // Call the sliding sync service
    let response = state
        .services
        .sliding_sync_service
        .sync(&auth_user.user_id, &device_id, body)
        .await?;

    Ok(Json(response))
}

fn resolve_sliding_sync_rate_limit(
    state: &AppState,
    file_config: Option<&RateLimitConfigFile>,
    is_initial: bool,
) -> (u32, u32) {
    match file_config {
        Some(config) if config.sync.enabled => {
            if is_initial {
                (
                    config.sync.initial.per_second,
                    config.sync.initial.burst_size,
                )
            } else {
                (
                    config.sync.incremental.per_second,
                    config.sync.incremental.burst_size,
                )
            }
        }
        _ => {
            let config = &state.services.config.rate_limit.sync;
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
    use super::resolve_sliding_sync_rate_limit;
    use crate::cache::CacheConfig;
    use crate::common::rate_limit_config::RateLimitConfigFile;
    use crate::web::routes::AppState;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_resolve_sliding_sync_rate_limit_prefers_file_config_when_enabled() {
        let mut services = crate::services::ServiceContainer::new_test();
        services.config.rate_limit.sync.enabled = true;
        services.config.rate_limit.sync.initial.per_second = 3;
        services.config.rate_limit.sync.initial.burst_size = 7;
        services.config.rate_limit.sync.incremental.per_second = 4;
        services.config.rate_limit.sync.incremental.burst_size = 8;

        let state = AppState::new(
            services,
            Arc::new(crate::cache::CacheManager::new(CacheConfig::default())),
        );

        let mut file_config = RateLimitConfigFile::default();
        file_config.sync.enabled = true;
        file_config.sync.initial.per_second = 11;
        file_config.sync.initial.burst_size = 22;
        file_config.sync.incremental.per_second = 33;
        file_config.sync.incremental.burst_size = 44;

        assert_eq!(
            resolve_sliding_sync_rate_limit(&state, Some(&file_config), true),
            (11, 22)
        );
        assert_eq!(
            resolve_sliding_sync_rate_limit(&state, Some(&file_config), false),
            (33, 44)
        );
    }

    #[tokio::test]
    async fn test_resolve_sliding_sync_rate_limit_falls_back_to_runtime_config() {
        let mut services = crate::services::ServiceContainer::new_test();
        services.config.rate_limit.sync.enabled = true;
        services.config.rate_limit.sync.initial.per_second = 5;
        services.config.rate_limit.sync.initial.burst_size = 50;
        services.config.rate_limit.sync.incremental.per_second = 6;
        services.config.rate_limit.sync.incremental.burst_size = 60;

        let state = AppState::new(
            services,
            Arc::new(crate::cache::CacheManager::new(CacheConfig::default())),
        );

        let mut file_config = RateLimitConfigFile::default();
        file_config.sync.enabled = false;
        file_config.sync.initial.per_second = 99;
        file_config.sync.initial.burst_size = 99;

        assert_eq!(
            resolve_sliding_sync_rate_limit(&state, Some(&file_config), true),
            (5, 50)
        );
        assert_eq!(
            resolve_sliding_sync_rate_limit(&state, None, false),
            (6, 60)
        );
    }
}
