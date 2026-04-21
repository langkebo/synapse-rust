// OIDC (OpenID Connect) 路由
// Matrix Spec: https://matrix.org/docs/spec/openid.html

use crate::common::error::ApiError;
use crate::services::builtin_oidc_provider::{
    AuthorizeRequest, OidcTokenRequest as BuiltinOidcTokenRequest,
};
use crate::services::oidc_service::OidcService;
use crate::web::routes::{AppState, AuthenticatedUser};
use axum::{
    extract::{Query, State},
    response::Redirect,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use validator::Validate;

const OIDC_AUTH_SESSION_TTL_SECONDS: u64 = 600;

#[derive(Debug, Clone)]
struct OidcAuthSession {
    nonce: String,
    code_verifier: String,
    code_challenge: String,
    code_challenge_method: String,
    redirect_uri: String,
    expires_at: u64,
}

static OIDC_AUTH_SESSIONS: OnceLock<Mutex<HashMap<String, OidcAuthSession>>> = OnceLock::new();

fn oidc_auth_sessions() -> &'static Mutex<HashMap<String, OidcAuthSession>> {
    OIDC_AUTH_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn current_unix_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn cleanup_expired_oidc_sessions(sessions: &mut HashMap<String, OidcAuthSession>, now: u64) {
    sessions.retain(|_, session| session.expires_at >= now);
}

fn store_oidc_auth_session(
    state: &str,
    nonce: &str,
    code_verifier: &str,
    code_challenge: &str,
    code_challenge_method: &str,
    redirect_uri: &str,
) -> Result<(), ApiError> {
    let now = current_unix_ts();
    let mut sessions = oidc_auth_sessions()
        .lock()
        .map_err(|_| ApiError::internal("Failed to acquire OIDC auth session lock".to_string()))?;
    cleanup_expired_oidc_sessions(&mut sessions, now);
    sessions.insert(
        state.to_string(),
        OidcAuthSession {
            nonce: nonce.to_string(),
            code_verifier: code_verifier.to_string(),
            code_challenge: code_challenge.to_string(),
            code_challenge_method: code_challenge_method.to_string(),
            redirect_uri: redirect_uri.to_string(),
            expires_at: now + OIDC_AUTH_SESSION_TTL_SECONDS,
        },
    );
    Ok(())
}

fn consume_oidc_auth_session(state: &str) -> Result<OidcAuthSession, ApiError> {
    let now = current_unix_ts();
    let mut sessions = oidc_auth_sessions()
        .lock()
        .map_err(|_| ApiError::internal("Failed to acquire OIDC auth session lock".to_string()))?;
    cleanup_expired_oidc_sessions(&mut sessions, now);
    let session = sessions.remove(state).ok_or_else(|| {
        ApiError::unauthorized("OIDC state is missing, expired, or already used".to_string())
    })?;
    if session.expires_at < now {
        return Err(ApiError::unauthorized(
            "OIDC authorization session expired".to_string(),
        ));
    }
    Ok(session)
}

fn validate_state_pkce_binding(auth_session: &OidcAuthSession) -> Result<(), ApiError> {
    if auth_session.code_challenge_method != "S256" {
        return Err(ApiError::unauthorized(
            "Unsupported OIDC PKCE challenge method".to_string(),
        ));
    }
    if auth_session.code_verifier.len() < 43 || auth_session.code_verifier.len() > 128 {
        return Err(ApiError::unauthorized(
            "Invalid OIDC PKCE verifier length".to_string(),
        ));
    }
    if !OidcService::verify_pkce(&auth_session.code_verifier, &auth_session.code_challenge) {
        return Err(ApiError::unauthorized(
            "OIDC state/PKCE binding validation failed".to_string(),
        ));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct SsoRedirectQuery {
    #[serde(rename = "redirectUrl")]
    redirect_url: Option<String>,
    #[serde(rename = "redirect_url")]
    redirect_url_compat: Option<String>,
}

fn resolve_sso_redirect_url(state: &AppState, query: &SsoRedirectQuery) -> String {
    query
        .redirect_url
        .clone()
        .or_else(|| query.redirect_url_compat.clone())
        .unwrap_or_else(|| {
            format!(
                "{}/_matrix/client/v3/oidc/callback",
                state.services.config.server.get_public_baseurl()
            )
        })
}

async fn sso_redirect(
    State(state): State<AppState>,
    Query(query): Query<SsoRedirectQuery>,
) -> Result<Redirect, ApiError> {
    let redirect_uri = resolve_sso_redirect_url(&state, &query);

    if let Some(oidc_service) = state.services.oidc_service.as_ref() {
        let state_value = OidcService::generate_state();
        let nonce_value = OidcService::generate_state();
        let (code_verifier, code_challenge) = OidcService::generate_pkce();

        store_oidc_auth_session(
            &state_value,
            &nonce_value,
            &code_verifier,
            &code_challenge,
            "S256",
            &redirect_uri,
        )?;

        let authorization_url = oidc_service.get_authorization_url(
            &state_value,
            &redirect_uri,
            Some(&code_challenge),
            Some("S256"),
        )?;

        return Ok(Redirect::temporary(&authorization_url));
    }

    #[cfg(feature = "saml-sso")]
    if state.services.saml_service.is_enabled() {
        let auth_request = state
            .services
            .saml_service
            .get_auth_redirect(Some(&redirect_uri))
            .await?;
        return Ok(Redirect::temporary(&auth_request.redirect_url));
    }

    Err(ApiError::bad_request("SSO is not enabled".to_string()))
}

pub fn create_oidc_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/_matrix/client/v3/login/sso/redirect", get(sso_redirect))
        .route("/_matrix/client/r0/login/sso/redirect", get(sso_redirect))
        .route("/_matrix/client/v3/login/sso/userinfo", get(oidc_userinfo))
        .route("/_matrix/client/r0/login/sso/userinfo", get(oidc_userinfo))
        // v3 路径
        .route("/_matrix/client/v3/oidc/userinfo", get(oidc_userinfo))
        .route("/_matrix/client/v3/oidc/token", post(oidc_token))
        .route("/_matrix/client/v3/oidc/logout", post(oidc_logout))
        .route("/_matrix/client/v3/oidc/authorize", get(oidc_authorize))
        .route("/_matrix/client/v3/oidc/register", post(oidc_register))
        .route("/_matrix/client/v3/oidc/callback", get(oidc_callback))
        // r0 路径兼容
        .route("/_matrix/client/r0/oidc/userinfo", get(oidc_userinfo))
        .route("/_matrix/client/r0/oidc/token", post(oidc_token))
        .route("/_matrix/client/r0/oidc/logout", post(oidc_logout))
        .route("/_matrix/client/r0/oidc/authorize", get(oidc_authorize))
        .route("/_matrix/client/r0/oidc/register", post(oidc_register))
        .route("/_matrix/client/r0/oidc/callback", get(oidc_callback))
        // 内置 OIDC Provider 端点
        .route("/_matrix/client/v3/oidc/login", post(builtin_oidc_login))
        .route("/.well-known/openid-configuration", get(openid_discovery))
        .route("/.well-known/jwks.json", get(jwks))
        .with_state(state)
}

async fn builtin_oidc_login(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let provider = state
        .services
        .builtin_oidc_provider
        .as_ref()
        .ok_or_else(|| ApiError::bad_request("Builtin OIDC provider is not enabled".to_string()))?;

    let client_id = body
        .get("client_id")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let redirect_uri = body
        .get("redirect_uri")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let scope = body
        .get("scope")
        .and_then(|v| v.as_str())
        .unwrap_or("openid");
    let state_str = body
        .get("state")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let nonce = body.get("nonce").and_then(|v| v.as_str());
    let code_verifier = body.get("code_verifier").and_then(|v| v.as_str());
    let username = body
        .get("username")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let password = body
        .get("password")
        .and_then(|v| v.as_str())
        .unwrap_or_default();

    let code = provider
        .authorize(AuthorizeRequest {
            client_id: client_id.to_string(),
            redirect_uri: redirect_uri.to_string(),
            scope: scope.to_string(),
            state: state_str.to_string(),
            nonce: nonce.map(String::from),
            code_verifier: code_verifier.map(String::from),
            username: username.to_string(),
            password: password.to_string(),
        })
        .map_err(|e| ApiError::unauthorized(format!("Authorization failed: {}", e)))?;

    Ok(Json(serde_json::json!({ "code": code })))
}

async fn jwks(State(state): State<AppState>) -> Result<Json<serde_json::Value>, ApiError> {
    if let Some(provider) = &state.services.builtin_oidc_provider {
        let jwks = provider.get_jwks();
        return Ok(Json(serde_json::to_value(jwks).map_err(|e| {
            ApiError::internal(format!("Failed to serialize JWKS: {}", e))
        })?));
    }
    Err(ApiError::bad_request(
        "Builtin OIDC provider is not enabled".to_string(),
    ))
}

/// OIDC UserInfo Response
#[derive(Debug, Serialize)]
pub struct OidcUserInfoResponse {
    pub sub: String,
    pub name: Option<String>,
    pub picture: Option<String>,
    pub email: Option<String>,
}

/// 获取 OpenID Connect 用户信息
async fn oidc_userinfo(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<OidcUserInfoResponse>, ApiError> {
    let user_id = &auth_user.user_id;

    // 尝试从 OIDC 服务获取用户信息（如果有 OIDC 访问令牌）
    // 注意：这里需要从认证中获取 OIDC access token
    // 目前先使用 registration_service 获取本地 profile

    // 获取用户 profile 信息
    let profile = state
        .services
        .registration_service
        .get_profile(user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get profile: {}", e)))?;

    let name = profile
        .get("displayname")
        .and_then(|v| v.as_str())
        .map(String::from);

    let picture = profile.get("avatar_url").and_then(|v| v.as_str()).map(|s| {
        if s.starts_with("mxc://") {
            s.to_string()
        } else {
            format!("mxc://{}", s)
        }
    });

    let email = profile
        .get("email")
        .and_then(|v| v.as_str())
        .map(String::from);

    Ok(Json(OidcUserInfoResponse {
        sub: user_id.clone(),
        name,
        picture,
        email,
    }))
}

/// OIDC Token Request
#[derive(Debug, Deserialize, Validate)]
pub struct OidcTokenRequest {
    #[validate(length(min = 1, max = 100))]
    pub grant_type: String,
    #[validate(length(min = 1, max = 2048))]
    pub code: Option<String>,
    #[validate(length(max = 2048))]
    pub redirect_uri: Option<String>,
    #[validate(length(max = 2048))]
    pub refresh_token: Option<String>,
    #[validate(length(max = 1024))]
    pub scope: Option<String>,
    #[validate(length(max = 255))]
    pub client_id: Option<String>,
    #[validate(length(min = 43, max = 128))]
    pub code_verifier: Option<String>,
}

/// OIDC Token Response
#[derive(Debug, Serialize)]
pub struct OidcTokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub refresh_token: Option<String>,
    pub scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matrix_user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
}

/// OIDC Token 端点
/// 处理授权码兑换和刷新令牌
async fn oidc_token(
    State(state): State<AppState>,
    Json(body): Json<OidcTokenRequest>,
) -> Result<Json<OidcTokenResponse>, ApiError> {
    // Validate input
    body.validate()
        .map_err(|e| ApiError::bad_request(format!("Validation error: {}", e)))?;

    // 优先使用内置 OIDC Provider
    if let Some(builtin_provider) = &state.services.builtin_oidc_provider {
        let request = BuiltinOidcTokenRequest {
            grant_type: body.grant_type.clone(),
            code: body.code.clone(),
            redirect_uri: body.redirect_uri.clone(),
            client_id: body.client_id.clone(),
            code_verifier: body.code_verifier.clone(),
            refresh_token: body.refresh_token.clone(),
            scope: body.scope.clone(),
        };

        let token_response = builtin_provider
            .token(request)
            .map_err(|e| ApiError::internal(format!("Builtin OIDC token failed: {}", e)))?;

        return Ok(Json(OidcTokenResponse {
            access_token: token_response.access_token,
            token_type: token_response.token_type,
            expires_in: token_response.expires_in,
            refresh_token: token_response.refresh_token,
            scope: token_response.scope.unwrap_or_default(),
            matrix_user_id: None,
            device_id: None,
        }));
    }

    // 检查外部 OIDC 服务是否启用
    let oidc_service = state
        .services
        .oidc_service
        .as_ref()
        .ok_or_else(|| ApiError::bad_request("OIDC is not enabled".to_string()))?;

    let OidcTokenRequest {
        grant_type,
        code,
        redirect_uri,
        refresh_token,
        scope,
        code_verifier,
        ..
    } = body;

    match grant_type.as_str() {
        "authorization_code" => {
            // 授权码模式
            let code =
                code.ok_or_else(|| ApiError::bad_request("Missing 'code' parameter".to_string()))?;
            let redirect_uri = redirect_uri.unwrap_or_default();

            // 使用 OIDC 服务兑换令牌
            let token_response = oidc_service
                .exchange_code(&code, &redirect_uri, code_verifier.as_deref())
                .await
                .map_err(|e| ApiError::internal(format!("Token exchange failed: {}", e)))?;

            // 获取用户信息
            let user_info = oidc_service
                .get_user_info(&token_response.access_token)
                .await
                .map_err(|e| ApiError::internal(format!("Failed to get user info: {}", e)))?;

            // 映射到 Matrix 用户
            let oidc_user = oidc_service.map_user(&user_info);

            let localpart = oidc_user.localpart.clone();
            let server_name = state
                .services
                .config
                .server
                .server_name
                .clone()
                .unwrap_or_else(|| "localhost".to_string());
            let matrix_user_id = format!("@{}:{}", localpart, server_name);
            let displayname = oidc_user.displayname.clone().unwrap_or(localpart.clone());

            // 检查用户是否存在，如果不存在则注册
            if state
                .services
                .user_storage
                .get_user_by_id(&matrix_user_id)
                .await
                .unwrap_or(None)
                .is_none()
            {
                tracing::info!("Creating new Matrix user from OIDC: {}", matrix_user_id);

                // 为了注册，我们需要创建一个随机的、长且安全的占位密码
                let random_password = uuid::Uuid::new_v4().to_string();

                state
                    .services
                    .registration_service
                    .register_user(&localpart, &random_password, Some(&displayname))
                    .await
                    .map_err(|e| {
                        ApiError::internal(format!("Failed to register OIDC user: {}", e))
                    })?;
            }

            // 生成设备ID
            let device_id = format!(
                "OIDC{}",
                &uuid::Uuid::new_v4().to_string().replace("-", "")[..8]
            );

            // 注册设备
            state
                .services
                .device_storage
                .create_device(&device_id, &matrix_user_id, Some("OIDC Device"))
                .await
                .map_err(|e| ApiError::internal(format!("Failed to create device: {}", e)))?;

            // 生成 Matrix Access Token
            let is_admin = state
                .services
                .user_storage
                .get_user_by_username(&localpart)
                .await
                .map_err(|e| ApiError::internal(format!("Failed to get user: {}", e)))?
                .map(|u| u.is_admin)
                .unwrap_or(false);

            let matrix_token = state
                .services
                .auth_service
                .generate_access_token(&matrix_user_id, &device_id, is_admin)
                .await?;

            tracing::info!(
                "OIDC token exchange successful for sub: {}, mapped to Matrix user: {}, device_id: {}",
                oidc_user.subject,
                matrix_user_id,
                device_id
            );

            Ok(Json(OidcTokenResponse {
                access_token: matrix_token,
                token_type: "Bearer".to_string(),
                expires_in: 3600,
                refresh_token: token_response.refresh_token,
                scope: scope.unwrap_or_else(|| "openid profile email".to_string()),
                matrix_user_id: Some(matrix_user_id),
                device_id: Some(device_id),
            }))
        }
        "refresh_token" => {
            // 刷新令牌模式
            let refresh_token = refresh_token.ok_or_else(|| {
                ApiError::bad_request("Missing 'refresh_token' parameter".to_string())
            })?;

            // 使用 OIDC 服务刷新令牌
            let token_response = oidc_service
                .refresh_token(&refresh_token)
                .await
                .map_err(|e| ApiError::internal(format!("Token refresh failed: {}", e)))?;

            tracing::info!("OIDC token refresh successful");

            Ok(Json(OidcTokenResponse {
                access_token: token_response.access_token,
                token_type: token_response.token_type,
                expires_in: token_response.expires_in.unwrap_or(3600),
                refresh_token: token_response.refresh_token,
                scope: scope.unwrap_or_else(|| "openid profile email".to_string()),
                matrix_user_id: None,
                device_id: None,
            }))
        }
        _ => Err(ApiError::bad_request(format!(
            "Unsupported grant_type: {}. Supported: authorization_code, refresh_token",
            grant_type
        ))),
    }
}

/// OIDC Logout Request
#[derive(Debug, Deserialize)]
pub struct OidcLogoutRequest {
    pub refresh_token: Option<String>,
    pub device_id: Option<String>,
}

/// OIDC Authorize Request
#[derive(Debug, Deserialize, Validate)]
pub struct OidcAuthorizeRequest {
    #[validate(length(min = 1, max = 50))]
    pub response_type: String,
    #[validate(length(min = 1, max = 255))]
    pub client_id: String,
    #[validate(length(min = 1, max = 2048))]
    pub redirect_uri: String,
    #[validate(length(max = 1024))]
    pub scope: Option<String>,
    #[validate(length(max = 512))]
    pub state: Option<String>,
    #[validate(length(max = 512))]
    pub nonce: Option<String>,
}

/// OIDC Authorization handler
/// Note: This endpoint does NOT require authentication - it's the first step in OIDC login
async fn oidc_authorize(
    State(state): State<AppState>,
    query: axum::extract::Query<OidcAuthorizeRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // 检查 OIDC 服务是否启用
    let oidc_service = state
        .services
        .oidc_service
        .as_ref()
        .ok_or_else(|| ApiError::bad_request("OIDC is not enabled".to_string()))?;

    let OidcAuthorizeRequest {
        response_type,
        client_id: _,
        redirect_uri,
        scope: _,
        state: auth_state,
        nonce,
    } = query.0;

    // 验证 response_type
    if response_type != "code" {
        return Err(ApiError::bad_request(
            "Only 'code' response type is supported".to_string(),
        ));
    }

    // 生成 state 和 nonce
    let state_value = auth_state.unwrap_or_else(OidcService::generate_state);
    let nonce_value = nonce.unwrap_or_else(OidcService::generate_state);

    // 生成 PKCE code_verifier 和 code_challenge
    let (code_verifier, code_challenge) = OidcService::generate_pkce();
    store_oidc_auth_session(
        &state_value,
        &nonce_value,
        &code_verifier,
        &code_challenge,
        "S256",
        &redirect_uri,
    )?;

    // 生成授权 URL (包含 PKCE)
    let authorization_url = oidc_service
        .get_authorization_url(
            &state_value,
            &redirect_uri,
            Some(&code_challenge),
            Some("S256"),
        )
        .map_err(|e| ApiError::internal(format!("Failed to generate authorization URL: {}", e)))?;

    tracing::info!(
        "OIDC authorization redirect_uri: {}, using PKCE",
        redirect_uri
    );

    Ok(Json(serde_json::json!({
        "authorization_url": authorization_url,
        "state": state_value,
        "nonce": nonce_value,
        "code_verifier": code_verifier,  // 返回给客户端，后续验证需要
    })))
}

/// OIDC Registration Request
#[derive(Debug, Deserialize)]
pub struct OidcRegistrationRequest {
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub redirect_uris: Option<Vec<String>>,
    pub client_name: Option<String>,
    pub token_endpoint_auth_method: Option<String>,
}

/// OIDC Registration Response
#[derive(Debug, Serialize)]
pub struct OidcRegistrationResponse {
    pub client_id: String,
    pub client_secret: Option<String>,
    pub client_id_issued_at: i64,
    pub client_secret_expires_at: i64,
    pub redirect_uris: Vec<String>,
    pub grant_types: Vec<String>,
    pub token_endpoint_auth_method: String,
}

/// OIDC Registration handler
async fn oidc_register(
    State(_state): State<AppState>,
    Json(_body): Json<OidcRegistrationRequest>,
) -> Result<Json<OidcRegistrationResponse>, ApiError> {
    Err(ApiError::bad_request(
        "Dynamic client registration not supported. Please configure OIDC in server configuration."
            .to_string(),
    ))
}

/// OIDC 登出
async fn oidc_logout(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<OidcLogoutRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // 如果提供了设备 ID，则删除该设备
    if let Some(device_id) = body.device_id {
        state
            .services
            .device_storage
            .delete_device(&device_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to delete device: {}", e)))?;
    }

    // 如果提供了 refresh_token，则尝试撤销
    if let Some(refresh_token) = body.refresh_token {
        state
            .services
            .refresh_token_service
            .revoke_token(&refresh_token, "OIDC logout")
            .await?;
    }

    tracing::info!("OIDC logout for user: {}", auth_user.user_id);

    Ok(Json(serde_json::json!({
        "success": true
    })))
}

// ============================================================================
// .well-known OpenID Connect Discovery
// ============================================================================

/// OpenID Connect 发现文档
#[derive(Debug, Serialize)]
pub struct OpenIdDiscovery {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub userinfo_endpoint: String,
    pub jwks_uri: String,
    pub registration_endpoint: Option<String>,
    pub scopes_supported: Vec<String>,
    pub response_types_supported: Vec<String>,
    pub response_modes_supported: Vec<String>,
    pub grant_types_supported: Vec<String>,
    pub token_endpoint_auth_methods_supported: Vec<String>,
    pub claims_supported: Vec<String>,
    pub ui_locales_supported: Vec<String>,
}

/// OpenID Connect Server Discovery
pub async fn openid_discovery(
    State(state): State<AppState>,
) -> Result<Json<OpenIdDiscovery>, ApiError> {
    let server_name = &state.services.server_name;
    let issuer = format!("https://{}", server_name);

    Ok(Json(OpenIdDiscovery {
        issuer: issuer.clone(),
        authorization_endpoint: format!("{}/_matrix/client/v3/oidc/authorize", issuer),
        token_endpoint: format!("{}/_matrix/client/v3/oidc/token", issuer),
        userinfo_endpoint: format!("{}/_matrix/client/v3/oidc/userinfo", issuer),
        jwks_uri: format!("{}/_matrix/keys/v1", issuer),
        registration_endpoint: None,
        scopes_supported: vec![
            "openid".to_string(),
            "profile".to_string(),
            "email".to_string(),
        ],
        response_types_supported: vec!["code".to_string()],
        response_modes_supported: vec!["query".to_string(), "fragment".to_string()],
        grant_types_supported: vec![
            "authorization_code".to_string(),
            "refresh_token".to_string(),
        ],
        token_endpoint_auth_methods_supported: vec!["client_secret_basic".to_string()],
        claims_supported: vec![
            "sub".to_string(),
            "name".to_string(),
            "picture".to_string(),
            "email".to_string(),
        ],
        ui_locales_supported: vec!["en".to_string()],
    }))
}

/// OIDC Callback Request - 处理 OIDC 授权回调
#[derive(Debug, Deserialize)]
pub struct OidcCallbackRequest {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
    pub error_description: Option<String>,
}

/// OIDC Callback handler - 处理用户授权后从 OIDC 提供商返回的回调
async fn oidc_callback(
    State(state): State<AppState>,
    query: axum::extract::Query<OidcCallbackRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // 检查 OIDC 服务是否启用
    let oidc_service = state
        .services
        .oidc_service
        .as_ref()
        .ok_or_else(|| ApiError::bad_request("OIDC is not enabled".to_string()))?;

    let OidcCallbackRequest {
        code,
        state: callback_state,
        error,
        error_description,
    } = query.0;

    // 检查错误
    if let Some(err) = error {
        return Err(ApiError::bad_request(format!(
            "OIDC authorization failed: {} - {}",
            err,
            error_description.unwrap_or_default()
        )));
    }

    // 需要授权码
    let code = code.ok_or_else(|| {
        ApiError::bad_request("Missing 'code' parameter in OIDC callback".to_string())
    })?;
    let callback_state = callback_state.ok_or_else(|| {
        ApiError::bad_request("Missing 'state' parameter in OIDC callback".to_string())
    })?;
    let auth_session = consume_oidc_auth_session(&callback_state)?;
    validate_state_pkce_binding(&auth_session)?;

    // 获取配置的回调 URL
    let callback_url = if auth_session.redirect_uri.is_empty() {
        oidc_service
            .get_config()
            .callback_url
            .clone()
            .unwrap_or_else(|| {
                format!(
                    "https://{}/_matrix/client/v3/oidc/callback",
                    state.services.server_name
                )
            })
    } else {
        auth_session.redirect_uri.clone()
    };

    // 兑换令牌
    let token_response = oidc_service
        .exchange_code(
            &code,
            &callback_url,
            Some(auth_session.code_verifier.as_str()),
        )
        .await
        .map_err(|e| ApiError::internal(format!("Token exchange failed: {}", e)))?;

    // 获取用户信息
    let user_info = oidc_service
        .get_user_info(&token_response.access_token)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get user info: {}", e)))?;

    // 映射到 Matrix 用户
    let oidc_user = oidc_service.map_user(&user_info);

    tracing::info!(
        "OIDC callback successful for sub: {}, localpart: {}, email: {:?}, nonce_len: {}",
        oidc_user.subject,
        oidc_user.localpart,
        oidc_user.email,
        auth_session.nonce.len()
    );

    // 创建或登录 Matrix 用户
    // First check if user exists by localpart
    let user_id = format!("@{}:{}", oidc_user.localpart, state.services.server_name);

    let existing_user = state
        .services
        .user_storage
        .get_user_by_username(&oidc_user.localpart)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let (user, access_token, refresh_token, device_id) = if let Some(existing) = existing_user {
        // User exists, generate tokens for them using a simple token generation
        // Use the existing user's admin status
        let device_id = uuid::Uuid::new_v4().to_string()[..8].to_string();
        let access_token = state
            .services
            .auth_service
            .generate_access_token(&user_id, &device_id, existing.is_admin)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to generate access token: {}", e)))?;
        let refresh_token = state
            .services
            .auth_service
            .generate_refresh_token(&user_id, &device_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to generate refresh token: {}", e)))?;
        (existing, access_token, refresh_token, device_id)
    } else {
        // Create new user - use a random password since authentication is done by OIDC provider
        let random_password = OidcService::generate_state();

        // Get displayname from OIDC user info
        let displayname = oidc_user.displayname.as_deref();

        match state
            .services
            .auth_service
            .register(&oidc_user.localpart, &random_password, false, displayname)
            .await
        {
            Ok(result) => result,
            Err(e) => {
                // Check if user was created by another request (race condition)
                let error_msg = e.to_string();
                if error_msg.contains("already taken")
                    || error_msg.contains("in use")
                    || error_msg.contains("conflict")
                {
                    // User was created by another request, try to get them
                    let existing = state
                        .services
                        .user_storage
                        .get_user_by_username(&oidc_user.localpart)
                        .await
                        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
                        .ok_or_else(|| ApiError::internal("User creation failed".to_string()))?;

                    let device_id = uuid::Uuid::new_v4().to_string()[..8].to_string();
                    let access_token = state
                        .services
                        .auth_service
                        .generate_access_token(&user_id, &device_id, existing.is_admin)
                        .await
                        .map_err(|e| {
                            ApiError::internal(format!("Failed to generate access token: {}", e))
                        })?;
                    let refresh_token = state
                        .services
                        .auth_service
                        .generate_refresh_token(&user_id, &device_id)
                        .await
                        .map_err(|e| {
                            ApiError::internal(format!("Failed to generate refresh token: {}", e))
                        })?;
                    (existing, access_token, refresh_token, device_id)
                } else {
                    return Err(e);
                }
            }
        }
    };

    let user_id_for_log = user.user_id();
    tracing::info!(
        "OIDC user logged in: {}, device_id: {}",
        user_id_for_log,
        device_id
    );

    Ok(Json(serde_json::json!({
        "access_token": access_token,
        "refresh_token": refresh_token,
        "expires_in": state.services.auth_service.token_expiry,
        "device_id": device_id,
        "user_id": user_id_for_log,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_oidc_auth_session_roundtrip() {
        let state = format!("state_{}", OidcService::generate_state());
        let (code_verifier, code_challenge) = OidcService::generate_pkce();
        store_oidc_auth_session(
            &state,
            "nonce",
            &code_verifier,
            &code_challenge,
            "S256",
            "https://example.com/callback",
        )
        .unwrap();

        let session = consume_oidc_auth_session(&state).unwrap();
        assert_eq!(session.nonce, "nonce");
        assert_eq!(session.code_verifier, code_verifier);
        assert_eq!(session.code_challenge, code_challenge);
        assert_eq!(session.redirect_uri, "https://example.com/callback");
    }

    #[test]
    fn test_oidc_auth_session_is_one_time_use() {
        let state = format!("state_{}", OidcService::generate_state());
        let (code_verifier, code_challenge) = OidcService::generate_pkce();
        store_oidc_auth_session(
            &state,
            "nonce",
            &code_verifier,
            &code_challenge,
            "S256",
            "https://example.com/callback",
        )
        .unwrap();

        let _ = consume_oidc_auth_session(&state).unwrap();
        let error = consume_oidc_auth_session(&state).unwrap_err();
        assert!(error.to_string().contains("state is missing"));
    }

    #[test]
    fn test_validate_state_pkce_binding_accepts_valid_binding() {
        let (code_verifier, code_challenge) = OidcService::generate_pkce();
        let session = OidcAuthSession {
            nonce: "nonce".to_string(),
            code_verifier,
            code_challenge,
            code_challenge_method: "S256".to_string(),
            redirect_uri: "https://example.com/callback".to_string(),
            expires_at: current_unix_ts() + 60,
        };

        assert!(validate_state_pkce_binding(&session).is_ok());
    }

    #[test]
    fn test_validate_state_pkce_binding_rejects_mismatched_challenge() {
        let (code_verifier, _) = OidcService::generate_pkce();
        let session = OidcAuthSession {
            nonce: "nonce".to_string(),
            code_verifier,
            code_challenge: "invalid_challenge".to_string(),
            code_challenge_method: "S256".to_string(),
            redirect_uri: "https://example.com/callback".to_string(),
            expires_at: current_unix_ts() + 60,
        };

        let error = validate_state_pkce_binding(&session).unwrap_err();
        assert!(error.to_string().contains("binding validation failed"));
    }

    #[test]
    fn test_oidc_userinfo_response() {
        let response = OidcUserInfoResponse {
            sub: "@test:example.com".to_string(),
            name: Some("Test User".to_string()),
            picture: Some("mxc://example.com/avatar".to_string()),
            email: None,
        };

        assert_eq!(response.sub, "@test:example.com");
        assert_eq!(response.name, Some("Test User".to_string()));
    }

    #[test]
    fn test_openid_discovery() {
        let discovery = OpenIdDiscovery {
            issuer: "https://example.com".to_string(),
            authorization_endpoint: "https://example.com/_matrix/client/v3/oidc/authorize"
                .to_string(),
            token_endpoint: "https://example.com/_matrix/client/v3/oidc/token".to_string(),
            userinfo_endpoint: "https://example.com/_matrix/client/v3/oidc/userinfo".to_string(),
            jwks_uri: "https://example.com/_matrix/keys/v1".to_string(),
            registration_endpoint: None,
            scopes_supported: vec!["openid".to_string()],
            response_types_supported: vec!["code".to_string()],
            response_modes_supported: vec!["query".to_string()],
            grant_types_supported: vec!["authorization_code".to_string()],
            token_endpoint_auth_methods_supported: vec!["client_secret_basic".to_string()],
            claims_supported: vec!["sub".to_string()],
            ui_locales_supported: vec!["en".to_string()],
        };

        assert_eq!(discovery.issuer, "https://example.com");
    }
}
