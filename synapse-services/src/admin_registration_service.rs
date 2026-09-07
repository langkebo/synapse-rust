use crate::auth::{CredentialAuth, TokenAuth};
use crate::UserService;
use crate::*;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::Utc;
use hmac::{Hmac, Mac};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::sync::Arc;
use synapse_cache::CacheManager;
use synapse_common::metrics::MetricsCollector;
use synapse_common::*;

type HmacSha256 = Hmac<Sha256>;

/// The `AdminRegistrationService` struct.
#[derive(Clone)]
#[allow(dead_code)] // Reserved fields for future use; see field-level comments.
pub struct AdminRegistrationService {
    token_auth: Arc<dyn TokenAuth>,
    credential_auth: Arc<dyn CredentialAuth>,
    server_name: String,
    config: AdminRegistrationConfig,
    user_storage: Arc<dyn UserStore>,
    user_service: Arc<UserService>,  // Reserved; constructor parity
    cache: Arc<CacheManager>,
    metrics: Arc<MetricsCollector>,
}

/// The `NonceResponse` struct.
#[derive(Debug, Serialize, Deserialize)]
pub struct NonceResponse {
    /// The `nonce` field.
    pub nonce: String,
}

/// The `AdminRegisterRequest` struct.
#[derive(Debug, Deserialize)]
pub struct AdminRegisterRequest {
    /// The `nonce` field.
    pub nonce: String,
    /// The `username` field.
    pub username: String,
    /// The `password` field.
    pub password: String,
    /// The `admin` field.
    pub admin: Option<bool>,
    /// The `user_type` field.
    pub user_type: Option<String>,
    /// The `displayname` field.
    pub displayname: Option<String>,
    /// The `mac` field.
    pub mac: String,
}

/// The `AdminRegisterResponse` struct.
#[derive(Debug, Serialize, Deserialize)]
pub struct AdminRegisterResponse {
    /// The `access_token` field.
    pub access_token: String,
    /// The `refresh_token` field.
    pub refresh_token: String,
    /// The `expires_in` field.
    pub expires_in: i64,
    /// The `device_id` field.
    pub device_id: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `home_server` field.
    pub home_server: String,
}

impl AdminRegistrationService {
    /// See [`new`].
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        token_auth: Arc<dyn TokenAuth>,
        credential_auth: Arc<dyn CredentialAuth>,
        server_name: String,
        config: AdminRegistrationConfig,
        user_storage: Arc<dyn UserStore>,
        user_service: Arc<UserService>,
        cache: Arc<CacheManager>,
        metrics: Arc<MetricsCollector>,
    ) -> Self {
        Self { token_auth, credential_auth, server_name, config, user_storage, user_service, cache, metrics }
    }

    /// See [`generate_nonce`].
    #[::tracing::instrument(skip(self))]
    pub async fn generate_nonce(&self) -> ApiResult<NonceResponse> {
        let start = std::time::Instant::now();
        let nonce = {
            let mut rng = rand::rng();
            let mut nonce_bytes = vec![0u8; 64];
            rng.fill_bytes(&mut nonce_bytes);
            URL_SAFE_NO_PAD.encode(&nonce_bytes)
        };

        let now = Utc::now().timestamp();
        let key = format!("admin:register:nonce:{nonce}");
        if let Err(e) = self.cache.set(&key, &now, self.config.nonce_timeout_seconds).await {
            ::tracing::warn!(error = %e, "Failed to persist admin registration nonce to cache; registration will fail");
        }

        let duration = start.elapsed().as_secs_f64();
        if let Some(hist) = self.metrics.get_histogram("admin_nonce_duration_seconds") {
            hist.observe(duration);
        } else {
            let hist = self.metrics.register_histogram("admin_nonce_duration_seconds".to_string());
            hist.observe(duration);
        }

        Ok(NonceResponse { nonce })
    }

    /// See [`register_admin_user`].
    #[::tracing::instrument(skip(self))]
    pub async fn register_admin_user(&self, request: AdminRegisterRequest) -> ApiResult<AdminRegisterResponse> {
        if !self.config.enabled {
            return Err(ApiError::forbidden("Admin registration is not enabled".to_string()));
        }

        let start = std::time::Instant::now();

        self.validate_and_consume_nonce(&request.nonce).await?;
        self.verify_hmac(&request)?;

        let admin = request.admin.unwrap_or(false);
        let displayname = request.displayname.as_deref();

        let (user, access_token, refresh_token, device_id) =
            self.credential_auth.register(&request.username, &request.password, admin, displayname).await?;

        if let Some(user_type) = request.user_type.as_deref() {
            self.user_storage
                .set_user_type(&user.user_id(), Some(user_type))
                .await
                .map_err(|e| ApiError::internal_with_context("Failed to persist user_type", &e))?;
        }

        let duration = start.elapsed().as_secs_f64();
        if let Some(hist) = self.metrics.get_histogram("admin_register_duration_seconds") {
            hist.observe(duration);
        } else {
            let hist = self.metrics.register_histogram("admin_register_duration_seconds".to_string());
            hist.observe(duration);
        }

        if let Some(counter) = self.metrics.get_counter("admin_register_success_total") {
            counter.inc();
        } else {
            let counter = self.metrics.register_counter("admin_register_success_total".to_string());
            counter.inc();
        }

        Ok(AdminRegisterResponse {
            access_token,
            refresh_token,
            expires_in: self.token_auth.token_expiry(),
            device_id,
            user_id: user.user_id(),
            home_server: self.server_name.clone(),
        })
    }

    async fn validate_and_consume_nonce(&self, nonce: &str) -> ApiResult<()> {
        let key = format!("admin:register:nonce:{nonce}");
        let existing = self.cache.get::<i64>(&key).await?;
        if existing.is_none() {
            return Err(ApiError::bad_request("Unrecognised nonce".to_string()));
        }
        self.cache.delete(&key).await;
        let after_delete = self.cache.get::<i64>(&key).await?;
        if after_delete.is_some() {
            return Err(ApiError::internal("Failed to consume nonce".to_string()));
        }
        Ok(())
    }

    fn verify_hmac(&self, request: &AdminRegisterRequest) -> ApiResult<()> {
        if self.config.shared_secret.is_empty() {
            return Err(ApiError::internal("Shared secret is not configured".to_string()));
        }

        let provided = synapse_common::crypto::decode_hex(&request.mac)
            .map_err(|_| ApiError::forbidden("HMAC incorrect".to_string()))?;

        let mut mac = HmacSha256::new_from_slice(self.config.shared_secret.as_bytes())
            .map_err(|e| ApiError::internal_with_context("Invalid shared secret", &e))?;

        mac.update(request.nonce.as_bytes());
        mac.update(b"\0");
        mac.update(request.username.as_bytes());
        mac.update(b"\0");
        mac.update(request.password.as_bytes());
        mac.update(b"\0");

        if request.admin.unwrap_or(false) {
            mac.update(b"admin\x00\x00\x00");
        } else {
            mac.update(b"notadmin");
        }

        if let Some(user_type) = &request.user_type {
            mac.update(b"\0");
            mac.update(user_type.as_bytes());
        }

        mac.verify_slice(&provided).map_err(|_| ApiError::forbidden("HMAC incorrect".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nonce_response_serialization() {
        let response = NonceResponse { nonce: "abc123".to_string() };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("abc123"));
    }

    #[test]
    fn test_admin_register_request_deserialization() {
        let json = r#"{
            "nonce": "test_nonce",
            "username": "admin",
            "password": "secret",
            "admin": true,
            "mac": "abcd1234"
        }"#;
        let request: AdminRegisterRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.nonce, "test_nonce");
        assert_eq!(request.username, "admin");
        assert_eq!(request.password, "secret");
        assert_eq!(request.admin, Some(true));
    }

    #[test]
    fn test_admin_register_response_serialization() {
        let response = AdminRegisterResponse {
            access_token: "token123".to_string(),
            refresh_token: "refresh123".to_string(),
            expires_in: 3600,
            device_id: "DEVICE".to_string(),
            user_id: "@admin:example.com".to_string(),
            home_server: "example.com".to_string(),
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("token123"));
        assert!(json.contains("@admin:example.com"));
    }

    #[test]
    fn test_admin_register_request_without_admin_field() {
        let json = r#"{
            "nonce": "test_nonce",
            "username": "admin",
            "password": "secret",
            "mac": "abcd1234"
        }"#;
        let request: AdminRegisterRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.nonce, "test_nonce");
        assert_eq!(request.admin, None);
    }

    #[test]
    fn test_admin_register_request_with_user_type() {
        let json = r#"{
            "nonce": "test_nonce",
            "username": "admin",
            "password": "secret",
            "admin": true,
            "user_type": "bot",
            "displayname": "Admin User",
            "mac": "abcd1234"
        }"#;
        let request: AdminRegisterRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.user_type, Some("bot".to_string()));
        assert_eq!(request.displayname, Some("Admin User".to_string()));
    }
}
