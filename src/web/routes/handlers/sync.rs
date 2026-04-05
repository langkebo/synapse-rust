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
    let (user_id, _, _) = state.services.auth_service.validate_token(&token).await?;

    let timeout = params
        .get("timeout")
        .and_then(|v| v.as_u64())
        .unwrap_or(30000);
    let full_state = params
        .get("full_state")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let set_presence = params
        .get("set_presence")
        .and_then(|v| v.as_str())
        .unwrap_or("online");
    let since = params.get("since").and_then(|v| v.as_str());

    let sync_result = tokio::time::timeout(
        std::time::Duration::from_secs(60),
        state
            .services
            .sync_service
            .sync(&user_id, timeout, full_state, set_presence, since),
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
    let timeout = params
        .get("timeout")
        .and_then(|v| v.as_u64())
        .unwrap_or(30000);

    let result = state
        .services
        .sync_service
        .get_events(&user_id, from, timeout)
        .await?;

    Ok(Json(result))
}
