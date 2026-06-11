use super::AuthService;
use synapse_common::*;
use synapse_storage::User;

impl AuthService {
    pub async fn register(
        &self,
        username: &str,
        password: &str,
        admin: bool,
        displayname: Option<&str>,
    ) -> ApiResult<(User, String, String, String)> {
        let start = std::time::Instant::now();
        let result = self.register_internal(username, password, admin, displayname, None).await;

        let duration = start.elapsed().as_secs_f64();
        if let Some(hist) = self.metrics.get_histogram("auth_register_duration_seconds") {
            hist.observe(duration);
        } else {
            let hist = self.metrics.register_histogram("auth_register_duration_seconds".to_string());
            hist.observe(duration);
        }

        if result.is_ok() {
            self.increment_counter("auth_register_success_total");
        } else {
            self.increment_counter("auth_register_failure_total");
        }
        result
    }

    pub async fn register_with_device_name(
        &self,
        username: &str,
        password: &str,
        admin: bool,
        displayname: Option<&str>,
        initial_device_display_name: Option<&str>,
    ) -> ApiResult<(User, String, String, String)> {
        let start = std::time::Instant::now();
        let result = self.register_internal(username, password, admin, displayname, initial_device_display_name).await;

        let duration = start.elapsed().as_secs_f64();
        if let Some(hist) = self.metrics.get_histogram("auth_register_duration_seconds") {
            hist.observe(duration);
        } else {
            let hist = self.metrics.register_histogram("auth_register_duration_seconds".to_string());
            hist.observe(duration);
        }

        if result.is_ok() {
            self.increment_counter("auth_register_success_total");
        } else {
            self.increment_counter("auth_register_failure_total");
        }
        result
    }

    async fn register_internal(
        &self,
        username: &str,
        password: &str,
        admin: bool,
        displayname: Option<&str>,
        initial_device_display_name: Option<&str>,
    ) -> ApiResult<(User, String, String, String)> {
        if username.is_empty() || password.is_empty() {
            return Err(ApiError::missing_param("Username and password are required".to_string()));
        }
        if let Err(e) = self.validator.validate_username(username) {
            return Err(ApiError::invalid_username(e.to_string()));
        }
        if let Err(e) = self.validator.validate_password(password) {
            return Err(ApiError::invalid_param(format!("Password does not meet policy requirements: {e}")));
        }

        let password_hash = self.hash_password(password)?;

        let user_id = format!("@{username}:{}", self.server_name);

        let user =
            self.user_storage.create_user(&user_id, username, Some(&password_hash), admin).await.map_err(|e| {
                if e.to_string().contains("duplicate key") || e.to_string().contains("unique constraint") {
                    ApiError::user_in_use("Username already exists".to_string())
                } else {
                    ApiError::internal_with_log("Failed to create user", &e)
                }
            })?;

        if let Some(name) = displayname {
            if let Err(e) = self.user_storage.update_displayname(&user.user_id, Some(name)).await {
                ::tracing::warn!(
                    error = %e,
                    user_id = %user.user_id,
                    displayname = %name,
                    "Failed to set displayname during registration"
                );
            }
        }

        let device_id = self.get_or_create_device_id(None, &user, initial_device_display_name).await?;

        let access_token = self.generate_access_token(&user.user_id, &device_id, user.is_admin).await?;
        let refresh_token = self.generate_refresh_token(&user.user_id, &device_id).await?;

        ::tracing::info!(
            target: "security_audit",
            event = "user_registered",
            user_id = user.user_id(),
            is_admin = admin,
            "New user registered"
        );

        Ok((user, access_token, refresh_token, device_id))
    }
}
