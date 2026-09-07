//! 外部 OIDC 客户端服务
//!
//! 通过 OIDC Discovery URL 对接外部身份提供商（如 Keycloak、Auth0、Okta 等），
//! 实现 Authorization Code Flow + PKCE 认证流程。
//!
//! # 适用场景
//!
//! - **生产环境**: 对接企业级身份提供商
//! - **多租户场景**: 需要统一身份管理的部署
//!
//! # 与 BuiltinOidcProvider 的关系
//!
//! - `OidcService`: 外部 IdP 客户端模式（本服务）
//! - `BuiltinOidcProvider`: 内置 Provider 模式，自身充当 OIDC Provider
//!
//! 两者不应同时启用。启动时会检测冲突并发出警告。

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::Duration;
use synapse_common::config::OidcConfig;
use synapse_common::error::ApiError;
use tokio::sync::RwLock;
use tracing::debug;

/// The `OidcDiscoveryDocument` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcDiscoveryDocument {
    /// The `issuer` field.
    pub issuer: String,
    /// The `authorization_endpoint` field.
    pub authorization_endpoint: String,
    /// The `token_endpoint` field.
    pub token_endpoint: String,
    /// The `userinfo_endpoint` field.
    pub userinfo_endpoint: String,
    /// The `jwks_uri` field.
    pub jwks_uri: String,
    /// The `response_types_supported` field.
    pub response_types_supported: Vec<String>,
    /// The `subject_types_supported` field.
    pub subject_types_supported: Vec<String>,
    /// The `id_token_signing_alg_values_supported` field.
    pub id_token_signing_alg_values_supported: Vec<String>,
    /// The `scopes_supported` field.
    pub scopes_supported: Option<Vec<String>>,
    /// The `claims_supported` field.
    pub claims_supported: Option<Vec<String>>,
}

/// The `OidcTokenResponse` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcTokenResponse {
    /// The `access_token` field.
    pub access_token: String,
    /// The `token_type` field.
    pub token_type: String,
    /// The `expires_in` field.
    pub expires_in: Option<i64>,
    /// The `refresh_token` field.
    pub refresh_token: Option<String>,
    /// The `id_token` field.
    pub id_token: Option<String>,
    /// The `scope` field.
    pub scope: Option<String>,
}

/// The `OidcUserInfo` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcUserInfo {
    /// The `sub` field.
    pub sub: String,
    /// The `name` field.
    pub name: Option<String>,
    /// The `given_name` field.
    pub given_name: Option<String>,
    /// The `family_name` field.
    pub family_name: Option<String>,
    /// The `preferred_username` field.
    pub preferred_username: Option<String>,
    /// The `email` field.
    pub email: Option<String>,
    /// The `email_verified` field.
    pub email_verified: Option<bool>,
    /// The `picture` field.
    pub picture: Option<String>,
    /// The `locale` field.
    pub locale: Option<String>,
}

/// The `OidcAuthRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcAuthRequest {
    /// The `url` field.
    pub url: String,
    /// The `state` field.
    pub state: String,
    /// The `nonce` field.
    pub nonce: String,
    /// The `code_verifier` field.
    pub code_verifier: String,
}

/// The `OidcUser` struct.
#[derive(Debug, Clone)]
pub struct OidcUser {
    /// The `subject` field.
    pub subject: String,
    /// The `localpart` field.
    pub localpart: String,
    /// The `displayname` field.
    pub displayname: Option<String>,
    /// The `email` field.
    pub email: Option<String>,
}

/// The `OidcJwks` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcJwks {
    /// The `keys` field.
    pub keys: Vec<OidcJwk>,
}

/// The `OidcJwk` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcJwk {
    /// The `kty` field.
    pub kty: String,
    #[serde(rename = "use")]
    /// The `use_` field.
    pub use_: Option<String>,
    /// The `kid` field.
    pub kid: Option<String>,
    /// The `alg` field.
    pub alg: Option<String>,
    /// The `n` field.
    pub n: Option<String>,
    /// The `e` field.
    pub e: Option<String>,
    #[serde(rename = "crv")]
    /// The `crv` field.
    pub crv: Option<String>,
    /// The `x` field.
    pub x: Option<String>,
    /// The `y` field.
    pub y: Option<String>,
}

/// The `OidcService` struct.
pub struct OidcService {
    config: Arc<OidcConfig>,
    http_client: reqwest::Client,
    discovery: RwLock<Option<OidcDiscoveryDocument>>,
    jwks: RwLock<Option<OidcJwks>>,
}

impl OidcService {
    /// See [`new`].
    /// See [`new`].
    pub fn new(config: Arc<OidcConfig>) -> Self {
        let http_client =
            reqwest::Client::builder().timeout(Duration::from_secs(config.timeout)).build().unwrap_or_else(|e| {
                // F-1: builder 失败不再静默退化，记录 warn 并回退共享默认 client
                tracing::warn!(error = %e, "Failed to build OIDC HTTP client, using shared default");
                synapse_common::http_client::default_client()
            });

        Self { config, http_client, discovery: RwLock::new(None), jwks: RwLock::new(None) }
    }

    /// See [`is_enabled`].
    /// See [`is_enabled`].
    pub fn is_enabled(&self) -> bool {
        self.config.is_enabled()
    }

    /// Returns the configured OIDC issuer URL. Used by MAS token validation
    /// to look up user mappings in `oidc_user_mapping` (keyed by issuer).
    pub fn issuer(&self) -> &str {
        &self.config.issuer
    }

    /// MSC3861: Verify a MAS-issued access token (JWT) against the OIDC
    /// provider's JWKS and return the decoded claims.
    ///
    /// Unlike `validate_id_token`, this method:
    /// - Does NOT validate the `nonce` claim (access tokens have no nonce).
    /// - Does NOT validate the `aud` claim against `client_id` — MAS access
    ///   tokens may target the homeserver as audience, not the OIDC client.
    ///   We still validate `iss` against the configured issuer.
    ///
    /// Returns the decoded JWT claims as a `serde_json::Value` so the caller
    /// can extract `sub`, `device_id`, and other MAS-specific claims.
    pub async fn verify_access_token(&self, token: &str) -> Result<serde_json::Value, String> {
        let header_bytes = URL_SAFE_NO_PAD
            .decode(token.split('.').next().unwrap_or(""))
            .map_err(|e| format!("Invalid access token header base64: {e}"))?;

        let header: serde_json::Value =
            serde_json::from_slice(&header_bytes).map_err(|e| format!("Invalid access token header JSON: {e}"))?;

        let kid = header.get("kid").and_then(|v| v.as_str());
        let alg_str = header.get("alg").and_then(|v| v.as_str()).unwrap_or("RS256");

        let algorithm = match alg_str {
            "RS256" => Algorithm::RS256,
            "RS384" => Algorithm::RS384,
            "RS512" => Algorithm::RS512,
            "ES256" => Algorithm::ES256,
            "ES384" => Algorithm::ES384,
            "EdDSA" => Algorithm::EdDSA,
            _ => return Err(format!("Unsupported access token algorithm: {alg_str}")),
        };

        let jwks = self.fetch_jwks().await?;

        let matching_key =
            jwks.keys.iter().find(|k| if let Some(ref key_kid) = k.kid { kid == Some(key_kid.as_str()) } else { true });

        let key = matching_key.ok_or_else(|| {
            tracing::error!(
                kid = ?kid,
                issuer = %self.config.issuer,
                "No matching JWKS key found for MAS access token kid; rejecting"
            );
            "access token signature key (kid) not found in JWKS".to_string()
        })?;

        let decoding_key = if key.kty == "RSA" {
            match (&key.n, &key.e) {
                (Some(n), Some(e)) => {
                    DecodingKey::from_rsa_components(n, e).map_err(|e| format!("Invalid RSA key: {e}"))?
                }
                _ => return Err("RSA key missing n/e components".to_string()),
            }
        } else if key.kty == "EC" {
            match (&key.crv, &key.x, &key.y) {
                (Some(_), Some(x), Some(y)) => {
                    DecodingKey::from_ec_components(x, y).map_err(|e| format!("Invalid EC key: {e}"))?
                }
                _ => return Err("EC key missing crv/x/y components".to_string()),
            }
        } else if key.kty == "OKP" {
            match (&key.crv, &key.x) {
                (Some(_), Some(x)) => {
                    DecodingKey::from_ed_components(x).map_err(|e| format!("Invalid EdDSA key: {e}"))?
                }
                _ => return Err("OKP key missing crv/x components".to_string()),
            }
        } else {
            return Err(format!("Unsupported key type: {}", key.kty));
        };

        // Validate issuer and expiry, but NOT audience (MAS access tokens
        // target the homeserver, not the OIDC client_id).
        let mut validation = Validation::new(algorithm);
        validation.set_issuer(&[&self.config.issuer]);
        validation.validate_exp = true;
        validation.validate_nbf = false;
        validation.validate_aud = false;

        let token_data = decode::<serde_json::Value>(token, &decoding_key, &validation)
            .map_err(|e| format!("MAS access token JWT signature verification failed: {e}"))?;

        debug!("MAS access token JWT signature verified successfully (kid={:?})", kid);

        Ok(token_data.claims)
    }

    /// See [`discover`].
    /// See [`discover`].
    pub async fn discover(&self) -> Result<OidcDiscoveryDocument, ApiError> {
        {
            let read = self.discovery.read().await;
            if let Some(ref discovery) = *read {
                return Ok(discovery.clone());
            }
        }

        let discovery_url = format!("{}/.well-known/openid-configuration", self.config.issuer);

        debug!(discovery_url_configured = !discovery_url.is_empty(), "Fetching OIDC discovery document");

        let response = self
            .http_client
            .get(&discovery_url)
            .send()
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to fetch discovery document", &e))?;

        if !response.status().is_success() {
            return Err(ApiError::internal_with_context("Discovery request failed", &response.status()));
        }

        let discovery: OidcDiscoveryDocument = response
            .json()
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to parse discovery document", &e))?;

        {
            let mut write = self.discovery.write().await;
            *write = Some(discovery.clone());
        }
        Ok(discovery)
    }

    /// See [`get_authorization_url`].
    pub async fn get_authorization_url(
        &self,
        state: &str,
        redirect_uri: &str,
        code_challenge: Option<&str>,
        code_challenge_method: Option<&str>,
    ) -> Result<String, ApiError> {
        let scope = self.config.scopes.join(" ");

        let default_auth = format!("{}/authorize", self.config.issuer);
        let auth_endpoint = {
            let read = self.discovery.read().await;
            self.config
                .authorization_endpoint
                .as_ref()
                .or_else(|| read.as_ref().map(|d| &d.authorization_endpoint))
                .cloned()
        };

        let auth_url = auth_endpoint.unwrap_or(default_auth);

        let mut url = url::Url::parse(&auth_url)
            .map_err(|e| ApiError::internal_with_context("Invalid OIDC authorization endpoint", &e))?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("client_id", &self.config.client_id);
            query.append_pair("response_type", "code");
            query.append_pair("scope", &scope);
            query.append_pair("redirect_uri", redirect_uri);
            query.append_pair("state", state);

            // PKCE support
            if let Some(challenge) = code_challenge {
                query.append_pair("code_challenge", challenge);
                query.append_pair("code_challenge_method", code_challenge_method.unwrap_or("S256"));
            }
        }

        Ok(url.to_string())
    }

    /// Generate PKCE code verifier and challenge
    pub fn generate_pkce() -> (String, String) {
        use rand::Rng;
        let mut rng = rand::rng();

        // PKCE charset as bytes for indexing
        const PKCE_CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";

        // Generate code_verifier (43-128 characters)
        let verifier_len = rng.random_range(43..=128);
        let code_verifier: String = (0..verifier_len)
            .map(|_| {
                let idx = rng.random_range(0..PKCE_CHARSET.len());
                PKCE_CHARSET[idx] as char
            })
            .collect();

        // Generate code_challenge (SHA256 hash base64url encoded)
        let mut hasher = Sha256::new();
        hasher.update(code_verifier.as_bytes());
        let hash = hasher.finalize();
        let code_challenge = Self::base64url_encode(&hash);

        (code_verifier, code_challenge)
    }

    fn base64url_encode(data: &[u8]) -> String {
        URL_SAFE_NO_PAD.encode(data)
    }

    /// Verify PKCE code verifier (constant-time comparison to mitigate timing attacks)
    pub fn verify_pkce(code_verifier: &str, code_challenge: &str) -> bool {
        let mut hasher = Sha256::new();
        hasher.update(code_verifier.as_bytes());
        let hash = hasher.finalize();
        let computed = Self::base64url_encode(&hash);
        synapse_common::crypto::secure_compare(&computed, code_challenge)
    }

    /// See [`exchange_code`].
    pub async fn exchange_code(
        &self,
        code: &str,
        redirect_uri: &str,
        code_verifier: Option<&str>,
        nonce: Option<&str>,
    ) -> Result<OidcTokenResponse, ApiError> {
        let default_token = format!("{}/token", self.config.issuer);
        let token_endpoint = {
            let read = self.discovery.read().await;
            self.config
                .token_endpoint
                .as_ref()
                .or_else(|| read.as_ref().map(|d| &d.token_endpoint))
                .cloned()
                .unwrap_or(default_token)
        };

        let mut params = vec![
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("client_id", self.config.client_id.as_str()),
        ];
        if let Some(code_verifier) = code_verifier {
            params.push(("code_verifier", code_verifier));
        }

        let mut request = self.http_client.post(token_endpoint).form(&params);

        if let Some(ref secret) = self.config.client_secret {
            request = request.basic_auth(&self.config.client_id, Some(secret));
        }

        let response =
            request.send().await.map_err(|e| ApiError::internal_with_context("Token exchange failed", &e))?;

        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(ApiError::internal_with_context("Token exchange failed", &body));
        }

        let token_response: OidcTokenResponse =
            response.json().await.map_err(|e| ApiError::internal_with_context("Failed to parse token response", &e))?;

        if let Some(ref id_token) = token_response.id_token {
            if let Err(e) = self.validate_id_token(id_token, nonce).await {
                tracing::warn!(
                    error = %e,
                    issuer = %self.config.issuer,
                    client_id = %self.config.client_id,
                    has_id_token = true,
                    nonce_provided = nonce.is_some(),
                    "OIDC ID token validation failed"
                );
            }
        }

        Ok(token_response)
    }

    async fn fetch_jwks(&self) -> Result<OidcJwks, String> {
        {
            let read = self.jwks.read().await;
            if let Some(ref jwks) = *read {
                return Ok(jwks.clone());
            }
        }

        let jwks_uri = if let Some(ref uri) = self.config.jwks_uri {
            uri.clone()
        } else {
            let read = self.discovery.read().await;
            match read.as_ref() {
                Some(discovery) => discovery.jwks_uri.clone(),
                None => return Err("No JWKS URI available: configure jwks_uri or run discovery first".to_string()),
            }
        };

        debug!(jwks_uri_configured = !jwks_uri.is_empty(), "Fetching OIDC JWKS");

        let response =
            self.http_client.get(&jwks_uri).send().await.map_err(|e| format!("Failed to fetch JWKS: {e}"))?;

        if !response.status().is_success() {
            return Err(format!("JWKS request failed: {}", response.status()));
        }

        let jwks: OidcJwks = response.json().await.map_err(|e| format!("Failed to parse JWKS: {e}"))?;

        {
            let mut write = self.jwks.write().await;
            *write = Some(jwks.clone());
        }
        Ok(jwks)
    }

    async fn validate_id_token(&self, id_token: &str, nonce: Option<&str>) -> Result<(), String> {
        let header_bytes = URL_SAFE_NO_PAD
            .decode(id_token.split('.').next().unwrap_or(""))
            .map_err(|e| format!("Invalid ID token header base64: {e}"))?;

        let header: serde_json::Value =
            serde_json::from_slice(&header_bytes).map_err(|e| format!("Invalid ID token header JSON: {e}"))?;

        let kid = header.get("kid").and_then(|v| v.as_str());
        let alg_str = header.get("alg").and_then(|v| v.as_str()).unwrap_or("RS256");

        let algorithm = match alg_str {
            "RS256" => Algorithm::RS256,
            "RS384" => Algorithm::RS384,
            "RS512" => Algorithm::RS512,
            "ES256" => Algorithm::ES256,
            "ES384" => Algorithm::ES384,
            "EdDSA" => Algorithm::EdDSA,
            // SSO-AUDIT: Reject symmetric algorithms outright to prevent
            // public-key-confusion attacks (CVE-2015-9235 / CVE-2018-0114).
            // If an IdP presented a token signed with HS256, an attacker who
            // knows the homeserver's JWKS public key could forge id_tokens
            // using that public key as the HMAC secret. OIDC core spec
            // §3.1.3.7 requires `alg` to be an asymmetric algorithm for
            // id_tokens in code-flow responses; symmetric `alg` values are
            // only valid for private-use JWTs exchanged in non-standard
            // client-side contexts.
            "HS256" | "HS384" | "HS512" => {
                return Err(format!(
                    "Symmetric algorithm '{}' is not permitted for id_token validation (rejected to prevent public-key-confusion attack)",
                    alg_str
                ));
            }
            _ => return Err(format!("Unsupported ID token algorithm: {alg_str}")),
        };

        match self.fetch_jwks().await {
            Ok(jwks) => {
                let matching_key = jwks.keys.iter().find(|k| {
                    if let Some(ref key_kid) = k.kid {
                        kid == Some(key_kid.as_str())
                    } else {
                        true
                    }
                });

                if let Some(key) = matching_key {
                    let decoding_key = if key.kty == "RSA" {
                        match (&key.n, &key.e) {
                            (Some(n), Some(e)) => {
                                DecodingKey::from_rsa_components(n, e).map_err(|e| format!("Invalid RSA key: {e}"))?
                            }
                            _ => return Err("RSA key missing n/e components".to_string()),
                        }
                    } else if key.kty == "EC" {
                        match (&key.crv, &key.x, &key.y) {
                            (Some(_), Some(x), Some(y)) => {
                                DecodingKey::from_ec_components(x, y).map_err(|e| format!("Invalid EC key: {e}"))?
                            }
                            _ => return Err("EC key missing crv/x/y components".to_string()),
                        }
                    } else if key.kty == "OKP" {
                        match (&key.crv, &key.x) {
                            (Some(_), Some(x)) => {
                                DecodingKey::from_ed_components(x).map_err(|e| format!("Invalid EdDSA key: {e}"))?
                            }
                            _ => return Err("OKP key missing crv/x components".to_string()),
                        }
                    } else {
                        return Err(format!("Unsupported key type: {}", key.kty));
                    };

                    let mut validation = Validation::new(algorithm);
                    validation.set_issuer(&[&self.config.issuer]);
                    validation.set_audience(&[&self.config.client_id]);
                    validation.validate_exp = true;
                    validation.validate_nbf = false;

                    let token_data = decode::<serde_json::Value>(id_token, &decoding_key, &validation)
                        .map_err(|e| format!("JWT signature verification failed: {e}"))?;

                    debug!("OIDC ID token JWT signature verified successfully (kid={:?})", kid);

                    // OPT-021: Validate nonce claim against stored nonce to prevent replay attacks
                    if let Some(expected_nonce) = nonce {
                        let token_nonce = token_data.claims.get("nonce").and_then(|v| v.as_str());
                        if token_nonce != Some(expected_nonce) {
                            return Err(format!(
                                "ID token nonce mismatch: expected '{}', got '{:?}'",
                                expected_nonce, token_nonce
                            ));
                        }
                        debug!("OIDC ID token nonce validated successfully");
                    }
                } else {
                    tracing::error!(
                        kid = ?kid,
                        issuer = %self.config.issuer,
                        client_id = %self.config.client_id,
                        "No matching JWKS key found for id_token kid; rejecting (no claim-only fallback)"
                    );
                    return Err("id_token signature key (kid) not found in JWKS".to_string());
                }
            }
            Err(e) => {
                tracing::error!(
                    error = %e,
                    issuer = %self.config.issuer,
                    client_id = %self.config.client_id,
                    "Failed to fetch JWKS; rejecting id_token (no claim-only fallback)"
                );
                return Err(format!("JWKS unavailable, cannot verify id_token signature: {e}"));
            }
        }

        Ok(())
    }

    /// Claim-only validation of an id_token (iss/aud/exp), WITHOUT signature verification.
    ///
    /// NOTE: As of OPT-001 (audit 07 #1) this is intentionally NOT used as a fallback in
    /// `validate_id_token`: accepting a token whose signature could not be verified is an
    /// authentication bypass. Retained (not deleted) because it is security-relevant and may
    /// be reused for contexts where the signature has already been verified separately.
    #[allow(dead_code)]
    fn validate_id_token_claims(&self, id_token: &str) -> Result<(), String> {
        let parts: Vec<&str> = id_token.split('.').collect();
        if parts.len() != 3 {
            return Err("Invalid ID token format: expected 3 parts".to_string());
        }

        let payload_bytes =
            URL_SAFE_NO_PAD.decode(parts[1]).map_err(|e| format!("Invalid ID token payload base64: {e}"))?;

        let payload: serde_json::Value =
            serde_json::from_slice(&payload_bytes).map_err(|e| format!("Invalid ID token payload JSON: {e}"))?;

        let token_issuer =
            payload.get("iss").and_then(|v| v.as_str()).ok_or_else(|| "Missing 'iss' claim in ID token".to_string())?;

        if token_issuer != self.config.issuer {
            return Err(format!("ID token issuer mismatch: expected {}, got {}", self.config.issuer, token_issuer));
        }

        let audiences = payload.get("aud").ok_or_else(|| "Missing 'aud' claim in ID token".to_string())?;

        let audience_matches = if let Some(aud_str) = audiences.as_str() {
            aud_str == self.config.client_id
        } else if let Some(aud_arr) = audiences.as_array() {
            aud_arr.iter().any(|v| v.as_str() == Some(&self.config.client_id))
        } else {
            false
        };

        if !audience_matches {
            return Err(format!("ID token audience mismatch: expected {}", self.config.client_id));
        }

        let now = chrono::Utc::now().timestamp();
        let expires_at = payload.get("exp").and_then(|v| v.as_i64()).unwrap_or(0);

        if expires_at < now {
            return Err(format!("ID token expired: exp={expires_at} now={now}"));
        }

        let azp = payload.get("azp").and_then(|v| v.as_str());
        if let Some(azp_val) = azp {
            if azp_val != self.config.client_id {
                return Err(format!(
                    "ID token authorized party mismatch: expected {}, got {}",
                    self.config.client_id, azp_val
                ));
            }
        }

        Ok(())
    }

    /// See [`refresh_token`].
    /// See [`refresh_token`].
    pub async fn refresh_token(&self, refresh_token: &str) -> Result<OidcTokenResponse, ApiError> {
        let default_token = format!("{}/token", self.config.issuer);
        let token_endpoint = {
            let read = self.discovery.read().await;
            self.config
                .token_endpoint
                .as_ref()
                .or_else(|| read.as_ref().map(|d| &d.token_endpoint))
                .cloned()
                .unwrap_or(default_token)
        };

        let params =
            [("grant_type", "refresh_token"), ("refresh_token", refresh_token), ("client_id", &self.config.client_id)];

        let mut request = self.http_client.post(token_endpoint).form(&params);

        if let Some(ref secret) = self.config.client_secret {
            request = request.basic_auth(&self.config.client_id, Some(secret));
        }

        let response = request.send().await.map_err(|e| ApiError::internal_with_context("Token refresh failed", &e))?;

        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(ApiError::internal_with_context("Token refresh failed", &body));
        }

        response.json().await.map_err(|e| ApiError::internal_with_context("Failed to parse token response", &e))
    }

    /// See [`get_user_info`].
    /// See [`get_user_info`].
    pub async fn get_user_info(&self, access_token: &str) -> Result<OidcUserInfo, ApiError> {
        let default_userinfo = format!("{}/userinfo", self.config.issuer);
        let userinfo_endpoint = {
            let read = self.discovery.read().await;
            self.config
                .userinfo_endpoint
                .as_ref()
                .or_else(|| read.as_ref().map(|d| &d.userinfo_endpoint))
                .cloned()
                .unwrap_or(default_userinfo)
        };

        let response = self
            .http_client
            .get(&userinfo_endpoint)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|e| ApiError::internal_with_context("UserInfo request failed", &e))?;

        if !response.status().is_success() {
            return Err(ApiError::internal_with_context("UserInfo request failed", &response.status()));
        }

        response.json().await.map_err(|e| ApiError::internal_with_context("Failed to parse UserInfo", &e))
    }

    /// See [`map_user`].
    /// See [`map_user`].
    pub fn map_user(&self, user_info: &OidcUserInfo) -> OidcUser {
        let mapping = &self.config.attribute_mapping;

        let localpart =
            mapping.localpart.as_ref().and_then(|attr| Self::get_attribute(user_info, attr)).unwrap_or(&user_info.sub);

        let displayname =
            mapping.displayname.as_ref().and_then(|attr| Self::get_attribute(user_info, attr)).map(|s| s.to_string());

        let email = mapping.email.as_ref().and_then(|attr| Self::get_attribute(user_info, attr)).map(|s| s.to_string());

        OidcUser { subject: user_info.sub.clone(), localpart: localpart.to_string(), displayname, email }
    }

    fn get_attribute<'a>(user_info: &'a OidcUserInfo, attr: &str) -> Option<&'a str> {
        match attr {
            "sub" => Some(&user_info.sub),
            "name" => user_info.name.as_deref(),
            "given_name" => user_info.given_name.as_deref(),
            "family_name" => user_info.family_name.as_deref(),
            "preferred_username" => user_info.preferred_username.as_deref(),
            "email" => user_info.email.as_deref(),
            "picture" => user_info.picture.as_deref(),
            "locale" => user_info.locale.as_deref(),
            _ => None,
        }
    }

    /// See [`generate_state`].
    /// See [`generate_state`].
    pub fn generate_state() -> String {
        use rand::Rng;
        let mut rng = rand::rng();
        (0..32).map(|_| rng.sample(rand::distr::Alphanumeric) as char).collect()
    }

    /// See [`get_config`].
    /// See [`get_config`].
    pub fn get_config(&self) -> &OidcConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use synapse_common::config::OidcAttributeMapping;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn create_test_config() -> OidcConfig {
        OidcConfig {
            enabled: true,
            issuer: "https://accounts.example.com".to_string(),
            client_id: "test-client-id".to_string(),
            client_secret: Some("test-client-secret".to_string()),
            scopes: vec!["openid".to_string(), "profile".to_string(), "email".to_string()],
            attribute_mapping: OidcAttributeMapping {
                localpart: Some("preferred_username".to_string()),
                displayname: Some("name".to_string()),
                email: Some("email".to_string()),
            },
            callback_url: Some("https://matrix.example.com/_matrix/client/r0/login/sso/redirect".to_string()),
            allow_existing_users: true,
            block_unknown_users: false,
            authorization_endpoint: None,
            token_endpoint: None,
            userinfo_endpoint: None,
            jwks_uri: None,
            registration_endpoint: None,
            timeout: 10,
            user_mapping_provider: None,
        }
    }

    fn create_test_service() -> OidcService {
        let config = Arc::new(create_test_config());
        OidcService::new(config)
    }

    #[test]
    fn test_oidc_config_enabled() {
        let config = create_test_config();
        assert!(config.is_enabled());
    }

    #[test]
    fn test_oidc_config_disabled() {
        let config = OidcConfig::default();
        assert!(!config.is_enabled());
    }

    #[test]
    fn test_service_enabled() {
        let service = create_test_service();
        assert!(service.is_enabled());
    }

    #[test]
    fn test_generate_state() {
        let state = OidcService::generate_state();
        assert_eq!(state.len(), 32);
        assert!(state.chars().all(|c| c.is_alphanumeric()));
    }

    #[test]
    fn test_get_authorization_url() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let service = create_test_service();
        let url = rt
            .block_on(service.get_authorization_url("test-state", "https://matrix.example.com/callback", None, None))
            .unwrap();

        assert!(url.contains("client_id=test-client-id"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("state=test-state"));
        assert!(url.contains("scope="));
    }

    #[tokio::test]
    async fn unknown_kid_must_not_fall_back_to_claim_only() {
        use base64::Engine as _;

        let service = create_test_service();

        // Seed an EMPTY JWKS so fetch_jwks() "succeeds" (returns the cached value)
        // but NO key matches the token's kid.
        *service.jwks.write().await = Some(OidcJwks { keys: vec![] });

        // Forge an unsigned id_token whose header references a kid that is not in the
        // JWKS, but whose claims (iss/aud/exp) are all valid. This ensures the ONLY
        // reason to reject is the missing signature key — not claim validation — so a
        // genuine claim-only fallback would (incorrectly) accept it.
        let header = serde_json::json!({ "alg": "RS256", "kid": "unknown-kid-not-in-jwks" });
        let exp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() + 3600;
        let payload = serde_json::json!({
            "iss": service.config.issuer,
            "aud": service.config.client_id,
            "exp": exp,
        });

        let header_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header).unwrap());
        let payload_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload).unwrap());
        let forged = format!("{header_b64}.{payload_b64}.sig");

        let result = service.validate_id_token(&forged, None).await;
        assert!(result.is_err(), "unknown kid must be rejected, not claim-only accepted; got {result:?}");
    }

    /// SSO-AUDIT: HS256/HS384/HS512 must be rejected outright for id_token
    /// validation regardless of JWKS availability, to prevent the
    /// public-key-confusion attack (CVE-2015-9235). An attacker who knows
    /// the homeserver's JWKS RSA public key could otherwise forge id_tokens
    /// by signing with HS256 using the public key as the HMAC secret.
    #[tokio::test]
    async fn hs256_id_token_must_be_rejected_even_with_valid_jwks() {
        use base64::Engine as _;

        let service = create_test_service();

        // Seed JWKS with a dummy RSA key (so the algorithm-matching step
        // would otherwise succeed if HS256 were permitted).
        *service.jwks.write().await = Some(OidcJwks {
            keys: vec![OidcJwk {
                kty: "RSA".to_string(),
                kid: Some("rsa-1".to_string()),
                alg: Some("RS256".to_string()),
                use_: Some("sig".to_string()),
                n: Some("0vx7agoebGcQSuuPiLJXZptN9nndrQmbXEps2aiAFbWhM78LhWx".to_string()),
                e: Some("AQAB".to_string()),
                crv: None,
                x: None,
                y: None,
            }],
        });

        // Forge an id_token with `alg: HS256`. Without the reject-HS256
        // check, the homeserver would fall into the RSA JWKS path (since
        // the alg_str default-unwrap means HS256 is treated as a valid
        // algorithm) and could be tricked into verifying with the public
        // key as HMAC secret.
        let header = serde_json::json!({ "alg": "HS256", "kid": "rsa-1" });
        let exp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() + 3600;
        let payload = serde_json::json!({
            "iss": service.config.issuer,
            "aud": service.config.client_id,
            "exp": exp,
        });
        let header_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header).unwrap());
        let payload_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload).unwrap());
        let forged = format!("{header_b64}.{payload_b64}.signature");

        let result = service.validate_id_token(&forged, None).await;
        let err = result.expect_err("HS256 id_token must be rejected outright");
        assert!(
            err.contains("Symmetric algorithm") && err.contains("not permitted"),
            "rejection error must explicitly mention symmetric algorithm + public-key confusion, got: {err}"
        );
    }

    #[test]
    fn test_map_user() {
        let service = create_test_service();
        let user_info = OidcUserInfo {
            sub: "user123".to_string(),
            name: Some("Test User".to_string()),
            given_name: Some("Test".to_string()),
            family_name: Some("User".to_string()),
            preferred_username: Some("testuser".to_string()),
            email: Some("test@example.com".to_string()),
            email_verified: Some(true),
            picture: Some("https://example.com/avatar.png".to_string()),
            locale: Some("en".to_string()),
        };

        let user = service.map_user(&user_info);

        assert_eq!(user.subject, "user123");
        assert_eq!(user.localpart, "testuser");
        assert_eq!(user.displayname, Some("Test User".to_string()));
        assert_eq!(user.email, Some("test@example.com".to_string()));
    }

    #[test]
    fn test_map_user_default_localpart() {
        let mut config = create_test_config();
        config.attribute_mapping.localpart = None;
        let service = OidcService::new(Arc::new(config));

        let user_info = OidcUserInfo {
            sub: "user123".to_string(),
            name: None,
            given_name: None,
            family_name: None,
            preferred_username: None,
            email: None,
            email_verified: None,
            picture: None,
            locale: None,
        };

        let user = service.map_user(&user_info);
        assert_eq!(user.localpart, "user123");
    }

    #[tokio::test]
    async fn test_exchange_code_sends_pkce_verifier() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "access-token",
                "token_type": "Bearer",
                "expires_in": 3600,
                "refresh_token": "refresh-token",
                "scope": "openid profile email"
            })))
            .mount(&server)
            .await;

        let mut config = create_test_config();
        config.issuer = server.uri();
        config.token_endpoint = Some(format!("{}/token", server.uri()));
        let service = OidcService::new(Arc::new(config));

        let response = service
            .exchange_code("auth-code", "https://matrix.example.com/callback", Some("verifier-123"), None)
            .await
            .unwrap();

        assert_eq!(response.access_token, "access-token");

        let requests = server.received_requests().await.unwrap();
        let body = String::from_utf8_lossy(&requests[0].body);
        assert!(body.contains("code_verifier=verifier-123"));
    }

    #[tokio::test]
    async fn nonce_mismatch_rejected() {
        use jsonwebtoken::{encode, EncodingKey, Header};
        use rsa::pkcs8::EncodePrivateKey;
        use rsa::traits::PublicKeyParts;
        use rsa::RsaPrivateKey;

        let service = create_test_service();

        // Generate a test RSA key pair using rsa's rand_core (v0.6) for compatibility
        let mut rng = rsa::rand_core::OsRng;
        let private_key = RsaPrivateKey::new(&mut rng, 2048).expect("failed to generate RSA key");
        let public_key = private_key.to_public_key();

        // Create EncodingKey from PEM
        let pem = private_key.to_pkcs8_pem(rsa::pkcs8::LineEnding::LF).expect("pkcs8 pem encode");
        let encoding_key = EncodingKey::from_rsa_pem(pem.as_bytes()).expect("create encoding key");

        // Build JWKS with the public key's n and e components
        let kid = "test-nonce-key";
        let n = URL_SAFE_NO_PAD.encode(public_key.n().to_bytes_be());
        let e = URL_SAFE_NO_PAD.encode(public_key.e().to_bytes_be());

        *service.jwks.write().await = Some(OidcJwks {
            keys: vec![OidcJwk {
                kty: "RSA".to_string(),
                use_: Some("sig".to_string()),
                kid: Some(kid.to_string()),
                alg: Some("RS256".to_string()),
                n: Some(n),
                e: Some(e),
                crv: None,
                x: None,
                y: None,
            }],
        });

        // Create a properly signed id_token with nonce "expected-nonce"
        let exp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() + 3600;
        let claims = serde_json::json!({
            "iss": service.config.issuer,
            "aud": service.config.client_id,
            "sub": "user123",
            "exp": exp,
            "nonce": "expected-nonce",
        });

        let mut header = Header::new(jsonwebtoken::Algorithm::RS256);
        header.kid = Some(kid.to_string());
        let id_token = encode(&header, &claims, &encoding_key).expect("jwt encode");

        // Valid nonce: should pass
        let result = service.validate_id_token(&id_token, Some("expected-nonce")).await;
        assert!(result.is_ok(), "correct nonce should pass: {:?}", result);

        // Wrong nonce: should fail
        let result = service.validate_id_token(&id_token, Some("wrong-nonce")).await;
        assert!(result.is_err(), "wrong nonce should be rejected");
        let err = result.unwrap_err();
        assert!(err.contains("nonce mismatch"), "error should mention nonce mismatch, got: {}", err);
    }
}
