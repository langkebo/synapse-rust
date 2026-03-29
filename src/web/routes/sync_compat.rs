use crate::common::ApiError;
use crate::web::routes::{extract_token_from_headers, AppState};
use axum::{
    extract::{Json, Path, Query, State},
    http::HeaderMap,
};
use serde::Serialize;
use serde_json::{json, Value};

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

#[allow(dead_code)]
pub(crate) async fn create_filter(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(_user_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<FilterResponse>, ApiError> {
    let token = extract_token_from_headers(&headers)?;
    let (_, _, _) = state.services.auth_service.validate_token(&token).await?;

    let filter_id = format!("f{}", uuid::Uuid::new_v4());

    Ok(Json(FilterResponse {
        filter_id,
        room: body.get("room").cloned(),
        presence: body.get("presence").cloned(),
    }))
}

#[allow(dead_code)]
pub(crate) async fn get_filter(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((_user_id, filter_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let token = extract_token_from_headers(&headers)?;
    let (_, _, _) = state.services.auth_service.validate_token(&token).await?;

    Ok(Json(json!({
        "filter_id": filter_id,
        "filter": {
            "room": {
                "state": {"limit": 50},
                "timeline": {"limit": 50}
            },
            "presence": {"limit": 100}
        }
    })))
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
        .await
        .unwrap_or(json!({
            "start": from,
            "end": from,
            "chunk": []
        }));

    Ok(Json(result))
}
