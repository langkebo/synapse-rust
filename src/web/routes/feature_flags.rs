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

#[derive(Debug, Deserialize)]
pub struct FeatureFlagListQuery {
    pub target_scope: Option<String>,
    pub status: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct FeatureFlagListResponse<T> {
    pub flags: Vec<T>,
    pub total: i64,
}

pub async fn create_feature_flag(
    State(state): State<AppState>,
    headers: HeaderMap,
    admin_user: AdminUser,
    Json(body): Json<CreateFeatureFlagRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let request_id = request_id(&headers);
    let flag = state
        .services
        .feature_flag_service
        .create_flag(&admin_user.user_id, &request_id, body)
        .await?;
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
    let flag = state
        .services
        .feature_flag_service
        .update_flag(&admin_user.user_id, &request_id, &flag_key, body)
        .await?;
    Ok(Json(flag))
}

pub async fn get_feature_flag(
    State(state): State<AppState>,
    Path(flag_key): Path<String>,
    _admin_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let flag = state
        .services
        .feature_flag_service
        .get_flag(&flag_key)
        .await?;
    Ok(Json(flag))
}

pub async fn list_feature_flags(
    State(state): State<AppState>,
    Query(query): Query<FeatureFlagListQuery>,
    _admin_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let filters = FeatureFlagFilters {
        target_scope: query.target_scope,
        status: query.status,
        limit: query.limit.unwrap_or(50).clamp(1, 200),
        offset: query.offset.unwrap_or(0).max(0),
    };
    let (flags, total) = state
        .services
        .feature_flag_service
        .list_flags(filters)
        .await?;
    Ok(Json(FeatureFlagListResponse { flags, total }))
}

pub fn create_feature_flags_router(state: AppState) -> axum::Router<AppState> {
    use axum::routing::*;

    axum::Router::new()
        .route(
            "/_synapse/admin/v1/feature-flags",
            post(create_feature_flag).get(list_feature_flags),
        )
        .route(
            "/_synapse/admin/v1/feature-flags/{flag_key}",
            get(get_feature_flag).patch(update_feature_flag),
        )
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::web::middleware::admin_auth_middleware,
        ))
        .with_state(state)
}

fn request_id(headers: &HeaderMap) -> String {
    headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("feature-flag-{}", uuid::Uuid::new_v4()))
}
