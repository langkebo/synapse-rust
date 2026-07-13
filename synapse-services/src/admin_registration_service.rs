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

#[derive(Clone)]
pub struct AdminRegistrationService {
    token_auth: Arc<dyn TokenAuth>,
    credential_auth: Arc<dyn CredentialAuth>,
    server_name: String,
    config: AdminRegistrationConfig,
    user_storage: Arc<dyn UserStore>,
    #[allow(dead_code)]
    user_service: Arc<UserService>,
    cache: Arc<CacheManager>,
    metrics: Arc<MetricsCollector>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NonceResponse {
    pub nonce: String,
}

#[derive(Debug, Deserialize)]
pub struct AdminRegisterRequest {
    pub nonce: String,
    pub username: String,
    pub password: String,
    pub admin: Option<bool>,
    pub user_type: Option<String>,
    pub displayname: Option<String>,
    pub mac: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AdminRegisterResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
    pub device_id: String,
    pub user_id: String,
    pub home_server: String,
}

impl AdminRegistrationService {
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

    pub fn start_nonce_cleanup_task(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
            loop {
                interval.tick().await;
                if let Err(e) = self.cleanup_expired_nonces() {
                    ::tracing::error!(
                        error = %e,
                        nonce_timeout_seconds = self.config.nonce_timeout_seconds,
                        cleanup_interval_secs = 300_u64,
                        "Failed to cleanup expired nonces"
                    );
                }
            }
        });
    }

    fn cleanup_expired_nonces(&self) -> Result<(), String> {
        let cutoff_ts = Utc::now().timestamp() - (self.config.nonce_timeout_seconds as i64 * 2);

        let keys = self.cache.get_keys_with_prefix("admin:register:nonce:");

        let mut cleaned = 0u64;
        for key in keys {
            if let Some(ts_str) = self.cache.get_local_raw(&key) {
                if let Ok(ts) = ts_str.parse::<i64>() {
                    if ts < cutoff_ts {
                        self.cache.remove_local(&key);
                        cleaned += 1;
                    }
                }
            }
        }

        if cleaned > 0 {
            ::tracing::debug!("Cleaned up {} expired admin registration nonces from local cache", cleaned);
            if let Some(counter) = self.metrics.get_counter("admin_nonce_cleanup_total") {
                counter.inc_by(cleaned);
            }
        }

        Ok(())
    }

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
                .map_err(|e| ApiError::internal_with_log("Failed to persist user_type", &e))?;
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
            .map_err(|e| ApiError::internal_with_log("Invalid shared secret", &e))?;

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
}
