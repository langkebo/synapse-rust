use async_trait::async_trait;
use std::sync::Arc;

use super::{
    CreateRefreshTokenRequest, RecordUsageRequest, RefreshToken, RefreshTokenFamily, RefreshTokenRotation,
    RefreshTokenStats, RefreshTokenUsage,
};

#[async_trait]
pub trait RefreshTokenRepository: Send + Sync {
    fn pool(&self) -> &Arc<sqlx::PgPool>;

    // CRUD
    async fn create_token(&self, request: CreateRefreshTokenRequest) -> Result<RefreshToken, sqlx::Error>;
    async fn get_token(&self, token_hash: &str) -> Result<Option<RefreshToken>, sqlx::Error>;
    async fn get_token_by_id(&self, id: i64) -> Result<Option<RefreshToken>, sqlx::Error>;
    async fn get_user_tokens(&self, user_id: &str) -> Result<Vec<RefreshToken>, sqlx::Error>;
    async fn get_active_tokens(&self, user_id: &str) -> Result<Vec<RefreshToken>, sqlx::Error>;
    async fn delete_token(&self, token_hash: &str) -> Result<(), sqlx::Error>;
    async fn delete_user_tokens(&self, user_id: &str) -> Result<i64, sqlx::Error>;

    // Revocation
    async fn revoke_token(&self, token_hash: &str, reason: &str) -> Result<(), sqlx::Error>;
    async fn revoke_token_cas(&self, token_hash: &str, reason: &str) -> Result<bool, sqlx::Error>;
    async fn revoke_token_by_id(&self, id: i64, reason: &str) -> Result<(), sqlx::Error>;
    async fn revoke_all_user_tokens(&self, user_id: &str, reason: &str) -> Result<i64, sqlx::Error>;
    async fn revoke_all_user_tokens_except_device(
        &self,
        user_id: &str,
        device_id: &str,
        reason: &str,
    ) -> Result<i64, sqlx::Error>;
    async fn revoke_device_tokens(&self, user_id: &str, device_id: &str, reason: &str) -> Result<i64, sqlx::Error>;

    // Usage tracking
    async fn update_token_usage(&self, token_hash: &str, access_token_id: &str) -> Result<(), sqlx::Error>;
    async fn record_usage(&self, request: &RecordUsageRequest) -> Result<(), sqlx::Error>;

    // Family / rotation
    async fn create_family(
        &self,
        family_id: &str,
        user_id: &str,
        device_id: Option<&str>,
    ) -> Result<RefreshTokenFamily, sqlx::Error>;
    async fn get_family(&self, family_id: &str) -> Result<Option<RefreshTokenFamily>, sqlx::Error>;
    async fn mark_family_compromised(&self, family_id: &str) -> Result<(), sqlx::Error>;
    async fn record_rotation(
        &self,
        family_id: &str,
        old_token_hash: Option<&str>,
        new_token_hash: &str,
        reason: &str,
    ) -> Result<(), sqlx::Error>;
    async fn get_rotations(&self, family_id: &str) -> Result<Vec<RefreshTokenRotation>, sqlx::Error>;

    // Blacklist
    async fn add_to_blacklist(
        &self,
        token_hash: &str,
        token_type: &str,
        user_id: &str,
        expires_at: i64,
        reason: Option<&str>,
    ) -> Result<(), sqlx::Error>;
    async fn is_blacklisted(&self, token_hash: &str) -> Result<bool, sqlx::Error>;

    // Cleanup
    async fn cleanup_expired_tokens(&self) -> Result<i64, sqlx::Error>;
    async fn cleanup_blacklist(&self) -> Result<i64, sqlx::Error>;

    // Stats
    async fn get_user_stats(&self, user_id: &str) -> Result<Option<RefreshTokenStats>, sqlx::Error>;
    async fn get_usage_history(&self, user_id: &str, limit: i64) -> Result<Vec<RefreshTokenUsage>, sqlx::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_refresh_token_repository_is_trait_object_safe() {
        fn _accept_trait_object(_: &dyn RefreshTokenRepository) {}
    }

    #[test]
    fn test_boxed_refresh_token_repository_is_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<Box<dyn RefreshTokenRepository>>();
        assert_sync::<Box<dyn RefreshTokenRepository>>();
    }

    #[test]
    fn test_arced_refresh_token_repository_is_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<std::sync::Arc<dyn RefreshTokenRepository>>();
        assert_sync::<std::sync::Arc<dyn RefreshTokenRepository>>();
    }
}
