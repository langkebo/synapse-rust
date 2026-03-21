// OIDC (OpenID Connect) 路由
// Matrix Spec: https://matrix.org/docs/spec/openid.html

use crate::common::error::ApiError;
use crate::web::routes::{AppState, AuthenticatedUser};
use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

pub fn create_oidc_router(state: AppState) -> Router<AppState> {
    Router::new()
        // v3 路径
        .route("/_matrix/client/v3/oidc/userinfo", get(oidc_userinfo))
        .route("/_matrix/client/v3/oidc/token", post(oidc_token))
        .route("/_matrix/client/v3/oidc/logout", post(oidc_logout))
        .route("/_matrix/client/v3/oidc/authorize", get(oidc_authorize))
        .route("/_matrix/client/v3/oidc/register", post(oidc_register))
        // r0 路径兼容
        .route("/_matrix/client/r0/oidc/userinfo", get(oidc_userinfo))
        .route("/_matrix/client/r0/oidc/token", post(oidc_token))
        .route("/_matrix/client/r0/oidc/logout", post(oidc_logout))
        .route("/_matrix/client/r0/oidc/authorize", get(oidc_authorize))
        .route("/_matrix/client/r0/oidc/register", post(oidc_register))
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

    Ok(Json(OidcUserInfoResponse {
        sub: user_id.clone(),
        name,
        picture,
        email: None, // 需要额外查询
    }))
}

/// OIDC Token Request
#[derive(Debug, Deserialize)]
pub struct OidcTokenRequest {
    pub grant_type: String,
    pub code: Option<String>,
    pub redirect_uri: Option<String>,
    pub refresh_token: Option<String>,
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
/// 注意: 这个端点通常由授权服务器处理，这里提供 Matrix 兼容的令牌验证
async fn oidc_token(
    State(_state): State<AppState>,
    Json(_body): Json<OidcTokenRequest>,
) -> Result<Json<OidcTokenResponse>, ApiError> {
    // OIDC token 端点通常由外部 OIDC 提供商处理
    // 这里返回错误，引导用户使用正确的 OIDC 流程
    Err(ApiError::bad_request(
        "OIDC token endpoint not available. Please use /login with OIDC provider.".to_string(),
    ))
}

/// OIDC Logout Request
#[derive(Debug, Deserialize)]
pub struct OidcLogoutRequest {
    pub refresh_token: Option<String>,
    pub device_id: Option<String>,
}

/// OIDC Authorize Request
#[derive(Debug, Deserialize)]
pub struct OidcAuthorizeRequest {
    pub response_type: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub scope: Option<String>,
    pub state: Option<String>,
    pub nonce: Option<String>,
}

/// OIDC Authorization handler
async fn oidc_authorize(
    State(_state): State<AppState>,
    _auth_user: AuthenticatedUser,
    _query: axum::extract::Query<OidcAuthorizeRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::bad_request(
        "OIDC authorization endpoint not available. Please use SAML or CAS authentication."
            .to_string(),
    ))
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
