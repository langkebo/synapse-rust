use async_trait::async_trait;

use super::models::*;
use super::repository::RegistrationTokenStorage;

// ── Trait ───────────────────────────────────────────────────────────────

/// The `RegistrationTokenStoreApi` trait.
#[async_trait]
pub trait RegistrationTokenStoreApi: Send + Sync {
    /// See [`create_token`].
    async fn create_token(&self, request: CreateRegistrationTokenRequest) -> Result<RegistrationToken, sqlx::Error>;
    /// See [`get_token`].
    async fn get_token(&self, token: &str) -> Result<Option<RegistrationToken>, sqlx::Error>;
    /// See [`get_token_by_id`].
    async fn get_token_by_id(&self, id: i64) -> Result<Option<RegistrationToken>, sqlx::Error>;
    /// See [`update_token`].
    async fn update_token(
        &self,
        id: i64,
        request: UpdateRegistrationTokenRequest,
    ) -> Result<RegistrationToken, sqlx::Error>;
    /// See [`delete_token`].
    async fn delete_token(&self, id: i64) -> Result<(), sqlx::Error>;
    /// See [`validate_token`].
    async fn validate_token(&self, token: &str) -> Result<TokenValidationResult, sqlx::Error>;
    /// See [`use_token`].
    async fn use_token(
        &self,
        token: &str,
        user_id: &str,
        username: Option<&str>,
        email: Option<&str>,
        ip_address: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<bool, sqlx::Error>;
    /// See [`get_all_tokens`].
    async fn get_all_tokens(
        &self,
        limit: i64,
        from: Option<RegistrationTokenCursor>,
    ) -> Result<(Vec<RegistrationToken>, Option<String>), sqlx::Error>;
    /// See [`get_active_tokens`].
    async fn get_active_tokens(&self) -> Result<Vec<RegistrationToken>, sqlx::Error>;
    /// See [`get_token_usage`].
    async fn get_token_usage(&self, token_id: i64) -> Result<Vec<RegistrationTokenUsage>, sqlx::Error>;
    /// See [`deactivate_token`].
    async fn deactivate_token(&self, id: i64) -> Result<(), sqlx::Error>;
    /// See [`cleanup_expired_tokens`].
    async fn cleanup_expired_tokens(&self) -> Result<i64, sqlx::Error>;
    /// See [`create_room_invite`].
    async fn create_room_invite(&self, request: CreateRoomInviteRequest) -> Result<RoomInvite, sqlx::Error>;
    /// See [`get_room_invite`].
    async fn get_room_invite(&self, invite_code: &str) -> Result<Option<RoomInvite>, sqlx::Error>;
    /// See [`use_room_invite`].
    async fn use_room_invite(&self, invite_code: &str, invitee_user_id: &str) -> Result<bool, sqlx::Error>;
    /// See [`revoke_room_invite`].
    async fn revoke_room_invite(&self, invite_code: &str, reason: &str) -> Result<(), sqlx::Error>;
    /// See [`create_batch`].
    async fn create_batch(&self, batch: &RegistrationTokenBatch, tokens: &[String]) -> Result<i64, sqlx::Error>;
    /// See [`get_batch`].
    async fn get_batch(&self, batch_id: &str) -> Result<Option<RegistrationTokenBatch>, sqlx::Error>;
}

// ── Delegation impl ─────────────────────────────────────────────────────

#[async_trait]
impl RegistrationTokenStoreApi for RegistrationTokenStorage {
    async fn create_token(&self, request: CreateRegistrationTokenRequest) -> Result<RegistrationToken, sqlx::Error> {
        self.create_token(request).await
    }
    async fn get_token(&self, token: &str) -> Result<Option<RegistrationToken>, sqlx::Error> {
        self.get_token(token).await
    }
    async fn get_token_by_id(&self, id: i64) -> Result<Option<RegistrationToken>, sqlx::Error> {
        self.get_token_by_id(id).await
    }
    async fn update_token(
        &self,
        id: i64,
        request: UpdateRegistrationTokenRequest,
    ) -> Result<RegistrationToken, sqlx::Error> {
        self.update_token(id, request).await
    }
    async fn delete_token(&self, id: i64) -> Result<(), sqlx::Error> {
        self.delete_token(id).await
    }
    async fn validate_token(&self, token: &str) -> Result<TokenValidationResult, sqlx::Error> {
        self.validate_token(token).await
    }
    async fn use_token(
        &self,
        token: &str,
        user_id: &str,
        username: Option<&str>,
        email: Option<&str>,
        ip_address: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<bool, sqlx::Error> {
        self.use_token(token, user_id, username, email, ip_address, user_agent).await
    }
    async fn get_all_tokens(
        &self,
        limit: i64,
        from: Option<RegistrationTokenCursor>,
    ) -> Result<(Vec<RegistrationToken>, Option<String>), sqlx::Error> {
        self.get_all_tokens(limit, from).await
    }
    async fn get_active_tokens(&self) -> Result<Vec<RegistrationToken>, sqlx::Error> {
        self.get_active_tokens().await
    }
    async fn get_token_usage(&self, token_id: i64) -> Result<Vec<RegistrationTokenUsage>, sqlx::Error> {
        self.get_token_usage(token_id).await
    }
    async fn deactivate_token(&self, id: i64) -> Result<(), sqlx::Error> {
        self.deactivate_token(id).await
    }
    async fn cleanup_expired_tokens(&self) -> Result<i64, sqlx::Error> {
        self.cleanup_expired_tokens().await
    }
    async fn create_room_invite(&self, request: CreateRoomInviteRequest) -> Result<RoomInvite, sqlx::Error> {
        self.create_room_invite(request).await
    }
    async fn get_room_invite(&self, invite_code: &str) -> Result<Option<RoomInvite>, sqlx::Error> {
        self.get_room_invite(invite_code).await
    }
    async fn use_room_invite(&self, invite_code: &str, invitee_user_id: &str) -> Result<bool, sqlx::Error> {
        self.use_room_invite(invite_code, invitee_user_id).await
    }
    async fn revoke_room_invite(&self, invite_code: &str, reason: &str) -> Result<(), sqlx::Error> {
        self.revoke_room_invite(invite_code, reason).await
    }
    async fn create_batch(&self, batch: &RegistrationTokenBatch, tokens: &[String]) -> Result<i64, sqlx::Error> {
        self.create_batch(batch, tokens).await
    }
    async fn get_batch(&self, batch_id: &str) -> Result<Option<RegistrationTokenBatch>, sqlx::Error> {
        self.get_batch(batch_id).await
    }
}
