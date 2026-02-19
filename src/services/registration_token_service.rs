use crate::common::ApiError;
use crate::storage::registration_token::*;
use std::sync::Arc;
use tracing::{info, instrument};

pub struct RegistrationTokenService {
    storage: Arc<RegistrationTokenStorage>,
}

impl RegistrationTokenService {
    pub fn new(storage: Arc<RegistrationTokenStorage>) -> Self {
        Self { storage }
    }

    #[instrument(skip(self))]
    pub async fn create_token(&self, request: CreateRegistrationTokenRequest) -> Result<RegistrationToken, ApiError> {
        info!("Creating registration token");

        if let Some(ref token) = request.token {
            if let Some(_) = self.storage.get_token(token).await
                .map_err(|e| ApiError::internal(format!("Failed to check token: {}", e)))? {
                return Err(ApiError::bad_request("Token already exists"));
            }
        }

        let token = self.storage.create_token(request).await
            .map_err(|e| ApiError::internal(format!("Failed to create token: {}", e)))?;

        let token_preview: String = token.token.chars().take(4).collect();
        info!("Created registration token: {}***", token_preview);

        Ok(token)
    }

    #[instrument(skip(self))]
    pub async fn get_token(&self, token: &str) -> Result<Option<RegistrationToken>, ApiError> {
        let token = self.storage.get_token(token).await
            .map_err(|e| ApiError::internal(format!("Failed to get token: {}", e)))?;

        Ok(token)
    }

    #[instrument(skip(self))]
    pub async fn get_token_by_id(&self, id: i64) -> Result<Option<RegistrationToken>, ApiError> {
        let token = self.storage.get_token_by_id(id).await
            .map_err(|e| ApiError::internal(format!("Failed to get token: {}", e)))?;

        Ok(token)
    }

    #[instrument(skip(self))]
    pub async fn validate_token(&self, token: &str) -> Result<TokenValidationResult, ApiError> {
        let result = self.storage.validate_token(token).await
            .map_err(|e| ApiError::internal(format!("Failed to validate token: {}", e)))?;

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
        info!("Using registration token for user: {}", user_id);

        let validation = self.validate_token(token).await?;

        if !validation.is_valid {
            return Err(ApiError::bad_request(
                validation.error_message.unwrap_or_else(|| "Invalid token".to_string())
            ));
        }

        let success = self.storage.use_token(token, user_id, username, email, ip_address, user_agent).await
            .map_err(|e| ApiError::internal(format!("Failed to use token: {}", e)))?;

        if !success {
            return Err(ApiError::bad_request("Failed to use token"));
        }

        info!("Successfully used registration token for user: {}", user_id);

        Ok(true)
    }

    #[instrument(skip(self))]
    pub async fn update_token(&self, id: i64, request: UpdateRegistrationTokenRequest) -> Result<RegistrationToken, ApiError> {
        let _existing = self.storage.get_token_by_id(id).await
            .map_err(|e| ApiError::internal(format!("Failed to check token: {}", e)))?
            .ok_or_else(|| ApiError::not_found("Token not found"))?;

        let token = self.storage.update_token(id, request).await
            .map_err(|e| ApiError::internal(format!("Failed to update token: {}", e)))?;

        Ok(token)
    }

    #[instrument(skip(self))]
    pub async fn delete_token(&self, id: i64) -> Result<(), ApiError> {
        let _existing = self.storage.get_token_by_id(id).await
            .map_err(|e| ApiError::internal(format!("Failed to check token: {}", e)))?
            .ok_or_else(|| ApiError::not_found("Token not found"))?;

        self.storage.delete_token(id).await
            .map_err(|e| ApiError::internal(format!("Failed to delete token: {}", e)))?;

        info!("Deleted registration token: {}", id);

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn deactivate_token(&self, id: i64) -> Result<(), ApiError> {
        self.storage.deactivate_token(id).await
            .map_err(|e| ApiError::internal(format!("Failed to deactivate token: {}", e)))?;

        info!("Deactivated registration token: {}", id);

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn get_all_tokens(&self, limit: i64, offset: i64) -> Result<Vec<RegistrationToken>, ApiError> {
        let tokens = self.storage.get_all_tokens(limit, offset).await
            .map_err(|e| ApiError::internal(format!("Failed to get tokens: {}", e)))?;

        Ok(tokens)
    }

    #[instrument(skip(self))]
    pub async fn get_active_tokens(&self) -> Result<Vec<RegistrationToken>, ApiError> {
        let tokens = self.storage.get_active_tokens().await
            .map_err(|e| ApiError::internal(format!("Failed to get active tokens: {}", e)))?;

        Ok(tokens)
    }

    #[instrument(skip(self))]
    pub async fn get_token_usage(&self, token_id: i64) -> Result<Vec<RegistrationTokenUsage>, ApiError> {
        let usage = self.storage.get_token_usage(token_id).await
            .map_err(|e| ApiError::internal(format!("Failed to get token usage: {}", e)))?;

        Ok(usage)
    }

    #[instrument(skip(self))]
    pub async fn cleanup_expired_tokens(&self) -> Result<i64, ApiError> {
        info!("Cleaning up expired registration tokens");

        let count = self.storage.cleanup_expired_tokens().await
            .map_err(|e| ApiError::internal(format!("Failed to cleanup tokens: {}", e)))?;

        info!("Cleaned up {} expired tokens", count);

        Ok(count)
    }

    #[instrument(skip(self))]
    pub async fn create_room_invite(&self, request: CreateRoomInviteRequest) -> Result<RoomInvite, ApiError> {
        info!("Creating room invite for room: {}", request.room_id);

        let invite = self.storage.create_room_invite(request).await
            .map_err(|e| ApiError::internal(format!("Failed to create room invite: {}", e)))?;

        Ok(invite)
    }

    #[instrument(skip(self))]
    pub async fn get_room_invite(&self, invite_code: &str) -> Result<Option<RoomInvite>, ApiError> {
        let invite = self.storage.get_room_invite(invite_code).await
            .map_err(|e| ApiError::internal(format!("Failed to get room invite: {}", e)))?;

        Ok(invite)
    }

    #[instrument(skip(self))]
    pub async fn use_room_invite(&self, invite_code: &str, invitee_user_id: &str) -> Result<bool, ApiError> {
        info!("Using room invite for user: {}", invitee_user_id);

        let success = self.storage.use_room_invite(invite_code, invitee_user_id).await
            .map_err(|e| ApiError::internal(format!("Failed to use room invite: {}", e)))?;

        if !success {
            return Err(ApiError::bad_request("Invalid or expired room invite"));
        }

        Ok(true)
    }

    #[instrument(skip(self))]
    pub async fn revoke_room_invite(&self, invite_code: &str, reason: &str) -> Result<(), ApiError> {
        self.storage.revoke_room_invite(invite_code, reason).await
            .map_err(|e| ApiError::internal(format!("Failed to revoke room invite: {}", e)))?;

        info!("Revoked room invite: {}", invite_code);

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn create_batch(&self, count: i32, description: Option<String>, expires_at: Option<i64>, created_by: Option<String>, allowed_email_domains: Option<Vec<String>>, auto_join_rooms: Option<Vec<String>>) -> Result<(String, Vec<String>), ApiError> {
        info!("Creating batch of {} registration tokens", count);

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
            is_active: true,
            allowed_email_domains: allowed_email_domains.clone(),
            auto_join_rooms: auto_join_rooms.clone(),
        };

        self.storage.create_batch(&batch, &tokens).await
            .map_err(|e| ApiError::internal(format!("Failed to create batch: {}", e)))?;

        info!("Created batch {} with {} tokens", batch_id, count);

        Ok((batch_id, tokens))
    }

    #[instrument(skip(self))]
    pub async fn get_batch(&self, batch_id: &str) -> Result<Option<RegistrationTokenBatch>, ApiError> {
        let batch = self.storage.get_batch(batch_id).await
            .map_err(|e| ApiError::internal(format!("Failed to get batch: {}", e)))?;

        Ok(batch)
    }

    pub async fn check_email_domain_allowed(&self, token: &str, email: &str) -> Result<bool, ApiError> {
        let token_record = self.storage.get_token(token).await
            .map_err(|e| ApiError::internal(format!("Failed to get token: {}", e)))?
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
