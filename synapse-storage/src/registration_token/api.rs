use async_trait::async_trait;

use super::models::*;
use super::repository::RegistrationTokenStorage;

// ── Trait ───────────────────────────────────────────────────────────────

#[async_trait]
pub trait RegistrationTokenStoreApi: Send + Sync {
    async fn create_token(&self, request: CreateRegistrationTokenRequest) -> Result<RegistrationToken, sqlx::Error>;
    async fn get_token(&self, token: &str) -> Result<Option<RegistrationToken>, sqlx::Error>;
    async fn get_token_by_id(&self, id: i64) -> Result<Option<RegistrationToken>, sqlx::Error>;
    async fn update_token(
        &self,
        id: i64,
        request: UpdateRegistrationTokenRequest,
    ) -> Result<RegistrationToken, sqlx::Error>;
    async fn delete_token(&self, id: i64) -> Result<(), sqlx::Error>;
    async fn validate_token(&self, token: &str) -> Result<TokenValidationResult, sqlx::Error>;
    async fn use_token(
        &self,
        token: &str,
        user_id: &str,
        username: Option<&str>,
        email: Option<&str>,
        ip_address: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<bool, sqlx::Error>;
    async fn get_all_tokens(
        &self,
        limit: i64,
        from: Option<RegistrationTokenCursor>,
    ) -> Result<(Vec<RegistrationToken>, Option<String>), sqlx::Error>;
    async fn get_active_tokens(&self) -> Result<Vec<RegistrationToken>, sqlx::Error>;
    async fn get_token_usage(&self, token_id: i64) -> Result<Vec<RegistrationTokenUsage>, sqlx::Error>;
    async fn deactivate_token(&self, id: i64) -> Result<(), sqlx::Error>;
    async fn cleanup_expired_tokens(&self) -> Result<i64, sqlx::Error>;
    async fn create_room_invite(&self, request: CreateRoomInviteRequest) -> Result<RoomInvite, sqlx::Error>;
    async fn get_room_invite(&self, invite_code: &str) -> Result<Option<RoomInvite>, sqlx::Error>;
    async fn use_room_invite(&self, invite_code: &str, invitee_user_id: &str) -> Result<bool, sqlx::Error>;
    async fn revoke_room_invite(&self, invite_code: &str, reason: &str) -> Result<(), sqlx::Error>;
    async fn create_batch(&self, batch: &RegistrationTokenBatch, tokens: &[String]) -> Result<i64, sqlx::Error>;
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
