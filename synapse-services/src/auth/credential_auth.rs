use async_trait::async_trait;
use synapse_common::ApiResult;
use synapse_storage::User;

/// Account and credential lifecycle: login, registration, password management,
/// guest accounts, and verification.
#[async_trait]
pub trait CredentialAuth: Send + Sync {
    async fn login(
        &self,
        username: &str,
        password: &str,
        device_id: Option<&str>,
        initial_display_name: Option<&str>,
    ) -> ApiResult<(User, String, String, String)>;

    async fn register(
        &self,
        username: &str,
        password: &str,
        admin: bool,
        displayname: Option<&str>,
    ) -> ApiResult<(User, String, String, String)>;

    async fn register_with_device_name(
        &self,
        username: &str,
        password: &str,
        admin: bool,
        displayname: Option<&str>,
        initial_device_display_name: Option<&str>,
    ) -> ApiResult<(User, String, String, String)>;

    /// Change the password for a user.
    ///
    /// `logout_devices` follows Matrix v1.3 spec semantics: when `true` (the
    /// default per spec), all access tokens belonging to the user are revoked
    /// after the password is updated. When `false`, only tokens belonging to
    /// devices other than `current_device_id` are revoked; the caller MUST
    /// supply `current_device_id` in that case (a `400` is returned otherwise).
    async fn change_password(
        &self,
        user_id: &str,
        current_password: Option<&str>,
        new_password: &str,
        current_device_id: Option<&str>,
        logout_devices: bool,
    ) -> ApiResult<()>;

    async fn deactivate_user(&self, user_id: &str) -> ApiResult<()>;

    async fn verify_user_credentials(&self, user_id: &str, password: &str) -> ApiResult<()>;

    async fn register_guest_account(&self) -> ApiResult<(User, String, String)>;

    async fn require_guest_user(&self, user_id: &str) -> ApiResult<User>;

    async fn upgrade_guest_account(
        &self,
        user_id: &str,
        device_id: Option<&str>,
        username: &str,
        password: &str,
    ) -> ApiResult<String>;

    fn generate_email_verification_token(&self) -> ApiResult<String>;
}
