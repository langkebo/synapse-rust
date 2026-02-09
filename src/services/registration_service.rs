use crate::common::metrics::MetricsCollector;
use crate::common::task_queue::RedisTaskQueue;
use crate::common::background_job::BackgroundJob;
use crate::common::*;
use crate::services::*;
use std::sync::Arc;

pub struct RegistrationService {
    user_storage: crate::storage::UserStorage,
    auth_service: crate::auth::AuthService,
    metrics: Arc<MetricsCollector>,
    server_name: String,
    // HP-2 FIX: Make base URL configurable instead of hardcoded
    base_url: String,
    enable_registration: bool,
    task_queue: Option<Arc<RedisTaskQueue>>,
}

impl RegistrationService {
    pub fn new(
        user_storage: crate::storage::UserStorage,
        auth_service: crate::auth::AuthService,
        metrics: Arc<MetricsCollector>,
        server_name: String,
        enable_registration: bool,
        task_queue: Option<Arc<RedisTaskQueue>>,
    ) -> Self {
        // HP-2 FIX: Construct base URL from server_name, but make it configurable
        // Default to HTTPS for production, can be overridden via environment variable
        let base_url = std::env::var("HOMESERVER_BASE_URL")
            .unwrap_or_else(|_| format!("https://{}:8448", server_name));

        Self {
            user_storage,
            auth_service,
            metrics,
            server_name,
            base_url,
            enable_registration,
            task_queue,
        }
    }

    pub async fn register_user(
        &self,
        username: &str,
        password: &str,
        admin: bool,
        displayname: Option<&str>,
    ) -> ApiResult<serde_json::Value> {
        if !self.enable_registration {
            return Err(ApiError::forbidden("Registration is disabled".to_string()));
        }

        let start = std::time::Instant::now();
        let result = self
            .auth_service
            .register(username, password, admin, displayname)
            .await;

        let duration = start.elapsed().as_secs_f64();
        if let Some(hist) = self.metrics.get_histogram("registration_duration_seconds") {
            hist.observe(duration);
        } else {
            let hist = self
                .metrics
                .register_histogram("registration_duration_seconds".to_string());
            hist.observe(duration);
        }

        let (user, access_token, refresh_token, device_id) = result?;

        if let Some(counter) = self.metrics.get_counter("registration_success_total") {
            counter.inc();
        } else {
            let counter = self
                .metrics
                .register_counter("registration_success_total".to_string());
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
                ::tracing::warn!("Failed to submit welcome email task: {}", e);
                // Non-blocking error, continue
            } else {
                ::tracing::info!("Submitted welcome email task for user {}", user.user_id);
            }
        }

        Ok(serde_json::json!({
            "access_token": access_token,
            "refresh_token": refresh_token,
            "expires_in": self.auth_service.token_expiry,
            "device_id": device_id,
            "user_id": user.user_id(),
            "well_known": {
                "m.homeserver": {
                    "base_url": self.base_url
                }
            }
        }))
    }

    pub async fn login(
        &self,
        username: &str,
        password: &str,
        device_id: Option<&str>,
        initial_display_name: Option<&str>,
    ) -> ApiResult<serde_json::Value> {
        let start = std::time::Instant::now();
        let result = self
            .auth_service
            .login(username, password, device_id, initial_display_name)
            .await;

        let duration = start.elapsed().as_secs_f64();
        if let Some(hist) = self.metrics.get_histogram("login_duration_seconds") {
            hist.observe(duration);
        } else {
            let hist = self
                .metrics
                .register_histogram("login_duration_seconds".to_string());
            hist.observe(duration);
        }

        let (user, access_token, refresh_token, device_id) = result?;

        if let Some(counter) = self.metrics.get_counter("login_success_total") {
            counter.inc();
        } else {
            let counter = self
                .metrics
                .register_counter("login_success_total".to_string());
            counter.inc();
        }

        Ok(serde_json::json!({
            "access_token": access_token,
            "refresh_token": refresh_token,
            "expires_in": self.auth_service.token_expiry,
            "device_id": device_id,
            "user_id": user.user_id(),
            "well_known": {
                "m.homeserver": {
                    "base_url": self.base_url
                }
            }
        }))
    }

    pub async fn change_password(&self, user_id: &str, new_password: &str) -> ApiResult<()> {
        self.auth_service
            .change_password(user_id, new_password)
            .await?;
        Ok(())
    }

    pub async fn deactivate_account(&self, user_id: &str) -> ApiResult<()> {
        self.auth_service.deactivate_user(user_id).await?;
        Ok(())
    }

    pub async fn get_profile(&self, user_id: &str) -> ApiResult<serde_json::Value> {
        let user = self
            .user_storage
            .get_user_by_id(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get user: {}", e)))?;

        match user {
            Some(u) => Ok(serde_json::json!({
                "user_id": u.user_id,
                "displayname": u.displayname,
                "avatar_url": u.avatar_url
            })),
            _ => Err(ApiError::not_found("User not found".to_string())),
        }
    }

    pub async fn get_profiles(&self, user_ids: &[String]) -> ApiResult<Vec<serde_json::Value>> {
        let profiles = self
            .user_storage
            .get_user_profiles_batch(user_ids)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get profiles: {}", e)))?;

        Ok(profiles
            .into_iter()
            .map(|u| {
                serde_json::json!({
                    "user_id": u.user_id,
                    "displayname": u.displayname,
                    "avatar_url": u.avatar_url
                })
            })
            .collect())
    }

    pub async fn set_displayname(&self, user_id: &str, displayname: &str) -> ApiResult<()> {
        self.user_storage
            .update_displayname(user_id, Some(displayname))
            .await
            .map_err(|e| {
                if e.to_string().contains("too long") {
                    ApiError::bad_request("Displayname too long (max 255 characters)".to_string())
                } else {
                    ApiError::internal(format!("Failed to update displayname: {}", e))
                }
            })?;
        Ok(())
    }

    pub async fn set_avatar_url(&self, user_id: &str, avatar_url: &str) -> ApiResult<()> {
        self.user_storage
            .update_avatar_url(user_id, Some(avatar_url))
            .await
            .map_err(|e| {
                if e.to_string().contains("too long") {
                    ApiError::bad_request("Avatar URL too long (max 255 characters)".to_string())
                } else {
                    ApiError::internal(format!("Failed to update avatar: {}", e))
                }
            })?;
        Ok(())
    }

    pub async fn update_user_profile(
        &self,
        user_id: &str,
        displayname: Option<&str>,
        avatar_url: Option<&str>,
    ) -> ApiResult<()> {
        if let Some(name) = displayname {
            self.set_displayname(user_id, name).await?;
        }
        if let Some(url) = avatar_url {
            self.set_avatar_url(user_id, url).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_registration_service_creation() {
        let services = ServiceContainer::new_test();
        let _registration_service = RegistrationService::new(
            services.user_storage.clone(),
            services.auth_service.clone(),
            services.metrics.clone(),
            services.server_name.clone(),
            services.config.server.enable_registration,
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
        let services = ServiceContainer::new_test();
        let registration_service = RegistrationService::new(
            services.user_storage.clone(),
            services.auth_service.clone(),
            services.metrics.clone(),
            services.server_name.clone(),
            false, // disabled
            None,
        );

        let result = registration_service
            .register_user("test", "pass", false, None)
            .await;
        assert!(matches!(result, Err(ApiError::Forbidden { .. })));
    }
}
