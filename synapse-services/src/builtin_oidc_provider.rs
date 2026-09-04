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

use argon2::{password_hash::PasswordHash, Argon2, PasswordVerifier};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use p256::elliptic_curve::sec1::ToSec1Point;
use p256::elliptic_curve::Generate;
use p256::pkcs8::{DecodePrivateKey, EncodePrivateKey};
use p256::{PublicKey as P256PublicKey, SecretKey};
use rsa::pkcs1::EncodeRsaPrivateKey;
use rsa::pkcs1v15::SigningKey;
use rsa::pkcs8::DecodePrivateKey as RsaDecodePrivateKey;
use rsa::pkcs8::EncodePublicKey;
use rsa::traits::PublicKeyParts;
use rsa::{RsaPrivateKey, RsaPublicKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use synapse_common::error::ApiError;
use synapse_common::{BuiltinOidcConfig, BuiltinOidcUser};

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
    // RSA components — present when kty == "RSA"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub e: Option<String>,
    // EC components — present when kty == "EC"
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "crv")]
    pub crv: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y: Option<String>,
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
    /// P-256 EC private key for ES256 signing (RFC 7518 §3.4).
    /// The EC and RSA keys are independent — both are advertised in the JWKS endpoint.
    ec_signing_key: SecretKey,
    ec_encoding_key: EncodingKey,
    ec_decoding_key: DecodingKey,
    ec_key_id: String,
    auth_sessions: std::sync::Arc<tokio::sync::RwLock<std::collections::HashMap<String, AuthSession>>>,
    refresh_tokens: std::sync::Arc<tokio::sync::RwLock<std::collections::HashMap<String, RefreshToken>>>,
}

#[derive(Debug, Clone)]
pub struct RefreshToken {
    pub user_id: String,
    pub client_id: String,
    pub scope: String,
    pub created_at: Instant,
    /// SSO-AUDIT: refresh token expiry. The default 30 days is set to match
    /// the local Matrix refresh-token expiry in synapse-common::config; the
    /// builtin OIDC provider's RefreshToken struct previously had no expiry
    /// field at all, which meant an in-memory entry could outlive the
    /// intended lifetime of the user's session and accumulate forever.
    pub expires_at: Instant,
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
        let der = signing_key.to_pkcs1_der().map_err(|e| ApiError::internal_with_context("OIDC RSA serialize", &e))?;
        let encoding_key = EncodingKey::from_rsa_der(der.as_bytes());

        let public_der = signing_key
            .to_public_key()
            .to_public_key_der()
            .map_err(|e| ApiError::internal_with_context("OIDC RSA pub serialize", &e))?;
        let decoding_key = DecodingKey::from_rsa_der(public_der.as_bytes());

        // 计算稳定的 kid: SHA256 over public-key DER, 取前 12B base64url
        let mut hasher = Sha256::new();
        hasher.update(public_der.as_bytes());
        let digest = hasher.finalize();
        let key_id = URL_SAFE_NO_PAD.encode(&digest[..12]);

        // P-256 EC signing key (parallel to RSA path).
        let ec_signing_key = Self::load_or_generate_ec_key(config.signing_key_ec_path.as_deref())?;
        // PEM for EncodingKey::from_ec_pem (PKCS#8 form required)
        let ec_pem = ec_signing_key
            .to_pkcs8_pem(rsa::pkcs8::LineEnding::LF)
            .map_err(|e| ApiError::internal_with_context("OIDC EC pem serialize", &e))?;
        let ec_encoding_key = EncodingKey::from_ec_pem(ec_pem.as_bytes())
            .map_err(|e| ApiError::internal_with_context("OIDC EC encoding key", &e))?;

        // Derive x/y for JWK and DecodingKey::from_ec_components
        let ec_pub: P256PublicKey = ec_signing_key.public_key();
        let ec_affine_point = ec_pub.to_sec1_point(false);
        let x_bytes = ec_affine_point
            .x()
            .ok_or_else(|| ApiError::internal_with_context("OIDC EC point", &"P-256 affine x coordinate missing"))?;
        let y_bytes = ec_affine_point
            .y()
            .ok_or_else(|| ApiError::internal_with_context("OIDC EC point", &"P-256 affine y coordinate missing"))?;
        let x_b64 = URL_SAFE_NO_PAD.encode(x_bytes);
        let y_b64 = URL_SAFE_NO_PAD.encode(y_bytes);
        let ec_decoding_key = DecodingKey::from_ec_components(&x_b64, &y_b64)
            .map_err(|e| ApiError::internal_with_context("OIDC EC decoding key", &e))?;

        // Stable kid for EC key: SHA256(x || y) first 12 bytes base64url.
        let mut ec_hasher = Sha256::new();
        ec_hasher.update(x_bytes);
        ec_hasher.update(y_bytes);
        let ec_digest = ec_hasher.finalize();
        let ec_key_id = URL_SAFE_NO_PAD.encode(&ec_digest[..12]);

        Ok(Self {
            config,
            signing_key,
            encoding_key,
            decoding_key,
            key_id,
            ec_signing_key,
            ec_encoding_key,
            ec_decoding_key,
            ec_key_id,
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
                    .map_err(|e| ApiError::internal_with_context("OIDC signing key read", &e))?;
                return RsaPrivateKey::from_pkcs8_pem(&pem)
                    .map_err(|e| ApiError::internal_with_context("OIDC signing key parse", &e));
            }
        }

        info!(
            key_algorithm = %"RSA-2048",
            has_signing_key_path = path.is_some(),
            "Generating new signing key for builtin OIDC provider"
        );
        let mut rng = aes_gcm::aead::OsRng;
        let key =
            RsaPrivateKey::new(&mut rng, 2048).map_err(|e| ApiError::internal_with_context("OIDC RSA generate", &e))?;

        if let Some(p) = path {
            use rsa::pkcs8::EncodePrivateKey;
            if let Some(parent) = p.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let pem = key
                .to_pkcs8_pem(rsa::pkcs8::LineEnding::LF)
                .map_err(|e| ApiError::internal_with_context("OIDC RSA pem", &e))?;
            std::fs::write(p, pem.as_bytes())
                .map_err(|e| ApiError::internal_with_context("OIDC signing key write", &e))?;
            info!(key_path = %p.display(), key_algorithm = %"RSA-2048", "Persisted builtin OIDC signing key");
        } else {
            warn!(
                "BuiltinOidcProvider: signing_key_path not configured; key is ephemeral and \
                 all issued tokens will be invalidated on restart"
            );
        }

        let _ = SigningKey::<Sha256>::new(key.clone()); // 探测 SHA256 sign 可用
        Ok(key)
    }

    /// Loads a P-256 EC key from a PEM file, or generates a new ephemeral key in-process.
    /// Generated keys are NOT persisted by default — configure `signing_key_ec_path` to persist.
    fn load_or_generate_ec_key(path: Option<&Path>) -> Result<SecretKey, ApiError> {
        if let Some(p) = path {
            if p.exists() {
                let pem =
                    std::fs::read_to_string(p).map_err(|e| ApiError::internal_with_context("OIDC EC key read", &e))?;
                return SecretKey::from_pkcs8_pem(&pem)
                    .map_err(|e| ApiError::internal_with_context("OIDC EC key parse", &e));
            }
        }
        info!(key_algorithm = %"P-256", "Generating new EC signing key (ephemeral, not persisted by default)");
        // p256 0.14: use Generate trait with the getrandom CSPRNG (default feature).
        Ok(SecretKey::generate())
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
            registration_endpoint: None,
            revocation_endpoint: Some(format!("{}/_matrix/client/v3/oidc/revoke", issuer)),
            end_session_endpoint: Some(format!("{}/_matrix/client/v3/oidc/logout", issuer)),
            response_types_supported: vec!["code".to_string()],
            subject_types_supported: vec!["public".to_string()],
            id_token_signing_alg_values_supported: vec!["RS256".to_string(), "ES256".to_string()],
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

    /// 获取 JWKS（同时暴露 RSA-2048 (RS256) 和 P-256 (ES256) 公钥）。
    /// 客户端按 `alg` 协商选择验签密钥；RSA kid 保持兼容现有部署，EC kid 独立命名空间。
    pub fn get_jwks(&self) -> Result<Jwks, ApiError> {
        // RSA RS256 entry
        let rsa_pub: RsaPublicKey = self.signing_key.to_public_key();
        let n = URL_SAFE_NO_PAD.encode(rsa_pub.n().to_bytes_be());
        let e = URL_SAFE_NO_PAD.encode(rsa_pub.e().to_bytes_be());

        // EC P-256 ES256 entry
        let ec_pub: P256PublicKey = self.ec_signing_key.public_key();
        let ec_affine_point = ec_pub.to_sec1_point(false);
        let x_bytes = ec_affine_point.x().ok_or_else(|| ApiError::internal("P-256 affine x coordinate missing"))?;
        let y_bytes = ec_affine_point.y().ok_or_else(|| ApiError::internal("P-256 affine y coordinate missing"))?;
        let x = URL_SAFE_NO_PAD.encode(x_bytes);
        let y = URL_SAFE_NO_PAD.encode(y_bytes);

        Ok(Jwks {
            keys: vec![
                Jwk {
                    kty: "RSA".to_string(),
                    use_: "sig".to_string(),
                    kid: self.key_id.clone(),
                    alg: "RS256".to_string(),
                    n: Some(n),
                    e: Some(e),
                    crv: None,
                    x: None,
                    y: None,
                },
                Jwk {
                    kty: "EC".to_string(),
                    use_: "sig".to_string(),
                    kid: self.ec_key_id.clone(),
                    alg: "ES256".to_string(),
                    n: None,
                    e: None,
                    crv: Some("P-256".to_string()),
                    x: Some(x),
                    y: Some(y),
                },
            ],
        })
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

        info!(user_id = %user.id, client_id = %self.config.client_id, "OIDC authorization code generated");
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
            if !synapse_common::crypto::secure_compare(&computed, challenge) {
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

        // Lazy cleanup: on each grant, evict a batch of expired entries so the
        // in-memory HashMap does not grow unbounded. We do this here rather than
        // a background task because BuiltinOidcProvider is intentionally kept
        // stateless (no wiring dependency on infra shutdown token), making a
        // background task disproportionate for a development-only component.
        let now = Instant::now();
        let mut write = self.refresh_tokens.write().await;
        write.retain(|_token, rt| rt.expires_at > now);
        drop(write);

        // 查找 refresh token
        let token_data = self
            .refresh_tokens
            .read()
            .await
            .get(refresh_token)
            .cloned()
            .ok_or(ApiError::unauthorized("Invalid refresh_token".to_string()))?;

        // SSO-AUDIT: enforce expiry. The RefreshToken struct previously had
        // no expires_at field, so a token issued in 2024 would still be
        // accepted in 2026 if the in-memory map survived. Now that we set
        // expires_at on creation, this guard rejects expired tokens and
        // prevents zombie tokens from accumulating.
        if token_data.expires_at < now {
            self.refresh_tokens.write().await.remove(refresh_token);
            return Err(ApiError::unauthorized("Refresh token expired".to_string()));
        }

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
                tracing::error!(
                    error = %e,
                    username_present = !username.is_empty(),
                    has_password_hash = true,
                    "Invalid password_hash"
                );
                ApiError::internal("Authentication configuration error".to_string())
            })?;
            Argon2::default()
                .verify_password(password.as_bytes(), &parsed)
                .map_err(|_| ApiError::unauthorized("Invalid username or password".to_string()))?;
            return Ok(user);
        }

        if let Some(ref plain) = user.password {
            // SEC-03: 明文密码默认拒绝（生产止血），仅在配置显式
            // allow_plaintext_passwords=true 时放行（开发/测试逃生门）。
            if !self.config.allow_plaintext_passwords {
                tracing::error!(
                    username_present = !username.is_empty(),
                    has_plaintext_password = true,
                    "BuiltinOidcProvider user has plaintext password but allow_plaintext_passwords=false; \
                     refusing authentication. Migrate to password_hash (argon2 PHC)."
                );
                return Err(ApiError::unauthorized("Invalid username or password".to_string()));
            }
            warn!(
                username_present = !username.is_empty(),
                has_plaintext_password = true,
                "BuiltinOidcProvider user has plaintext password configured; migrate to password_hash (argon2 PHC) for production"
            );
            // 常量时间比较
            if synapse_common::crypto::secure_compare(plain, password) {
                return Ok(user);
            }
        }

        Err(ApiError::unauthorized("Invalid username or password".to_string()))
    }

    /// 计算 at_hash: BASE64URL( left-128-bit( SHA256(access_token) ) ).
    /// 算法无关 (RS256 与 ES256 同样按 RFC 7518 §3.1/§3.4 取 left-128-bit(SHA256(input)))。
    fn compute_at_hash(access_token: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(access_token.as_bytes());
        let digest = hasher.finalize();
        URL_SAFE_NO_PAD.encode(&digest[..16])
    }

    /// ID Token / Access Token 签名算法选择。当前双轨期统一 RS256（向后兼容）；
    /// 未来切换 ES256-only 时改这里即可（不影响 JWKS / discovery）。
    fn default_signing_algorithm() -> Algorithm {
        Algorithm::RS256
    }

    /// 生成 ID Token（默认 RS256，可按算法切换）
    fn generate_id_token(
        &self,
        user: &BuiltinOidcUser,
        client_id: &str,
        nonce: Option<&str>,
        access_token: &str,
    ) -> Result<String, ApiError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| ApiError::internal_with_context("clock", &e))?
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

        let alg = Self::default_signing_algorithm();
        let (encoding_key_ref, kid) = self.select_encoding_key(alg)?;
        let mut header = Header::new(alg);
        header.kid = Some(kid.to_string());
        encode(&header, &claims, encoding_key_ref)
            .map_err(|e| ApiError::internal_with_context("Failed to generate ID token", &e))
    }

    /// 生成 Access Token（默认 RS256，与 ID Token 算法一致防止 alg 混淆）
    fn generate_access_token(&self, user: &BuiltinOidcUser, scope: &str) -> Result<String, ApiError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| ApiError::internal_with_context("clock", &e))?
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

        let alg = Self::default_signing_algorithm();
        let (encoding_key_ref, kid) = self.select_encoding_key(alg)?;
        let mut header = Header::new(alg);
        header.kid = Some(kid.to_string());
        encode(&header, &claims, encoding_key_ref)
            .map_err(|e| ApiError::internal_with_context("Failed to generate access token", &e))
    }

    /// 生成 Refresh Token
    async fn generate_refresh_token(&self, user: &BuiltinOidcUser, scope: &str) -> Result<String, ApiError> {
        let token = Uuid::new_v4().to_string();
        let expires_at = Instant::now()
            + Duration::from_secs(
                (synapse_common::DEFAULT_REFRESH_TOKEN_EXPIRY_SECS as u64)
                    .max(86400), // at least 1 day
            );

        let refresh_token = RefreshToken {
            user_id: user.id.clone(),
            client_id: self.config.client_id.clone(),
            scope: scope.to_string(),
            created_at: Instant::now(),
            expires_at,
        };

        self.refresh_tokens.write().await.insert(token.clone(), refresh_token);

        Ok(token)
    }

    /// 选择 signing key (encoding + kid) 按算法。返回 `(&EncodingKey, &str)`。
    /// 双轨期内 RS256 + ES256 双开；其他算法拒绝。
    fn select_encoding_key(&self, alg: Algorithm) -> Result<(&EncodingKey, &str), ApiError> {
        match alg {
            Algorithm::RS256 => Ok((&self.encoding_key, &self.key_id)),
            Algorithm::ES256 => Ok((&self.ec_encoding_key, &self.ec_key_id)),
            other => Err(ApiError::internal_with_context("OIDC unsupported signing alg", &format!("{other:?}"))),
        }
    }

    /// 选择 decoding key (DecodingKey + kid) 按算法。
    fn select_decoding_key(&self, alg: Algorithm) -> Result<(&DecodingKey, &str), ApiError> {
        match alg {
            Algorithm::RS256 => Ok((&self.decoding_key, &self.key_id)),
            Algorithm::ES256 => Ok((&self.ec_decoding_key, &self.ec_key_id)),
            other => Err(ApiError::internal_with_context("OIDC unsupported verify alg", &format!("{other:?}"))),
        }
    }

    /// 验证 Access Token（按 JWT header.alg 自动选 RSA / EC decoding key）。
    /// 同时支持 RS256（默认签发）与 ES256（双轨期外部签发）。
    fn verify_access_token(&self, token: &str) -> Result<AccessTokenClaims, ApiError> {
        // 解析 header.alg 用于选 decoding key
        let alg = Self::peek_jwt_algorithm(token)?;
        let (decoding_key_ref, _kid) = self.select_decoding_key(alg)?;

        let mut validation = Validation::new(alg);
        validation.set_audience(&[&self.config.issuer]);
        validation.set_issuer(&[&self.config.issuer]);
        let claims = decode::<AccessTokenClaims>(token, decoding_key_ref, &validation)
            .map_err(|e| ApiError::unauthorized(format!("Invalid token: {}", e)))?
            .claims;

        Ok(claims)
    }

    /// 从 JWT header 解析 `alg` 字段。
    fn peek_jwt_algorithm(token: &str) -> Result<Algorithm, ApiError> {
        let header_b64 = token.split('.').next().unwrap_or("");
        let header_bytes = URL_SAFE_NO_PAD
            .decode(header_b64)
            .map_err(|e| ApiError::unauthorized(format!("Invalid JWT header: {e}")))?;
        let header: serde_json::Value = serde_json::from_slice(&header_bytes)
            .map_err(|e| ApiError::unauthorized(format!("Invalid JWT header JSON: {e}")))?;
        let alg_str = header.get("alg").and_then(|v| v.as_str()).unwrap_or("");
        match alg_str {
            "RS256" => Ok(Algorithm::RS256),
            "ES256" => Ok(Algorithm::ES256),
            other => Err(ApiError::unauthorized(format!("Unsupported JWT alg: {other}"))),
        }
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

        info!(user_id = %user_id, "OIDC logout revoked sessions");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use synapse_common::BuiltinOidcConfig;
    use synapse_common::BuiltinOidcUser;

    fn test_config() -> Arc<BuiltinOidcConfig> {
        Arc::new(BuiltinOidcConfig {
            enabled: true,
            issuer: "https://synapse.test".to_string(),
            client_id: "test-client".to_string(),
            allow_redirect_uris: vec!["https://app.test/callback".to_string()],
            allow_client_ids: vec!["test-client".to_string()],
            users: vec![BuiltinOidcUser {
                id: "@testuser:synapse.test".to_string(),
                username: "testuser".to_string(),
                password: Some("password123".to_string()),
                password_hash: None,
                email: "test@example.com".to_string(),
                displayname: Some("Test User".to_string()),
            }],
            allow_plaintext_passwords: true,
            signing_key_path: None,
            signing_key_ec_path: None,
        })
    }

    fn create_provider() -> BuiltinOidcProvider {
        BuiltinOidcProvider::new(test_config()).expect("Failed to create provider")
    }

    #[test]
    fn test_hash_token_deterministic() {
        let token1 = "access_token_value";
        let token2 = "access_token_value";
        let hash1 = BuiltinOidcProvider::compute_at_hash(token1);
        let hash2 = BuiltinOidcProvider::compute_at_hash(token2);
        assert_eq!(hash1, hash2);

        // Different tokens should produce different hashes
        let token3 = "different_token";
        let hash3 = BuiltinOidcProvider::compute_at_hash(token3);
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_compute_at_hash_length() {
        let hash = BuiltinOidcProvider::compute_at_hash("test_token");
        // SHA256 left-128-bit base64url: 16 bytes = 22 base64url chars (no padding)
        assert_eq!(hash.len(), 22);
    }

    #[test]
    fn test_get_discovery_document() {
        let provider = create_provider();
        let doc = provider.get_discovery_document();

        assert_eq!(doc.issuer, "https://synapse.test");
        assert!(doc.authorization_endpoint.contains("/oidc/authorize"));
        assert!(doc.token_endpoint.contains("/oidc/token"));
        assert!(doc.userinfo_endpoint.contains("/oidc/userinfo"));
        assert!(doc.jwks_uri.contains("/.well-known/jwks.json"));
        assert!(doc.response_types_supported.contains(&"code".to_string()));
        assert!(doc.id_token_signing_alg_values_supported.contains(&"RS256".to_string()));
        assert!(doc.id_token_signing_alg_values_supported.contains(&"ES256".to_string()));
        assert!(doc.code_challenge_methods_supported.contains(&"S256".to_string()));
    }

    #[test]
    fn test_get_jwks() {
        let provider = create_provider();
        let jwks = provider.get_jwks().expect("ES256 key derivation failed");

        // Dual key JWKS: RSA RS256 + EC P-256 ES256 (RFC 7518 §3.4).
        assert_eq!(jwks.keys.len(), 2);

        let rsa_jwk = &jwks.keys[0];
        assert_eq!(rsa_jwk.kty, "RSA");
        assert_eq!(rsa_jwk.use_, "sig");
        assert_eq!(rsa_jwk.alg, "RS256");
        assert!(!rsa_jwk.kid.is_empty());
        let rsa_n = rsa_jwk.n.as_ref().expect("RSA jwk must have n");
        let rsa_e = rsa_jwk.e.as_ref().expect("RSA jwk must have e");
        assert!(!rsa_n.is_empty(), "RSA n must be present");
        assert!(!rsa_e.is_empty(), "RSA e must be present");
        assert!(rsa_jwk.crv.is_none(), "RSA jwk must not have EC crv");
        assert!(rsa_jwk.x.is_none(), "RSA jwk must not have EC x");
        assert!(rsa_jwk.y.is_none(), "RSA jwk must not have EC y");

        let ec_jwk = &jwks.keys[1];
        assert_eq!(ec_jwk.kty, "EC");
        assert_eq!(ec_jwk.use_, "sig");
        assert_eq!(ec_jwk.alg, "ES256");
        assert!(!ec_jwk.kid.is_empty());
        assert_ne!(rsa_jwk.kid, ec_jwk.kid, "RSA and EC kids must differ");
        let crv = ec_jwk.crv.as_ref().expect("EC jwk must have crv");
        let x = ec_jwk.x.as_ref().expect("EC jwk must have x");
        let y = ec_jwk.y.as_ref().expect("EC jwk must have y");
        assert_eq!(crv, "P-256");
        // P-256 affine coordinates are 32 bytes each = 43 base64url chars (no padding).
        assert_eq!(x.len(), 43, "P-256 x must be 32 bytes base64url-encoded");
        assert_eq!(y.len(), 43, "P-256 y must be 32 bytes base64url-encoded");
        assert!(ec_jwk.n.is_none(), "EC jwk must not have RSA n");
        assert!(ec_jwk.e.is_none(), "EC jwk must not have RSA e");
    }

    #[tokio::test]
    async fn test_authorize_creates_session() {
        let provider = create_provider();
        let request = AuthorizeRequest {
            client_id: "test-client".to_string(),
            redirect_uri: "https://app.test/callback".to_string(),
            scope: "openid profile".to_string(),
            state: "state123".to_string(),
            nonce: Some("nonce456".to_string()),
            code_verifier: None,
            username: "testuser".to_string(),
            password: "password123".to_string(),
        };

        let code = provider.authorize(request).await.unwrap();
        assert!(!code.is_empty());

        // Verify session was stored
        let sessions = provider.auth_sessions.read().await;
        assert!(sessions.contains_key(&code));
        let session = &sessions[&code];
        assert_eq!(session.client_id, "test-client");
        assert_eq!(session.redirect_uri, "https://app.test/callback");
        assert_eq!(session.scope, "openid profile");
        assert_eq!(session.state, "state123");
        assert_eq!(session.user_id, "@testuser:synapse.test");
    }

    #[tokio::test]
    async fn test_authorize_invalid_user() {
        let provider = create_provider();
        let request = AuthorizeRequest {
            client_id: "test-client".to_string(),
            redirect_uri: "https://app.test/callback".to_string(),
            scope: "openid".to_string(),
            state: "state".to_string(),
            nonce: None,
            code_verifier: None,
            username: "nonexistent".to_string(),
            password: "wrong".to_string(),
        };

        let result = provider.authorize(request).await;
        assert!(result.is_err());
    }

    // SEC-03: 默认（未显式开启 allow_plaintext_passwords）时，即使明文密码
    // 正确也必须拒绝认证。
    #[tokio::test]
    async fn sec03_plaintext_password_rejected_by_default() {
        let mut config = (*test_config()).clone();
        config.allow_plaintext_passwords = false;
        let provider = BuiltinOidcProvider::new(Arc::new(config)).expect("Failed to create provider");
        let request = AuthorizeRequest {
            client_id: "test-client".to_string(),
            redirect_uri: "https://app.test/callback".to_string(),
            scope: "openid".to_string(),
            state: "state".to_string(),
            nonce: None,
            code_verifier: None,
            username: "testuser".to_string(),
            password: "password123".to_string(),
        };

        let result = provider.authorize(request).await;
        assert!(result.is_err(), "plaintext password must be rejected when allow_plaintext_passwords=false");
    }

    // SEC-03: 显式开启后明文密码可用（开发/测试逃生门）
    #[tokio::test]
    async fn sec03_plaintext_password_allowed_when_explicitly_enabled() {
        let provider = create_provider(); // test_config 显式 allow_plaintext_passwords: true
        let request = AuthorizeRequest {
            client_id: "test-client".to_string(),
            redirect_uri: "https://app.test/callback".to_string(),
            scope: "openid".to_string(),
            state: "state".to_string(),
            nonce: None,
            code_verifier: None,
            username: "testuser".to_string(),
            password: "password123".to_string(),
        };

        assert!(provider.authorize(request).await.is_ok());
    }

    #[tokio::test]
    async fn test_authorize_invalid_client_id() {
        let provider = create_provider();
        let request = AuthorizeRequest {
            client_id: "bad-client".to_string(),
            redirect_uri: "https://app.test/callback".to_string(),
            scope: "openid".to_string(),
            state: "state".to_string(),
            nonce: None,
            code_verifier: None,
            username: "testuser".to_string(),
            password: "password123".to_string(),
        };

        let result = provider.authorize(request).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_handle_authorization_code_grant_consumes_code() {
        let provider = create_provider();

        // First authorize
        let request = AuthorizeRequest {
            client_id: "test-client".to_string(),
            redirect_uri: "https://app.test/callback".to_string(),
            scope: "openid".to_string(),
            state: "state".to_string(),
            nonce: None,
            code_verifier: None,
            username: "testuser".to_string(),
            password: "password123".to_string(),
        };
        let code = provider.authorize(request).await.unwrap();

        // Exchange code for token
        let token_request = OidcTokenRequest {
            grant_type: "authorization_code".to_string(),
            code: Some(code.clone()),
            redirect_uri: Some("https://app.test/callback".to_string()),
            client_id: Some("test-client".to_string()),
            code_verifier: None,
            refresh_token: None,
            scope: None,
        };

        let result = provider.token(token_request).await;
        assert!(result.is_ok());
        let token_response = result.unwrap();
        assert_eq!(token_response.token_type, "Bearer");
        assert!(!token_response.access_token.is_empty());
        assert!(!token_response.id_token.is_empty());
        assert!(token_response.refresh_token.is_some());

        // Code should be consumed — second attempt should fail
        let token_request2 = OidcTokenRequest {
            grant_type: "authorization_code".to_string(),
            code: Some(code),
            redirect_uri: Some("https://app.test/callback".to_string()),
            client_id: Some("test-client".to_string()),
            code_verifier: None,
            refresh_token: None,
            scope: None,
        };
        let result2 = provider.token(token_request2).await;
        assert!(result2.is_err());
    }

    #[tokio::test]
    async fn test_handle_refresh_token_grant() {
        let provider = create_provider();

        // Authorize and get initial tokens
        let request = AuthorizeRequest {
            client_id: "test-client".to_string(),
            redirect_uri: "https://app.test/callback".to_string(),
            scope: "openid".to_string(),
            state: "state".to_string(),
            nonce: None,
            code_verifier: None,
            username: "testuser".to_string(),
            password: "password123".to_string(),
        };
        let code = provider.authorize(request).await.unwrap();

        let token_request = OidcTokenRequest {
            grant_type: "authorization_code".to_string(),
            code: Some(code),
            redirect_uri: Some("https://app.test/callback".to_string()),
            client_id: Some("test-client".to_string()),
            code_verifier: None,
            refresh_token: None,
            scope: None,
        };
        let initial = provider.token(token_request).await.unwrap();
        let refresh_token = initial.refresh_token.unwrap();

        // Use refresh token
        let refresh_request = OidcTokenRequest {
            grant_type: "refresh_token".to_string(),
            code: None,
            redirect_uri: None,
            client_id: None,
            code_verifier: None,
            refresh_token: Some(refresh_token),
            scope: None,
        };
        let refreshed = provider.token(refresh_request).await.unwrap();
        assert!(!refreshed.access_token.is_empty());
        assert!(!refreshed.id_token.is_empty());
        assert_eq!(refreshed.token_type, "Bearer");
    }

    #[tokio::test]
    async fn test_logout_revokes_user_sessions() {
        let provider = create_provider();

        // Create a session
        let request = AuthorizeRequest {
            client_id: "test-client".to_string(),
            redirect_uri: "https://app.test/callback".to_string(),
            scope: "openid".to_string(),
            state: "state".to_string(),
            nonce: None,
            code_verifier: None,
            username: "testuser".to_string(),
            password: "password123".to_string(),
        };
        let code = provider.authorize(request).await.unwrap();

        // Exchange for tokens to get a refresh token
        let token_request = OidcTokenRequest {
            grant_type: "authorization_code".to_string(),
            code: Some(code),
            redirect_uri: Some("https://app.test/callback".to_string()),
            client_id: Some("test-client".to_string()),
            code_verifier: None,
            refresh_token: None,
            scope: None,
        };
        let tokens = provider.token(token_request).await.unwrap();
        let refresh_token = tokens.refresh_token.unwrap();

        // Logout
        provider.logout(Some(&refresh_token)).await.unwrap();

        // Refresh token should be gone
        let refresh_map = provider.refresh_tokens.read().await;
        assert!(!refresh_map.contains_key(&refresh_token));
    }

    /// ES256 sign+verify round-trip: explicitly produce an ES256-signed JWT
    /// using the provider's EC private key, then verify_access_token must accept it.
    #[test]
    fn test_issue_and_verify_es256_access_token() {
        use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};

        let provider = create_provider();
        // Encode a JWT manually using the provider's EC private key (via PEM form).
        let ec_pem = provider.ec_signing_key.to_pkcs8_pem(rsa::pkcs8::LineEnding::LF).expect("EC PEM serialize");
        let ec_encoding = EncodingKey::from_ec_pem(ec_pem.as_bytes()).expect("EC encoding key");

        let now = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock").as_secs() as i64;
        let claims = AccessTokenClaims {
            iss: provider.config.issuer.clone(),
            sub: "@alice:synapse.test".to_string(),
            aud: vec![provider.config.issuer.clone()],
            exp: now + OIDC_TOKEN_EXPIRY_SECS,
            iat: now,
            jti: Uuid::new_v4().to_string(),
            scope: "openid".to_string(),
        };

        let mut header = Header::new(Algorithm::ES256);
        header.kid = Some(provider.ec_key_id.clone());
        let token = encode(&header, &claims, &ec_encoding).expect("ES256 encode");

        let parsed = provider.verify_access_token(&token).expect("ES256 verify must accept");
        assert_eq!(parsed.sub, "@alice:synapse.test");
        assert_eq!(parsed.scope, "openid");
    }

    /// ES256 discovery advertises both algorithms and JWKS exposes both keys.
    #[test]
    fn test_discovery_and_jwks_advertise_both_rs256_and_es256() {
        let provider = create_provider();

        // Discovery document advertises both
        let discovery = provider.get_discovery_document();
        assert!(discovery.id_token_signing_alg_values_supported.contains(&"RS256".to_string()));
        assert!(discovery.id_token_signing_alg_values_supported.contains(&"ES256".to_string()));

        // JWKS contains both keys with distinct kids
        let jwks = provider.get_jwks().expect("JWKS");
        assert_eq!(jwks.keys.len(), 2);
        let rsa_jwk = jwks.keys.iter().find(|k| k.alg == "RS256").expect("RS256 in JWKS");
        let ec_jwk = jwks.keys.iter().find(|k| k.alg == "ES256").expect("ES256 in JWKS");
        assert_eq!(rsa_jwk.kty, "RSA");
        assert_eq!(ec_jwk.kty, "EC");
        assert_ne!(rsa_jwk.kid, ec_jwk.kid);
    }

    /// EC key PEM persistence round-trip: serialize, parse via load_or_generate_ec_key path,
    /// and verify the resulting x coordinate matches.
    #[test]
    fn test_ec_key_pem_persistence_round_trip() {
        use p256::pkcs8::DecodePrivateKey;

        let provider = create_provider();
        let pem = provider.ec_signing_key.to_pkcs8_pem(rsa::pkcs8::LineEnding::LF).expect("EC PEM serialize");

        let reloaded = SecretKey::from_pkcs8_pem(&pem).expect("EC PEM reload");
        let original_pub = provider.ec_signing_key.public_key();
        let reloaded_pub = reloaded.public_key();
        assert_eq!(
            original_pub.to_sec1_point(false).x().map(|a| a.as_slice()),
            reloaded_pub.to_sec1_point(false).x().map(|a| a.as_slice()),
            "EC x must round-trip"
        );
    }

    #[test]
    fn test_oidc_discovery_document_serialization() {
        let doc = OidcDiscoveryDocument {
            issuer: "https://test".to_string(),
            authorization_endpoint: "https://test/authorize".to_string(),
            token_endpoint: "https://test/token".to_string(),
            userinfo_endpoint: "https://test/userinfo".to_string(),
            jwks_uri: "https://test/jwks".to_string(),
            registration_endpoint: None,
            revocation_endpoint: None,
            end_session_endpoint: None,
            response_types_supported: vec!["code".to_string()],
            subject_types_supported: vec!["public".to_string()],
            id_token_signing_alg_values_supported: vec!["RS256".to_string()],
            scopes_supported: vec!["openid".to_string()],
            token_endpoint_auth_methods_supported: vec!["client_secret_basic".to_string()],
            claims_supported: vec!["sub".to_string()],
            code_challenge_methods_supported: vec!["S256".to_string()],
        };

        let json = serde_json::to_string(&doc).unwrap();
        assert!(json.contains("issuer"));
        assert!(json.contains("authorization_endpoint"));
    }

    /// SSO-AUDIT: refresh tokens now have an `expires_at` field. A token
    /// with `expires_at` in the past must be rejected outright and removed
    /// from the in-memory map. The lazy cleanup in handle_refresh_token_grant
    /// also evicts other expired entries on every grant.
    #[tokio::test]
    async fn test_refresh_token_expires_at_is_enforced() {
        let provider = create_provider();
        let now = Instant::now();
        let valid_user_id = "@testuser:synapse.test";

        // Insert a long-expired entry directly (bypassing the public API)
        // so we can exercise the expiry guard deterministically.
        let stale = RefreshToken {
            user_id: valid_user_id.to_string(),
            client_id: "test-client".to_string(),
            scope: "openid".to_string(),
            created_at: now - Duration::from_secs(1_000_000),
            expires_at: now - Duration::from_secs(60),
        };
        provider.refresh_tokens.write().await.insert("stale-token".to_string(), stale);

        // Insert a fresh entry that has not yet expired.
        let fresh = RefreshToken {
            user_id: valid_user_id.to_string(),
            client_id: "test-client".to_string(),
            scope: "openid".to_string(),
            created_at: now,
            expires_at: now + Duration::from_secs(86_400),
        };
        provider.refresh_tokens.write().await.insert("fresh-token".to_string(), fresh);

        // First refresh call evicts the stale entry via retain().
        let request = OidcTokenRequest {
            grant_type: "refresh_token".to_string(),
            code: None,
            redirect_uri: None,
            client_id: None,
            code_verifier: None,
            refresh_token: Some("fresh-token".to_string()),
            scope: None,
        };
        let _ = provider.token(request).await.expect("fresh token must succeed");

        // Stale entry should be gone after the lazy cleanup.
        let map = provider.refresh_tokens.read().await;
        assert!(!map.contains_key("stale-token"), "stale token must be evicted by lazy cleanup");
        assert!(map.contains_key("fresh-token"), "fresh token must remain in the map");
        drop(map);

        // Direct expiry check: a refresh call with an expired token returns
        // an error AND removes it from the map.
        let stale_again = RefreshToken {
            user_id: valid_user_id.to_string(),
            client_id: "test-client".to_string(),
            scope: "openid".to_string(),
            created_at: now - Duration::from_secs(1_000_000),
            expires_at: now - Duration::from_secs(60),
        };
        provider.refresh_tokens.write().await.insert("stale-token-2".to_string(), stale_again);
        let stale_request = OidcTokenRequest {
            grant_type: "refresh_token".to_string(),
            code: None,
            redirect_uri: None,
            client_id: None,
            code_verifier: None,
            refresh_token: Some("stale-token-2".to_string()),
            scope: None,
        };
        let result = provider.token(stale_request).await;
        assert!(result.is_err(), "stale token refresh must fail");
        let map = provider.refresh_tokens.read().await;
        assert!(!map.contains_key("stale-token-2"), "stale token must be removed on expiry check");
    }
}
