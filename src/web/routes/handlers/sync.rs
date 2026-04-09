use crate::common::ApiError;
use crate::web::routes::{extract_token_from_headers, AppState};
use axum::{
    extract::{Json, Query, State},
    http::HeaderMap,
};
use serde::Serialize;
use serde_json::Value;

pub(crate) async fn sync(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<Value>,
) -> Result<Json<Value>, ApiError> {
    let token = extract_token_from_headers(&headers)?;
    let (user_id, device_id, _) = state.services.auth_service.validate_token(&token).await?;

    let timeout = parse_u64_query_param(&params, "timeout").unwrap_or(30000);
    let full_state = parse_bool_query_param(&params, "full_state").unwrap_or(false);
    let set_presence = params
        .get("set_presence")
        .and_then(|v| v.as_str())
        .unwrap_or("online");
    let since = params.get("since").and_then(|v| v.as_str());

    let file_config = state
        .rate_limit_config_manager
        .as_ref()
        .map(|m| m.get_config());
    let sync_rate_limit_enabled = file_config
        .as_ref()
        .map(|c| c.sync.enabled)
        .unwrap_or(state.services.config.rate_limit.sync.enabled);
    if sync_rate_limit_enabled {
        let is_initial = since.is_none();
        let (per_second, burst_size) = match &file_config {
            Some(c) if c.sync.enabled => {
                if is_initial {
                    (c.sync.initial.per_second, c.sync.initial.burst_size)
                } else {
                    (c.sync.incremental.per_second, c.sync.incremental.burst_size)
                }
            }
            _ => {
                let c = &state.services.config.rate_limit.sync;
                if is_initial {
                    (c.initial.per_second, c.initial.burst_size)
                } else {
                    (c.incremental.per_second, c.incremental.burst_size)
                }
            }
        };

        let device_id_for_ratelimit = device_id.as_deref().unwrap_or("default");
        let kind = if is_initial { "initial" } else { "incremental" };
        let rate_limit_key = format!(
            "ratelimit:sync:{}:{}:{}",
            user_id, device_id_for_ratelimit, kind
        );
        let decision = state
            .cache
            .rate_limit_token_bucket_take(&rate_limit_key, per_second, burst_size)
            .await
            .map_err(|e| ApiError::internal(format!("Sync rate limit failed: {}", e)))?;
        if !decision.allowed {
            let retry_after_ms = decision.retry_after_seconds.saturating_mul(1000);
            return Err(ApiError::rate_limited_with_retry(retry_after_ms));
        }
    }

    let sync_result = tokio::time::timeout(
        std::time::Duration::from_secs(60),
        state.services.sync_service.sync(
            &user_id,
            device_id.as_deref(),
            timeout,
            full_state,
            set_presence,
            since,
        ),
    )
    .await;

    match sync_result {
        Ok(Ok(result)) => Ok(Json(result)),
        Ok(Err(e)) => {
            ::tracing::error!("Sync error for user {}: {}", user_id, e);
            Err(e)
        }
        Err(_) => {
            ::tracing::error!("Sync timeout for user {}", user_id);
            Err(ApiError::internal("Sync operation timed out".to_string()))
        }
    }
}

#[derive(Serialize)]
#[allow(dead_code)]
pub(crate) struct FilterResponse {
    filter_id: String,
    room: Option<Value>,
    presence: Option<Value>,
}

pub(crate) async fn get_events(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<Value>,
) -> Result<Json<Value>, ApiError> {
    let token = extract_token_from_headers(&headers)?;
    let (user_id, _, _) = state.services.auth_service.validate_token(&token).await?;

    let from = params.get("from").and_then(|v| v.as_str()).unwrap_or("0");
    let timeout = parse_u64_query_param(&params, "timeout").unwrap_or(30000);

    let result = state
        .services
        .sync_service
        .get_events(&user_id, from, timeout)
        .await?;

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
