//! 内置 OIDC Provider 服务
//!
//! 提供简单的内置 OIDC 认证服务，用于开发测试环境

use crate::common::error::ApiError;
use crate::common::{BuiltinOidcConfig, BuiltinOidcUser};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tracing::info;
use uuid::Uuid;

// ============ 类型定义 ============

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcDiscoveryDocument {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub userinfo_endpoint: String,
    pub jwks_uri: String,
    pub end_session_endpoint: Option<String>,
    pub response_types_supported: Vec<String>,
    pub subject_types_supported: Vec<String>,
    pub id_token_signing_alg_values_supported: Vec<String>,
    pub scopes_supported: Vec<String>,
    pub token_endpoint_auth_methods_supported: Vec<String>,
    pub claims_supported: Vec<String>,
    pub code_challenge_methods_supported: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcTokenRequest {
    pub grant_type: String,
    pub code: Option<String>,
    pub redirect_uri: Option<String>,
    pub client_id: Option<String>,
    pub code_verifier: Option<String>,
    pub refresh_token: Option<String>,
    pub scope: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcTokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub id_token: String,
    pub refresh_token: Option<String>,
    pub scope: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcUserInfo {
    pub sub: String,
    pub name: Option<String>,
    pub given_name: Option<String>,
    pub family_name: Option<String>,
    pub preferred_username: Option<String>,
    pub email: Option<String>,
    pub email_verified: bool,
    pub picture: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Jwks {
    pub keys: Vec<Jwk>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Jwk {
    pub kty: String,
    pub use_: String,
    pub kid: String,
    pub alg: String,
    pub n: String,
    pub e: String,
}

// ============ JWT Claims ============

#[derive(Debug, Serialize, Deserialize)]
pub struct JwtClaims {
    pub iss: String,
    pub sub: String,
    pub aud: String,
    pub exp: i64,
    pub iat: i64,
    pub nonce: Option<String>,
    pub at_hash: Option<String>,
    pub email: Option<String>,
    pub email_verified: bool,
    pub name: Option<String>,
    pub picture: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AccessTokenClaims {
    pub iss: String,
    pub sub: String,
    pub aud: Vec<String>,
    pub exp: i64,
    pub iat: i64,
    pub jti: String,
    pub scope: String,
}

// ============ 授权会话 ============

#[derive(Debug, Clone)]
pub struct AuthSession {
    pub code: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub scope: String,
    pub state: String,
    pub nonce: Option<String>,
    pub code_verifier: Option<String>,
    pub user_id: String,
    pub created_at: Instant,
}

// ============ 内置 OIDC Provider ============

pub struct BuiltinOidcProvider {
    config: Arc<BuiltinOidcConfig>,
    signing_key: Vec<u8>,
    auth_sessions:
        std::sync::Arc<std::sync::RwLock<std::collections::HashMap<String, AuthSession>>>,
    refresh_tokens:
        std::sync::Arc<std::sync::RwLock<std::collections::HashMap<String, RefreshToken>>>,
}

#[derive(Debug, Clone)]
pub struct RefreshToken {
    pub user_id: String,
    pub client_id: String,
    pub scope: String,
    pub created_at: Instant,
}

#[derive(Debug, Deserialize)]
pub struct AuthorizeRequest {
    pub client_id: String,
    pub redirect_uri: String,
    pub scope: String,
    pub state: String,
    pub nonce: Option<String>,
    pub code_verifier: Option<String>,
    pub username: String,
    pub password: String,
}

impl BuiltinOidcProvider {
    pub fn new(config: Arc<BuiltinOidcConfig>) -> Self {
        // 生成随机签名密钥
        let mut rng = rand::thread_rng();
        let signing_key: Vec<u8> = (0..32).map(|_| rng.gen()).collect();

        Self {
            config,
            signing_key,
            auth_sessions: Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
            refresh_tokens: Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
        }
    }

    /// 获取 OIDC 发现文档
    pub fn get_discovery_document(&self) -> OidcDiscoveryDocument {
        let issuer = &self.config.issuer;
        OidcDiscoveryDocument {
            issuer: issuer.clone(),
            authorization_endpoint: format!("{}/_matrix/client/v3/oidc/authorize", issuer),
            token_endpoint: format!("{}/_matrix/client/v3/oidc/token", issuer),
            userinfo_endpoint: format!("{}/_matrix/client/v3/oidc/userinfo", issuer),
            jwks_uri: format!("{}/.well-known/jwks.json", issuer),
            end_session_endpoint: Some(format!("{}/_matrix/client/v3/oidc/logout", issuer)),
            response_types_supported: vec!["code".to_string()],
            subject_types_supported: vec!["public".to_string()],
            id_token_signing_alg_values_supported: vec!["RS256".to_string()],
            scopes_supported: vec![
                "openid".to_string(),
                "profile".to_string(),
                "email".to_string(),
            ],
            token_endpoint_auth_methods_supported: vec![
                "client_secret_basic".to_string(),
                "client_secret_post".to_string(),
            ],
            claims_supported: vec![
                "sub".to_string(),
                "iss".to_string(),
                "aud".to_string(),
                "exp".to_string(),
                "iat".to_string(),
                "nonce".to_string(),
                "email".to_string(),
                "email_verified".to_string(),
                "name".to_string(),
                "picture".to_string(),
            ],
            code_challenge_methods_supported: vec!["S256".to_string()],
        }
    }

    /// 获取 JWKS
    pub fn get_jwks(&self) -> Jwks {
        // 从签名密钥派生 RSA 模数和指数（简化实现）
        // 生产环境应该使用真正的 RSA 密钥
        let mut rng = rand::thread_rng();
        let e: u32 = 65537;

        // 生成伪随机 n（实际应使用真实 RSA 密钥）
        let n_bytes: Vec<u8> = (0..256).map(|_| rng.gen()).collect();
        let n = URL_SAFE_NO_PAD.encode(&n_bytes);
        let e_encoded = URL_SAFE_NO_PAD.encode(e.to_be_bytes());

        Jwks {
            keys: vec![Jwk {
                kty: "RSA".to_string(),
                use_: "sig".to_string(),
                kid: "builtin-oidc-key-1".to_string(),
                alg: "RS256".to_string(),
                n,
                e: e_encoded,
            }],
        }
    }

    /// 处理授权请求
    pub fn authorize(&self, request: AuthorizeRequest) -> Result<String, ApiError> {
        // 验证用户
        let user = self.verify_user(&request.username, &request.password)?;

        // 验证 client_id
        if !self.config.allow_client_ids.is_empty()
            && !self.config.allow_client_ids.contains(&request.client_id)
        {
            return Err(ApiError::unauthorized("Invalid client_id".to_string()));
        }

        // 验证 redirect_uri
        if !self.config.allow_redirect_uris.is_empty()
            && !self
                .config
                .allow_redirect_uris
                .contains(&request.redirect_uri)
        {
            return Err(ApiError::unauthorized("Invalid redirect_uri".to_string()));
        }

        // 生成授权码
        let code = Uuid::new_v4().to_string();

        // 存储会话
        let session = AuthSession {
            code: code.clone(),
            client_id: request.client_id,
            redirect_uri: request.redirect_uri,
            scope: request.scope,
            state: request.state,
            nonce: request.nonce,
            code_verifier: request.code_verifier,
            user_id: user.id.clone(),
            created_at: Instant::now(),
        };

        self.auth_sessions
            .write()
            .unwrap()
            .insert(code.clone(), session);

        info!("OIDC authorization code generated for user: {}", user.id);
        Ok(code)
    }

    /// 处理令牌请求
    pub fn token(&self, request: OidcTokenRequest) -> Result<OidcTokenResponse, ApiError> {
        let grant_type = &request.grant_type;

        match grant_type.as_str() {
            "authorization_code" => self.handle_authorization_code_grant(request),
            "refresh_token" => self.handle_refresh_token_grant(request),
            _ => Err(ApiError::bad_request("Unsupported grant_type".to_string())),
        }
    }

    fn handle_authorization_code_grant(
        &self,
        request: OidcTokenRequest,
    ) -> Result<OidcTokenResponse, ApiError> {
        let code = request
            .code
            .as_ref()
            .ok_or(ApiError::bad_request("Missing code".to_string()))?;
        let redirect_uri = request
            .redirect_uri
            .as_ref()
            .ok_or(ApiError::bad_request("Missing redirect_uri".to_string()))?;
        let client_id = request
            .client_id
            .as_ref()
            .ok_or(ApiError::bad_request("Missing client_id".to_string()))?;

        // 提取会话
        let session =
            self.auth_sessions
                .write()
                .unwrap()
                .remove(code)
                .ok_or(ApiError::unauthorized(
                    "Invalid or expired code".to_string(),
                ))?;

        // 验证会话
        if session.redirect_uri != *redirect_uri {
            return Err(ApiError::unauthorized("Redirect URI mismatch".to_string()));
        }
        if session.client_id != *client_id {
            return Err(ApiError::unauthorized("Client ID mismatch".to_string()));
        }
        if session.created_at.elapsed() > Duration::from_secs(600) {
            return Err(ApiError::unauthorized("Code expired".to_string()));
        }

        // 验证 PKCE 如果提供
        if let Some(ref verifier) = request.code_verifier {
            if let Some(ref _challenge) = session.code_verifier {
                // 简化验证：实际应该做 SHA256 hash 比较
                if verifier.len() < 43 {
                    return Err(ApiError::bad_request("Invalid code_verifier".to_string()));
                }
            }
        }

        // 获取用户信息
        let user = self
            .config
            .users
            .iter()
            .find(|u| u.id == session.user_id)
            .ok_or(ApiError::not_found("User not found".to_string()))?;

        // 生成令牌
        let access_token = self.generate_access_token(user, session.scope.as_str())?;
        let id_token = self.generate_id_token(user, client_id, session.nonce.as_deref())?;
        let refresh_token = self.generate_refresh_token(user, session.scope.as_str());

        Ok(OidcTokenResponse {
            access_token,
            token_type: "Bearer".to_string(),
            expires_in: 3600,
            id_token,
            refresh_token: Some(refresh_token),
            scope: Some(session.scope),
        })
    }

    fn handle_refresh_token_grant(
        &self,
        request: OidcTokenRequest,
    ) -> Result<OidcTokenResponse, ApiError> {
        let refresh_token = request
            .refresh_token
            .as_ref()
            .ok_or(ApiError::bad_request("Missing refresh_token".to_string()))?;

        // 查找 refresh token
        let token_data = self
            .refresh_tokens
            .read()
            .unwrap()
            .get(refresh_token)
            .cloned()
            .ok_or(ApiError::unauthorized("Invalid refresh_token".to_string()))?;

        // 获取用户
        let user = self
            .config
            .users
            .iter()
            .find(|u| u.id == token_data.user_id)
            .ok_or(ApiError::not_found("User not found".to_string()))?;

        // 生成新令牌
        let access_token = self.generate_access_token(user, token_data.scope.as_str())?;
        let id_token = self.generate_id_token(user, &token_data.client_id, None)?;

        Ok(OidcTokenResponse {
            access_token,
            token_type: "Bearer".to_string(),
            expires_in: 3600,
            id_token,
            refresh_token: Some(refresh_token.clone()),
            scope: Some(token_data.scope),
        })
    }

    /// 获取用户信息
    pub fn userinfo(&self, access_token: &str) -> Result<OidcUserInfo, ApiError> {
        let claims = self.verify_access_token(access_token)?;

        let user = self
            .config
            .users
            .iter()
            .find(|u| u.id == claims.sub)
            .ok_or(ApiError::not_found("User not found".to_string()))?;

        Ok(OidcUserInfo {
            sub: user.id.clone(),
            name: user.displayname.clone(),
            given_name: None,
            family_name: None,
            preferred_username: Some(user.username.clone()),
            email: Some(user.email.clone()),
            email_verified: true,
            picture: None,
        })
    }

    /// 验证用户
    fn verify_user(&self, username: &str, password: &str) -> Result<&BuiltinOidcUser, ApiError> {
        let user = self
            .config
            .users
            .iter()
            .find(|u| u.username == username && u.password == password)
            .ok_or(ApiError::unauthorized(
                "Invalid username or password".to_string(),
            ))?;
        Ok(user)
    }

    /// 生成 ID Token
    fn generate_id_token(
        &self,
        user: &BuiltinOidcUser,
        client_id: &str,
        nonce: Option<&str>,
    ) -> Result<String, ApiError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let mut claims = JwtClaims {
            iss: self.config.issuer.clone(),
            sub: user.id.clone(),
            aud: client_id.to_string(),
            exp: now + 3600,
            iat: now,
            nonce: nonce.map(String::from),
            at_hash: None,
            email: Some(user.email.clone()),
            email_verified: true,
            name: user.displayname.clone(),
            picture: None,
        };

        // 计算 at_hash
        // 简化实现
        claims.at_hash = Some("".to_string());

        let header = Header::new(Algorithm::RS256);
        let token = encode(
            &header,
            &claims,
            &EncodingKey::from_rsa_pem(&self.signing_key).unwrap_or_else(|_| {
                // 如果密钥格式错误，使用 HMAC
                EncodingKey::from_secret(&self.signing_key)
            }),
        )
        .map_err(|e| ApiError::internal(format!("Failed to generate ID token: {}", e)))?;

        Ok(token)
    }

    /// 生成 Access Token
    fn generate_access_token(
        &self,
        user: &BuiltinOidcUser,
        scope: &str,
    ) -> Result<String, ApiError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let claims = AccessTokenClaims {
            iss: self.config.issuer.clone(),
            sub: user.id.clone(),
            aud: vec![self.config.issuer.clone()],
            exp: now + 3600,
            iat: now,
            jti: Uuid::new_v4().to_string(),
            scope: scope.to_string(),
        };

        let header = Header::new(Algorithm::HS256);
        let token = encode(
            &header,
            &claims,
            &EncodingKey::from_secret(&self.signing_key),
        )
        .map_err(|e| ApiError::internal(format!("Failed to generate access token: {}", e)))?;

        Ok(token)
    }

    /// 生成 Refresh Token
    fn generate_refresh_token(&self, user: &BuiltinOidcUser, scope: &str) -> String {
        let token = Uuid::new_v4().to_string();

        let refresh_token = RefreshToken {
            user_id: user.id.clone(),
            client_id: self.config.client_id.clone(),
            scope: scope.to_string(),
            created_at: Instant::now(),
        };

        self.refresh_tokens
            .write()
            .unwrap()
            .insert(token.clone(), refresh_token);

        token
    }

    /// 验证 Access Token
    fn verify_access_token(&self, token: &str) -> Result<AccessTokenClaims, ApiError> {
        let validation = Validation::new(Algorithm::HS256);
        let claims = decode::<AccessTokenClaims>(
            token,
            &DecodingKey::from_secret(&self.signing_key),
            &validation,
        )
        .map_err(|e| ApiError::unauthorized(format!("Invalid token: {}", e)))?
        .claims;

        Ok(claims)
    }

    /// 登出
    pub fn logout(&self, refresh_token: Option<&str>) -> Result<(), ApiError> {
        if let Some(token) = refresh_token {
            self.refresh_tokens.write().unwrap().remove(token);
        }

        // 清除所有授权会话
        self.auth_sessions.write().unwrap().clear();

        Ok(())
    }
}
