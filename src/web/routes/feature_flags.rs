use crate::common::ApiError;
use crate::storage::{CreateFeatureFlagRequest, FeatureFlagFilters, UpdateFeatureFlagRequest};
use crate::web::routes::{AdminUser, AppState};
use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};

fn decode_feature_flag_cursor(cursor: Option<&str>) -> Option<(i64, &str)> {
    let cursor = cursor?;
    let (updated_ts, flag_key) = cursor.split_once('|')?;
    let updated_ts = updated_ts.parse::<i64>().ok()?;
    if flag_key.is_empty() {
        return None;
    }
    Some((updated_ts, flag_key))
}

fn encode_feature_flag_cursor(updated_ts: i64, flag_key: &str) -> String {
    format!("{updated_ts}|{flag_key}")
}

#[cfg(test)]
mod cursor_tests {
    use super::{decode_feature_flag_cursor, encode_feature_flag_cursor};

    #[test]
    fn test_feature_flag_cursor_round_trip() {
        let cursor = encode_feature_flag_cursor(1_700_000_000_000, "my.flag");
        assert_eq!(decode_feature_flag_cursor(Some(&cursor)), Some((1_700_000_000_000, "my.flag")));
    }

    #[test]
    fn test_feature_flag_cursor_rejects_invalid_value() {
        assert_eq!(decode_feature_flag_cursor(Some("bad-cursor")), None);
        assert_eq!(decode_feature_flag_cursor(Some("123|")), None);
    }
}

#[derive(Debug, Deserialize)]
pub struct FeatureFlagListQuery {
    pub target_scope: Option<String>,
    pub status: Option<String>,
    pub limit: Option<i64>,
    pub from: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct FeatureFlagListResponse<T> {
    pub flags: Vec<T>,
    pub total: i64,
    pub next_batch: Option<String>,
}

pub async fn create_feature_flag(
    State(state): State<AppState>,
    headers: HeaderMap,
    admin_user: AdminUser,
    Json(body): Json<CreateFeatureFlagRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let request_id = request_id(&headers);
    let flag = state.services.admin.feature_flag_service.create_flag(&admin_user.user_id, &request_id, body).await?;
    Ok(Json(flag))
}

pub async fn update_feature_flag(
    State(state): State<AppState>,
    headers: HeaderMap,
    admin_user: AdminUser,
    Path(flag_key): Path<String>,
    Json(body): Json<UpdateFeatureFlagRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let request_id = request_id(&headers);
    let flag =
        state.services.admin.feature_flag_service.update_flag(&admin_user.user_id, &request_id, &flag_key, body).await?;
    Ok(Json(flag))
}

pub async fn get_feature_flag(
    State(state): State<AppState>,
    Path(flag_key): Path<String>,
    _admin_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let flag = state.services.admin.feature_flag_service.get_flag(&flag_key).await?;
    Ok(Json(flag))
}

pub async fn list_feature_flags(
    State(state): State<AppState>,
    Query(query): Query<FeatureFlagListQuery>,
    _admin_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let cursor = decode_feature_flag_cursor(query.from.as_deref());
    if query.from.is_some() && cursor.is_none() {
        return Err(ApiError::bad_request("Invalid from cursor".to_string()));
    }

    let filters = FeatureFlagFilters {
        target_scope: query.target_scope,
        status: query.status,
        limit: query.limit.unwrap_or(50).clamp(1, 200),
        cursor_updated_ts: cursor.map(|(updated_ts, _)| updated_ts),
        cursor_flag_key: cursor.map(|(_, flag_key)| flag_key.to_string()),
    };
    let (flags, total) = state.services.admin.feature_flag_service.list_flags(filters).await?;
    let next_batch = flags
        .last()
        .map(|flag| encode_feature_flag_cursor(flag.updated_ts, &flag.flag_key))
        .filter(|_| flags.len() as i64 == query.limit.unwrap_or(50).clamp(1, 200));
    Ok(Json(FeatureFlagListResponse { flags, total, next_batch }))
}

pub fn create_feature_flags_router(state: AppState) -> axum::Router<AppState> {
    use axum::routing::*;

    axum::Router::new()
        .route("/_synapse/admin/v1/feature-flags", post(create_feature_flag).get(list_feature_flags))
        .route("/_synapse/admin/v1/feature-flags/{flag_key}", get(get_feature_flag).patch(update_feature_flag))
        .route_layer(axum::middleware::from_fn_with_state(state.clone(), crate::web::middleware::admin_auth_middleware))
        .with_state(state)
}

pub fn feature_flags_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::RouteEntry;
    use axum::http::Method;
    [
        (Method::POST, "/_synapse/admin/v1/feature-flags"),
        (Method::GET, "/_synapse/admin/v1/feature-flags"),
        (Method::GET, "/_synapse/admin/v1/feature-flags/{flag_key}"),
        // PATCH would normally be the probe method we use to detect 405; the
        // ledger probe test treats non-405 statuses as "route is wired" so a
        // real PATCH handler here is fine — we just won't get an Allow-header
        // assertion for this entry.
        (Method::PATCH, "/_synapse/admin/v1/feature-flags/{flag_key}"),
    ]
    .into_iter()
    .map(|(m, p)| RouteEntry::new(m, p, "feature_flags"))
    .collect()
}

fn request_id(headers: &HeaderMap) -> String {
    headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty())
        .map_or_else(|| format!("feature-flag-{}", uuid::Uuid::new_v4()), ToOwned::to_owned)
}
