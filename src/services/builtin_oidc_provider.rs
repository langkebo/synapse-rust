//! 内置 OIDC Provider 服务
//!
//! 提供简单的内置 OIDC 认证服务，用于开发测试环境。
//!
//! # 适用场景
//!
//! - **开发/测试环境**: 快速搭建认证服务，无需外部 IdP
//! - **内部部署**: 不需要对接外部身份提供商的小型私有部署
//!
//! # 不适用场景
//!
//! - **生产环境**: 应使用外部 IdP（如 Keycloak、Auth0）通过 `OidcService` 接入
//! - **需要高安全性的场景**: 内置 Provider 的密钥管理较为简单
//!
//! # 与 OidcService 的关系
//!
//! - `OidcService`: 外部 IdP 客户端模式，通过 discovery URL 对接外部身份提供商
//! - `BuiltinOidcProvider`: 内置 Provider 模式，自身充当 OIDC Provider
//!
//! 两者不应同时启用。启动时会检测冲突并发出警告。

use crate::common::error::ApiError;
use crate::common::{BuiltinOidcConfig, BuiltinOidcUser};
use argon2::{password_hash::PasswordHash, Argon2, PasswordVerifier};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use rsa::pkcs1::EncodeRsaPrivateKey;
use rsa::pkcs1v15::SigningKey;
use rsa::pkcs8::{DecodePrivateKey, EncodePublicKey};
use rsa::traits::PublicKeyParts;
use rsa::{RsaPrivateKey, RsaPublicKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use subtle::ConstantTimeEq;
use tracing::{info, warn};
use uuid::Uuid;

// ============ 类型定义 ============

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcDiscoveryDocument {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub userinfo_endpoint: String,
    pub jwks_uri: String,
    pub registration_endpoint: Option<String>,
    pub revocation_endpoint: Option<String>,
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
    #[serde(rename = "use")]
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
    /// 客户端在 /authorize 提交的 PKCE code_challenge (S256, BASE64URL(SHA256(verifier)))
    pub code_challenge: Option<String>,
    pub user_id: String,
    pub created_at: Instant,
}

// ============ 内置 OIDC Provider ============

pub struct BuiltinOidcProvider {
    config: Arc<BuiltinOidcConfig>,
    signing_key: RsaPrivateKey,
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    key_id: String,
    auth_sessions: std::sync::Arc<tokio::sync::RwLock<std::collections::HashMap<String, AuthSession>>>,
    refresh_tokens: std::sync::Arc<tokio::sync::RwLock<std::collections::HashMap<String, RefreshToken>>>,
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
    /// PKCE code_challenge 由客户端生成: BASE64URL(SHA256(code_verifier))
    /// 字段名保留为 `code_verifier` 以兼容外层路由参数, 语义见文档.
    #[serde(alias = "code_challenge")]
    pub code_verifier: Option<String>,
    pub username: String,
    pub password: String,
}

const OIDC_TOKEN_EXPIRY_SECS: i64 = 3600;
const AUTH_CODE_EXPIRY_SECS: i64 = 600;

impl BuiltinOidcProvider {
    pub fn new(config: Arc<BuiltinOidcConfig>) -> Result<Self, ApiError> {
        let signing_key = Self::load_or_generate_key(config.signing_key_path.as_deref())?;
        let der = signing_key.to_pkcs1_der().map_err(|e| ApiError::internal_with_log("OIDC RSA serialize", &e))?;
        let encoding_key = EncodingKey::from_rsa_der(der.as_bytes());

        let public_der = signing_key
            .to_public_key()
            .to_public_key_der()
            .map_err(|e| ApiError::internal_with_log("OIDC RSA pub serialize", &e))?;
        let decoding_key = DecodingKey::from_rsa_der(public_der.as_bytes());

        // 计算稳定的 kid: SHA256 over public-key DER, 取前 12B base64url
        let mut hasher = Sha256::new();
        hasher.update(public_der.as_bytes());
        let digest = hasher.finalize();
        let key_id = URL_SAFE_NO_PAD.encode(&digest[..12]);

        Ok(Self {
            config,
            signing_key,
            encoding_key,
            decoding_key,
            key_id,
            auth_sessions: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            refresh_tokens: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        })
    }

    /// 从指定路径加载 RSA 私钥, 不存在则生成并持久化.
    /// path 为 None 时仅在内存中生成 (重启后所有 token 失效, 仅适合开发).
    fn load_or_generate_key(path: Option<&Path>) -> Result<RsaPrivateKey, ApiError> {
        if let Some(p) = path {
            if p.exists() {
                let pem = std::fs::read_to_string(p)
                    .map_err(|e| ApiError::internal_with_log("OIDC signing key read", &e))?;
                return RsaPrivateKey::from_pkcs8_pem(&pem)
                    .map_err(|e| ApiError::internal_with_log("OIDC signing key parse", &e));
            }
        }

        info!("Generating new RSA-2048 key for builtin OIDC provider");
        let mut rng = rand::thread_rng();
        let key =
            RsaPrivateKey::new(&mut rng, 2048).map_err(|e| ApiError::internal_with_log("OIDC RSA generate", &e))?;

        if let Some(p) = path {
            use rsa::pkcs8::EncodePrivateKey;
            if let Some(parent) = p.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let pem = key
                .to_pkcs8_pem(rsa::pkcs8::LineEnding::LF)
                .map_err(|e| ApiError::internal_with_log("OIDC RSA pem", &e))?;
            std::fs::write(p, pem.as_bytes())
                .map_err(|e| ApiError::internal_with_log("OIDC signing key write", &e))?;
            info!("Persisted builtin OIDC signing key to {}", p.display());
        } else {
            warn!(
                "BuiltinOidcProvider: signing_key_path not configured; key is ephemeral and \
                 all issued tokens will be invalidated on restart"
            );
        }

        let _ = SigningKey::<Sha256>::new(key.clone()); // 探测 SHA256 sign 可用
        Ok(key)
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
            registration_endpoint: Some(format!("{}/_matrix/client/v3/oidc/register", issuer)),
            revocation_endpoint: Some(format!("{}/_matrix/client/v3/oidc/revoke", issuer)),
            end_session_endpoint: Some(format!("{}/_matrix/client/v3/oidc/logout", issuer)),
            response_types_supported: vec!["code".to_string()],
            subject_types_supported: vec!["public".to_string()],
            id_token_signing_alg_values_supported: vec!["RS256".to_string()],
            scopes_supported: vec!["openid".to_string(), "profile".to_string(), "email".to_string()],
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

    /// 获取 JWKS (从真实 RSA 公钥导出 n/e)
    pub fn get_jwks(&self) -> Jwks {
        let public: RsaPublicKey = self.signing_key.to_public_key();
        let n = URL_SAFE_NO_PAD.encode(public.n().to_bytes_be());
        let e = URL_SAFE_NO_PAD.encode(public.e().to_bytes_be());

        Jwks {
            keys: vec![Jwk {
                kty: "RSA".to_string(),
                use_: "sig".to_string(),
                kid: self.key_id.clone(),
                alg: "RS256".to_string(),
                n,
                e,
            }],
        }
    }

    /// 处理授权请求
    pub async fn authorize(&self, request: AuthorizeRequest) -> Result<String, ApiError> {
        // 验证用户
        let user = self.verify_user(&request.username, &request.password)?;

        // 验证 client_id
        if !self.config.allow_client_ids.is_empty() && !self.config.allow_client_ids.contains(&request.client_id) {
            return Err(ApiError::unauthorized("Invalid client_id".to_string()));
        }

        // 验证 redirect_uri
        if !self.config.allow_redirect_uris.is_empty()
            && !self.config.allow_redirect_uris.contains(&request.redirect_uri)
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
            code_challenge: request.code_verifier,
            user_id: user.id.clone(),
            created_at: Instant::now(),
        };

        self.auth_sessions.write().await.insert(code.clone(), session);

        info!("OIDC authorization code generated for user: {}", user.id);
        Ok(code)
    }

    /// 处理令牌请求
    pub async fn token(&self, request: OidcTokenRequest) -> Result<OidcTokenResponse, ApiError> {
        let grant_type = &request.grant_type;

        match grant_type.as_str() {
            "authorization_code" => self.handle_authorization_code_grant(request).await,
            "refresh_token" => self.handle_refresh_token_grant(request).await,
            _ => Err(ApiError::bad_request("Unsupported grant_type".to_string())),
        }
    }

    async fn handle_authorization_code_grant(&self, request: OidcTokenRequest) -> Result<OidcTokenResponse, ApiError> {
        let code = request.code.as_ref().ok_or(ApiError::bad_request("Missing code".to_string()))?;
        let redirect_uri =
            request.redirect_uri.as_ref().ok_or(ApiError::bad_request("Missing redirect_uri".to_string()))?;
        let client_id = request.client_id.as_ref().ok_or(ApiError::bad_request("Missing client_id".to_string()))?;

        // 提取会话
        let session = self
            .auth_sessions
            .write()
            .await
            .remove(code)
            .ok_or(ApiError::unauthorized("Invalid or expired code".to_string()))?;

        // 验证会话
        if session.redirect_uri != *redirect_uri {
            return Err(ApiError::unauthorized("Redirect URI mismatch".to_string()));
        }
        if session.client_id != *client_id {
            return Err(ApiError::unauthorized("Client ID mismatch".to_string()));
        }
        if session.created_at.elapsed() > Duration::from_secs(AUTH_CODE_EXPIRY_SECS as u64) {
            return Err(ApiError::unauthorized("Code expired".to_string()));
        }

        // PKCE 验证: 若 authorize 阶段绑定了 code_challenge, 则必须提供并匹配 code_verifier
        if let Some(ref challenge) = session.code_challenge {
            let verifier = request
                .code_verifier
                .as_deref()
                .ok_or_else(|| ApiError::bad_request("Missing code_verifier (PKCE required)".to_string()))?;
            if verifier.len() < 43 || verifier.len() > 128 {
                return Err(ApiError::bad_request("code_verifier length must be 43..=128".to_string()));
            }
            let mut hasher = Sha256::new();
            hasher.update(verifier.as_bytes());
            let computed = URL_SAFE_NO_PAD.encode(hasher.finalize());
            // 常量时间比较, 抵御 timing
            if computed.as_bytes().ct_eq(challenge.as_bytes()).unwrap_u8() != 1 {
                return Err(ApiError::unauthorized("PKCE code_verifier mismatch".to_string()));
            }
        }

        // 获取用户信息
        let user = self
            .config
            .users
            .iter()
            .find(|u| u.id == session.user_id)
            .ok_or(ApiError::not_found("User not found".to_string()))?;

        // 生成令牌 (先 access, 再用 access 计算 at_hash)
        let access_token = self.generate_access_token(user, session.scope.as_str())?;
        let id_token = self.generate_id_token(user, client_id, session.nonce.as_deref(), &access_token)?;
        let refresh_token = self.generate_refresh_token(user, session.scope.as_str()).await?;

        Ok(OidcTokenResponse {
            access_token,
            token_type: "Bearer".to_string(),
            expires_in: OIDC_TOKEN_EXPIRY_SECS,
            id_token,
            refresh_token: Some(refresh_token),
            scope: Some(session.scope),
        })
    }

    async fn handle_refresh_token_grant(&self, request: OidcTokenRequest) -> Result<OidcTokenResponse, ApiError> {
        let refresh_token =
            request.refresh_token.as_ref().ok_or(ApiError::bad_request("Missing refresh_token".to_string()))?;

        // 查找 refresh token
        let token_data = self
            .refresh_tokens
            .read()
            .await
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
        let id_token = self.generate_id_token(user, &token_data.client_id, None, &access_token)?;

        Ok(OidcTokenResponse {
            access_token,
            token_type: "Bearer".to_string(),
            expires_in: OIDC_TOKEN_EXPIRY_SECS,
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

    /// 验证用户 (优先 argon2 password_hash, 兜底 plaintext + 启动告警)
    fn verify_user(&self, username: &str, password: &str) -> Result<&BuiltinOidcUser, ApiError> {
        let user = self
            .config
            .users
            .iter()
            .find(|u| u.username == username)
            .ok_or(ApiError::unauthorized("Invalid username or password".to_string()))?;

        if let Some(ref phc) = user.password_hash {
            let parsed = PasswordHash::new(phc).map_err(|e| {
                tracing::error!("Invalid password_hash for {}: {}", username, e);
                ApiError::internal("Authentication configuration error".to_string())
            })?;
            Argon2::default()
                .verify_password(password.as_bytes(), &parsed)
                .map_err(|_| ApiError::unauthorized("Invalid username or password".to_string()))?;
            return Ok(user);
        }

        if let Some(ref plain) = user.password {
            warn!(
                "BuiltinOidcProvider: user '{}' has plaintext password configured; \
                 migrate to password_hash (argon2 PHC) for production",
                username
            );
            // 常量时间比较
            if plain.as_bytes().ct_eq(password.as_bytes()).unwrap_u8() == 1 {
                return Ok(user);
            }
        }

        Err(ApiError::unauthorized("Invalid username or password".to_string()))
    }

    /// 计算 at_hash: BASE64URL( left-128-bit( SHA256(access_token) ) ) for RS256
    fn compute_at_hash(access_token: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(access_token.as_bytes());
        let digest = hasher.finalize();
        URL_SAFE_NO_PAD.encode(&digest[..16])
    }

    /// 生成 ID Token (RS256)
    fn generate_id_token(
        &self,
        user: &BuiltinOidcUser,
        client_id: &str,
        nonce: Option<&str>,
        access_token: &str,
    ) -> Result<String, ApiError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| ApiError::internal_with_log("clock", &e))?
            .as_secs() as i64;

        let claims = JwtClaims {
            iss: self.config.issuer.clone(),
            sub: user.id.clone(),
            aud: client_id.to_string(),
            exp: now + OIDC_TOKEN_EXPIRY_SECS,
            iat: now,
            nonce: nonce.map(String::from),
            at_hash: Some(Self::compute_at_hash(access_token)),
            email: Some(user.email.clone()),
            email_verified: true,
            name: user.displayname.clone(),
            picture: None,
        };

        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(self.key_id.clone());
        encode(&header, &claims, &self.encoding_key)
            .map_err(|e| ApiError::internal_with_log("Failed to generate ID token", &e))
    }

    /// 生成 Access Token (RS256, 与 id_token 算法一致, 防止 alg 混淆)
    fn generate_access_token(&self, user: &BuiltinOidcUser, scope: &str) -> Result<String, ApiError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| ApiError::internal_with_log("clock", &e))?
            .as_secs() as i64;

        let claims = AccessTokenClaims {
            iss: self.config.issuer.clone(),
            sub: user.id.clone(),
            aud: vec![self.config.issuer.clone()],
            exp: now + OIDC_TOKEN_EXPIRY_SECS,
            iat: now,
            jti: Uuid::new_v4().to_string(),
            scope: scope.to_string(),
        };

        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(self.key_id.clone());
        encode(&header, &claims, &self.encoding_key)
            .map_err(|e| ApiError::internal_with_log("Failed to generate access token", &e))
    }

    /// 生成 Refresh Token
    async fn generate_refresh_token(&self, user: &BuiltinOidcUser, scope: &str) -> Result<String, ApiError> {
        let token = Uuid::new_v4().to_string();

        let refresh_token = RefreshToken {
            user_id: user.id.clone(),
            client_id: self.config.client_id.clone(),
            scope: scope.to_string(),
            created_at: Instant::now(),
        };

        self.refresh_tokens.write().await.insert(token.clone(), refresh_token);

        Ok(token)
    }

    /// 验证 Access Token (RS256)
    fn verify_access_token(&self, token: &str) -> Result<AccessTokenClaims, ApiError> {
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_audience(&[&self.config.issuer]);
        validation.set_issuer(&[&self.config.issuer]);
        let claims = decode::<AccessTokenClaims>(token, &self.decoding_key, &validation)
            .map_err(|e| ApiError::unauthorized(format!("Invalid token: {}", e)))?
            .claims;

        Ok(claims)
    }

    /// 登出: 仅撤销给定 refresh_token 关联用户的所有 refresh, 不动其他用户.
    /// 不再清空全局 auth_sessions.
    pub async fn logout(&self, refresh_token: Option<&str>) -> Result<(), ApiError> {
        let Some(token) = refresh_token else {
            return Ok(());
        };

        // 找出 token 所属用户
        let owner = {
            let map = self.refresh_tokens.read().await;
            map.get(token).map(|t| t.user_id.clone())
        };

        let Some(user_id) = owner else {
            return Ok(());
        };

        {
            let mut map = self.refresh_tokens.write().await;
            map.retain(|_, t| t.user_id != user_id);
        }

        {
            let mut map = self.auth_sessions.write().await;
            map.retain(|_, s| s.user_id != user_id);
        }

        info!("OIDC logout: revoked sessions for user {}", user_id);
        Ok(())
    }
}
