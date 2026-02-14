use crate::common::ApiError;
use crate::storage::refresh_token::*;
use std::sync::Arc;
use tracing::{info, warn, instrument};
use sha2::{Sha256, Digest};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

pub struct RefreshTokenService {
    storage: Arc<RefreshTokenStorage>,
    default_expiry_ms: i64,
}

impl RefreshTokenService {
    pub fn new(storage: Arc<RefreshTokenStorage>, default_expiry_ms: i64) -> Self {
        Self { storage, default_expiry_ms }
    }

    pub fn hash_token(token: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(token.as_bytes());
        let result = hasher.finalize();
        URL_SAFE_NO_PAD.encode(result)
    }

    pub fn generate_token() -> String {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let random_bytes: [u8; 32] = rng.gen();
        URL_SAFE_NO_PAD.encode(random_bytes)
    }

    pub fn generate_family_id() -> String {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let random_bytes: [u8; 16] = rng.gen();
        URL_SAFE_NO_PAD.encode(random_bytes)
    }

    #[instrument(skip(self))]
    pub async fn create_token(&self, request: CreateRefreshTokenRequest) -> Result<RefreshToken, ApiError> {
        info!("Creating refresh token for user: {}", request.user_id);

        let token = self.storage.create_token(request).await
            .map_err(|e| ApiError::internal(format!("Failed to create refresh token: {}", e)))?;

        Ok(token)
    }

    #[instrument(skip(self))]
    pub async fn validate_token(&self, token: &str) -> Result<RefreshToken, ApiError> {
        let token_hash = Self::hash_token(token);

        let is_blacklisted = self.storage.is_blacklisted(&token_hash).await
            .map_err(|e| ApiError::internal(format!("Failed to check blacklist: {}", e)))?;

        if is_blacklisted {
            return Err(ApiError::unauthorized("Token has been revoked"));
        }

        let token_record = self.storage.get_token(&token_hash).await
            .map_err(|e| ApiError::internal(format!("Failed to get token: {}", e)))?
            .ok_or_else(|| ApiError::unauthorized("Invalid refresh token"))?;

        if token_record.is_revoked {
            return Err(ApiError::unauthorized("Token has been revoked"));
        }

        let now = chrono::Utc::now().timestamp_millis();
        if token_record.expires_at < now {
            return Err(ApiError::unauthorized("Token has expired"));
        }

        Ok(token_record)
    }

    #[instrument(skip(self))]
    pub async fn refresh_access_token(
        &self,
        refresh_token: &str,
        new_access_token_id: &str,
        ip_address: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<(String, RefreshToken), ApiError> {
        let old_token = self.validate_token(refresh_token).await?;

        info!("Refreshing access token for user: {}", old_token.user_id);

        let new_refresh_token = Self::generate_token();
        let new_token_hash = Self::hash_token(&new_refresh_token);
        let old_token_hash = Self::hash_token(refresh_token);

        let family_id = match self.storage.get_rotations(&old_token.user_id).await {
            Ok(rotations) if !rotations.is_empty() => {
                rotations[0].family_id.clone()
            }
            _ => {
                let family_id = Self::generate_family_id();
                if let Err(e) = self.storage.create_family(&family_id, &old_token.user_id, old_token.device_id.as_deref()).await {
                    warn!("Failed to create token family: {}", e);
                }
                family_id
            }
        };

        let rotations = self.storage.get_rotations(&family_id).await.unwrap_or_default();
        if let Some(last_rotation) = rotations.first() {
            if last_rotation.new_token_hash != old_token_hash {
                warn!("Potential token replay attack detected for user: {}", old_token.user_id);
                
                self.storage.mark_family_compromised(&family_id).await
                    .map_err(|e| ApiError::internal(format!("Failed to mark family compromised: {}", e)))?;

                self.storage.revoke_all_user_tokens(&old_token.user_id, "Potential token replay attack").await
                    .map_err(|e| ApiError::internal(format!("Failed to revoke tokens: {}", e)))?;

                return Err(ApiError::unauthorized("Token reuse detected. All tokens revoked."));
            }
        }

        self.storage.revoke_token(&old_token_hash, "Rotated").await
            .map_err(|e| ApiError::internal(format!("Failed to revoke old token: {}", e)))?;

        let new_token_record = self.storage.create_token(CreateRefreshTokenRequest {
            token_hash: new_token_hash.clone(),
            user_id: old_token.user_id.clone(),
            device_id: old_token.device_id.clone(),
            access_token_id: Some(new_access_token_id.to_string()),
            scope: old_token.scope.clone(),
            expires_at: chrono::Utc::now().timestamp_millis() + self.default_expiry_ms,
            client_info: old_token.client_info.clone(),
            ip_address: ip_address.map(|s| s.to_string()),
            user_agent: user_agent.map(|s| s.to_string()),
        }).await.map_err(|e| ApiError::internal(format!("Failed to create new token: {}", e)))?;

        self.storage.record_rotation(&family_id, Some(&old_token_hash), &new_token_hash, "refresh").await
            .map_err(|e| ApiError::internal(format!("Failed to record rotation: {}", e)))?;

        self.storage.record_usage(
            old_token.id,
            &old_token.user_id,
            old_token.access_token_id.as_deref(),
            new_access_token_id,
            ip_address,
            user_agent,
            true,
            None,
        ).await.ok();

        Ok((new_refresh_token, new_token_record))
    }

    #[instrument(skip(self))]
    pub async fn revoke_token(&self, token: &str, reason: &str) -> Result<(), ApiError> {
        let token_hash = Self::hash_token(token);

        self.storage.revoke_token(&token_hash, reason).await
            .map_err(|e| ApiError::internal(format!("Failed to revoke token: {}", e)))?;

        info!("Token revoked: {}", token_hash);

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn revoke_token_by_id(&self, id: i64, reason: &str) -> Result<(), ApiError> {
        self.storage.revoke_token_by_id(id, reason).await
            .map_err(|e| ApiError::internal(format!("Failed to revoke token: {}", e)))?;

        info!("Token revoked by id: {}", id);

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn revoke_all_user_tokens(&self, user_id: &str, reason: &str) -> Result<i64, ApiError> {
        info!("Revoking all tokens for user: {}", user_id);

        let count = self.storage.revoke_all_user_tokens(user_id, reason).await
            .map_err(|e| ApiError::internal(format!("Failed to revoke tokens: {}", e)))?;

        Ok(count)
    }

    #[instrument(skip(self))]
    pub async fn get_user_tokens(&self, user_id: &str) -> Result<Vec<RefreshToken>, ApiError> {
        let tokens = self.storage.get_user_tokens(user_id).await
            .map_err(|e| ApiError::internal(format!("Failed to get tokens: {}", e)))?;

        Ok(tokens)
    }

    #[instrument(skip(self))]
    pub async fn get_active_tokens(&self, user_id: &str) -> Result<Vec<RefreshToken>, ApiError> {
        let tokens = self.storage.get_active_tokens(user_id).await
            .map_err(|e| ApiError::internal(format!("Failed to get active tokens: {}", e)))?;

        Ok(tokens)
    }

    #[instrument(skip(self))]
    pub async fn get_user_stats(&self, user_id: &str) -> Result<Option<RefreshTokenStats>, ApiError> {
        let stats = self.storage.get_user_stats(user_id).await
            .map_err(|e| ApiError::internal(format!("Failed to get stats: {}", e)))?;

        Ok(stats)
    }

    #[instrument(skip(self))]
    pub async fn get_usage_history(&self, user_id: &str, limit: i64) -> Result<Vec<RefreshTokenUsage>, ApiError> {
        let history = self.storage.get_usage_history(user_id, limit).await
            .map_err(|e| ApiError::internal(format!("Failed to get usage history: {}", e)))?;

        Ok(history)
    }

    #[instrument(skip(self))]
    pub async fn add_to_blacklist(&self, token: &str, token_type: &str, user_id: &str, expires_at: i64, reason: Option<&str>) -> Result<(), ApiError> {
        let token_hash = Self::hash_token(token);

        self.storage.add_to_blacklist(&token_hash, token_type, user_id, expires_at, reason).await
            .map_err(|e| ApiError::internal(format!("Failed to add to blacklist: {}", e)))?;

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn cleanup_expired_tokens(&self) -> Result<i64, ApiError> {
        info!("Cleaning up expired tokens");

        let count = self.storage.cleanup_expired_tokens().await
            .map_err(|e| ApiError::internal(format!("Failed to cleanup tokens: {}", e)))?;

        let blacklist_count = self.storage.cleanup_blacklist().await
            .map_err(|e| ApiError::internal(format!("Failed to cleanup blacklist: {}", e)))?;

        info!("Cleaned up {} expired tokens and {} blacklist entries", count, blacklist_count);

        Ok(count)
    }

    #[instrument(skip(self))]
    pub async fn delete_token(&self, token: &str) -> Result<(), ApiError> {
        let token_hash = Self::hash_token(token);

        self.storage.delete_token(&token_hash).await
            .map_err(|e| ApiError::internal(format!("Failed to delete token: {}", e)))?;

        Ok(())
    }

    pub fn get_default_expiry_ms(&self) -> i64 {
        self.default_expiry_ms
    }
}
