use crate::common::config::OidcConfig;
use crate::common::error::ApiError;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::Duration;
use tracing::debug;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcDiscoveryDocument {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub userinfo_endpoint: String,
    pub jwks_uri: String,
    pub response_types_supported: Vec<String>,
    pub subject_types_supported: Vec<String>,
    pub id_token_signing_alg_values_supported: Vec<String>,
    pub scopes_supported: Option<Vec<String>>,
    pub claims_supported: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcTokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: Option<i64>,
    pub refresh_token: Option<String>,
    pub id_token: Option<String>,
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
    pub email_verified: Option<bool>,
    pub picture: Option<String>,
    pub locale: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcAuthRequest {
    pub url: String,
    pub state: String,
    pub nonce: String,
    pub code_verifier: String,
}

#[derive(Debug, Clone)]
pub struct OidcUser {
    pub subject: String,
    pub localpart: String,
    pub displayname: Option<String>,
    pub email: Option<String>,
}

pub struct OidcService {
    config: Arc<OidcConfig>,
    http_client: reqwest::Client,
    discovery: Option<OidcDiscoveryDocument>,
}

impl OidcService {
    pub fn new(config: Arc<OidcConfig>) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            config,
            http_client,
            discovery: None,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.config.is_enabled()
    }

    pub async fn discover(&mut self) -> Result<OidcDiscoveryDocument, ApiError> {
        let cached = self.discovery.clone();
        if let Some(discovery) = cached {
            return Ok(discovery);
        }

        let discovery_url = format!("{}/.well-known/openid-configuration", self.config.issuer);

        debug!("Fetching OIDC discovery document from {}", discovery_url);

        let response = self
            .http_client
            .get(&discovery_url)
            .send()
            .await
            .map_err(|e| {
                ApiError::internal(format!("Failed to fetch discovery document: {}", e))
            })?;

        if !response.status().is_success() {
            return Err(ApiError::internal(format!(
                "Discovery request failed: {}",
                response.status()
            )));
        }

        let discovery: OidcDiscoveryDocument = response.json().await.map_err(|e| {
            ApiError::internal(format!("Failed to parse discovery document: {}", e))
        })?;

        self.discovery = Some(discovery.clone());
        Ok(discovery)
    }

    pub fn get_authorization_url(
        &self,
        state: &str,
        redirect_uri: &str,
        code_challenge: Option<&str>,
        code_challenge_method: Option<&str>,
    ) -> Result<String, ApiError> {
        let scope = self.config.scopes.join(" ");

        let default_auth = format!("{}/authorize", self.config.issuer);
        let auth_endpoint = self
            .config
            .authorization_endpoint
            .as_ref()
            .or_else(|| self.discovery.as_ref().map(|d| &d.authorization_endpoint))
            .map(|s| s.as_str())
            .unwrap_or(&default_auth);

        let mut url = url::Url::parse(auth_endpoint).map_err(|e| {
            ApiError::internal(format!("Invalid OIDC authorization endpoint: {}", e))
        })?;
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
                query.append_pair(
                    "code_challenge_method",
                    code_challenge_method.unwrap_or("S256"),
                );
            }
        }

        Ok(url.to_string())
    }

    /// Generate PKCE code verifier and challenge
    pub fn generate_pkce() -> (String, String) {
        use rand::Rng;
        let mut rng = rand::thread_rng();

        // PKCE charset as bytes for indexing
        const PKCE_CHARSET: &[u8] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";

        // Generate code_verifier (43-128 characters)
        let verifier_len = rng.gen_range(43..=128);
        let code_verifier: String = (0..verifier_len)
            .map(|_| {
                let idx = rng.gen_range(0..PKCE_CHARSET.len());
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

    /// Verify PKCE code verifier
    pub fn verify_pkce(code_verifier: &str, code_challenge: &str) -> bool {
        let mut hasher = Sha256::new();
        hasher.update(code_verifier.as_bytes());
        let hash = hasher.finalize();
        let computed = Self::base64url_encode(&hash);
        computed == code_challenge
    }

    pub async fn exchange_code(
        &self,
        code: &str,
        redirect_uri: &str,
        code_verifier: Option<&str>,
    ) -> Result<OidcTokenResponse, ApiError> {
        let default_token = format!("{}/token", self.config.issuer);
        let token_endpoint = self
            .config
            .token_endpoint
            .as_ref()
            .or_else(|| self.discovery.as_ref().map(|d| &d.token_endpoint))
            .map(|s| s.as_str())
            .unwrap_or(&default_token);

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

        let response = request
            .send()
            .await
            .map_err(|e| ApiError::internal(format!("Token exchange failed: {}", e)))?;

        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(ApiError::internal(format!(
                "Token exchange failed: {}",
                body
            )));
        }

        let token_response: OidcTokenResponse = response
            .json()
            .await
            .map_err(|e| ApiError::internal(format!("Failed to parse token response: {}", e)))?;

        if let Some(ref id_token) = token_response.id_token {
            if let Err(e) = self.validate_id_token(id_token) {
                tracing::warn!("OIDC ID token validation failed: {}", e);
            }
        }

        Ok(token_response)
    }

    fn validate_id_token(&self, id_token: &str) -> Result<(), String> {
        let parts: Vec<&str> = id_token.split('.').collect();
        if parts.len() != 3 {
            return Err("Invalid ID token format: expected 3 parts".to_string());
        }

        let payload_bytes = URL_SAFE_NO_PAD
            .decode(parts[1])
            .map_err(|e| format!("Invalid ID token payload base64: {}", e))?;

        let payload: serde_json::Value = serde_json::from_slice(&payload_bytes)
            .map_err(|e| format!("Invalid ID token payload JSON: {}", e))?;

        let token_issuer = payload
            .get("iss")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing 'iss' claim in ID token".to_string())?;

        if token_issuer != self.config.issuer {
            return Err(format!(
                "ID token issuer mismatch: expected {}, got {}",
                self.config.issuer, token_issuer
            ));
        }

        let audiences = payload
            .get("aud")
            .ok_or_else(|| "Missing 'aud' claim in ID token".to_string())?;

        let audience_matches = if let Some(aud_str) = audiences.as_str() {
            aud_str == self.config.client_id
        } else if let Some(aud_arr) = audiences.as_array() {
            aud_arr
                .iter()
                .any(|v| v.as_str() == Some(&self.config.client_id))
        } else {
            false
        };

        if !audience_matches {
            return Err(format!(
                "ID token audience mismatch: expected {}",
                self.config.client_id
            ));
        }

        let now = chrono::Utc::now().timestamp();
        let expires_at = payload
            .get("exp")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        if expires_at < now {
            return Err(format!(
                "ID token expired: exp={} now={}",
                expires_at, now
            ));
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

    pub async fn refresh_token(&self, refresh_token: &str) -> Result<OidcTokenResponse, ApiError> {
        let default_token = format!("{}/token", self.config.issuer);
        let token_endpoint = self
            .config
            .token_endpoint
            .as_ref()
            .or_else(|| self.discovery.as_ref().map(|d| &d.token_endpoint))
            .map(|s| s.as_str())
            .unwrap_or(&default_token);

        let params = [
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", &self.config.client_id),
        ];

        let mut request = self.http_client.post(token_endpoint).form(&params);

        if let Some(ref secret) = self.config.client_secret {
            request = request.basic_auth(&self.config.client_id, Some(secret));
        }

        let response = request
            .send()
            .await
            .map_err(|e| ApiError::internal(format!("Token refresh failed: {}", e)))?;

        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(ApiError::internal(format!(
                "Token refresh failed: {}",
                body
            )));
        }

        response
            .json()
            .await
            .map_err(|e| ApiError::internal(format!("Failed to parse token response: {}", e)))
    }

    pub async fn get_user_info(&self, access_token: &str) -> Result<OidcUserInfo, ApiError> {
        let default_userinfo = format!("{}/userinfo", self.config.issuer);
        let userinfo_endpoint = self
            .config
            .userinfo_endpoint
            .as_ref()
            .or_else(|| self.discovery.as_ref().map(|d| &d.userinfo_endpoint))
            .map(|s| s.as_str())
            .unwrap_or(&default_userinfo);

        let response = self
            .http_client
            .get(userinfo_endpoint)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|e| ApiError::internal(format!("UserInfo request failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(ApiError::internal(format!(
                "UserInfo request failed: {}",
                response.status()
            )));
        }

        response
            .json()
            .await
            .map_err(|e| ApiError::internal(format!("Failed to parse UserInfo: {}", e)))
    }

    pub fn map_user(&self, user_info: &OidcUserInfo) -> OidcUser {
        let mapping = &self.config.attribute_mapping;

        let localpart = mapping
            .localpart
            .as_ref()
            .and_then(|attr| Self::get_attribute(user_info, attr))
            .unwrap_or(&user_info.sub);

        let displayname = mapping
            .displayname
            .as_ref()
            .and_then(|attr| Self::get_attribute(user_info, attr))
            .map(|s| s.to_string());

        let email = mapping
            .email
            .as_ref()
            .and_then(|attr| Self::get_attribute(user_info, attr))
            .map(|s| s.to_string());

        OidcUser {
            subject: user_info.sub.clone(),
            localpart: localpart.to_string(),
            displayname,
            email,
        }
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

    pub fn generate_state() -> String {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        (0..32)
            .map(|_| rng.sample(rand::distributions::Alphanumeric) as char)
            .collect()
    }

    pub fn get_config(&self) -> &OidcConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::config::OidcAttributeMapping;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn create_test_config() -> OidcConfig {
        OidcConfig {
            enabled: true,
            issuer: "https://accounts.example.com".to_string(),
            client_id: "test-client-id".to_string(),
            client_secret: Some("test-client-secret".to_string()),
            scopes: vec![
                "openid".to_string(),
                "profile".to_string(),
                "email".to_string(),
            ],
            attribute_mapping: OidcAttributeMapping {
                localpart: Some("preferred_username".to_string()),
                displayname: Some("name".to_string()),
                email: Some("email".to_string()),
            },
            callback_url: Some(
                "https://matrix.example.com/_matrix/client/r0/login/sso/redirect".to_string(),
            ),
            allow_existing_users: true,
            block_unknown_users: false,
            authorization_endpoint: None,
            token_endpoint: None,
            userinfo_endpoint: None,
            jwks_uri: None,
            timeout: 10,
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
        let service = create_test_service();
        let url = service
            .get_authorization_url(
                "test-state",
                "https://matrix.example.com/callback",
                None,
                None,
            )
            .unwrap();

        assert!(url.contains("client_id=test-client-id"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("state=test-state"));
        assert!(url.contains("scope="));
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
            .exchange_code(
                "auth-code",
                "https://matrix.example.com/callback",
                Some("verifier-123"),
            )
            .await
            .unwrap();

        assert_eq!(response.access_token, "access-token");

        let requests = server.received_requests().await.unwrap();
        let body = String::from_utf8_lossy(&requests[0].body);
        assert!(body.contains("code_verifier=verifier-123"));
    }
}
