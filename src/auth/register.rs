use super::AuthService;
use crate::common::*;
use crate::storage::User;

impl AuthService {
    pub async fn register_guest_account(&self) -> ApiResult<(User, String, String)> {
        let guest_num = rand::random::<u64>();
        let username = format!("guest_{guest_num}");
        let user_id = format!("@{}:{}", username, self.server_name);
        let device_id = format!("guest_device_{guest_num}");

        let user = self.user_storage.create_user(&user_id, &username, None, false).await.map_err(|e| {
            if e.to_string().contains("duplicate key") || e.to_string().contains("unique constraint") {
                ApiError::user_in_use("Username already exists".to_string())
            } else {
                ApiError::internal_with_log("Failed to create guest user", &e)
            }
        })?;

        self.user_storage
            .set_guest_status(&user.user_id, true)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to mark guest user", &e))?;

        self.device_storage
            .create_device(&device_id, &user.user_id, Some("Guest Device"))
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to create device", &e))?;

        let access_token = self
            .generate_access_token(&user.user_id, &device_id, false)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to generate guest token", &e))?;

        Ok((user, device_id, access_token))
    }

    pub async fn require_guest_user(&self, user_id: &str) -> ApiResult<User> {
        let user = self
            .user_storage
            .get_user_by_id(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get user", &e))?
            .ok_or_else(|| ApiError::not_found("User not found".to_string()))?;

        if !user.is_guest {
            return Err(ApiError::forbidden("User is not a guest".to_string()));
        }

        Ok(user)
    }

    pub async fn upgrade_guest_account(
        &self,
        user_id: &str,
        device_id: Option<&str>,
        username: &str,
        password: &str,
    ) -> ApiResult<String> {
        self.validator.validate_username(username)?;
        self.validator.validate_password(password)?;

        let guest_user = self.require_guest_user(user_id).await?;
        let existing =
            self.user_storage.get_user_by_username(username).await.map_err(|e| ApiError::internal_with_log("Failed to check username", &e))?;

        if existing.as_ref().is_some_and(|user| user.user_id != user_id) {
            return Err(ApiError::conflict("Username already exists".to_string()));
        }

        let password_hash = self.hash_password_for_storage(password).await?;
        self.user_storage
            .upgrade_guest_account(&guest_user.user_id, username, &password_hash)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to upgrade account", &e))?;

        self.generate_access_token(&guest_user.user_id, device_id.unwrap_or(""), false)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to generate token", &e))
    }

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
                ::tracing::warn!("Failed to set displayname during registration: {}", e);
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
