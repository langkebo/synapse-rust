use super::AuthService;
use synapse_common::*;
use synapse_storage::user::NewRegistration;
use synapse_storage::User;

impl AuthService {
    /// See [`register`].
    pub async fn register(
        &self,
        username: &str,
        password: &str,
        admin: bool,
        displayname: Option<&str>,
    ) -> ApiResult<(User, String, String, String)> {
        let start = std::time::Instant::now();
        let result = self.register_internal(username, password, admin, displayname, None).await;

        let duration = start.elapsed().as_secs_f64();
        if let Some(hist) = self.metrics.get_histogram("auth_register_duration_seconds") {
            hist.observe(duration);
        } else {
            let hist = self.metrics.register_histogram("auth_register_duration_seconds".to_string());
            hist.observe(duration);
        }

        if result.is_ok() {
            self.increment_counter("auth_register_success_total");
        } else {
            self.increment_counter("auth_register_failure_total");
        }
        result
    }

    /// See [`register_with_device_name`].
    pub async fn register_with_device_name(
        &self,
        username: &str,
        password: &str,
        admin: bool,
        displayname: Option<&str>,
        initial_device_display_name: Option<&str>,
    ) -> ApiResult<(User, String, String, String)> {
        let start = std::time::Instant::now();
        let result = self.register_internal(username, password, admin, displayname, initial_device_display_name).await;

        let duration = start.elapsed().as_secs_f64();
        if let Some(hist) = self.metrics.get_histogram("auth_register_duration_seconds") {
            hist.observe(duration);
        } else {
            let hist = self.metrics.register_histogram("auth_register_duration_seconds".to_string());
            hist.observe(duration);
        }

        if result.is_ok() {
            self.increment_counter("auth_register_success_total");
        } else {
            self.increment_counter("auth_register_failure_total");
        }
        result
    }

    async fn register_internal(
        &self,
        username: &str,
        password: &str,
        admin: bool,
        displayname: Option<&str>,
        initial_device_display_name: Option<&str>,
    ) -> ApiResult<(User, String, String, String)> {
        if username.is_empty() || password.is_empty() {
            return Err(ApiError::missing_param("Username and password are required".to_string()));
        }
        if let Err(e) = self.validator.validate_username(username) {
            return Err(ApiError::invalid_username(e.to_string()));
        }
        if let Err(e) = self.validator.validate_password(password) {
            return Err(ApiError::invalid_param(format!("Password does not meet policy requirements: {e}")));
        }

        let password_hash = self.hash_password_for_storage(password).await?;

        let user_id = format!("@{username}:{}", self.server_name);

        // PERF-02：设备 ID 与两枚令牌都必须在**进入事务之前**算好——事务内只做
        // INSERT。原先这 5 次写各自独立提交，任一步失败都会留下"用户存在但拿不到
        // 会话"的半成品账号。
        Self::validate_device_display_name(initial_device_display_name)?;
        let device_id = super::auth_generate_token(16);
        let (access_token, access_token_expires_at) = self.build_access_token(&user_id, &device_id, admin, None)?;
        // P2-12: link refresh_token to access_token so rotation can invalidate
        // the old access_token cache entry (Synapse v1.154 #19483).
        let (refresh_token, refresh_request) = self.build_refresh_token(&user_id, &device_id, &access_token);

        let registration = NewRegistration {
            user_id: user_id.clone(),
            username: username.to_string(),
            password_hash: Some(password_hash),
            is_admin: admin,
            displayname: displayname.map(str::to_string),
            device_id: device_id.clone(),
            device_display_name: initial_device_display_name.map(str::to_string),
            access_token: access_token.clone(),
            access_token_expires_at: Some(access_token_expires_at),
            refresh_token_hash: refresh_request.token_hash,
            refresh_token_expires_at: refresh_request.expires_at,
        };

        let user = self.user_storage.create_registration(registration).await.map_err(|e| {
            if e.to_string().contains("duplicate key") || e.to_string().contains("unique constraint") {
                ApiError::user_in_use("Username already exists".to_string())
            } else if e.to_string().contains("too long") {
                ApiError::bad_request("Displayname too long (max 255 characters)".to_string())
            } else {
                ApiError::internal_with_cause("Failed to create user", e)
            }
        })?;

        ::tracing::info!(
            target: "security_audit",
            event = "user_registered",
            user_id = user.user_id(),
            is_admin = admin,
            "New user registered"
        );

        Ok((user, access_token, refresh_token, device_id))
    }

    /// 应用服务注册虚拟用户（MSC4190）：**只建用户，不签发 token、不建设备**。
    ///
    /// 为什么不能复用 [`Self::register_with_device_name`]：那条路径会顺便设备化并签发
    /// access/refresh token，而 AS 注册的语义恰恰是"不要登录"——AS 之后要么用
    /// `m.login.application_service` 替该用户换 token，要么直接用 `as_token` 代理。
    /// 若在这里发 token，等于把一个不该存在的会话写进 `devices` / `access_tokens`。
    ///
    /// 密码为 `None`：虚拟用户不靠密码登录（这也是 AS 用户常见的形态）。
    /// 用户名/密码策略**不适用于**这条路径（没有密码可校验），但用户名格式仍校验。
    pub async fn register_without_login(&self, username: &str, displayname: Option<&str>) -> ApiResult<User> {
        if username.is_empty() {
            return Err(ApiError::missing_param("Username is required".to_string()));
        }
        if let Err(error) = self.validator.validate_username(username) {
            return Err(ApiError::invalid_username(error.to_string()));
        }

        let user_id = format!("@{username}:{}", self.server_name);
        let user = self.user_storage.create_user(&user_id, username, None, false).await.map_err(|error| {
            if error.to_string().contains("duplicate key") || error.to_string().contains("unique constraint") {
                ApiError::user_in_use("Username already exists".to_string())
            } else {
                ApiError::internal_with_cause("Failed to create user", error)
            }
        })?;

        if let Some(name) = displayname {
            if let Err(error) = self.user_service.update_displayname(&user.user_id, Some(name)).await {
                ::tracing::warn!(
                    error = %error,
                    user_id = %user.user_id,
                    "Failed to set displayname during appservice registration"
                );
            }
        }

        ::tracing::info!(
            target: "security_audit",
            event = "appservice_user_registered",
            user_id = user.user_id(),
            "Application service registered a virtual user (no login)"
        );

        Ok(user)
    }
}
