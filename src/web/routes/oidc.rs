// OIDC (OpenID Connect) 路由
// Matrix Spec: https://matrix.org/docs/spec/openid.html

use crate::common::error::ApiError;
use crate::services::oidc_service::OidcService;
use crate::web::routes::{AppState, AuthenticatedUser};
use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use validator::Validate;

pub fn create_oidc_router(state: AppState) -> Router<AppState> {
    Router::new()
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
        // OIDC 发现
        .route("/.well-known/openid-configuration", get(openid_discovery))
        .with_state(state)
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

    let email = profile.get("email").and_then(|v| v.as_str()).map(String::from);

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
}

/// OIDC Token Response
#[derive(Debug, Serialize)]
pub struct OidcTokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub refresh_token: Option<String>,
    pub scope: String,
}

/// OIDC Token 端点
/// 处理授权码兑换和刷新令牌
async fn oidc_token(
    State(state): State<AppState>,
    Json(body): Json<OidcTokenRequest>,
) -> Result<Json<OidcTokenResponse>, ApiError> {
    // Validate input
    body.validate().map_err(|e| ApiError::bad_request(format!("Validation error: {}", e)))?;

    // 检查 OIDC 服务是否启用
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
    } = body;

    match grant_type.as_str() {
        "authorization_code" => {
            // 授权码模式
            let code = code.ok_or_else(|| {
                ApiError::bad_request("Missing 'code' parameter".to_string())
            })?;
            let redirect_uri = redirect_uri.unwrap_or_default();

            // 使用 OIDC 服务兑换令牌
            let token_response = oidc_service
                .exchange_code(&code, &redirect_uri)
                .await
                .map_err(|e| ApiError::internal(format!("Token exchange failed: {}", e)))?;

            // 获取用户信息
            let user_info = oidc_service
                .get_user_info(&token_response.access_token)
                .await
                .map_err(|e| ApiError::internal(format!("Failed to get user info: {}", e)))?;

            // 映射到 Matrix 用户
            let oidc_user = oidc_service.map_user(&user_info);

            // TODO: 创建或更新 Matrix 用户会话
            // 这里应该调用 registration_service 创建用户

            tracing::info!(
                "OIDC token exchange successful for sub: {}, localpart: {}",
                oidc_user.subject,
                oidc_user.localpart
            );

            Ok(Json(OidcTokenResponse {
                access_token: token_response.access_token,
                token_type: token_response.token_type,
                expires_in: token_response.expires_in.unwrap_or(3600),
                refresh_token: token_response.refresh_token,
                scope: scope.unwrap_or_else(|| "openid profile email".to_string()),
            }))
        }
        "refresh_token" => {
            // 刷新令牌模式 - 需要实现
            let _refresh_token = refresh_token.ok_or_else(|| {
                ApiError::bad_request("Missing 'refresh_token' parameter".to_string())
            })?;
            Err(ApiError::bad_request(
                "Refresh token grant not yet implemented".to_string(),
            ))
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
async fn oidc_authorize(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
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
    let state = auth_state.unwrap_or_else(OidcService::generate_state);
    let nonce = nonce.unwrap_or_else(OidcService::generate_state);

    // 生成授权 URL
    let authorization_url = oidc_service
        .get_authorization_url(&state, &redirect_uri)
        .map_err(|e| ApiError::internal(format!("Failed to generate authorization URL: {}", e)))?;

    tracing::info!(
        "OIDC authorization for user: {}, redirect_uri: {}",
        auth_user.user_id,
        redirect_uri
    );

    Ok(Json(serde_json::json!({
        "authorization_url": authorization_url,
        "state": state,
        "nonce": nonce,
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
    let pool = state.services.user_storage.pool.clone();

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
        sqlx::query("DELETE FROM refresh_tokens WHERE token = $1")
            .bind(&refresh_token)
            .execute(&*pool)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to revoke token: {}", e)))?;
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
        state: _callback_state,
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

    // 获取配置的回调 URL
    let callback_url = oidc_service
        .get_config()
        .callback_url
        .clone()
        .unwrap_or_else(|| format!("https://{}/_matrix/client/v3/oidc/callback", state.services.server_name));

    // 兑换令牌
    let token_response = oidc_service
        .exchange_code(&code, &callback_url)
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
        "OIDC callback successful for sub: {}, localpart: {}, email: {:?}",
        oidc_user.subject,
        oidc_user.localpart,
        oidc_user.email
    );

    // TODO: 创建或登录 Matrix 用户
    // 这里应该调用 registration_service 来创建用户或关联现有用户

    Ok(Json(serde_json::json!({
        "success": true,
        "sub": oidc_user.subject,
        "localpart": oidc_user.localpart,
        "displayname": oidc_user.displayname,
        "email": oidc_user.email,
        "access_token": token_response.access_token,
        "token_type": token_response.token_type,
        "expires_in": token_response.expires_in,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

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
