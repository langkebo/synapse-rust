use crate::common::error::ApiError;
use crate::services::module_service::*;
use crate::storage::module::*;
use crate::web::routes::{AppState, AuthenticatedUser};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateModuleBody {
    pub module_name: String,
    pub module_type: String,
    pub version: String,
    pub description: Option<String>,
    pub enabled: Option<bool>,
    pub priority: Option<i32>,
    pub config: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateModuleConfigBody {
    pub config: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EnableModuleBody {
    pub enabled: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CheckSpamBody {
    pub event_id: String,
    pub room_id: String,
    pub sender: String,
    pub event_type: String,
    pub content: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CheckThirdPartyRuleBody {
    pub event_id: String,
    pub room_id: String,
    pub sender: String,
    pub event_type: String,
    pub content: serde_json::Value,
    pub state_events: Vec<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateAccountValidityBody {
    pub user_id: String,
    pub expiration_ts: i64,
    pub is_valid: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RenewAccountBody {
    pub renewal_token: String,
    pub new_expiration_ts: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreatePasswordAuthProviderBody {
    pub provider_name: String,
    pub provider_type: String,
    pub config: serde_json::Value,
    pub enabled: Option<bool>,
    pub priority: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreatePresenceRouteBody {
    pub route_name: String,
    pub route_type: String,
    pub config: serde_json::Value,
    pub enabled: Option<bool>,
    pub priority: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateMediaCallbackBody {
    pub callback_name: String,
    pub callback_type: String,
    pub url: String,
    pub method: Option<String>,
    pub headers: Option<serde_json::Value>,
    pub enabled: Option<bool>,
    pub timeout_ms: Option<i32>,
    pub retry_count: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateRateLimitCallbackBody {
    pub callback_name: String,
    pub callback_type: String,
    pub config: serde_json::Value,
    pub enabled: Option<bool>,
    pub priority: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateAccountDataCallbackBody {
    pub callback_name: String,
    pub callback_type: String,
    pub config: serde_json::Value,
    pub enabled: Option<bool>,
    pub priority: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ModuleResponse {
    pub id: i32,
    pub module_name: String,
    pub module_type: String,
    pub version: String,
    pub description: Option<String>,
    pub enabled: bool,
    pub priority: i32,
    pub config: Option<serde_json::Value>,
    pub created_ts: i64,
    pub updated_ts: i64,
    pub last_executed_ts: Option<i64>,
    pub execution_count: i32,
    pub error_count: i32,
    pub last_error: Option<String>,
}

impl From<Module> for ModuleResponse {
    fn from(m: Module) -> Self {
        Self {
            id: m.id,
            module_name: m.module_name,
            module_type: m.module_type,
            version: m.version,
            description: m.description,
            enabled: m.enabled,
            priority: m.priority,
            config: m.config,
            created_ts: m.created_ts,
            updated_ts: m.updated_ts,
            last_executed_ts: m.last_executed_ts,
            execution_count: m.execution_count,
            error_count: m.error_count,
            last_error: m.last_error,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SpamCheckResultResponse {
    pub id: i32,
    pub event_id: String,
    pub room_id: String,
    pub sender: String,
    pub event_type: String,
    pub content: Option<serde_json::Value>,
    pub result: String,
    pub score: i32,
    pub reason: Option<String>,
    pub checker_module: String,
    pub checked_ts: i64,
    pub action_taken: Option<String>,
}

impl From<SpamCheckResult> for SpamCheckResultResponse {
    fn from(r: SpamCheckResult) -> Self {
        Self {
            id: r.id,
            event_id: r.event_id,
            room_id: r.room_id,
            sender: r.sender,
            event_type: r.event_type,
            content: r.content,
            result: r.result,
            score: r.score,
            reason: r.reason,
            checker_module: r.checker_module,
            checked_ts: r.checked_ts,
            action_taken: r.action_taken,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ThirdPartyRuleResultResponse {
    pub id: i32,
    pub event_id: String,
    pub room_id: String,
    pub sender: String,
    pub event_type: String,
    pub rule_name: String,
    pub allowed: bool,
    pub reason: Option<String>,
    pub modified_content: Option<serde_json::Value>,
    pub checked_ts: i64,
}

impl From<ThirdPartyRuleResult> for ThirdPartyRuleResultResponse {
    fn from(r: ThirdPartyRuleResult) -> Self {
        Self {
            id: r.id,
            event_id: r.event_id,
            room_id: r.room_id,
            sender: r.sender,
            event_type: r.event_type,
            rule_name: r.rule_name,
            allowed: r.allowed,
            reason: r.reason,
            modified_content: r.modified_content,
            checked_ts: r.checked_ts,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AccountValidityResponse {
    pub user_id: String,
    pub expiration_ts: i64,
    pub email_sent_ts: Option<i64>,
    pub renewal_token: Option<String>,
    pub renewal_token_ts: Option<i64>,
    pub is_valid: bool,
    pub created_ts: i64,
    pub updated_ts: i64,
}

impl From<AccountValidity> for AccountValidityResponse {
    fn from(v: AccountValidity) -> Self {
        Self {
            user_id: v.user_id,
            expiration_ts: v.expiration_ts,
            email_sent_ts: v.email_sent_ts,
            renewal_token: v.renewal_token,
            renewal_token_ts: v.renewal_token_ts,
            is_valid: v.is_valid,
            created_ts: v.created_ts,
            updated_ts: v.updated_ts,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PasswordAuthProviderResponse {
    pub id: i32,
    pub provider_name: String,
    pub provider_type: String,
    pub config: Option<serde_json::Value>,
    pub enabled: bool,
    pub priority: i32,
    pub created_ts: i64,
    pub updated_ts: i64,
}

impl From<PasswordAuthProvider> for PasswordAuthProviderResponse {
    fn from(p: PasswordAuthProvider) -> Self {
        Self {
            id: p.id,
            provider_name: p.provider_name,
            provider_type: p.provider_type,
            config: p.config,
            enabled: p.enabled,
            priority: p.priority,
            created_ts: p.created_ts,
            updated_ts: p.updated_ts,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PresenceRouteResponse {
    pub id: i32,
    pub route_name: String,
    pub route_type: String,
    pub config: Option<serde_json::Value>,
    pub enabled: bool,
    pub priority: i32,
    pub created_ts: i64,
    pub updated_ts: i64,
}

impl From<PresenceRoute> for PresenceRouteResponse {
    fn from(r: PresenceRoute) -> Self {
        Self {
            id: r.id,
            route_name: r.route_name,
            route_type: r.route_type,
            config: r.config,
            enabled: r.enabled,
            priority: r.priority,
            created_ts: r.created_ts,
            updated_ts: r.updated_ts,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MediaCallbackResponse {
    pub id: i32,
    pub callback_name: String,
    pub callback_type: String,
    pub url: String,
    pub method: String,
    pub headers: Option<serde_json::Value>,
    pub enabled: bool,
    pub timeout_ms: i32,
    pub retry_count: i32,
    pub created_ts: i64,
    pub updated_ts: i64,
}

impl From<MediaCallback> for MediaCallbackResponse {
    fn from(c: MediaCallback) -> Self {
        Self {
            id: c.id,
            callback_name: c.callback_name,
            callback_type: c.callback_type,
            url: c.url,
            method: c.method,
            headers: c.headers,
            enabled: c.enabled,
            timeout_ms: c.timeout_ms,
            retry_count: c.retry_count,
            created_ts: c.created_ts,
            updated_ts: c.updated_ts,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RateLimitCallbackResponse {
    pub id: i32,
    pub callback_name: String,
    pub callback_type: String,
    pub config: Option<serde_json::Value>,
    pub enabled: bool,
    pub priority: i32,
    pub created_ts: i64,
    pub updated_ts: i64,
}

impl From<RateLimitCallback> for RateLimitCallbackResponse {
    fn from(c: RateLimitCallback) -> Self {
        Self {
            id: c.id,
            callback_name: c.callback_name,
            callback_type: c.callback_type,
            config: c.config,
            enabled: c.enabled,
            priority: c.priority,
            created_ts: c.created_ts,
            updated_ts: c.updated_ts,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AccountDataCallbackResponse {
    pub id: i32,
    pub callback_name: String,
    pub callback_type: String,
    pub config: Option<serde_json::Value>,
    pub enabled: bool,
    pub priority: i32,
    pub created_ts: i64,
    pub updated_ts: i64,
}

impl From<AccountDataCallback> for AccountDataCallbackResponse {
    fn from(c: AccountDataCallback) -> Self {
        Self {
            id: c.id,
            callback_name: c.callback_name,
            callback_type: c.callback_type,
            config: c.config,
            enabled: c.enabled,
            priority: c.priority,
            created_ts: c.created_ts,
            updated_ts: c.updated_ts,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct SpamCheckQuery {
    pub limit: Option<i64>,
}

pub async fn create_module(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Json(body): Json<CreateModuleBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request = CreateModuleRequest {
        module_name: body.module_name,
        module_type: body.module_type,
        version: body.version,
        description: body.description,
        enabled: body.enabled,
        priority: body.priority,
        config: body.config,
    };

    let module = state
        .services
        .module_service
        .register_module(request)
        .await?;

    Ok((StatusCode::CREATED, Json(ModuleResponse::from(module))))
}

pub async fn get_module(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(module_name): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let module = state
        .services
        .module_service
        .get_module(&module_name)
        .await?
        .ok_or_else(|| ApiError::not_found("Module not found"))?;

    Ok(Json(ModuleResponse::from(module)))
}

pub async fn get_modules_by_type(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(module_type): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let modules = state
        .services
        .module_service
        .get_modules_by_type(&module_type)
        .await?;

    let responses: Vec<ModuleResponse> = modules.into_iter().map(ModuleResponse::from).collect();

    Ok(Json(responses))
}

pub async fn get_all_modules(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Query(query): Query<ListQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = query.limit.unwrap_or(100);
    let offset = query.offset.unwrap_or(0);

    let modules = state
        .services
        .module_service
        .get_all_modules(limit, offset)
        .await?;

    let responses: Vec<ModuleResponse> = modules.into_iter().map(ModuleResponse::from).collect();

    Ok(Json(responses))
}

pub async fn update_module_config(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(module_name): Path<String>,
    Json(body): Json<UpdateModuleConfigBody>,
) -> Result<impl IntoResponse, ApiError> {
    let module = state
        .services
        .module_service
        .update_module_config(&module_name, body.config)
        .await?;

    Ok(Json(ModuleResponse::from(module)))
}

pub async fn enable_module(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(module_name): Path<String>,
    Json(body): Json<EnableModuleBody>,
) -> Result<impl IntoResponse, ApiError> {
    let module = state
        .services
        .module_service
        .enable_module(&module_name, body.enabled)
        .await?;

    Ok(Json(ModuleResponse::from(module)))
}

pub async fn delete_module(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(module_name): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    state
        .services
        .module_service
        .delete_module(&module_name)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn check_spam(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Json(body): Json<CheckSpamBody>,
) -> Result<impl IntoResponse, ApiError> {
    let context = SpamCheckContext {
        event_id: body.event_id,
        room_id: body.room_id,
        sender: body.sender,
        event_type: body.event_type,
        content: body.content,
    };

    let result = state.services.module_service.check_spam(&context).await?;

    Ok(Json(result))
}

pub async fn check_third_party_rule(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Json(body): Json<CheckThirdPartyRuleBody>,
) -> Result<impl IntoResponse, ApiError> {
    let context = ThirdPartyRuleContext {
        event_id: body.event_id,
        room_id: body.room_id,
        sender: body.sender,
        event_type: body.event_type,
        content: body.content,
        state_events: body.state_events,
    };

    let result = state
        .services
        .module_service
        .check_third_party_rules(&context)
        .await?;

    Ok(Json(result))
}

pub async fn get_spam_check_result(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(event_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let result = state
        .services
        .module_service
        .get_spam_check_result(&event_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Spam check result not found"))?;

    Ok(Json(SpamCheckResultResponse::from(result)))
}

pub async fn get_spam_check_results_by_sender(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(sender): Path<String>,
    Query(query): Query<SpamCheckQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = query.limit.unwrap_or(100);

    let results = state
        .services
        .module_service
        .get_spam_check_results_by_sender(&sender, limit)
        .await?;

    let responses: Vec<SpamCheckResultResponse> = results
        .into_iter()
        .map(SpamCheckResultResponse::from)
        .collect();

    Ok(Json(responses))
}

pub async fn get_third_party_rule_results(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(event_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let results = state
        .services
        .module_service
        .get_third_party_rule_results(&event_id)
        .await?;

    let responses: Vec<ThirdPartyRuleResultResponse> = results
        .into_iter()
        .map(ThirdPartyRuleResultResponse::from)
        .collect();

    Ok(Json(responses))
}

pub async fn get_execution_logs(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(module_name): Path<String>,
    Query(query): Query<SpamCheckQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = query.limit.unwrap_or(100);

    let logs = state
        .services
        .module_service
        .get_execution_logs(&module_name, limit)
        .await?;

    Ok(Json(logs))
}

pub async fn create_account_validity(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Json(body): Json<CreateAccountValidityBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request = CreateAccountValidityRequest {
        user_id: body.user_id,
        expiration_ts: body.expiration_ts,
        is_valid: body.is_valid,
    };

    let validity = state
        .services
        .account_validity_service
        .create_validity(request)
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(AccountValidityResponse::from(validity)),
    ))
}

pub async fn get_account_validity(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let validity = state
        .services
        .account_validity_service
        .get_validity(&user_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Account validity not found"))?;

    Ok(Json(AccountValidityResponse::from(validity)))
}

pub async fn renew_account(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
    Json(body): Json<RenewAccountBody>,
) -> Result<impl IntoResponse, ApiError> {
    let validity = state
        .services
        .account_validity_service
        .renew_account(&user_id, &body.renewal_token, body.new_expiration_ts)
        .await?;

    Ok(Json(AccountValidityResponse::from(validity)))
}

pub async fn create_password_auth_provider(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Json(body): Json<CreatePasswordAuthProviderBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request = CreatePasswordAuthProviderRequest {
        provider_name: body.provider_name,
        provider_type: body.provider_type,
        config: body.config,
        enabled: body.enabled,
        priority: body.priority,
    };

    let provider = state
        .services
        .module_storage
        .create_password_auth_provider(request)
        .await
        .map_err(|e| {
            ApiError::internal(format!("Failed to create password auth provider: {}", e))
        })?;

    Ok((
        StatusCode::CREATED,
        Json(PasswordAuthProviderResponse::from(provider)),
    ))
}

pub async fn get_password_auth_providers(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    let providers = state
        .services
        .module_storage
        .get_password_auth_providers()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get password auth providers: {}", e)))?;

    let responses: Vec<PasswordAuthProviderResponse> = providers
        .into_iter()
        .map(PasswordAuthProviderResponse::from)
        .collect();

    Ok(Json(responses))
}

pub async fn create_presence_route(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Json(body): Json<CreatePresenceRouteBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request = CreatePresenceRouteRequest {
        route_name: body.route_name,
        route_type: body.route_type,
        config: body.config,
        enabled: body.enabled,
        priority: body.priority,
    };

    let route = state
        .services
        .module_storage
        .create_presence_route(request)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create presence route: {}", e)))?;

    Ok((
        StatusCode::CREATED,
        Json(PresenceRouteResponse::from(route)),
    ))
}

pub async fn get_presence_routes(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    let routes = state
        .services
        .module_storage
        .get_presence_routes()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get presence routes: {}", e)))?;

    let responses: Vec<PresenceRouteResponse> = routes
        .into_iter()
        .map(PresenceRouteResponse::from)
        .collect();

    Ok(Json(responses))
}

pub async fn create_media_callback(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Json(body): Json<CreateMediaCallbackBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request = CreateMediaCallbackRequest {
        callback_name: body.callback_name,
        callback_type: body.callback_type,
        url: body.url,
        method: body.method,
        headers: body.headers,
        enabled: body.enabled,
        timeout_ms: body.timeout_ms,
        retry_count: body.retry_count,
    };

    let callback = state
        .services
        .module_storage
        .create_media_callback(request)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create media callback: {}", e)))?;

    Ok((
        StatusCode::CREATED,
        Json(MediaCallbackResponse::from(callback)),
    ))
}

pub async fn get_media_callbacks(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(callback_type): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let callbacks = state
        .services
        .module_storage
        .get_media_callbacks(Some(&callback_type))
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get media callbacks: {}", e)))?;

    let responses: Vec<MediaCallbackResponse> = callbacks
        .into_iter()
        .map(MediaCallbackResponse::from)
        .collect();

    Ok(Json(responses))
}

pub async fn get_all_media_callbacks(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    let callbacks = state
        .services
        .module_storage
        .get_media_callbacks(None)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get media callbacks: {}", e)))?;

    let responses: Vec<MediaCallbackResponse> = callbacks
        .into_iter()
        .map(MediaCallbackResponse::from)
        .collect();

    Ok(Json(responses))
}

pub async fn create_rate_limit_callback(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Json(body): Json<CreateRateLimitCallbackBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request = CreateRateLimitCallbackRequest {
        callback_name: body.callback_name,
        callback_type: body.callback_type,
        config: body.config,
        enabled: body.enabled,
        priority: body.priority,
    };

    let callback = state
        .services
        .module_storage
        .create_rate_limit_callback(request)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create rate limit callback: {}", e)))?;

    Ok((
        StatusCode::CREATED,
        Json(RateLimitCallbackResponse::from(callback)),
    ))
}

pub async fn get_rate_limit_callbacks(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    let callbacks = state
        .services
        .module_storage
        .get_rate_limit_callbacks()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get rate limit callbacks: {}", e)))?;

    let responses: Vec<RateLimitCallbackResponse> = callbacks
        .into_iter()
        .map(RateLimitCallbackResponse::from)
        .collect();

    Ok(Json(responses))
}

pub async fn create_account_data_callback(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Json(body): Json<CreateAccountDataCallbackBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request = CreateAccountDataCallbackRequest {
        callback_name: body.callback_name,
        callback_type: body.callback_type,
        config: body.config,
        enabled: body.enabled,
        priority: body.priority,
    };

    let callback = state
        .services
        .module_storage
        .create_account_data_callback(request)
        .await
        .map_err(|e| {
            ApiError::internal(format!("Failed to create account data callback: {}", e))
        })?;

    Ok((
        StatusCode::CREATED,
        Json(AccountDataCallbackResponse::from(callback)),
    ))
}

pub async fn get_account_data_callbacks(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    let callbacks = state
        .services
        .module_storage
        .get_account_data_callbacks()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get account data callbacks: {}", e)))?;

    let responses: Vec<AccountDataCallbackResponse> = callbacks
        .into_iter()
        .map(AccountDataCallbackResponse::from)
        .collect();

    Ok(Json(responses))
}

pub fn create_module_router() -> Router<AppState> {
    Router::new()
        .route("/_synapse/admin/v1/modules", post(create_module))
        .route("/_synapse/admin/v1/modules", get(get_all_modules))
        .route(
            "/_synapse/admin/v1/modules/type/{module_type}",
            get(get_modules_by_type),
        )
        .route("/_synapse/admin/v1/modules/{module_name}", get(get_module))
        .route(
            "/_synapse/admin/v1/modules/{module_name}/config",
            put(update_module_config),
        )
        .route(
            "/_synapse/admin/v1/modules/{module_name}/enable",
            post(enable_module),
        )
        .route(
            "/_synapse/admin/v1/modules/{module_name}",
            delete(delete_module),
        )
        .route("/_synapse/admin/v1/modules/check_spam", post(check_spam))
        .route(
            "/_synapse/admin/v1/modules/check_third_party_rule",
            post(check_third_party_rule),
        )
        .route(
            "/_synapse/admin/v1/modules/spam_check/{event_id}",
            get(get_spam_check_result),
        )
        .route(
            "/_synapse/admin/v1/modules/spam_check/sender/{sender}",
            get(get_spam_check_results_by_sender),
        )
        .route(
            "/_synapse/admin/v1/modules/third_party_rule/{event_id}",
            get(get_third_party_rule_results),
        )
        .route(
            "/_synapse/admin/v1/modules/logs/{module_name}",
            get(get_execution_logs),
        )
        .route(
            "/_synapse/admin/v1/account_validity",
            post(create_account_validity),
        )
        .route(
            "/_synapse/admin/v1/account_validity/{user_id}",
            get(get_account_validity),
        )
        .route(
            "/_synapse/admin/v1/account_validity/{user_id}/renew",
            post(renew_account),
        )
        .route(
            "/_synapse/admin/v1/password_auth_providers",
            post(create_password_auth_provider),
        )
        .route(
            "/_synapse/admin/v1/password_auth_providers",
            get(get_password_auth_providers),
        )
        .route(
            "/_synapse/admin/v1/presence_routes",
            post(create_presence_route),
        )
        .route(
            "/_synapse/admin/v1/presence_routes",
            get(get_presence_routes),
        )
        .route(
            "/_synapse/admin/v1/media_callbacks",
            post(create_media_callback),
        )
        .route(
            "/_synapse/admin/v1/media_callbacks",
            get(get_all_media_callbacks),
        )
        .route(
            "/_synapse/admin/v1/media_callbacks/{callback_type}",
            get(get_media_callbacks),
        )
        .route(
            "/_synapse/admin/v1/rate_limit_callbacks",
            post(create_rate_limit_callback),
        )
        .route(
            "/_synapse/admin/v1/rate_limit_callbacks",
            get(get_rate_limit_callbacks),
        )
        .route(
            "/_synapse/admin/v1/account_data_callbacks",
            post(create_account_data_callback),
        )
        .route(
            "/_synapse/admin/v1/account_data_callbacks",
            get(get_account_data_callbacks),
        )
}
