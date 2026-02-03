use crate::cache::CacheManager;
use crate::common::metrics::MetricsCollector;
use crate::common::*;
use crate::services::*;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::Utc;
use hmac::{Hmac, Mac};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::sync::Arc;

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone)]
pub struct AdminRegistrationService {
    auth_service: AuthService,
    config: AdminRegistrationConfig,
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
    pub fn new(
        auth_service: AuthService,
        config: AdminRegistrationConfig,
        cache: Arc<CacheManager>,
        metrics: Arc<MetricsCollector>,
    ) -> Self {
        Self {
            auth_service,
            config,
            cache,
            metrics,
        }
    }

    pub async fn generate_nonce(&self) -> ApiResult<NonceResponse> {
        let start = std::time::Instant::now();
        let nonce = {
            let mut rng = rand::thread_rng();
            let mut nonce_bytes = vec![0u8; 64];
            rng.fill_bytes(&mut nonce_bytes);
            URL_SAFE_NO_PAD.encode(&nonce_bytes)
        };

        let now = Utc::now().timestamp();
        let key = format!("admin:register:nonce:{}", nonce);
        let _ = self
            .cache
            .set(&key, &now, self.config.nonce_timeout_seconds)
            .await;

        let duration = start.elapsed().as_secs_f64();
        if let Some(hist) = self.metrics.get_histogram("admin_nonce_duration_seconds") {
            hist.observe(duration);
        } else {
            let hist = self
                .metrics
                .register_histogram("admin_nonce_duration_seconds".to_string());
            hist.observe(duration);
        }

        Ok(NonceResponse { nonce })
    }

    pub async fn register_admin_user(
        &self,
        request: AdminRegisterRequest,
    ) -> ApiResult<AdminRegisterResponse> {
        if !self.config.enabled {
            return Err(ApiError::forbidden(
                "Admin registration is not enabled".to_string(),
            ));
        }

        let start = std::time::Instant::now();

        self.validate_nonce(&request.nonce).await?;
        self.consume_nonce(&request.nonce).await?;
        self.verify_hmac(&request)?;

        let admin = request.admin.unwrap_or(false);
        let displayname = request.displayname.as_deref();

        let (user, access_token, refresh_token, device_id) = self
            .auth_service
            .register(&request.username, &request.password, admin, displayname)
            .await?;

        let duration = start.elapsed().as_secs_f64();
        if let Some(hist) = self
            .metrics
            .get_histogram("admin_register_duration_seconds")
        {
            hist.observe(duration);
        } else {
            let hist = self
                .metrics
                .register_histogram("admin_register_duration_seconds".to_string());
            hist.observe(duration);
        }

        if let Some(counter) = self.metrics.get_counter("admin_register_success_total") {
            counter.inc();
        } else {
            let counter = self
                .metrics
                .register_counter("admin_register_success_total".to_string());
            counter.inc();
        }

        Ok(AdminRegisterResponse {
            access_token,
            refresh_token,
            expires_in: self.auth_service.token_expiry,
            device_id,
            user_id: user.user_id(),
            home_server: self.auth_service.server_name.clone(),
        })
    }

    async fn validate_nonce(&self, nonce: &str) -> ApiResult<()> {
        let key = format!("admin:register:nonce:{}", nonce);
        let exists = self.cache.get::<i64>(&key).await?.is_some();
        if !exists {
            return Err(ApiError::bad_request("Unrecognised nonce".to_string()));
        }

        Ok(())
    }

    async fn consume_nonce(&self, nonce: &str) -> ApiResult<()> {
        let key = format!("admin:register:nonce:{}", nonce);
        let _ = self.cache.delete(&key).await;
        Ok(())
    }

    fn verify_hmac(&self, request: &AdminRegisterRequest) -> ApiResult<()> {
        if self.config.shared_secret.is_empty() {
            return Err(ApiError::internal(
                "Shared secret is not configured".to_string(),
            ));
        }

        let mut mac = HmacSha256::new_from_slice(self.config.shared_secret.as_bytes())
            .map_err(|_| ApiError::internal("Invalid shared secret".to_string()))?;

        mac.update(request.nonce.as_bytes());
        mac.update(b"\0");
        mac.update(request.username.as_bytes());
        mac.update(b"\0");
        mac.update(request.password.as_bytes());
        mac.update(b"\0");

        if request.admin.unwrap_or(false) {
            mac.update(b"admin");
        } else {
            mac.update(b"notadmin");
        }

        if let Some(user_type) = &request.user_type {
            mac.update(b"\0");
            mac.update(user_type.as_bytes());
        }

        let expected_mac = mac.finalize().into_bytes();
        let expected_hex = expected_mac
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>();

        if !constant_time_eq(&expected_hex, &request.mac) {
            return Err(ApiError::forbidden("HMAC incorrect".to_string()));
        }

        Ok(())
    }
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }

    let mut result = 0u8;
    for (x, y) in a.bytes().zip(b.bytes()) {
        result |= x ^ y;
    }

    result == 0
}
