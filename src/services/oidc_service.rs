use crate::common::config::OidcConfig;
use crate::common::error::ApiError;
use serde::{Deserialize, Serialize};
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

        let response = self.http_client
            .get(&discovery_url)
            .send()
            .await
            .map_err(|e| ApiError::internal(format!("Failed to fetch discovery document: {}", e)))?;

        if !response.status().is_success() {
            return Err(ApiError::internal(format!("Discovery request failed: {}", response.status())));
        }

        let discovery: OidcDiscoveryDocument = response.json().await
            .map_err(|e| ApiError::internal(format!("Failed to parse discovery document: {}", e)))?;

        self.discovery = Some(discovery.clone());
        Ok(discovery)
    }

    pub fn get_authorization_url(&self, state: &str, redirect_uri: &str) -> String {
        let scope = self.config.scopes.join(" ");
        
        let default_auth = format!("{}/authorize", self.config.issuer);
        let auth_endpoint = self.config.authorization_endpoint.as_ref()
            .or_else(|| self.discovery.as_ref().map(|d| &d.authorization_endpoint))
            .map(|s| s.as_str())
            .unwrap_or(&default_auth);

        let mut url = url::Url::parse(auth_endpoint).unwrap();
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("client_id", &self.config.client_id);
            query.append_pair("response_type", "code");
            query.append_pair("scope", &scope);
            query.append_pair("redirect_uri", redirect_uri);
            query.append_pair("state", state);
        }

        url.to_string()
    }

    pub async fn exchange_code(
        &self,
        code: &str,
        redirect_uri: &str,
    ) -> Result<OidcTokenResponse, ApiError> {
        let default_token = format!("{}/token", self.config.issuer);
        let token_endpoint = self.config.token_endpoint.as_ref()
            .or_else(|| self.discovery.as_ref().map(|d| &d.token_endpoint))
            .map(|s| s.as_str())
            .unwrap_or(&default_token);

        let params = [
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("client_id", &self.config.client_id),
        ];

        let mut request = self.http_client.post(token_endpoint).form(&params);

        if let Some(ref secret) = self.config.client_secret {
            request = request.basic_auth(&self.config.client_id, Some(secret));
        }

        let response = request.send().await
            .map_err(|e| ApiError::internal(format!("Token exchange failed: {}", e)))?;

        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(ApiError::internal(format!("Token exchange failed: {}", body)));
        }

        response.json().await
            .map_err(|e| ApiError::internal(format!("Failed to parse token response: {}", e)))
    }

    pub async fn get_user_info(&self, access_token: &str) -> Result<OidcUserInfo, ApiError> {
        let default_userinfo = format!("{}/userinfo", self.config.issuer);
        let userinfo_endpoint = self.config.userinfo_endpoint.as_ref()
            .or_else(|| self.discovery.as_ref().map(|d| &d.userinfo_endpoint))
            .map(|s| s.as_str())
            .unwrap_or(&default_userinfo);

        let response = self.http_client
            .get(userinfo_endpoint)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|e| ApiError::internal(format!("UserInfo request failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(ApiError::internal(format!("UserInfo request failed: {}", response.status())));
        }

        response.json().await
            .map_err(|e| ApiError::internal(format!("Failed to parse UserInfo: {}", e)))
    }

    pub fn map_user(&self, user_info: &OidcUserInfo) -> OidcUser {
        let mapping = &self.config.attribute_mapping;

        let localpart = mapping.localpart.as_ref()
            .and_then(|attr| Self::get_attribute(user_info, attr))
            .unwrap_or(&user_info.sub);

        let displayname = mapping.displayname.as_ref()
            .and_then(|attr| Self::get_attribute(user_info, attr))
            .map(|s| s.to_string());

        let email = mapping.email.as_ref()
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
        (0..32).map(|_| rng.sample(rand::distributions::Alphanumeric) as char).collect()
    }

    pub fn get_config(&self) -> &OidcConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::config::OidcAttributeMapping;

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
        let url = service.get_authorization_url("test-state", "https://matrix.example.com/callback");
        
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
}
