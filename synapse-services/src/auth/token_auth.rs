use async_trait::async_trait;
use synapse_common::ApiResult;

/// Token and session lifecycle: validation, generation, refresh, and revocation.
#[async_trait]
pub trait TokenAuth: Send + Sync {
    /// See [`validate_token`].
    async fn validate_token(&self, token: &str) -> ApiResult<(String, Option<String>, bool, bool, bool)>;

    /// See [`generate_access_token`].
    async fn generate_access_token(&self, user_id: &str, device_id: &str, admin: bool) -> ApiResult<String>;

    /// Generate a new refresh token, linked to the given `access_token` for
    /// cache invalidation during rotation (P2-12, Synapse v1.154 #19483).
    async fn generate_refresh_token(&self, user_id: &str, device_id: &str, access_token: &str) -> ApiResult<String>;

    /// See [`refresh_token`].
    async fn refresh_token(&self, refresh_token: &str) -> ApiResult<(String, String, String)>;

    /// See [`logout`].
    async fn logout(&self, access_token: &str, device_id: Option<&str>) -> ApiResult<()>;

    /// See [`logout_all`].
    async fn logout_all(&self, user_id: &str) -> ApiResult<()>;

    /// See [`revoke_device`].
    async fn revoke_device(&self, user_id: &str, device_id: &str) -> ApiResult<u64>;

    /// See [`revoke_devices`].
    async fn revoke_devices(&self, user_id: &str, device_ids: &[String]) -> ApiResult<u64>;

    /// See [`token_expiry`].
    fn token_expiry(&self) -> i64;
}
