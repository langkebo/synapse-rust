use async_trait::async_trait;
use std::sync::Arc;
use synapse_common::ApiError;
use synapse_storage::registration_token::*;
use tracing::{info, instrument};

#[async_trait]
pub trait RegistrationTokenApi: Send + Sync {
    async fn create_token(&self, request: CreateRegistrationTokenRequest) -> Result<RegistrationToken, ApiError>;
    async fn get_token(&self, token: &str) -> Result<Option<RegistrationToken>, ApiError>;
    async fn delete_token(&self, id: i64) -> Result<(), ApiError>;
    async fn update_token(
        &self,
        id: i64,
        request: UpdateRegistrationTokenRequest,
    ) -> Result<RegistrationToken, ApiError>;
}

pub struct RegistrationTokenService {
    storage: Arc<dyn RegistrationTokenStoreApi>,
}

impl RegistrationTokenService {
    pub fn new(storage: Arc<dyn RegistrationTokenStoreApi>) -> Self {
        Self { storage }
    }

    #[instrument(skip(self))]
    pub async fn create_token(&self, request: CreateRegistrationTokenRequest) -> Result<RegistrationToken, ApiError> {
        info!("Creating registration token");

        if let Some(ref token) = request.token {
            if self
                .storage
                .get_token(token)
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to check token", &e))?
                .is_some()
            {
                return Err(ApiError::bad_request("Token already exists"));
            }
        }

        let token = self
            .storage
            .create_token(request)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to create token", &e))?;

        let token_preview: String = token.token.chars().take(4).collect();
        info!(token_preview = %format!("{token_preview}***"), "Created registration token");

        Ok(token)
    }

    #[instrument(skip(self))]
    pub async fn get_token(&self, token: &str) -> Result<Option<RegistrationToken>, ApiError> {
        let token =
            self.storage.get_token(token).await.map_err(|e| ApiError::internal_with_log("Failed to get token", &e))?;

        Ok(token)
    }

    #[instrument(skip(self))]
    pub async fn get_token_by_id(&self, id: i64) -> Result<Option<RegistrationToken>, ApiError> {
        let token = self
            .storage
            .get_token_by_id(id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get token", &e))?;

        Ok(token)
    }

    #[instrument(skip(self))]
    pub async fn validate_token(&self, token: &str) -> Result<TokenValidationResult, ApiError> {
        let result = self
            .storage
            .validate_token(token)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to validate token", &e))?;

        Ok(result)
    }

    #[instrument(skip(self))]
    pub async fn use_token(
        &self,
        token: &str,
        user_id: &str,
        username: Option<&str>,
        email: Option<&str>,
        ip_address: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<bool, ApiError> {
        info!(user_id = %user_id, "Using registration token");

        let validation = self.validate_token(token).await?;

        if !validation.is_valid {
            return Err(ApiError::bad_request(validation.error_message.unwrap_or_else(|| "Invalid token".to_string())));
        }

        let success = self
            .storage
            .use_token(token, user_id, username, email, ip_address, user_agent)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to use token", &e))?;

        if !success {
            return Err(ApiError::bad_request("Failed to use token"));
        }

        info!(user_id = %user_id, "Used registration token");

        Ok(true)
    }

    #[instrument(skip(self))]
    pub async fn update_token(
        &self,
        id: i64,
        request: UpdateRegistrationTokenRequest,
    ) -> Result<RegistrationToken, ApiError> {
        let _existing = self
            .storage
            .get_token_by_id(id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check token", &e))?
            .ok_or_else(|| ApiError::not_found("Token not found"))?;

        let token = self
            .storage
            .update_token(id, request)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to update token", &e))?;

        Ok(token)
    }

    #[instrument(skip(self))]
    pub async fn delete_token(&self, id: i64) -> Result<(), ApiError> {
        let _existing = self
            .storage
            .get_token_by_id(id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check token", &e))?
            .ok_or_else(|| ApiError::not_found("Token not found"))?;

        self.storage.delete_token(id).await.map_err(|e| ApiError::internal_with_log("Failed to delete token", &e))?;

        info!(token_id = id, "Deleted registration token");

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn deactivate_token(&self, id: i64) -> Result<(), ApiError> {
        self.storage
            .deactivate_token(id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to deactivate token", &e))?;

        info!(token_id = id, "Deactivated registration token");

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn get_all_tokens(
        &self,
        limit: i64,
        from: Option<RegistrationTokenCursor>,
    ) -> Result<(Vec<RegistrationToken>, Option<String>), ApiError> {
        let tokens = self
            .storage
            .get_all_tokens(limit, from)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get tokens", &e))?;

        Ok(tokens)
    }

    #[instrument(skip(self))]
    pub async fn get_active_tokens(&self) -> Result<Vec<RegistrationToken>, ApiError> {
        let tokens = self
            .storage
            .get_active_tokens()
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get active tokens", &e))?;

        Ok(tokens)
    }

    #[instrument(skip(self))]
    pub async fn get_token_usage(&self, token_id: i64) -> Result<Vec<RegistrationTokenUsage>, ApiError> {
        let usage = self
            .storage
            .get_token_usage(token_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get token usage", &e))?;

        Ok(usage)
    }

    #[instrument(skip(self))]
    pub async fn cleanup_expired_tokens(&self) -> Result<i64, ApiError> {
        info!("Cleaning up expired registration tokens");

        let count = self
            .storage
            .cleanup_expired_tokens()
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to cleanup tokens", &e))?;

        info!(expired_token_count = count, "Cleaned up expired registration tokens");

        Ok(count)
    }

    #[instrument(skip(self))]
    pub async fn create_room_invite(&self, request: CreateRoomInviteRequest) -> Result<RoomInvite, ApiError> {
        info!(room_id = %request.room_id, "Creating room invite");

        let invite = self
            .storage
            .create_room_invite(request)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to create room invite", &e))?;

        Ok(invite)
    }

    #[instrument(skip(self))]
    pub async fn get_room_invite(&self, invite_code: &str) -> Result<Option<RoomInvite>, ApiError> {
        let invite = self
            .storage
            .get_room_invite(invite_code)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get room invite", &e))?;

        Ok(invite)
    }

    #[instrument(skip(self))]
    pub async fn use_room_invite(&self, invite_code: &str, invitee_user_id: &str) -> Result<bool, ApiError> {
        info!(invitee_user_id = %invitee_user_id, "Using room invite");

        let success = self
            .storage
            .use_room_invite(invite_code, invitee_user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to use room invite", &e))?;

        if !success {
            return Err(ApiError::bad_request("Invalid or expired room invite"));
        }

        Ok(true)
    }

    #[instrument(skip(self))]
    pub async fn revoke_room_invite(&self, invite_code: &str, reason: &str) -> Result<(), ApiError> {
        self.storage
            .revoke_room_invite(invite_code, reason)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to revoke room invite", &e))?;

        info!(
            invite_code_prefix = %invite_code.chars().take(6).collect::<String>(),
            reason = %reason,
            "Revoked room invite"
        );

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn create_batch(
        &self,
        count: i32,
        description: Option<String>,
        expires_at: Option<i64>,
        created_by: Option<String>,
        allowed_email_domains: Option<Vec<String>>,
        auto_join_rooms: Option<Vec<String>>,
    ) -> Result<(String, Vec<String>), ApiError> {
        info!(token_count = count, "Creating registration token batch");

        let batch_id = uuid::Uuid::new_v4().to_string();
        let mut tokens = Vec::new();

        for _ in 0..count {
            let token = RegistrationTokenStorage::generate_token();
            tokens.push(token);
        }

        let batch = RegistrationTokenBatch {
            id: 0,
            batch_id: batch_id.clone(),
            description: description.clone(),
            token_count: count,
            tokens_used: 0,
            created_by: created_by.clone(),
            created_ts: 0,
            expires_at,
            is_enabled: true,
            allowed_email_domains: allowed_email_domains.clone(),
            auto_join_rooms: auto_join_rooms.clone(),
        };

        self.storage
            .create_batch(&batch, &tokens)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to create batch", &e))?;

        info!(batch_id = %batch_id, token_count = count, "Created registration token batch");

        Ok((batch_id, tokens))
    }

    #[instrument(skip(self))]
    pub async fn get_batch(&self, batch_id: &str) -> Result<Option<RegistrationTokenBatch>, ApiError> {
        let batch = self
            .storage
            .get_batch(batch_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get batch", &e))?;

        Ok(batch)
    }

    pub async fn check_email_domain_allowed(&self, token: &str, email: &str) -> Result<bool, ApiError> {
        let token_record = self
            .storage
            .get_token(token)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get token", &e))?
            .ok_or_else(|| ApiError::not_found("Token not found"))?;

        if let Some(domains) = token_record.allowed_email_domains {
            if domains.is_empty() {
                return Ok(true);
            }

            let email_domain = email.split('@').next_back().unwrap_or("");
            return Ok(domains.iter().any(|d| d.to_lowercase() == email_domain.to_lowercase()));
        }

        Ok(true)
    }
}

#[async_trait]
impl RegistrationTokenApi for RegistrationTokenService {
    async fn create_token(&self, request: CreateRegistrationTokenRequest) -> Result<RegistrationToken, ApiError> {
        self.create_token(request).await
    }

    async fn get_token(&self, token: &str) -> Result<Option<RegistrationToken>, ApiError> {
        self.get_token(token).await
    }

    async fn delete_token(&self, id: i64) -> Result<(), ApiError> {
        self.delete_token(id).await
    }

    async fn update_token(
        &self,
        id: i64,
        request: UpdateRegistrationTokenRequest,
    ) -> Result<RegistrationToken, ApiError> {
        self.update_token(id, request).await
    }
}

#[cfg(test)]
mod tests {
    fn create_test_token() -> synapse_storage::registration_token::RegistrationToken {
        synapse_storage::registration_token::RegistrationToken {
            id: 1,
            token: "test_token_123".to_string(),
            token_type: "single".to_string(),
            description: Some("Test token".to_string()),
            max_uses: 1,
            uses_count: 0,
            is_used: false,
            is_enabled: true,
            expires_at: None,
            created_by: Some("@admin:example.com".to_string()),
            created_ts: 1234567890,
            updated_ts: Some(1234567890),
            last_used_ts: None,
            allowed_email_domains: None,
            allowed_user_ids: None,
            auto_join_rooms: None,
            display_name: None,
            email: None,
        }
    }

    #[test]
    fn test_registration_token_structure() {
        let token = create_test_token();
        assert_eq!(token.id, 1);
        assert_eq!(token.token, "test_token_123");
        assert_eq!(token.token_type, "single");
        assert!(!token.is_used);
        assert!(token.is_enabled);
    }

    #[test]
    fn test_registration_token_max_uses() {
        let token = create_test_token();
        assert_eq!(token.max_uses, 1);
        assert_eq!(token.uses_count, 0);
        assert!(token.uses_count < token.max_uses);
    }

    #[test]
    fn test_create_registration_token_request() {
        let request = synapse_storage::registration_token::CreateRegistrationTokenRequest {
            token: Some("custom_token".to_string()),
            token_type: Some("multi".to_string()),
            description: Some("Multi-use token".to_string()),
            max_uses: Some(10),
            expires_at: Some(9999999999),
            created_by: Some("@admin:example.com".to_string()),
            allowed_email_domains: Some(vec!["example.com".to_string()]),
            allowed_user_ids: None,
            auto_join_rooms: Some(vec!["!room:example.com".to_string()]),
            display_name: None,
            email: None,
        };
        assert_eq!(request.token, Some("custom_token".to_string()));
        assert_eq!(request.max_uses, Some(10));
        assert!(request.allowed_email_domains.is_some());
    }

    #[test]
    fn test_update_registration_token_request() {
        let request = synapse_storage::registration_token::UpdateRegistrationTokenRequest {
            description: Some("Updated description".to_string()),
            max_uses: Some(5),
            is_enabled: Some(false),
            expires_at: Some(8888888888),
        };
        assert_eq!(request.description, Some("Updated description".to_string()));
        assert_eq!(request.is_enabled, Some(false));
    }

    #[test]
    fn test_update_registration_token_request_default() {
        let request = synapse_storage::registration_token::UpdateRegistrationTokenRequest::default();
        assert!(request.description.is_none());
        assert!(request.max_uses.is_none());
        assert!(request.is_enabled.is_none());
    }

    #[test]
    fn test_room_invite_structure() {
        let invite = synapse_storage::registration_token::RoomInvite {
            id: 1,
            invite_code: "INVITE123".to_string(),
            room_id: "!room:example.com".to_string(),
            inviter_user_id: "@user:example.com".to_string(),
            invitee_email: Some("invitee@example.com".to_string()),
            invitee_user_id: None,
            is_used: false,
            is_revoked: false,
            expires_at: Some(9999999999),
            created_ts: 1234567890,
            used_ts: None,
            revoked_at: None,
            revoked_reason: None,
        };
        assert_eq!(invite.invite_code, "INVITE123");
        assert!(!invite.is_used);
        assert!(!invite.is_revoked);
    }

    #[test]
    fn test_create_room_invite_request() {
        let request = synapse_storage::registration_token::CreateRoomInviteRequest {
            room_id: "!room:example.com".to_string(),
            inviter_user_id: "@user:example.com".to_string(),
            invitee_email: Some("invitee@example.com".to_string()),
            expires_at: Some(9999999999),
        };
        assert_eq!(request.room_id, "!room:example.com");
        assert!(request.invitee_email.is_some());
    }

    #[test]
    fn test_registration_token_batch() {
        let batch = synapse_storage::registration_token::RegistrationTokenBatch {
            id: 1,
            batch_id: "batch-uuid".to_string(),
            description: Some("Batch of tokens".to_string()),
            token_count: 10,
            tokens_used: 3,
            created_by: Some("@admin:example.com".to_string()),
            created_ts: 1234567890,
            expires_at: None,
            is_enabled: true,
            allowed_email_domains: None,
            auto_join_rooms: None,
        };
        assert_eq!(batch.token_count, 10);
        assert_eq!(batch.tokens_used, 3);
        assert!(batch.is_enabled);
    }

    #[test]
    fn test_registration_token_usage() {
        let usage = synapse_storage::registration_token::RegistrationTokenUsage {
            id: 1,
            token_id: Some(1),
            token: "test_token".to_string(),
            user_id: "@user:example.com".to_string(),
            username: Some("user".to_string()),
            email: Some("user@example.com".to_string()),
            ip_address: Some("127.0.0.1".to_string()),
            user_agent: Some("Mozilla/5.0".to_string()),
            used_ts: 1234567890,
            is_success: true,
            error_message: None,
        };
        assert!(usage.is_success);
        assert!(usage.error_message.is_none());
    }

    #[test]
    fn test_token_with_allowed_domains() {
        let mut token = create_test_token();
        token.allowed_email_domains = Some(vec!["example.com".to_string(), "test.com".to_string()]);

        let domains = token.allowed_email_domains.unwrap();
        assert_eq!(domains.len(), 2);
        assert!(domains.contains(&"example.com".to_string()));
    }

    #[test]
    fn test_token_with_auto_join_rooms() {
        let mut token = create_test_token();
        token.auto_join_rooms = Some(vec!["!room1:example.com".to_string(), "!room2:example.com".to_string()]);

        let rooms = token.auto_join_rooms.unwrap();
        assert_eq!(rooms.len(), 2);
    }
}
