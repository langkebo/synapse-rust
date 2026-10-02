use crate::routes::context::SyncContext;
use crate::routes::AuthenticatedUser;
use axum::{
    extract::{Json, Query, State},
    http::HeaderMap,
};
use serde_json::Value;
use synapse_common::rate_limit_config::RateLimitConfigFile;
use synapse_common::ApiError;
use synapse_services::sync_service::{SyncServiceRequest, SyncToken};

/// S26 / B-2: 429 与长轮询互为掩护 —— 限流触发必须独立计数。
/// v2 /sync 用该计数器作告警，与 sliding_sync 的 `sliding_sync_rate_limited_total` 分离。
/// 若滑稽：v2 sync 429 > 0 → 长轮询失效；若正常：v2 sync 429 ≈ 0。
const SYNC_RATE_LIMITED_COUNTER: &str = "sync_rate_limited_total";

/// 记录一次 v2 /sync 限流拒绝（429）。
fn record_rate_limited(metrics: &synapse_common::metrics::MetricsCollector) {
    let counter = metrics
        .get_counter(SYNC_RATE_LIMITED_COUNTER)
        .unwrap_or_else(|| metrics.register_counter(SYNC_RATE_LIMITED_COUNTER.to_string()));
    counter.inc();
}

struct SyncParams {
    ctx: SyncContext,
    user_id: String,
    device_id: Option<String>,
    timeout: u64,
    is_full_state: bool,
    request_id: String,
    set_presence: String,
    filter: Option<String>,
    since: Option<String>,
    /// MSC4222: `?use_state_after=true`（不稳定拼写 `org.matrix.msc4222.use_state_after`）。
    use_state_after: bool,
    /// 客户端用的是**不稳定**拼写时为 true ⇒ 响应字段镜像为
    /// `org.matrix.msc4222.state_after`（MSC4222 §Unstable prefix）。
    state_after_is_unstable: bool,
}

/// Build an effective `SyncRateLimitOverride`-like struct from the context's
/// rate-limit config manager (dynamic) falling back to the static config.
fn resolve_rate_limit_override(ctx: &SyncContext) -> (bool, bool, u32, u32, u32, u32) {
    if let Some(manager) = &ctx.rate_limit_config_manager {
        let config: RateLimitConfigFile = manager.get_config();
        (
            config.fail_open_on_error,
            config.sync.enabled,
            config.sync.initial.per_second,
            config.sync.initial.burst_size,
            config.sync.incremental.per_second,
            config.sync.incremental.burst_size,
        )
    } else {
        let config = &ctx.config.rate_limit;
        (
            config.fail_open_on_error,
            config.sync.enabled,
            config.sync.initial.per_second,
            config.sync.initial.burst_size,
            config.sync.incremental.per_second,
            config.sync.incremental.burst_size,
        )
    }
}

/// See [`sync`].
pub(crate) async fn sync(
    State(ctx): State<SyncContext>,
    headers: HeaderMap,
    auth_user: AuthenticatedUser,
    Query(params): Query<Value>,
) -> Result<Json<Value>, ApiError> {
    let user_id = auth_user.user_id;
    let device_id = auth_user.device_id;

    let timeout = parse_u64_query_param(&params, "timeout").unwrap_or(30000);
    let is_full_state = parse_bool_query_param(&params, "full_state").unwrap_or(false);
    let request_id = crate::utils::auth::resolve_request_id(&headers);
    let set_presence = params.get("set_presence").and_then(|v| v.as_str()).unwrap_or("online").to_string();
    let filter = params.get("filter").and_then(|v| v.as_str()).map(|s| s.to_string());
    let mut since = params.get("since").and_then(|v| v.as_str()).map(|s| s.to_string());
    let (use_state_after, state_after_is_unstable) = resolve_use_state_after(&params);

    // P-049: Validate timeout is non-negative. `parse_u64_query_param` already
    // rejects negative values (u64 cannot represent them) but silently falls
    // back to the default. Detect a present-but-negative value and reject it
    // with M_BAD_JSON instead of silently using the default.
    if let Some(raw) = params.get("timeout") {
        let as_i64 = match raw {
            Value::Number(n) => n.as_i64(),
            Value::String(s) => s.parse::<i64>().ok(),
            _ => None,
        };
        if let Some(t) = as_i64 {
            if t < 0 {
                return Err(ApiError::bad_request("timeout must be a non-negative integer".to_string()));
            }
        }
    }

    // P-048: Validate the since token before using it. A malformed token
    // (e.g. "invalid_token") cannot be parsed as a SyncToken and must be
    // rejected. An empty string is treated as "no since" (initial sync).
    //
    // P-048 / B2 fix: Use M_BAD_PAGINATION (HTTP 400) instead of
    // M_UNKNOWN_TOKEN (HTTP 401). The access token is valid; only the
    // pagination cursor is bad. Per Matrix client-server spec,
    // M_BAD_PAGINATION is the correct errcode for "bad pagination query
    // parameters" such as an unparseable `since` token. Returning 401
    // M_UNKNOWN_TOKEN here caused SDK clients to discard their access
    // token and force the user to re-login, which is wrong.
    if let Some(ref since_token) = since {
        if since_token.trim().is_empty() {
            since = None;
        } else if SyncToken::parse(since_token).is_none() {
            return Err(ApiError::bad_pagination("Invalid since token".to_string()));
        }
    }

    let (fail_open_on_error, sync_rate_limit_enabled, init_per_second, init_burst_size, inc_per_second, inc_burst_size) =
        resolve_rate_limit_override(&ctx);

    if sync_rate_limit_enabled {
        let is_initial = since.is_none();
        let (per_second, burst_size) =
            if is_initial { (init_per_second, init_burst_size) } else { (inc_per_second, inc_burst_size) };

        let device_id_for_ratelimit = device_id.as_deref().unwrap_or("default");
        let kind = if is_initial { "initial" } else { "incremental" };
        let rate_limit_key = format!("ratelimit:sync:{user_id}:{device_id_for_ratelimit}:{kind}");
        let decision = match ctx.cache.rate_limit_token_bucket_take(&rate_limit_key, per_second, burst_size).await {
            Ok(decision) => decision,
            Err(error) => {
                if fail_open_on_error {
                    tracing::warn!(
                        request_id = %request_id,
                        user_id = %user_id,
                        device_id = %device_id_for_ratelimit,
                        kind,
                        error = %error,
                        "Sync rate limiter failed; allowing request"
                    );
                    synapse_cache::RateLimitDecision { allowed: true, retry_after_seconds: 0, remaining: burst_size }
                } else {
                    return Err(ApiError::internal_with_context("Sync rate limit failed", &error));
                }
            }
        };
        if !decision.allowed {
            let retry_after_ms = decision.retry_after_seconds.saturating_mul(1000);
            record_rate_limited(&ctx.metrics);
            return Err(ApiError::rate_limited_with_retry(retry_after_ms));
        }
    }

    execute_sync(SyncParams {
        ctx,
        user_id,
        device_id,
        timeout,
        is_full_state,
        request_id,
        set_presence,
        filter,
        since,
        use_state_after,
        state_after_is_unstable,
    })
    .await
}

async fn execute_sync(params: SyncParams) -> Result<Json<Value>, ApiError> {
    // Server-side timeout = client's requested timeout + 15s buffer (covers
    // response serialization overhead). S13/N6: 与 room_sync_with_timeout 共用
    // 同一公式，禁止再出现不一致的硬编码外层超时。
    let server_timeout = synapse_common::constants::sync_server_timeout(params.timeout);

    let sync_result = tokio::time::timeout(
        server_timeout,
        params.ctx.sync_service.sync_with_request(SyncServiceRequest {
            user_id: &params.user_id,
            device_id: params.device_id.as_deref(),
            timeout: params.timeout,
            is_full_state: params.is_full_state,
            set_presence: &params.set_presence,
            filter_id: params.filter.as_deref(),
            since: params.since.as_deref(),
            use_state_after: params.use_state_after,
            state_after_is_unstable: params.state_after_is_unstable,
        }),
    )
    .await;

    match sync_result {
        Ok(Ok(result)) => Ok(Json(result)),
        Ok(Err(e)) => {
            ::tracing::error!(request_id = %params.request_id, user_id = %params.user_id, error = %e, "Sync error");
            Err(e)
        }
        Err(_) => {
            ::tracing::error!(request_id = %params.request_id, user_id = %params.user_id, "Sync timeout");
            Err(ApiError::internal("Sync operation timed out".to_string()))
        }
    }
}

/// See [`get_events`].
pub(crate) async fn get_events(
    State(ctx): State<SyncContext>,
    auth_user: AuthenticatedUser,
    Query(params): Query<Value>,
) -> Result<Json<Value>, ApiError> {
    let from = params.get("from").and_then(|v| v.as_str()).unwrap_or("0");
    let timeout = parse_u64_query_param(&params, "timeout").unwrap_or(30000);

    let result = ctx.sync_service.get_events(&auth_user.user_id, from, timeout).await?;

    Ok(Json(result))
}

fn parse_u64_query_param(params: &Value, key: &str) -> Option<u64> {
    let value = params.get(key)?;
    match value {
        Value::Number(number) => number.as_u64(),
        Value::String(raw) => raw.parse::<u64>().ok(),
        _ => None,
    }
}

/// MSC4222: 解析 `use_state_after` 的 opt-in。
///
/// 稳定名与不稳定名都接受；**不稳定名优先**（客户端若明确用了 unstable 拼写，
/// 响应就回 `org.matrix.msc4222.state_after`，这符合 MSC 的 unstable-prefix 约定）。
/// 返回 `(是否启用, 是否不稳定拼写)`。
fn resolve_use_state_after(params: &Value) -> (bool, bool) {
    if parse_bool_query_param(params, "org.matrix.msc4222.use_state_after").unwrap_or(false) {
        return (true, true);
    }
    (parse_bool_query_param(params, "use_state_after").unwrap_or(false), false)
}

fn parse_bool_query_param(params: &Value, key: &str) -> Option<bool> {
    let value = params.get(key)?;
    match value {
        Value::Bool(v) => Some(*v),
        Value::String(raw) => match raw.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" => Some(true),
            "0" | "false" | "no" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // S26 / B-2: 429 计数器独立于慢请求计数器。
    // 若 sync_rate_limited_total > 0，说明 v2 /sync 触发限流，
    // 这在长轮询失效（A-3）时是重要告警信号。

    #[test]
    fn test_record_rate_limited_increments_dedicated_counter() {
        let metrics = synapse_common::metrics::MetricsCollector::new();
        record_rate_limited(&metrics);
        record_rate_limited(&metrics);
        let counter = metrics.get_counter(SYNC_RATE_LIMITED_COUNTER).expect("rate-limited counter must be registered");
        assert_eq!(counter.get(), 2, "每次 429 拒绝都必须独立计数");
    }

    // ── MSC4222：`use_state_after` opt-in 解析 ─────────────────────────────

    /// 稳定与不稳定两种拼写都接受；不稳定拼写**优先**（响应字段要镜像它）。
    #[test]
    fn msc4222_use_state_after_accepts_both_spellings() {
        let unstable = json!({ "org.matrix.msc4222.use_state_after": "true" });
        assert_eq!(resolve_use_state_after(&unstable), (true, true), "不稳定拼写 ⇒ (启用, 不稳定=true)");

        let stable = json!({ "use_state_after": true });
        assert_eq!(resolve_use_state_after(&stable), (true, false), "稳定拼写 ⇒ (启用, 不稳定=false)");

        let both = json!({ "use_state_after": false, "org.matrix.msc4222.use_state_after": "1" });
        assert_eq!(resolve_use_state_after(&both), (true, true), "两种都给时不稳定名优先");

        let absent = json!({});
        assert_eq!(resolve_use_state_after(&absent), (false, false), "缺省不启用");
    }

    /// 只有真值才启用：`false`/`0`/乱码都不算 opt-in（避免拼错就静默改变响应形状）。
    #[test]
    fn msc4222_use_state_after_requires_a_truthy_value() {
        for falsy in [json!("false"), json!("0"), json!(false), json!("yes-please")] {
            let params = json!({ "use_state_after": falsy });
            assert_eq!(resolve_use_state_after(&params), (false, false), "非真值不得启用：{params}");
        }
        for truthy in [json!("1"), json!("true"), json!("YES"), json!(true)] {
            let params = json!({ "use_state_after": truthy });
            assert_eq!(resolve_use_state_after(&params), (true, false), "真值应启用：{params}");
        }
    }
}
