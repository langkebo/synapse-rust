use crate::auth::AuthService;
use crate::common::*;
use crate::storage::User;

#[allow(async_fn_in_trait)]
pub trait GuestAuthExt {
    async fn register_guest_account(&self) -> ApiResult<(User, String, String)>;
    async fn require_guest_user(&self, user_id: &str) -> ApiResult<User>;
    async fn upgrade_guest_account(
        &self,
        user_id: &str,
        device_id: Option<&str>,
        username: &str,
        password: &str,
    ) -> ApiResult<String>;
}

impl GuestAuthExt for AuthService {
    async fn register_guest_account(&self) -> ApiResult<(User, String, String)> {
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

    async fn require_guest_user(&self, user_id: &str) -> ApiResult<User> {
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

    async fn upgrade_guest_account(
        &self,
        user_id: &str,
        device_id: Option<&str>,
        username: &str,
        password: &str,
    ) -> ApiResult<String> {
        self.validator.validate_username(username)?;
        self.validator.validate_password(password)?;

        let guest_user = self.require_guest_user(user_id).await?;
        let existing = self
            .user_storage
            .get_user_by_username(username)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check username", &e))?;

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
}
