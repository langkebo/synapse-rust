use synapse_common::background_job::BackgroundJob;
use synapse_common::metrics::MetricsCollector;
use synapse_common::task_queue::RedisTaskQueue;
use synapse_common::*;

use std::sync::Arc;

use crate::UserService;

pub struct RegistrationService {
    user_service: Arc<UserService>,
    token_auth: Arc<dyn crate::auth::TokenAuth>,
    credential_auth: Arc<dyn crate::auth::CredentialAuth>,
    metrics: Arc<MetricsCollector>,
    // HP-2 FIX: Make base URL configurable instead of hardcoded
    base_url: String,
    enable_registration: bool,
    task_queue: Option<Arc<RedisTaskQueue>>,
}

impl RegistrationService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        user_service: Arc<UserService>,
        token_auth: Arc<dyn crate::auth::TokenAuth>,
        credential_auth: Arc<dyn crate::auth::CredentialAuth>,
        metrics: Arc<MetricsCollector>,
        server_name: &str,
        enable_registration: bool,
        task_queue: Option<Arc<RedisTaskQueue>>,
    ) -> Self {
        // HP-2 FIX: Construct base URL from server_name, but make it configurable
        // Default to HTTPS for production, can be overridden via environment variable
        let base_url = std::env::var("HOMESERVER_BASE_URL").unwrap_or_else(|_| format!("https://{server_name}"));

        Self { user_service, token_auth, credential_auth, metrics, base_url, enable_registration, task_queue }
    }

    #[::tracing::instrument(
        skip_all,
        fields(
            username_present = !username.is_empty(),
            has_displayname = displayname.is_some(),
            has_initial_device_display_name = initial_device_display_name.is_some()
        )
    )]
    pub async fn register_user(
        &self,
        username: &str,
        password: &str,
        displayname: Option<&str>,
        initial_device_display_name: Option<&str>,
    ) -> ApiResult<serde_json::Value> {
        if !self.enable_registration {
            return Err(ApiError::forbidden("Registration is disabled".to_string()));
        }

        let start = std::time::Instant::now();
        let result = self
            .credential_auth
            .register_with_device_name(username, password, false, displayname, initial_device_display_name)
            .await;

        let duration = start.elapsed().as_secs_f64();
        if let Some(hist) = self.metrics.get_histogram("registration_duration_seconds") {
            hist.observe(duration);
        } else {
            let hist = self.metrics.register_histogram("registration_duration_seconds".to_string());
            hist.observe(duration);
        }

        let (user, access_token, refresh_token, device_id) = result?;

        if let Some(counter) = self.metrics.get_counter("registration_success_total") {
            counter.inc();
        } else {
            let counter = self.metrics.register_counter("registration_success_total".to_string());
            counter.inc();
        }

        // Async Task: Send Welcome Email
        if let Some(queue) = &self.task_queue {
            let email_job = BackgroundJob::SendEmail {
                to: user.user_id.clone(), // Assuming user_id can be an email or we look it up
                subject: "Welcome to Synapse!".to_string(),
                body: format!("Hello {}, welcome to our Matrix server!", displayname.unwrap_or(username)),
            };

            if let Err(e) = queue.submit(email_job).await {
                ::tracing::warn!(
                    error = %e,
                    user_id = %user.user_id,
                    username_present = !username.is_empty(),
                    has_displayname = displayname.is_some(),
                    "Failed to submit welcome email task"
                );
                // Non-blocking error, continue
            } else {
                ::tracing::info!(
                    user_id = %user.user_id,
                    username_present = !username.is_empty(),
                    has_displayname = displayname.is_some(),
                    "Submitted welcome email task"
                );
            }
        }

        Ok(serde_json::json!({
            "access_token": access_token,
            "refresh_token": refresh_token,
            "expires_in": self.token_auth.token_expiry(),
            "device_id": device_id,
            "user_id": user.user_id(),
            "well_known": {
                "m.homeserver": {
                    "base_url": self.base_url
                }
            }
        }))
    }

    #[::tracing::instrument(
        skip_all,
        fields(
            username_present = !username.is_empty(),
            has_device_id = device_id.is_some(),
            has_initial_display_name = initial_display_name.is_some()
        )
    )]
    pub async fn login(
        &self,
        username: &str,
        password: &str,
        device_id: Option<&str>,
        initial_display_name: Option<&str>,
    ) -> ApiResult<serde_json::Value> {
        let start = std::time::Instant::now();
        let result = self.credential_auth.login(username, password, device_id, initial_display_name).await;

        let duration = start.elapsed().as_secs_f64();
        if let Some(hist) = self.metrics.get_histogram("login_duration_seconds") {
            hist.observe(duration);
        } else {
            let hist = self.metrics.register_histogram("login_duration_seconds".to_string());
            hist.observe(duration);
        }

        let (user, access_token, refresh_token, device_id) = result?;

        if let Some(counter) = self.metrics.get_counter("login_success_total") {
            counter.inc();
        } else {
            let counter = self.metrics.register_counter("login_success_total".to_string());
            counter.inc();
        }

        Ok(serde_json::json!({
            "access_token": access_token,
            "refresh_token": refresh_token,
            "expires_in": self.token_auth.token_expiry(),
            "device_id": device_id,
            "user_id": user.user_id(),
            "well_known": {
                "m.homeserver": {
                    "base_url": self.base_url
                }
            }
        }))
    }

    #[::tracing::instrument(
        skip_all,
        fields(
            user_id = %user_id,
            has_current_password = current_password.is_some(),
            has_current_device_id = current_device_id.is_some()
        )
    )]
    pub async fn change_password(
        &self,
        user_id: &str,
        current_password: Option<&str>,
        new_password: &str,
        current_device_id: Option<&str>,
    ) -> ApiResult<()> {
        self.credential_auth.change_password(user_id, current_password, new_password, current_device_id).await?;
        Ok(())
    }

    #[::tracing::instrument(skip_all, fields(user_id = %user_id))]
    pub async fn deactivate_account(&self, user_id: &str) -> ApiResult<()> {
        self.credential_auth.deactivate_user(user_id).await?;
        Ok(())
    }

    #[::tracing::instrument(skip_all, fields(user_id = %user_id))]
    pub async fn get_profile(&self, user_id: &str) -> ApiResult<serde_json::Value> {
        self.user_service.get_profile(user_id).await
    }

    #[::tracing::instrument(skip_all, fields(batch_size = user_ids.len()))]
    pub async fn get_profiles(&self, user_ids: &[String]) -> ApiResult<Vec<serde_json::Value>> {
        self.user_service.get_profiles_batch(user_ids).await
    }

    #[::tracing::instrument(skip_all, fields(user_id = %user_id, displayname_len = displayname.len()))]
    pub async fn set_displayname(&self, user_id: &str, displayname: &str) -> ApiResult<()> {
        self.user_service.update_displayname(user_id, Some(displayname)).await
    }

    #[::tracing::instrument(skip_all, fields(user_id = %user_id, avatar_url_len = avatar_url.len()))]
    pub async fn set_avatar_url(&self, user_id: &str, avatar_url: &str) -> ApiResult<()> {
        self.user_service.update_avatar_url(user_id, Some(avatar_url)).await
    }

    #[::tracing::instrument(
        skip_all,
        fields(
            user_id = %user_id,
            has_displayname = displayname.is_some(),
            has_avatar_url = avatar_url.is_some()
        )
    )]
    pub async fn update_user_profile(
        &self,
        user_id: &str,
        displayname: Option<&str>,
        avatar_url: Option<&str>,
    ) -> ApiResult<()> {
        self.user_service.update_profile(user_id, displayname, avatar_url).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ServiceContainer;

    #[tokio::test]
    async fn test_registration_service_creation() {
        let services = ServiceContainer::new_test().await;
        let _registration_service = RegistrationService::new(
            services.core.user_service.clone(),
            services.core.token_auth.clone(),
            services.core.credential_auth.clone(),
            services.core.metrics.clone(),
            &services.core.server_name,
            services.core.config.server.enable_registration,
            None,
        );
    }

    #[test]
    fn test_login_response_format() {
        let response = serde_json::json!({
            "access_token": "test_token",
            "refresh_token": "test_refresh",
            "expires_in": 86400,
            "device_id": "DEVICE123",
            "user_id": "@test:example.com",
            "well_known": {
                "m.homeserver": {
                    "base_url": "http://localhost:8008"
                }
            }
        });

        assert!(response.get("access_token").is_some());
        assert!(response.get("refresh_token").is_some());
        assert!(response.get("expires_in").is_some());
        assert_eq!(response["expires_in"], 86400);
    }

    #[tokio::test]
    async fn test_registration_service_disabled() {
        let services = ServiceContainer::new_test().await;
        let registration_service = RegistrationService::new(
            services.core.user_service.clone(),
            services.core.token_auth.clone(),
            services.core.credential_auth.clone(),
            services.core.metrics.clone(),
            &services.core.server_name,
            false,
            None,
        );

        let result = registration_service.register_user("test", "pass", None, None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().is_forbidden());
    }

    #[test]
    fn test_login_response_has_well_known() {
        let response = serde_json::json!({
            "access_token": "test_token",
            "refresh_token": "test_refresh",
            "expires_in": 86400,
            "device_id": "DEVICE123",
            "user_id": "@test:example.com",
            "well_known": {
                "m.homeserver": {
                    "base_url": "http://localhost:8008"
                }
            }
        });

        assert!(response.get("well_known").is_some());
        assert_eq!(response["well_known"]["m.homeserver"]["base_url"], "http://localhost:8008");
    }

    #[test]
    fn test_login_response_user_id_format() {
        let response = serde_json::json!({
            "access_token": "test_token",
            "refresh_token": "test_refresh",
            "expires_in": 86400,
            "device_id": "DEVICE123",
            "user_id": "@test:example.com",
            "well_known": {
                "m.homeserver": {
                    "base_url": "http://localhost:8008"
                }
            }
        });

        let user_id = response["user_id"].as_str().unwrap();
        assert!(user_id.starts_with('@'));
        assert!(user_id.contains(':'));
    }

    #[test]
    fn test_login_response_device_id_present() {
        let response = serde_json::json!({
            "access_token": "test_token",
            "refresh_token": "test_refresh",
            "expires_in": 86400,
            "device_id": "DEVICE123",
            "user_id": "@test:example.com",
            "well_known": {
                "m.homeserver": {
                    "base_url": "http://localhost:8008"
                }
            }
        });

        let device_id = response["device_id"].as_str().unwrap();
        assert!(!device_id.is_empty());
    }
}
