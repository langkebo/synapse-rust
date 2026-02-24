use axum::{
    extract::State,
    routing::{get, post, put, delete},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::common::rate_limit_config::{
    RateLimitEndpointRule, RateLimitRule,
};
use crate::common::ApiError;
use crate::web::routes::{AdminUser, AppState};

#[derive(Debug, Serialize)]
pub struct RateLimitStatusResponse {
    pub enabled: bool,
    pub default_rule: RateLimitRuleInfo,
    pub endpoint_rules: Vec<EndpointRuleInfo>,
    pub exempt_paths: Vec<String>,
    pub exempt_path_prefixes: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct RateLimitRuleInfo {
    pub per_second: u32,
    pub burst_size: u32,
}

impl From<RateLimitRule> for RateLimitRuleInfo {
    fn from(rule: RateLimitRule) -> Self {
        Self {
            per_second: rule.per_second,
            burst_size: rule.burst_size,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct EndpointRuleInfo {
    pub path: String,
    pub match_type: String,
    pub rule: RateLimitRuleInfo,
}

impl From<RateLimitEndpointRule> for EndpointRuleInfo {
    fn from(rule: RateLimitEndpointRule) -> Self {
        Self {
            path: rule.path,
            match_type: match rule.match_type {
                crate::common::rate_limit_config::RateLimitMatchType::Exact => "exact".to_string(),
                crate::common::rate_limit_config::RateLimitMatchType::Prefix => "prefix".to_string(),
            },
            rule: rule.rule.into(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct UpdateEnabledRequest {
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct UpdateDefaultRuleRequest {
    pub per_second: u32,
    pub burst_size: u32,
}

#[derive(Debug, Deserialize)]
pub struct AddEndpointRuleRequest {
    pub path: String,
    #[serde(default)]
    pub match_type: String,
    pub per_second: u32,
    pub burst_size: u32,
}

#[derive(Debug, Deserialize)]
pub struct ExemptPathRequest {
    pub path: String,
}

#[derive(Debug, Serialize)]
pub struct ApiResponse {
    pub success: bool,
    pub message: String,
}

pub fn create_rate_limit_admin_router() -> Router<AppState> {
    Router::new()
        .route("/_admin/rate-limit/status", get(get_rate_limit_status))
        .route("/_admin/rate-limit/enabled", put(set_rate_limit_enabled))
        .route("/_admin/rate-limit/default", put(update_default_rule))
        .route("/_admin/rate-limit/endpoints", get(get_endpoint_rules))
        .route("/_admin/rate-limit/endpoints", post(add_endpoint_rule))
        .route("/_admin/rate-limit/endpoints/{path}", delete(remove_endpoint_rule))
        .route("/_admin/rate-limit/exempt-paths", get(get_exempt_paths))
        .route("/_admin/rate-limit/exempt-paths", post(add_exempt_path))
        .route("/_admin/rate-limit/exempt-paths/{path}", delete(remove_exempt_path))
        .route("/_admin/rate-limit/reload", post(reload_config))
}

#[axum::debug_handler]
pub async fn get_rate_limit_status(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<RateLimitStatusResponse>, ApiError> {
    let manager = state.rate_limit_config_manager.as_ref()
        .ok_or_else(|| ApiError::internal("Rate limit config manager not initialized"))?;

    let config = manager.get_config();

    Ok(Json(RateLimitStatusResponse {
        enabled: config.enabled,
        default_rule: config.default.into(),
        endpoint_rules: config.endpoints.into_iter().map(EndpointRuleInfo::from).collect(),
        exempt_paths: config.exempt_paths,
        exempt_path_prefixes: config.exempt_path_prefixes,
    }))
}

#[axum::debug_handler]
pub async fn set_rate_limit_enabled(
    _admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<UpdateEnabledRequest>,
) -> Result<Json<ApiResponse>, ApiError> {
    let manager = state.rate_limit_config_manager.as_ref()
        .ok_or_else(|| ApiError::internal("Rate limit config manager not initialized"))?;

    manager.set_enabled(body.enabled).await
        .map_err(|e| ApiError::internal(format!("Failed to update rate limit enabled status: {}", e)))?;

    Ok(Json(ApiResponse {
        success: true,
        message: format!("Rate limit enabled set to {}", body.enabled),
    }))
}

#[axum::debug_handler]
pub async fn update_default_rule(
    _admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<UpdateDefaultRuleRequest>,
) -> Result<Json<ApiResponse>, ApiError> {
    let manager = state.rate_limit_config_manager.as_ref()
        .ok_or_else(|| ApiError::internal("Rate limit config manager not initialized"))?;

    let rule = RateLimitRule {
        per_second: body.per_second,
        burst_size: body.burst_size,
    };

    manager.set_default_rule(rule).await
        .map_err(|e| ApiError::internal(format!("Failed to update default rule: {}", e)))?;

    Ok(Json(ApiResponse {
        success: true,
        message: format!(
            "Default rule updated: {} req/s, burst {}",
            body.per_second, body.burst_size
        ),
    }))
}

#[axum::debug_handler]
pub async fn get_endpoint_rules(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<EndpointRuleInfo>>, ApiError> {
    let manager = state.rate_limit_config_manager.as_ref()
        .ok_or_else(|| ApiError::internal("Rate limit config manager not initialized"))?;

    let config = manager.get_config();
    Ok(Json(
        config
            .endpoints
            .into_iter()
            .map(EndpointRuleInfo::from)
            .collect(),
    ))
}

#[axum::debug_handler]
pub async fn add_endpoint_rule(
    _admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<AddEndpointRuleRequest>,
) -> Result<Json<ApiResponse>, ApiError> {
    let manager = state.rate_limit_config_manager.as_ref()
        .ok_or_else(|| ApiError::internal("Rate limit config manager not initialized"))?;

    let match_type = match body.match_type.to_lowercase().as_str() {
        "prefix" => crate::common::rate_limit_config::RateLimitMatchType::Prefix,
        _ => crate::common::rate_limit_config::RateLimitMatchType::Exact,
    };

    let rule = RateLimitEndpointRule {
        path: body.path.clone(),
        match_type,
        rule: RateLimitRule {
            per_second: body.per_second,
            burst_size: body.burst_size,
        },
    };

    manager.add_endpoint_rule(rule).await
        .map_err(|e| ApiError::internal(format!("Failed to add endpoint rule: {}", e)))?;

    Ok(Json(ApiResponse {
        success: true,
        message: format!("Endpoint rule added for path: {}", body.path),
    }))
}

#[axum::debug_handler]
pub async fn remove_endpoint_rule(
    _admin: AdminUser,
    State(state): State<AppState>,
    axum::extract::Path(path): axum::extract::Path<String>,
) -> Result<Json<ApiResponse>, ApiError> {
    let manager = state.rate_limit_config_manager.as_ref()
        .ok_or_else(|| ApiError::internal("Rate limit config manager not initialized"))?;

    let decoded_path = urlencoding::decode(&path).unwrap_or_default().to_string();

    manager.remove_endpoint_rule(&decoded_path).await
        .map_err(|e| ApiError::internal(format!("Failed to remove endpoint rule: {}", e)))?;

    Ok(Json(ApiResponse {
        success: true,
        message: format!("Endpoint rule removed for path: {}", decoded_path),
    }))
}

#[axum::debug_handler]
pub async fn get_exempt_paths(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<String>>, ApiError> {
    let manager = state.rate_limit_config_manager.as_ref()
        .ok_or_else(|| ApiError::internal("Rate limit config manager not initialized"))?;

    let config = manager.get_config();
    Ok(Json(config.exempt_paths))
}

#[axum::debug_handler]
pub async fn add_exempt_path(
    _admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<ExemptPathRequest>,
) -> Result<Json<ApiResponse>, ApiError> {
    let manager = state.rate_limit_config_manager.as_ref()
        .ok_or_else(|| ApiError::internal("Rate limit config manager not initialized"))?;

    manager.add_exempt_path(body.path.clone()).await
        .map_err(|e| ApiError::internal(format!("Failed to add exempt path: {}", e)))?;

    Ok(Json(ApiResponse {
        success: true,
        message: format!("Exempt path added: {}", body.path),
    }))
}

#[axum::debug_handler]
pub async fn remove_exempt_path(
    _admin: AdminUser,
    State(state): State<AppState>,
    axum::extract::Path(path): axum::extract::Path<String>,
) -> Result<Json<ApiResponse>, ApiError> {
    let manager = state.rate_limit_config_manager.as_ref()
        .ok_or_else(|| ApiError::internal("Rate limit config manager not initialized"))?;

    let decoded_path = urlencoding::decode(&path).unwrap_or_default().to_string();

    manager.remove_exempt_path(&decoded_path).await
        .map_err(|e| ApiError::internal(format!("Failed to remove exempt path: {}", e)))?;

    Ok(Json(ApiResponse {
        success: true,
        message: format!("Exempt path removed: {}", decoded_path),
    }))
}

#[axum::debug_handler]
pub async fn reload_config(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse>, ApiError> {
    let manager = state.rate_limit_config_manager.as_ref()
        .ok_or_else(|| ApiError::internal("Rate limit config manager not initialized"))?;

    manager.reload().await
        .map_err(|e| ApiError::internal(format!("Failed to reload config: {}", e)))?;

    Ok(Json(ApiResponse {
        success: true,
        message: "Rate limit configuration reloaded".to_string(),
    }))
}
