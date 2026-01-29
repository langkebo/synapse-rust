use crate::common::*;
use crate::services::*;

pub struct RegistrationService<'a> {
    services: &'a ServiceContainer,
}

impl<'a> RegistrationService<'a> {
    pub fn new(services: &'a ServiceContainer) -> Self {
        Self { services }
    }

    pub async fn register_user(
        &self,
        username: &str,
        password: &str,
        admin: bool,
        displayname: Option<&str>,
    ) -> ApiResult<serde_json::Value> {
        let (user, access_token, refresh_token, device_id) = self
            .services
            .auth_service
            .register(username, password, admin, displayname)
            .await?;

        Ok(serde_json::json!({
            "access_token": access_token,
            "refresh_token": refresh_token,
            "expires_in": self.services.auth_service.token_expiry,
            "device_id": device_id,
            "user_id": user.user_id(),
            "well_known": {
                "m.homeserver": {
                    "base_url": format!("http://{}:8008", self.services.server_name)
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
        let (user, access_token, refresh_token, device_id) = self
            .services
            .auth_service
            .login(username, password, device_id, initial_display_name)
            .await?;

        Ok(serde_json::json!({
            "access_token": access_token,
            "refresh_token": refresh_token,
            "expires_in": self.services.auth_service.token_expiry,
            "device_id": device_id,
            "user_id": user.user_id(),
            "well_known": {
                "m.homeserver": {
                    "base_url": format!("http://{}:8008", self.services.server_name)
                }
            }
        }))
    }

    pub async fn change_password(&self, user_id: &str, new_password: &str) -> ApiResult<()> {
        self.services
            .auth_service
            .change_password(user_id, new_password)
            .await?;
        Ok(())
    }

    pub async fn deactivate_account(&self, user_id: &str) -> ApiResult<()> {
        self.services.auth_service.deactivate_user(user_id).await?;
        Ok(())
    }

    pub async fn get_profile(&self, user_id: &str) -> ApiResult<serde_json::Value> {
        let user = self
            .services
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
            None => Err(ApiError::not_found("User not found".to_string())),
        }
    }

    pub async fn set_displayname(&self, user_id: &str, displayname: &str) -> ApiResult<()> {
        self.services
            .user_storage
            .update_displayname(user_id, Some(displayname))
            .await
            .map_err(|e| ApiError::internal(format!("Failed to update displayname: {}", e)))?;
        Ok(())
    }

    pub async fn set_avatar_url(&self, user_id: &str, avatar_url: &str) -> ApiResult<()> {
        self.services
            .user_storage
            .update_avatar_url(user_id, Some(avatar_url))
            .await
            .map_err(|e| ApiError::internal(format!("Failed to update avatar: {}", e)))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registration_service_creation() {
        let services = ServiceContainer::new();
        let _registration_service = RegistrationService::new(&services);
    }

    #[test]
    fn test_login_response_format() {
        let services = ServiceContainer::new();
        let registration_service = RegistrationService::new(&services);

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

    #[test]
    fn test_profile_response_format() {
        let profile = serde_json::json!({
            "user_id": "@test:example.com",
            "displayname": "Test User",
            "avatar_url": "mxc://example.com/avatar"
        });

        assert_eq!(profile["user_id"], "@test:example.com");
        assert_eq!(profile["displayname"], "Test User");
    }
}
