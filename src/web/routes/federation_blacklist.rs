use crate::common::error::ApiError;
use crate::services::federation_blacklist_service::{AddBlacklistRequest, CheckResult};
use crate::storage::federation_blacklist::{
    CreateRuleRequest, FederationBlacklist, FederationBlacklistRule,
};
use crate::web::routes::AppState;
use crate::web::routes::AuthenticatedUser;
use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct AddBlacklistBody {
    pub server_name: String,
    pub block_type: String,
    pub reason: Option<String>,
    pub expires_in_days: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct CheckServerQuery {
    pub server_name: String,
}

#[derive(Debug, Deserialize)]
pub struct PaginationQuery {
    pub limit: Option<i32>,
    pub offset: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct CreateRuleBody {
    pub rule_name: String,
    pub rule_type: String,
    pub pattern: String,
    pub action: String,
    pub priority: Option<i32>,
    pub description: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct BlacklistEntryResponse {
    pub id: i32,
    pub server_name: String,
    pub block_type: String,
    pub reason: Option<String>,
    pub blocked_by: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub is_enabled: bool,
}

impl From<FederationBlacklist> for BlacklistEntryResponse {
    fn from(entry: FederationBlacklist) -> Self {
        Self {
            id: entry.id,
            server_name: entry.server_name,
            block_type: entry.block_type,
            reason: entry.reason,
            blocked_by: entry.blocked_by,
            created_at: entry.created_at,
            expires_at: entry.expires_at,
            is_enabled: entry.is_enabled,
        }
    }
}

pub async fn add_to_blacklist(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Json(body): Json<AddBlacklistBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request = AddBlacklistRequest {
        server_name: body.server_name,
        block_type: body.block_type,
        reason: body.reason,
        expires_in_days: body.expires_in_days,
    };

    let entry = state.services.federation_blacklist_service
        .add_to_blacklist(request, &_auth_user.user_id)
        .await?;

    Ok(Json(BlacklistEntryResponse::from(entry)))
}

pub async fn remove_from_blacklist(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(server_name): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    state.services.federation_blacklist_service
        .remove_from_blacklist(&server_name, &_auth_user.user_id)
        .await?;

    Ok(Json(serde_json::json!({
        "message": format!("Server {} removed from blacklist", server_name)
    })))
}

pub async fn check_server(
    State(state): State<AppState>,
    Query(query): Query<CheckServerQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let result = state.services.federation_blacklist_service
        .check_server(&query.server_name)
        .await?;

    Ok(Json(CheckResultResponse::from(result)))
}

#[derive(Debug, Serialize)]
pub struct CheckResultResponse {
    pub is_blocked: bool,
    pub is_whitelisted: bool,
    pub is_quarantined: bool,
    pub reason: Option<String>,
    pub matched_rule: Option<String>,
}

impl From<CheckResult> for CheckResultResponse {
    fn from(result: CheckResult) -> Self {
        Self {
            is_blocked: result.is_blocked,
            is_whitelisted: result.is_whitelisted,
            is_quarantined: result.is_quarantined,
            reason: result.reason,
            matched_rule: result.matched_rule,
        }
    }
}

pub async fn get_blacklist(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Query(query): Query<PaginationQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = query.limit.unwrap_or(100).min(1000);
    let offset = query.offset.unwrap_or(0);

    let entries = state.services.federation_blacklist_service
        .get_blacklist(limit, offset)
        .await?;

    let response: Vec<BlacklistEntryResponse> = entries.into_iter().map(BlacklistEntryResponse::from).collect();

    Ok(Json(response))
}

pub async fn get_server_stats(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(server_name): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let stats = state.services.federation_blacklist_service
        .get_stats(&server_name)
        .await?;

    Ok(Json(stats))
}

#[derive(Debug, Serialize)]
pub struct RuleResponse {
    pub id: i32,
    pub rule_name: String,
    pub rule_type: String,
    pub pattern: String,
    pub action: String,
    pub priority: i32,
    pub description: Option<String>,
    pub is_enabled: bool,
}

impl From<FederationBlacklistRule> for RuleResponse {
    fn from(rule: FederationBlacklistRule) -> Self {
        Self {
            id: rule.id,
            rule_name: rule.rule_name,
            rule_type: rule.rule_type,
            pattern: rule.pattern,
            action: rule.action,
            priority: rule.priority,
            description: rule.description,
            is_enabled: rule.is_enabled,
        }
    }
}

pub async fn create_rule(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Json(body): Json<CreateRuleBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request = CreateRuleRequest {
        rule_name: body.rule_name,
        rule_type: body.rule_type,
        pattern: body.pattern,
        action: body.action,
        priority: body.priority.unwrap_or(100),
        description: body.description,
        created_by: _auth_user.user_id.clone(),
    };

    let rule = state.services.federation_blacklist_service.create_rule(request).await?;

    Ok(Json(RuleResponse::from(rule)))
}

pub async fn get_rules(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    let rules = state.services.federation_blacklist_service.get_rules().await?;

    let response: Vec<RuleResponse> = rules.into_iter().map(RuleResponse::from).collect();

    Ok(Json(response))
}

pub async fn cleanup_expired(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    let count = state.services.federation_blacklist_service.cleanup_expired().await?;

    Ok(Json(serde_json::json!({
        "cleaned_count": count,
        "message": format!("Cleaned up {} expired entries", count)
    })))
}

pub fn create_federation_blacklist_router() -> axum::Router<AppState> {
    use axum::routing::*;

    axum::Router::new()
        .route("/_synapse/admin/v1/federation/blacklist", post(add_to_blacklist))
        .route("/_synapse/admin/v1/federation/blacklist/{server_name}", delete(remove_from_blacklist))
        .route("/_synapse/admin/v1/federation/blacklist/check", get(check_server))
        .route("/_synapse/admin/v1/federation/blacklist/list", get(get_blacklist))
        .route("/_synapse/admin/v1/federation/blacklist/stats/{server_name}", get(get_server_stats))
        .route("/_synapse/admin/v1/federation/blacklist/rules", post(create_rule))
        .route("/_synapse/admin/v1/federation/blacklist/rules", get(get_rules))
        .route("/_synapse/admin/v1/federation/blacklist/cleanup", post(cleanup_expired))
}
