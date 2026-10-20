use async_trait::async_trait;
use synapse_common::ApiResult;
use synapse_storage::User;

/// Account and credential lifecycle: login, registration, password management,
/// guest accounts, and verification.
#[async_trait]
pub trait CredentialAuth: Send + Sync {
    /// See [`login`].
    async fn login(
        &self,
        username: &str,
        password: &str,
        device_id: Option<&str>,
        initial_display_name: Option<&str>,
    ) -> ApiResult<(User, String, String, String)>;

    /// See [`register`].
    async fn register(
        &self,
        username: &str,
        password: &str,
        admin: bool,
        displayname: Option<&str>,
    ) -> ApiResult<(User, String, String, String)>;

    /// See [`register_with_device_name`].
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

    /// See [`deactivate_user`].
    async fn deactivate_user(&self, user_id: &str) -> ApiResult<()>;

    /// See [`verify_user_credentials`].
    async fn verify_user_credentials(&self, user_id: &str, password: &str) -> ApiResult<()>;

    /// MSC4190：应用服务注册虚拟用户 —— **只建用户，不签发 access_token / device**。
    ///
    /// 默认实现返回 `M_UNRECOGNIZED`：这个方法只为真实服务（`AuthService`）而加，
    /// 测试替身（`FakeCredentialAuth`）不该被迫实现一条它永远不该走的路径。
    async fn register_without_login(&self, username: &str, displayname: Option<&str>) -> ApiResult<User> {
        let _ = (username, displayname);
        Err(synapse_common::ApiError::not_implemented(
            "register_without_login is not implemented by this credential auth".to_string(),
        ))
    }

    /// See [`register_guest_account`].
    ///
    /// `device_id` / `display_name` are the optional `device_id` and
    /// `initial_device_display_name` fields of the guest registration body.
    async fn register_guest_account(
        &self,
        device_id: Option<&str>,
        display_name: Option<&str>,
    ) -> ApiResult<(User, String, String)>;

    /// See [`require_guest_user`].
    async fn require_guest_user(&self, user_id: &str) -> ApiResult<User>;

    /// See [`upgrade_guest_account`].
    async fn upgrade_guest_account(
        &self,
        user_id: &str,
        device_id: Option<&str>,
        username: &str,
        password: &str,
    ) -> ApiResult<String>;

    /// See [`generate_email_verification_token`].
    fn generate_email_verification_token(&self) -> ApiResult<String>;
}
