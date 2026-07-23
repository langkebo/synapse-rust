use async_trait::async_trait;
use synapse_common::ApiResult;

/// Token and session lifecycle: validation, generation, refresh, and revocation.
#[async_trait]
pub trait TokenAuth: Send + Sync {
    async fn validate_token(&self, token: &str) -> ApiResult<(String, Option<String>, bool, bool, bool)>;

    async fn generate_access_token(&self, user_id: &str, device_id: &str, admin: bool) -> ApiResult<String>;

    async fn generate_refresh_token(&self, user_id: &str, device_id: &str) -> ApiResult<String>;

    async fn refresh_token(&self, refresh_token: &str) -> ApiResult<(String, String, String)>;

    async fn logout(&self, access_token: &str, device_id: Option<&str>) -> ApiResult<()>;

    async fn logout_all(&self, user_id: &str) -> ApiResult<()>;

    async fn revoke_device(&self, user_id: &str, device_id: &str) -> ApiResult<u64>;

    async fn revoke_devices(&self, user_id: &str, device_ids: &[String]) -> ApiResult<u64>;

    fn token_expiry(&self) -> i64;
}
