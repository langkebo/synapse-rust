use crate::common::error::ApiError;
use crate::web::routes::context::AdminContext;
use crate::web::routes::extractors::{EventId, UserId};
use crate::web::routes::{AdminUser, AppState};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use synapse_services::module_service::*;
use synapse_storage::module::*;

/// The `CreateModuleBody` struct.
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateModuleBody {
    /// The `module_name` field.
    pub module_name: String,
    /// The `module_type` field.
    pub module_type: String,
    /// The `version` field.
    pub version: String,
    /// The `description` field.
    pub description: Option<String>,
    /// The `is_enabled` field.
    pub is_enabled: Option<bool>,
    /// The `priority` field.
    pub priority: Option<i32>,
    /// The `config` field.
    pub config: Option<serde_json::Value>,
}

/// The `UpdateModuleConfigBody` struct.
#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateModuleConfigBody {
    /// The `config` field.
    pub config: serde_json::Value,
}

/// The `EnableModuleBody` struct.
#[derive(Debug, Serialize, Deserialize)]
pub struct EnableModuleBody {
    /// The `is_enabled` field.
    pub is_enabled: bool,
}

/// The `CheckSpamBody` struct.
#[derive(Debug, Serialize, Deserialize)]
pub struct CheckSpamBody {
    /// The `event_id` field.
    pub event_id: String,
    /// The `room_id` field.
    pub room_id: String,
    /// The `sender` field.
    pub sender: String,
    /// The `event_type` field.
    pub event_type: String,
    /// The `content` field.
    pub content: serde_json::Value,
}

/// The `CheckThirdPartyRuleBody` struct.
#[derive(Debug, Serialize, Deserialize)]
pub struct CheckThirdPartyRuleBody {
    /// The `event_id` field.
    pub event_id: String,
    /// The `room_id` field.
    pub room_id: String,
    /// The `sender` field.
    pub sender: String,
    /// The `event_type` field.
    pub event_type: String,
    /// The `content` field.
    pub content: serde_json::Value,
    /// The `state_events` field.
    pub state_events: Vec<serde_json::Value>,
}

/// The `CreateAccountValidityBody` struct.
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateAccountValidityBody {
    /// The `user_id` field.
    pub user_id: String,
    /// The `expiration_ts` field.
    pub expiration_ts: i64,
    /// The `is_valid` field.
    pub is_valid: Option<bool>,
}

/// The `RenewAccountBody` struct.
#[derive(Debug, Serialize, Deserialize)]
pub struct RenewAccountBody {
    /// The `renewal_token` field.
    pub renewal_token: String,
    /// The `new_expiration_ts` field.
    pub new_expiration_ts: i64,
}

/// The `CreatePasswordAuthProviderBody` struct.
#[derive(Debug, Serialize, Deserialize)]
pub struct CreatePasswordAuthProviderBody {
    /// The `provider_name` field.
    pub provider_name: String,
    /// The `provider_type` field.
    pub provider_type: String,
    /// The `config` field.
    pub config: serde_json::Value,
    /// The `is_enabled` field.
    pub is_enabled: Option<bool>,
    /// The `priority` field.
    pub priority: Option<i32>,
}

/// The `CreateMediaCallbackBody` struct.
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateMediaCallbackBody {
    /// The `callback_name` field.
    pub callback_name: String,
    /// The `callback_type` field.
    pub callback_type: String,
    /// The `url` field.
    pub url: String,
    /// The `method` field.
    pub method: Option<String>,
    /// The `headers` field.
    pub headers: Option<serde_json::Value>,
    /// The `is_enabled` field.
    pub is_enabled: Option<bool>,
    /// The `timeout_ms` field.
    pub timeout_ms: Option<i32>,
    /// The `retry_count` field.
    pub retry_count: Option<i32>,
}

/// The `CreateAccountDataCallbackBody` struct.
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateAccountDataCallbackBody {
    /// The `callback_name` field.
    pub callback_name: String,
    /// The `config` field.
    pub config: serde_json::Value,
    /// The `is_enabled` field.
    pub is_enabled: Option<bool>,
    /// The `data_types` field.
    pub data_types: Option<Vec<String>>,
}

/// The `ModuleResponse` struct.
#[derive(Debug, Serialize, Deserialize)]
pub struct ModuleResponse {
    /// The `id` field.
    pub id: i64,
    /// The `module_name` field.
    pub module_name: String,
    /// The `module_type` field.
    pub module_type: String,
    /// The `version` field.
    pub version: String,
    /// The `description` field.
    pub description: Option<String>,
    /// The `is_enabled` field.
    pub is_enabled: bool,
    /// The `priority` field.
    pub priority: i32,
    /// The `config` field.
    pub config: Option<serde_json::Value>,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `updated_ts` field.
    pub updated_ts: i64,
    /// The `last_executed_ts` field.
    pub last_executed_ts: Option<i64>,
    /// The `execution_count` field.
    pub execution_count: i32,
    /// The `error_count` field.
    pub error_count: i32,
    /// The `last_error` field.
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
            is_enabled: m.is_enabled,
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

/// The `SpamCheckResultResponse` struct.
#[derive(Debug, Serialize, Deserialize)]
pub struct SpamCheckResultResponse {
    /// The `id` field.
    pub id: i64,
    /// The `event_id` field.
    pub event_id: String,
    /// The `room_id` field.
    pub room_id: String,
    /// The `sender` field.
    pub sender: String,
    /// The `event_type` field.
    pub event_type: String,
    /// The `content` field.
    pub content: Option<serde_json::Value>,
    /// The `result` field.
    pub result: String,
    /// The `score` field.
    pub score: i32,
    /// The `reason` field.
    pub reason: Option<String>,
    /// The `checker_module` field.
    pub checker_module: String,
    /// The `checked_ts` field.
    pub checked_ts: i64,
    /// The `action_taken` field.
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

/// The `ThirdPartyRuleResultResponse` struct.
#[derive(Debug, Serialize, Deserialize)]
pub struct ThirdPartyRuleResultResponse {
    /// The `id` field.
    pub id: i64,
    /// The `event_id` field.
    pub event_id: String,
    /// The `room_id` field.
    pub room_id: String,
    /// The `sender` field.
    pub sender: String,
    /// The `event_type` field.
    pub event_type: String,
    /// The `rule_name` field.
    pub rule_name: String,
    #[serde(rename = "allowed")]
    /// The `is_allowed` field.
    pub is_allowed: bool,
    /// The `reason` field.
    pub reason: Option<String>,
    /// The `modified_content` field.
    pub modified_content: Option<serde_json::Value>,
    /// The `checked_ts` field.
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
            is_allowed: r.is_allowed,
            reason: r.reason,
            modified_content: r.modified_content,
            checked_ts: r.checked_ts,
        }
    }
}

/// The `AccountValidityResponse` struct.
#[derive(Debug, Serialize, Deserialize)]
pub struct AccountValidityResponse {
    /// The `user_id` field.
    pub user_id: String,
    /// The `expiration_ts` field.
    pub expiration_ts: Option<i64>,
    /// The `last_check_at` field.
    pub last_check_at: Option<i64>,
    /// The `renewal_token` field.
    pub renewal_token: Option<String>,
    /// The `is_valid` field.
    pub is_valid: bool,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `updated_ts` field.
    pub updated_ts: i64,
}

async fn ensure_user_exists(ctx: &AdminContext, user_id: &str) -> Result<(), ApiError> {
    let user = ctx.account_identity_service.get_user_by_identifier(user_id).await?;

    if user.is_none() {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    Ok(())
}

impl From<AccountValidity> for AccountValidityResponse {
    fn from(v: AccountValidity) -> Self {
        Self {
            user_id: v.user_id,
            expiration_ts: v.expiration_at,
            last_check_at: v.last_check_at,
            renewal_token: v.renewal_token,
            is_valid: v.is_valid,
            created_ts: v.created_ts,
            updated_ts: v.updated_ts,
        }
    }
}

/// The `PasswordAuthProviderResponse` struct.
#[derive(Debug, Serialize, Deserialize)]
pub struct PasswordAuthProviderResponse {
    /// The `id` field.
    pub id: i64,
    /// The `provider_name` field.
    pub provider_name: String,
    /// The `provider_type` field.
    pub provider_type: String,
    /// The `config` field.
    pub config: Option<serde_json::Value>,
    /// The `is_enabled` field.
    pub is_enabled: bool,
    /// The `priority` field.
    pub priority: i32,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `updated_ts` field.
    pub updated_ts: i64,
}

impl From<PasswordAuthProvider> for PasswordAuthProviderResponse {
    fn from(p: PasswordAuthProvider) -> Self {
        Self {
            id: p.id,
            provider_name: p.provider_name,
            provider_type: p.provider_type,
            config: p.config,
            is_enabled: p.is_enabled,
            priority: p.priority,
            created_ts: p.created_ts,
            updated_ts: p.updated_ts,
        }
    }
}

/// The `MediaCallbackResponse` struct.
#[derive(Debug, Serialize, Deserialize)]
pub struct MediaCallbackResponse {
    /// The `id` field.
    pub id: i64,
    /// The `callback_type` field.
    pub callback_type: String,
    /// The `media_id` field.
    pub media_id: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `status` field.
    pub status: String,
    /// The `result` field.
    pub result: Option<serde_json::Value>,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `completed_ts` field.
    pub completed_ts: Option<i64>,
    /// The `is_enabled` field.
    pub is_enabled: bool,
}

impl From<MediaCallback> for MediaCallbackResponse {
    fn from(c: MediaCallback) -> Self {
        Self {
            id: c.id,
            callback_type: c.callback_type,
            media_id: c.media_id,
            user_id: c.user_id,
            status: c.status,
            result: c.result,
            created_ts: c.created_ts,
            completed_ts: c.completed_ts,
            is_enabled: c.is_enabled,
        }
    }
}

/// The `AccountDataCallbackResponse` struct.
#[derive(Debug, Serialize, Deserialize)]
pub struct AccountDataCallbackResponse {
    /// The `id` field.
    pub id: i64,
    /// The `callback_name` field.
    pub callback_name: String,
    /// The `is_enabled` field.
    pub is_enabled: bool,
    /// The `data_types` field.
    pub data_types: Option<Vec<String>>,
    /// The `config` field.
    pub config: Option<serde_json::Value>,
    /// The `created_ts` field.
    pub created_ts: i64,
}

impl From<AccountDataCallback> for AccountDataCallbackResponse {
    fn from(c: AccountDataCallback) -> Self {
        Self {
            id: c.id,
            callback_name: c.callback_name,
            is_enabled: c.is_enabled,
            data_types: c.data_types,
            config: c.config,
            created_ts: c.created_ts,
        }
    }
}

/// The `ListQuery` struct.
#[derive(Debug, Deserialize)]
pub struct ListQuery {
    /// The `limit` field.
    pub limit: Option<i64>,
    /// The `from` field.
    pub from: Option<String>,
}

/// The `SpamCheckQuery` struct.
#[derive(Debug, Deserialize)]
pub struct SpamCheckQuery {
    /// The `limit` field.
    pub limit: Option<i64>,
}

/// See [`create_module`].
pub async fn create_module(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
    Json(body): Json<CreateModuleBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request = CreateModuleRequest {
        module_name: body.module_name,
        module_type: body.module_type,
        version: body.version,
        description: body.description,
        is_enabled: body.is_enabled,
        priority: body.priority,
        config: body.config,
    };

    let module = ctx.module_service.register_module(request).await?;

    Ok((StatusCode::CREATED, Json(ModuleResponse::from(module))))
}

/// See [`get_module`].
pub async fn get_module(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
    Path(module_name): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let module =
        ctx.module_service.get_module(&module_name).await?.ok_or_else(|| ApiError::not_found("Module not found"))?;

    Ok(Json(ModuleResponse::from(module)))
}

/// See [`get_modules_by_type`].
pub async fn get_modules_by_type(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
    Path(module_type): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let modules = ctx.module_service.get_modules_by_type(&module_type).await?;

    let responses: Vec<ModuleResponse> = modules.into_iter().map(ModuleResponse::from).collect();

    Ok(Json(responses))
}

/// See [`get_all_modules`].
pub async fn get_all_modules(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
    Query(query): Query<ListQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = query.limit.unwrap_or(100).clamp(1, 500);

    let (modules, next_batch) = ctx.module_service.get_all_modules(limit, query.from).await?;

    let responses: Vec<ModuleResponse> = modules.into_iter().map(ModuleResponse::from).collect();

    Ok(Json(serde_json::json!({
        "modules": responses,
        "next_batch": next_batch
    })))
}

/// See [`update_module_config`].
pub async fn update_module_config(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
    Path(module_name): Path<String>,
    Json(body): Json<UpdateModuleConfigBody>,
) -> Result<impl IntoResponse, ApiError> {
    let module = ctx.module_service.update_module_config(&module_name, body.config).await?;

    Ok(Json(ModuleResponse::from(module)))
}

/// See [`enable_module`].
pub async fn enable_module(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
    Path(module_name): Path<String>,
    Json(body): Json<EnableModuleBody>,
) -> Result<impl IntoResponse, ApiError> {
    let module = ctx.module_service.enable_module(&module_name, body.is_enabled).await?;

    Ok(Json(ModuleResponse::from(module)))
}

/// See [`delete_module`].
pub async fn delete_module(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
    Path(module_name): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    ctx.module_service.delete_module(&module_name).await?;

    Ok(StatusCode::NO_CONTENT)
}

/// See [`check_spam`].
pub async fn check_spam(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
    Json(body): Json<CheckSpamBody>,
) -> Result<impl IntoResponse, ApiError> {
    let context = SpamCheckContext {
        event_id: body.event_id,
        room_id: body.room_id,
        sender: body.sender,
        event_type: body.event_type,
        content: body.content,
    };

    let result = ctx.module_service.check_spam(&context).await?;

    Ok(Json(result))
}

/// See [`check_third_party_rule`].
pub async fn check_third_party_rule(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
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

    let result = ctx.module_service.check_third_party_rules(&context).await?;

    Ok(Json(result))
}

/// See [`get_spam_check_result`].
pub async fn get_spam_check_result(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
    Path(event_id): Path<EventId>,
) -> Result<impl IntoResponse, ApiError> {
    let result = ctx
        .module_service
        .get_spam_check_result(&event_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Spam check result not found"))?;

    Ok(Json(SpamCheckResultResponse::from(result)))
}

/// See [`get_spam_check_results_by_sender`].
pub async fn get_spam_check_results_by_sender(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
    Path(sender): Path<String>,
    Query(query): Query<SpamCheckQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = query.limit.unwrap_or(100);

    let results = ctx.module_service.get_spam_check_results_by_sender(&sender, limit).await?;

    let responses: Vec<SpamCheckResultResponse> = results.into_iter().map(SpamCheckResultResponse::from).collect();

    Ok(Json(responses))
}

/// See [`get_third_party_rule_results`].
pub async fn get_third_party_rule_results(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
    Path(event_id): Path<EventId>,
) -> Result<impl IntoResponse, ApiError> {
    let results = ctx.module_service.get_third_party_rule_results(&event_id).await?;

    let responses: Vec<ThirdPartyRuleResultResponse> =
        results.into_iter().map(ThirdPartyRuleResultResponse::from).collect();

    Ok(Json(responses))
}

/// See [`get_execution_logs`].
pub async fn get_execution_logs(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
    Path(module_name): Path<String>,
    Query(query): Query<SpamCheckQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = query.limit.unwrap_or(100);

    let logs = ctx.module_service.get_execution_logs(&module_name, limit).await?;

    Ok(Json(logs))
}

/// See [`create_account_validity`].
pub async fn create_account_validity(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
    Json(body): Json<CreateAccountValidityBody>,
) -> Result<impl IntoResponse, ApiError> {
    ensure_user_exists(&ctx, &body.user_id).await?;

    let request = CreateAccountValidityRequest {
        user_id: body.user_id,
        expiration_at: body.expiration_ts,
        is_valid: body.is_valid,
    };

    let validity = ctx.account_validity_service.create_validity(request).await?;

    Ok((StatusCode::CREATED, Json(AccountValidityResponse::from(validity))))
}

/// See [`get_account_validity`].
pub async fn get_account_validity(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
    Path(user_id): Path<UserId>,
) -> Result<impl IntoResponse, ApiError> {
    let validity = ctx
        .account_validity_service
        .get_validity(&user_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Account validity not found"))?;

    Ok(Json(AccountValidityResponse::from(validity)))
}

/// See [`renew_account`].
pub async fn renew_account(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
    Path(user_id): Path<UserId>,
    Json(body): Json<RenewAccountBody>,
) -> Result<impl IntoResponse, ApiError> {
    ensure_user_exists(&ctx, &user_id).await?;

    if ctx.account_validity_service.get_validity(&user_id).await?.is_none() {
        return Err(ApiError::not_found("Account validity not found"));
    }

    let validity =
        ctx.account_validity_service.renew_account(&user_id, &body.renewal_token, body.new_expiration_ts).await?;

    Ok(Json(AccountValidityResponse::from(validity)))
}

/// See [`create_password_auth_provider`].
pub async fn create_password_auth_provider(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
    Json(body): Json<CreatePasswordAuthProviderBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request = CreatePasswordAuthProviderRequest {
        provider_name: body.provider_name,
        provider_type: body.provider_type,
        config: body.config,
        is_enabled: body.is_enabled,
        priority: body.priority,
    };

    let provider = ctx
        .module_storage
        .create_password_auth_provider(request)
        .await
        .map_err(|e| ApiError::internal_with_context("Failed to create password auth provider", &e))?;

    Ok((StatusCode::CREATED, Json(PasswordAuthProviderResponse::from(provider))))
}

/// See [`get_password_auth_providers`].
pub async fn get_password_auth_providers(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let providers = ctx
        .module_storage
        .get_password_auth_providers()
        .await
        .map_err(|e| ApiError::internal_with_context("Failed to get password auth providers", &e))?;

    let responses: Vec<PasswordAuthProviderResponse> =
        providers.into_iter().map(PasswordAuthProviderResponse::from).collect();

    Ok(Json(responses))
}

/// See [`create_media_callback`].
pub async fn create_media_callback(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
    Json(body): Json<CreateMediaCallbackBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request = CreateMediaCallbackRequest {
        callback_name: body.callback_name,
        callback_type: body.callback_type,
        url: body.url,
        method: body.method,
        headers: body.headers,
        is_enabled: body.is_enabled,
        timeout_ms: body.timeout_ms,
        retry_count: body.retry_count,
    };

    let callback = ctx
        .module_storage
        .create_media_callback(request)
        .await
        .map_err(|e| ApiError::internal_with_context("Failed to create media callback", &e))?;

    Ok((StatusCode::CREATED, Json(MediaCallbackResponse::from(callback))))
}

/// See [`get_media_callbacks`].
pub async fn get_media_callbacks(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
    Path(callback_type): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let callbacks = ctx
        .module_storage
        .get_media_callbacks(Some(&callback_type))
        .await
        .map_err(|e| ApiError::internal_with_context("Failed to get media callbacks", &e))?;

    let responses: Vec<MediaCallbackResponse> = callbacks.into_iter().map(MediaCallbackResponse::from).collect();

    Ok(Json(responses))
}

/// See [`get_all_media_callbacks`].
pub async fn get_all_media_callbacks(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let callbacks = ctx
        .module_storage
        .get_media_callbacks(None)
        .await
        .map_err(|e| ApiError::internal_with_context("Failed to get media callbacks", &e))?;

    let responses: Vec<MediaCallbackResponse> = callbacks.into_iter().map(MediaCallbackResponse::from).collect();

    Ok(Json(responses))
}

/// See [`create_account_data_callback`].
pub async fn create_account_data_callback(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
    Json(body): Json<CreateAccountDataCallbackBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request = CreateAccountDataCallbackRequest {
        callback_name: body.callback_name,
        config: body.config,
        is_enabled: body.is_enabled,
        data_types: body.data_types,
    };

    let callback = ctx
        .module_storage
        .create_account_data_callback(request)
        .await
        .map_err(|e| ApiError::internal_with_context("Failed to create account data callback", &e))?;

    Ok((StatusCode::CREATED, Json(AccountDataCallbackResponse::from(callback))))
}

/// See [`get_account_data_callbacks`].
pub async fn get_account_data_callbacks(
    State(ctx): State<AdminContext>,
    _auth_user: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let callbacks = ctx
        .module_storage
        .get_account_data_callbacks()
        .await
        .map_err(|e| ApiError::internal_with_context("Failed to get account data callbacks", &e))?;

    let responses: Vec<AccountDataCallbackResponse> =
        callbacks.into_iter().map(AccountDataCallbackResponse::from).collect();

    Ok(Json(responses))
}

/// See [`create_module_router`].
pub fn create_module_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/_synapse/admin/v1/modules", post(create_module))
        .route("/_synapse/admin/v1/modules", get(get_all_modules))
        .route("/_synapse/admin/v1/modules/type/{module_type}", get(get_modules_by_type))
        .route("/_synapse/admin/v1/modules/{module_name}", get(get_module))
        .route("/_synapse/admin/v1/modules/{module_name}/config", put(update_module_config))
        .route("/_synapse/admin/v1/modules/{module_name}/enable", post(enable_module))
        .route("/_synapse/admin/v1/modules/{module_name}", delete(delete_module))
        .route("/_synapse/admin/v1/modules/check_spam", post(check_spam))
        .route("/_synapse/admin/v1/modules/check_third_party_rule", post(check_third_party_rule))
        .route("/_synapse/admin/v1/modules/spam_check/{event_id}", get(get_spam_check_result))
        .route("/_synapse/admin/v1/modules/spam_check/sender/{sender}", get(get_spam_check_results_by_sender))
        .route("/_synapse/admin/v1/modules/third_party_rule/{event_id}", get(get_third_party_rule_results))
        .route("/_synapse/admin/v1/modules/logs/{module_name}", get(get_execution_logs))
        .route("/_synapse/admin/v1/account_validity", post(create_account_validity))
        .route("/_synapse/admin/v1/account_validity/{user_id}", get(get_account_validity))
        .route("/_synapse/admin/v1/account_validity/{user_id}/renew", post(renew_account))
        .route("/_synapse/admin/v1/password_auth_providers", post(create_password_auth_provider))
        .route("/_synapse/admin/v1/password_auth_providers", get(get_password_auth_providers))
        .route("/_synapse/admin/v1/media_callbacks", post(create_media_callback))
        .route("/_synapse/admin/v1/media_callbacks", get(get_all_media_callbacks))
        .route("/_synapse/admin/v1/media_callbacks/{callback_type}", get(get_media_callbacks))
        .route("/_synapse/admin/v1/account_data_callbacks", post(create_account_data_callback))
        .route("/_synapse/admin/v1/account_data_callbacks", get(get_account_data_callbacks))
        .route_layer(axum::middleware::from_fn_with_state(<crate::web::routes::context::AdminContext as axum::extract::FromRef<crate::web::routes::AppState>>::from_ref(&state), crate::web::middleware::admin_auth_middleware))
        .with_state(state)
}

/// See [`module_route_manifest`].
pub fn module_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::RouteEntry;
    use axum::http::Method;
    [
        (Method::POST, "/_synapse/admin/v1/modules"),
        (Method::GET, "/_synapse/admin/v1/modules"),
        (Method::GET, "/_synapse/admin/v1/modules/type/{module_type}"),
        (Method::GET, "/_synapse/admin/v1/modules/{module_name}"),
        (Method::PUT, "/_synapse/admin/v1/modules/{module_name}/config"),
        (Method::POST, "/_synapse/admin/v1/modules/{module_name}/enable"),
        (Method::DELETE, "/_synapse/admin/v1/modules/{module_name}"),
        (Method::POST, "/_synapse/admin/v1/modules/check_spam"),
        (Method::POST, "/_synapse/admin/v1/modules/check_third_party_rule"),
        (Method::GET, "/_synapse/admin/v1/modules/spam_check/{event_id}"),
        (Method::GET, "/_synapse/admin/v1/modules/spam_check/sender/{sender}"),
        (Method::GET, "/_synapse/admin/v1/modules/third_party_rule/{event_id}"),
        (Method::GET, "/_synapse/admin/v1/modules/logs/{module_name}"),
        (Method::POST, "/_synapse/admin/v1/account_validity"),
        (Method::GET, "/_synapse/admin/v1/account_validity/{user_id}"),
        (Method::POST, "/_synapse/admin/v1/account_validity/{user_id}/renew"),
        (Method::POST, "/_synapse/admin/v1/password_auth_providers"),
        (Method::GET, "/_synapse/admin/v1/password_auth_providers"),
        (Method::POST, "/_synapse/admin/v1/media_callbacks"),
        (Method::GET, "/_synapse/admin/v1/media_callbacks"),
        (Method::GET, "/_synapse/admin/v1/media_callbacks/{callback_type}"),
        (Method::POST, "/_synapse/admin/v1/account_data_callbacks"),
        (Method::GET, "/_synapse/admin/v1/account_data_callbacks"),
    ]
    .into_iter()
    .map(|(m, p)| RouteEntry::new(m, p, "module"))
    .collect()
}
