use crate::routes::context::AuthContext;
use crate::routes::extractors::{AuthenticatedUser, MatrixJson};
use crate::routes::formatting::format_token_response;
use crate::utils::admin_auth::enforce_admin_login_mfa_svc;
use crate::utils::auth::resolve_request_id;
use crate::utils::login_lockout::{check_login_lockout, clear_login_failures, record_login_failure};
use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};
use synapse_common::{ApiError, MatrixErrorCode};
/// See [`register`].
pub(crate) async fn register(
    State(ctx): State<AuthContext>,
    headers: HeaderMap,
    Query(query): Query<Value>,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Response, ApiError> {
    let is_guest = query.get("kind").and_then(|v| v.as_str()) == Some("guest")
        || body.get("kind").and_then(|v| v.as_str()) == Some("guest");

    if is_guest {
        if !ctx.config.server.enable_registration {
            return Err(ApiError::forbidden("Registration is disabled".to_string()));
        }
        let (user, device_id, access_token) = ctx.credential_auth.register_guest_account().await?;

        return Ok(Json(json!({
            "access_token": access_token,
            "device_id": device_id,
            "user_id": user.user_id,
            "is_guest": true,
            "expires_in": ctx.token_auth.token_expiry(),
            "well_known": {
                "m.homeserver": {
                    "base_url": ctx.config.server.get_public_baseurl()
                }
            }
        }))
        .into_response());
    }

    // MSC4190：应用服务注册虚拟用户。
    //
    // 判据同上游 —— **看 token，不看 body 里的 type**：AS 用它的 as_token 调
    // `POST /register`，服务器认出这是 AS 就按 AS 语义处理。用 body 判会挡住
    // 按上游写法实现的 bridge（它们不带 `type`）。
    if let Some(service) = appservice_for_register(&ctx, &headers).await? {
        return register_appservice_user(&ctx, &service, &body).await;
    }

    let auth = body.get("auth").cloned();

    let username = body.get("username").and_then(|v| v.as_str());
    let password = body.get("password").and_then(|v| v.as_str());

    // P-038: When the `auth` field is absent the request has not completed any
    // UIA stage. Matrix spec requires returning HTTP 401 with the available
    // flows so the client can start the User-Interactive Authentication flow.
    // Returning 200 makes Element interpret the body as a successful
    // registration and crash reading a missing user_id.
    if auth.is_none() {
        return Ok((
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "flows": [
                    { "stages": ["m.login.dummy"] },
                    { "stages": ["m.login.password"] }
                ],
                "params": {},
                "session": uuid::Uuid::new_v4().to_string()
            })),
        )
            .into_response());
    }

    // Auth field is present. Password is always required for non-guest
    // registration. A missing password at this point is a client error.
    let password = password.ok_or_else(|| ApiError::bad_request("Password required".to_string()))?;

    // P-037: Matrix spec allows servers to auto-generate a localpart when the
    // `username` field is omitted. Generate a random hex localpart that
    // satisfies the username validation regex (`^[a-z0-9._=\-]{1,255}$`).
    let username = match username {
        Some(u) => u.to_string(),
        None => format!("auto{}", &uuid::Uuid::new_v4().as_simple().to_string()[..12]),
    };

    ctx.validator.validate_username(&username)?;
    ctx.validator.validate_password(password)?;

    let displayname = body.get("displayname").and_then(|v| v.as_str());
    let initial_device_display_name = body.get("initial_device_display_name").and_then(|v| v.as_str());

    // D6/R3: Standard register endpoint intentionally ignores the `admin` field.
    // Admin accounts must be created via the shared-secret endpoint
    // (/_synapse/admin/v1/register) for security.
    if body.get("admin").is_some() {
        tracing::warn!(
            username = %username,
            "Register request included 'admin' field; ignoring. Use /_synapse/admin/v1/register for admin account creation."
        );
    }

    Ok(Json(
        ctx.registration_service.register_user(&username, password, displayname, initial_device_display_name).await?,
    )
    .into_response())
}

/// 若请求携带的 token 属于某个应用服务，返回该服务（MSC4190 的注册身份判据）。
///
/// 只在**确实带了 `Authorization`** 时才尝试解析：普通客户端不带 token、或带的是用户
/// token 时一律按普通注册处理（`validate_token` 不认用户 token ⇒ 落到 `None`）。
async fn appservice_for_register(
    ctx: &AuthContext,
    headers: &HeaderMap,
) -> Result<Option<synapse_services::application_service::ApplicationService>, ApiError> {
    if !headers.contains_key(axum::http::header::AUTHORIZATION) {
        return Ok(None);
    }
    let Ok(as_token) = crate::routes::app_service::extract_as_token(headers) else {
        return Ok(None);
    };
    match ctx.app_service_manager.validate_token(&as_token).await {
        Ok(service) => Ok(Some(service)),
        // 不是 AS token：按普通注册继续，让后面的分支给出 `password required` 或 401，
        // 避免把普通客户端误判成 AS（误判的代价是它永远注册不上）。
        Err(_) => Ok(None),
    }
}

/// MSC4190 的注册语义：**只建用户、不登录**。
///
/// 三条判据（缺一条就偏离上游）：
/// 1. `inhibit_login` 必须显式为 `true` —— 否则 400 `M_APPSERVICE_LOGIN_UNSUPPORTED`。
///    这条路径**不签发 token**；静默忽略会让 AS 以为拿到了会话。
/// 2. 用户必须落在该 AS 的 `users` 命名空间内（**任一条目即可**，不要求 `exclusive`，
///    与上游 `is_interested_in_user` 同口径；`exclusive` 只在 `/login` 冒充时要求）。
/// 3. 响应只有 `user_id`（+ `well_known`），**没有** `access_token` / `device_id`。
async fn register_appservice_user(
    ctx: &AuthContext,
    service: &synapse_services::application_service::ApplicationService,
    body: &Value,
) -> Result<Response, ApiError> {
    let inhibit_login = body.get("inhibit_login").and_then(Value::as_bool).unwrap_or(false);
    if !inhibit_login {
        return Err(ApiError::appservice_login_unsupported(
            "Application services must register users with 'inhibit_login' set to true",
        ));
    }

    let username = body.get("username").and_then(Value::as_str).ok_or_else(|| {
        ApiError::bad_request("username is required for application service registration".to_string())
    })?;

    let user_id = format!("@{username}:{}", ctx.server_name);
    if !ctx.app_service_manager.is_interested_in_user(service, &user_id) {
        return Err(ApiError::forbidden(
            "Cannot register this user: it is outside the application service's user namespace".to_string(),
        ));
    }

    let displayname = body.get("displayname").and_then(Value::as_str);
    let user = ctx.credential_auth.register_without_login(username, displayname).await?;

    Ok(Json(json!({
        "user_id": user.user_id,
        "well_known": {
            "m.homeserver": { "base_url": ctx.config.server.get_public_baseurl() }
        }
    }))
    .into_response())
}

/// See [`check_username_availability`].
pub(crate) async fn check_username_availability(
    State(ctx): State<AuthContext>,
    Query(params): Query<Value>,
) -> Result<Json<Value>, ApiError> {
    let username = params
        .get("username")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Username required".to_string()))?;

    if let Err(e) = ctx.validator.validate_username(username) {
        return Err(e.into());
    }

    // `inhibit_user_in_use_error` (upstream parity, default `false`): report every
    // well-formed localpart as available **without touching the users table**, so
    // this endpoint stops being a membership oracle. Registration still rejects a
    // conflicting localpart when it *completes* (the unique-violation path in
    // `synapse-services/src/auth/register.rs`), which upstream keeps even with the
    // flag on.
    //
    // The syntax check above stays unconditional on purpose: an invalid localpart
    // is rejected regardless of which accounts exist, so a 400 here leaks nothing.
    if ctx.config.server.inhibit_user_in_use_error {
        return Ok(Json(json!({
            "available": true,
            "username": username
        })));
    }

    let user_id = format!("@{}:{}", username, ctx.server_name);
    // U-2: username availability is an *existence* probe, not an authorization
    // one. A deactivated account's row still exists and `register_with_device_name`
    // enforces uniqueness with an INSERT (users.user_id PK), so reporting the
    // name as available would be a lie that the registration path then rejects
    // with M_USER_IN_USE. Hence the row-existence predicate here.
    let exists = ctx.account_identity_service.user_exists(&user_id).await?;

    Ok(Json(json!({
        "available": !exists,
        "username": username
    })))
}

/// Which `/requestToken` flow is being served.
///
/// The anti-enumeration gate has to know **which** answer about an address's
/// registration state is the sensitive one, because the two directions are
/// opposite: a password reset leaks "no account uses this address", while
/// registration / 3PID-add leak "an account already uses this address".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RequestTokenPurpose {
    /// `POST /account/password/email/requestToken`.
    PasswordReset,
    /// `POST /register/email/requestToken`.
    Register,
    /// `POST /account/3pid/email/requestToken`.
    ThreePidAdd,
}

impl RequestTokenPurpose {
    /// The `session_data.purpose` marker persisted with the verification
    /// session. The 3PID-add completion path matches on the literal
    /// `"3pid_add"`, so these values are load-bearing.
    fn as_str(self) -> &'static str {
        match self {
            Self::PasswordReset => "password_reset",
            Self::Register => "register",
            Self::ThreePidAdd => "3pid_add",
        }
    }
}

/// Does this flow's response have to hide `is_registered`?
///
/// `true` means the observed registration state is exactly the fact an attacker
/// is probing for, so the response must not vary with it. This is the predicate
/// that `request_token_inhibit_3pid_errors` gates.
///
/// Deliberately a pure function so the truth table is exercised by unit tests
/// without needing a live `AuthContext` (an anti-enumeration test that never
/// calls production code cannot fail when the behaviour is removed).
pub(crate) fn hides_registration_state(purpose: RequestTokenPurpose, is_registered: bool) -> bool {
    match purpose {
        // Hides "this address is *not* registered".
        RequestTokenPurpose::PasswordReset => !is_registered,
        // Hides "this address is already taken".
        RequestTokenPurpose::Register | RequestTokenPurpose::ThreePidAdd => is_registered,
    }
}

/// The error returned once `request_token_inhibit_3pid_errors` is turned off.
///
/// Statuses and codes are the ones the Matrix spec documents for these
/// endpoints (both `400`): `M_THREEPID_NOT_FOUND` for the password-reset request
/// (`password_management.yaml`; the spec wording explicitly adds that
/// `M_THREEPID_IN_USE` "may not be returned" here), and `M_THREEPID_IN_USE` for
/// registration — which `/account/3pid/email/requestToken` is specified to
/// mirror (`administrative_contact.yaml`).
fn honest_request_token_error(purpose: RequestTokenPurpose) -> ApiError {
    match purpose {
        RequestTokenPurpose::PasswordReset => ApiError::threepid_not_found("Email not found".to_string()),
        RequestTokenPurpose::Register | RequestTokenPurpose::ThreePidAdd => {
            // Note: 400, not the 409 that `ApiError::threepid_in_use` builds —
            // the spec puts `M_THREEPID_IN_USE` under 400 for this endpoint.
            ApiError::bad_request("Email is already in use".to_string()).with_code(MatrixErrorCode::ThreepidInUse)
        }
    }
}

/// See [`request_email_verification`].
pub(crate) async fn request_email_verification(
    State(ctx): State<AuthContext>,
    headers: HeaderMap,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);
    request_email_verification_with_submit_path(
        &ctx,
        &body,
        "/_matrix/client/v3/register/email/submitToken",
        None,
        RequestTokenPurpose::Register,
        &request_id,
    )
    .await
}

/// Issue (or appear to issue) an email verification token for `email`.
///
/// `actor_user_id` is the authenticated caller when the flow has one (3PID-add);
/// `purpose` selects both the persisted session marker and the direction of the
/// anti-enumeration gate.
///
/// Anti-enumeration (`request_token_inhibit_3pid_errors`, default `true`,
/// upstream 20243): when the registration state we just learned is the sensitive
/// one for this flow, the response must not vary with it. A session is still
/// created — so the client gets a *real* `sid` and a byte-identical response —
/// but it is bound to **no** user. Every downstream consumer requires a bound
/// user (the 3PID-add completion path rejects a session without one), so the
/// placeholder can never be redeemed into a password reset or a 3PID binding.
///
/// With the knob off, the honest spec error is returned instead. Either way the
/// caller cannot tell the two registration states apart from this endpoint.
pub(crate) async fn request_email_verification_with_submit_path(
    ctx: &AuthContext,
    body: &Value,
    submit_path: &str,
    actor_user_id: Option<&str>,
    purpose: RequestTokenPurpose,
    request_id: &str,
) -> Result<Json<Value>, ApiError> {
    let email = body
        .get("email")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Email is required".to_string()))?;

    if ctx.validator.validate_email(email).is_err() {
        return Err(ApiError::bad_request("Invalid email address format".to_string()));
    }

    let client_secret = body
        .get("client_secret")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("client_secret is required".to_string()))?;

    let _send_attempt = body.get("send_attempt").and_then(|v| v.as_u64()).unwrap_or(1);

    // ── Anti-enumeration gate ────────────────────────────────────────────────
    // Resolve the address once, then decide whether what we learned is something
    // this flow must not disclose.
    let owner_user_id = ctx.account_identity_service.resolve_user_id_by_email(email, request_id).await;
    let is_registered = owner_user_id.is_some();

    if purpose == RequestTokenPurpose::PasswordReset && !is_registered {
        // Operator-side observability only; never surfaced to the client.
        ::tracing::info!(
            request_id = %request_id,
            event = "password_reset_email_not_registered",
            email = %email,
            "Password reset requested for an email with no associated account"
        );
    }

    let must_hide = hides_registration_state(purpose, is_registered);
    if must_hide && !ctx.config.server.request_token_inhibit_3pid_errors {
        return Err(honest_request_token_error(purpose));
    }

    // The session is bound to a user only when this flow legitimately knows one
    // *and* we are not hiding a registration state. Dropping the binding is what
    // makes the placeholder inert.
    let session_user_id: Option<&str> = if must_hide {
        None
    } else {
        match purpose {
            RequestTokenPurpose::PasswordReset => owner_user_id.as_deref(),
            RequestTokenPurpose::Register => None,
            RequestTokenPurpose::ThreePidAdd => actor_user_id,
        }
    };

    let purpose_marker = purpose.as_str();

    let token = ctx.credential_auth.generate_email_verification_token().map_err(|e| {
        ::tracing::error!(
            request_id = %request_id,
            purpose = %purpose_marker,
            error = %e,
            "Failed to generate email verification token"
        );
        e
    })?;

    let session_data = serde_json::json!({
        "client_secret": client_secret,
        "purpose": purpose_marker,
    });

    let token_id = ctx
        .email_verification_service
        .create_verification_token(email, &token, 3600, session_user_id, Some(session_data))
        .await
        .map_err(|e| {
            ::tracing::error!(
                request_id = %request_id,
                purpose = %purpose_marker,
                email = %email,
                user_id = ?session_user_id,
                error = %e,
                "Failed to store email verification token"
            );
            ApiError::internal("Failed to store verification token. Please try again later.".to_string())
        })?;

    let sid = format!("{token_id}");

    let submit_url = format!("{}{}", ctx.config.server.get_public_baseurl(), submit_path);

    ::tracing::info!(
        request_id = %request_id,
        purpose = %purpose_marker,
        email = %email,
        user_id = ?session_user_id,
        registration_state_hidden = must_hide,
        sid = %sid,
        "Email verification token created"
    );

    Ok(Json(json!({
        "sid": sid,
        "submit_url": submit_url,
        "expires_in": 3600
    })))
}

/// See [`session_client_secret`].
pub(crate) fn session_client_secret(session_data: Option<&Value>) -> Option<&str> {
    match session_data {
        Some(Value::String(secret)) => Some(secret.as_str()),
        Some(Value::Object(map)) => map.get("client_secret").and_then(|v| v.as_str()),
        _ => None,
    }
}

/// See [`submit_email_token`].
pub(crate) async fn submit_email_token(
    State(ctx): State<AuthContext>,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, ApiError> {
    let sid = body
        .get("sid")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Session ID (sid) is required".to_string()))?;

    let client_secret = body
        .get("client_secret")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Client secret is required".to_string()))?;

    let token = body
        .get("token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Verification token is required".to_string()))?;

    let sid_int: i64 = sid.parse().map_err(|_| ApiError::bad_request("Invalid session ID format".to_string()))?;

    // `request_token_inhibit_3pid_errors` (default true) also hides *which*
    // session ids exist: without it, an unknown `sid` and a wrong token are
    // rejected with different messages, and `sid` is a small sequential integer,
    // so the difference is trivial to walk.
    ctx.email_verification_service
        .validate_and_consume_token(sid_int, token, client_secret, ctx.config.server.request_token_inhibit_3pid_errors)
        .await?;

    Ok(Json(json!({
        "success": true
    })))
}

/// See [`get_login_flows`].
pub(crate) async fn get_login_flows(State(ctx): State<AuthContext>) -> Json<Value> {
    let mut flows = vec![json!({"type": "m.login.password"}), json!({"type": "m.login.token"})];

    let mut sso_providers = Vec::new();

    // 检查 SAML SSO
    #[cfg(feature = "saml-sso")]
    {
        sso_providers.push(json!({
            "id": "saml",
            "name": "SAML",
            "brand": "saml"
        }));
    }

    // 检查 OIDC
    if ctx.oidc_service.is_some() {
        sso_providers.push(json!({
            "id": "oidc",
            "name": "OIDC",
            "brand": "oidc"
        }));
    }

    // 检查 CAS
    #[cfg(feature = "cas-sso")]
    {
        sso_providers.push(json!({
            "id": "cas",
            "name": "CAS",
            "brand": "cas"
        }));
        flows.push(json!({"type": "m.login.cas"}));
    }

    // 如果有任何 SSO 提供商，添加 m.login.sso 类型
    if !sso_providers.is_empty() {
        flows.push(json!({
            "type": "m.login.sso",
            "identity_providers": sso_providers
        }));
    }

    // 检查内置 OIDC Provider
    #[cfg(feature = "builtin-oidc")]
    if ctx.builtin_oidc_provider.is_some() {
        flows.push(json!({"type": "m.login.oidc"}));
    }

    Json(json!({ "flows": flows }))
}

/// See [`get_register_flows`].
pub(crate) async fn get_register_flows() -> Json<Value> {
    Json(json!({
        "flows": [
            {"type": "m.login.dummy"},
            {"type": "m.login.password"}
        ],
        "params": {}
    }))
}

/// See [`login`].
pub(crate) async fn login(
    State(ctx): State<AuthContext>,
    peer: crate::utils::ip::PeerAddr,
    headers: HeaderMap,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, ApiError> {
    let login_type = body.get("type").and_then(|v| v.as_str()).unwrap_or("m.login.password");

    // ── m.login.token: QR sign-in token exchange ──
    // The new device sends a login token (generated by the existing device
    // via POST /v1/login/qr_token) to obtain its own access token.
    if login_type == "m.login.token" {
        let token = body
            .get("token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ApiError::bad_request("Token required for m.login.token".to_string()))?;

        let (user_id, _existing_device_id) = ctx
            .login_token_service
            .consume(token)
            .await?
            .ok_or_else(|| ApiError::forbidden("Invalid or expired login token".to_string()))?;

        let device_id = body.get("device_id").and_then(|v| v.as_str()).unwrap_or("QR_LOGIN_DEVICE");

        // Generate a new access token for the new device
        let access_token = ctx
            .token_auth
            .generate_access_token(&user_id, device_id, false)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to generate access token", e))?;

        let refresh_token = ctx
            .token_auth
            .generate_refresh_token(&user_id, device_id, &access_token)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to generate refresh token", e))?;

        return Ok(Json(format_token_response(
            &access_token,
            &refresh_token,
            ctx.token_auth.token_expiry(),
            device_id,
            &user_id,
            &ctx.config.server.get_public_baseurl(),
        )));
    }

    // ── m.login.application_service: Application Service impersonation ──
    // The AS authenticates with its `as_token` and may only log in as a user
    // inside one of its exclusive namespaces.  Deliberately NOT advertised in
    // `/login` `flows` (upstream does not either).
    if login_type == "m.login.application_service" {
        let as_token = crate::routes::app_service::extract_as_token(&headers)?;
        let service = ctx.app_service_manager.validate_token(&as_token).await?;

        let user_id = body
            .get("identifier")
            .and_then(|identifier| identifier.get("user"))
            .or_else(|| body.get("user"))
            .and_then(|value| value.as_str())
            .ok_or_else(|| ApiError::bad_request("User required for m.login.application_service".to_string()))?;

        if !ctx.app_service_manager.can_login_as(&service, user_id) {
            return Err(ApiError::forbidden("Application service may not log in as this user".to_string()));
        }

        let user = ctx
            .user_service
            .get_user(user_id)
            .await?
            .ok_or_else(|| ApiError::forbidden("User not found".to_string()))?;
        if user.is_deactivated {
            return Err(ApiError::forbidden("User is deactivated".to_string()));
        }

        let device_id = body.get("device_id").and_then(|value| value.as_str()).unwrap_or("AS_LOGIN_DEVICE");

        // The refresh token carries a FK to `devices(device_id)`, so an
        // application-service login must materialise the device row before
        // tokens are issued (the AS supplies a device_id, it does not go
        // through the password/QR device flow).
        if ctx.account_device_list_service.get_device(device_id).await?.is_none() {
            ctx.account_device_list_service.create_device(device_id, user_id, None).await?;
        }

        let access_token = ctx
            .token_auth
            .generate_appservice_access_token(user_id, device_id, user.is_admin, &service.as_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to generate access token", e))?;

        let refresh_token = ctx
            .token_auth
            .generate_refresh_token(user_id, device_id, &access_token)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to generate refresh token", e))?;

        return Ok(Json(format_token_response(
            &access_token,
            &refresh_token,
            ctx.token_auth.token_expiry(),
            device_id,
            user_id,
            &ctx.config.server.get_public_baseurl(),
        )));
    }

    // ── m.login.password (default) ──
    let username = body
        .get("identifier")
        .and_then(|id| id.get("user"))
        .or_else(|| body.get("user"))
        .or_else(|| body.get("username"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Username required".to_string()))?;
    let password = body
        .get("password")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Password required".to_string()))?;

    if username.is_empty() || password.is_empty() {
        return Err(ApiError::bad_request("Username and password are required".to_string()));
    }

    if username.len() > 255 {
        return Err(ApiError::bad_request("Username too long".to_string()));
    }

    if password.len() > 128 {
        return Err(ApiError::bad_request("Password too long (max 128 characters)".to_string()));
    }

    let device_id = body.get("device_id").and_then(|v| v.as_str());
    let initial_display_name = body.get("initial_display_name").and_then(|v| v.as_str());
    let mfa_code = body.get("mfa_code").and_then(|v| v.as_str());

    // D8: Check login lockout before attempting authentication.
    //
    // The key must not be attacker-chosen: this used to read the **left-most**
    // `X-Forwarded-For` entry directly, and since nginx is configured with
    // `$proxy_add_x_forwarded_for` (which *appends*) that entry is whatever the
    // client sent — so rotating the header handed the attacker a fresh
    // `login_fail:{ip}:{username}` bucket and the 5-attempt lockout never engaged.
    // `effective_client_ip` is the same decision the rate limiter makes: forwarded
    // headers only when the deployment declares them trustworthy, and even then the
    // right-most untrusted hop rather than the client-supplied left edge.
    //
    // Operational note: this follows `rate_limit.trust_forwarded`, which is **false**
    // by default and unset in `docker/config/`. Behind a reverse proxy that means the
    // peer address (the proxy) is used, so the `(ip, username)` key degenerates to
    // per-username — the lockout still engages, but a client cannot be isolated and
    // any client can lock a name out. Deployments behind a proxy should therefore set
    // `SYNAPSE__RATE_LIMIT__TRUST_FORWARDED=true` in the environment (or
    // `rate_limit.trust_forwarded: true` in `homeserver.yaml`: this lockout reads
    // `ctx.config.rate_limit`, i.e. the `homeserver.yaml` view) *and* list the proxy in
    // `rate_limit.trusted_proxies`, which yields the real client address via the
    // right-most untrusted hop while remaining unspoofable (an unlisted peer's headers
    // are ignored outright).
    //
    // The `TRUST_FORWARDED_HEADERS` env var is **not** the knob here: its only consumer
    // is `is_forwarded_headers_trusted()` (`middleware/mod.rs`) →
    // `extract_request_origin()` → `same_origin()`, i.e. the CSRF (`csrf.rs`) and CORS
    // (`cors.rs`) same-origin decisions. It does not feed the rate limiter or this
    // lockout.
    let rate_limit = &ctx.config.rate_limit;
    let client_ip = crate::utils::ip::effective_client_ip(
        &headers,
        peer.0,
        rate_limit.trust_forwarded,
        &rate_limit.ip_header_priority,
        &rate_limit.trusted_proxies,
    );
    check_login_lockout(&ctx.cache, &ctx.config, &client_ip, username).await?;

    enforce_admin_login_mfa_svc(&ctx.config.security, ctx.user_service.as_ref(), username, mfa_code).await?;

    // D8: Record failures and clear on success.
    match ctx.credential_auth.login(username, password, device_id, initial_display_name).await {
        Ok((user, access_token, refresh_token, device_id)) => {
            clear_login_failures(&ctx.cache, &client_ip, username).await;
            Ok(Json(format_token_response(
                &access_token,
                &refresh_token,
                ctx.token_auth.token_expiry(),
                &device_id,
                &user.user_id(),
                &ctx.config.server.get_public_baseurl(),
            )))
        }
        Err(e) => {
            record_login_failure(&ctx.cache, &ctx.config, &client_ip, username).await;
            Err(e)
        }
    }
}

/// Generate a short-lived login token for QR sign-in.
/// POST /_matrix/client/v1/login/qr_token
/// Requires authentication (the existing device must be logged in).
pub(crate) async fn generate_qr_login_token(
    State(ctx): State<AuthContext>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let token = ctx.login_token_service.generate(&auth_user.user_id, auth_user.device_id.as_deref()).await?;

    Ok(Json(json!({
        "login_token": token,
        "expires_in_ms": ctx.login_token_service.ttl_ms()
    })))
}

/// See [`logout`].
pub(crate) async fn logout(
    State(ctx): State<AuthContext>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    ctx.token_auth.logout(&auth_user.access_token, auth_user.device_id.as_deref()).await?;

    Ok(Json(json!({})))
}

/// See [`logout_all`].
pub(crate) async fn logout_all(
    State(ctx): State<AuthContext>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    ctx.token_auth.logout_all(&auth_user.user_id).await?;

    Ok(Json(json!({})))
}

/// See [`refresh_token`].
pub(crate) async fn refresh_token(
    State(ctx): State<AuthContext>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let refresh_token = body
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Refresh token required".to_string()))?;

    let (new_access, new_refresh, device_id) = ctx.token_auth.refresh_token(refresh_token).await?;

    Ok(Json(json!({
        "access_token": new_access,
        "refresh_token": new_refresh,
        "expires_in": ctx.token_auth.token_expiry(),
        "device_id": device_id
    })))
}

/// See [`login_fallback_page`].
pub(crate) async fn login_fallback_page(
    State(ctx): State<AuthContext>,
) -> Result<axum::response::Html<String>, ApiError> {
    let flows = get_login_flows(State(ctx)).await;
    let empty_vec = vec![];
    let flows_data = flows.0.get("flows").and_then(|f| f.as_array()).unwrap_or(&empty_vec);

    let mut flows_html = String::new();

    for flow in flows_data {
        let flow_type = flow.get("type").and_then(|t| t.as_str()).unwrap_or("");

        match flow_type {
            "m.login.password" => {
                flows_html.push_str(
                    r#"
                <div class="flow">
                    <h3>Password Login</h3>
                    <form method="POST" action="/_matrix/client/v3/login">
                        <input type="hidden" name="type" value="m.login.password">
                        <div>
                            <label>Username:</label>
                            <input type="text" name="identifier[user]" required>
                        </div>
                        <div>
                            <label>Password:</label>
                            <input type="password" name="password" required>
                        </div>
                        <button type="submit">Login</button>
                    </form>
                </div>
                "#,
                );
            }
            "m.login.sso" => {
                if let Some(providers) = flow.get("identity_providers").and_then(|p| p.as_array()) {
                    flows_html.push_str("<div class=\"flow\"><h3>SSO Login</h3>");
                    for provider in providers {
                        let id = provider.get("id").and_then(|i| i.as_str()).unwrap_or("");
                        let name = provider.get("name").and_then(|n| n.as_str()).unwrap_or(id);
                        let safe_name = html_escape(name);
                        flows_html.push_str(&format!(
                            r#"<a href="/_matrix/client/v3/login/sso/redirect?redirectUrl=/">Login with {safe_name}</a><br>"#
                        ));
                    }
                    flows_html.push_str("</div>");
                }
            }
            "m.login.cas" => {
                flows_html.push_str(
                    r#"
                <div class="flow">
                    <h3>CAS Login</h3>
                    <a href="/cas/login?service=/">Login with CAS</a>
                </div>
                "#,
                );
            }
            _ => {}
        }
    }

    let html = format!(
        r#"<!doctype html>
<html>
<head>
    <meta charset="utf-8">
    <title>Login - Matrix</title>
    <style>
        body {{
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
            max-width: 600px;
            margin: 50px auto;
            padding: 20px;
        }}
        h1 {{
            color: #333;
        }}
        .flow {{
            margin: 20px 0;
            padding: 20px;
            border: 1px solid #ddd;
            border-radius: 8px;
        }}
        .flow h3 {{
            margin-top: 0;
        }}
        form div {{
            margin: 10px 0;
        }}
        label {{
            display: inline-block;
            width: 100px;
        }}
        input[type="text"], input[type="password"] {{
            padding: 8px;
            width: 300px;
            border: 1px solid #ddd;
            border-radius: 4px;
        }}
        button, a {{
            display: inline-block;
            padding: 10px 20px;
            background: #0066cc;
            color: white;
            text-decoration: none;
            border: none;
            border-radius: 4px;
            cursor: pointer;
        }}
        button:hover, a:hover {{
            background: #0052a3;
        }}
    </style>
</head>
<body>
    <h1>Login to Matrix</h1>
    {flows_html}
</body>
</html>"#
    );

    Ok(axum::response::Html(html))
}

/// Escape HTML special characters to prevent XSS when inserting untrusted
/// strings (e.g. SSO provider names) into HTML templates.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#x27;")
}

/// Spec fallback authentication page: `GET /_matrix/client/v3/auth/{authType}/fallback/web`.
///
/// Matrix spec (Client-Server API §3.3.4): when a client cannot handle a
/// particular authentication stage natively, it opens this URL in a web view
/// and the user completes the stage there. The page must call back into the
/// client via `window.onAuthDone` (per spec) once the stage completes.
///
/// This server exposes only password / SSO / token flows today, so the page
/// renders whichever stages the running build actually offers and tells the
/// user when the requested `auth_type` is not available. A per-request UIA
/// session id is echoed back as a hidden field so that, once server-side UIA
/// session persistence lands, the same page can submit into it without a
/// contract change.
pub(crate) async fn auth_fallback_web(
    State(ctx): State<AuthContext>,
    axum::extract::Path(auth_type): axum::extract::Path<String>,
) -> Result<axum::response::Html<String>, ApiError> {
    let flows = get_login_flows(State(ctx)).await;
    let empty_vec = vec![];
    let flows_data = flows.0.get("flows").and_then(|f| f.as_array()).unwrap_or(&empty_vec);

    let auth_type_escaped = html_escape(&auth_type);
    let mut stages_html = String::new();

    for flow in flows_data {
        let flow_type = flow.get("type").and_then(|t| t.as_str()).unwrap_or("");

        match flow_type {
            "m.login.password" => {
                stages_html.push_str(
                    r#"
                <div class="flow">
                    <h3>Password Login</h3>
                    <form method="POST" action="/_matrix/client/v3/login">
                        <input type="hidden" name="type" value="m.login.password">
                        <div>
                            <label>Username:</label>
                            <input type="text" name="identifier[user]" required>
                        </div>
                        <div>
                            <label>Password:</label>
                            <input type="password" name="password" required>
                        </div>
                        <button type="submit">Login</button>
                    </form>
                </div>
                "#,
                );
            }
            "m.login.sso" => {
                if let Some(providers) = flow.get("identity_providers").and_then(|p| p.as_array()) {
                    stages_html.push_str("<div class=\"flow\"><h3>SSO Login</h3>");
                    for provider in providers {
                        let id = provider.get("id").and_then(|i| i.as_str()).unwrap_or("");
                        let name = provider.get("name").and_then(|n| n.as_str()).unwrap_or(id);
                        let safe_name = html_escape(name);
                        stages_html.push_str(&format!(
                            r#"<a href="/_matrix/client/v3/login/sso/redirect?redirectUrl=/">Login with {safe_name}</a><br>"#
                        ));
                    }
                    stages_html.push_str("</div>");
                }
            }
            "m.login.cas" => {
                stages_html.push_str(
                    r#"
                <div class="flow">
                    <h3>CAS Login</h3>
                    <a href="/cas/login?service=/">Login with CAS</a>
                </div>
                "#,
                );
            }
            _ => {}
        }
    }

    // Spec: unknown auth types must not 404 — clients rely on the page being
    // reachable to display *something*; render an explicit "not supported"
    // notice instead so the webview closes gracefully via onAuthDone.
    let unsupported_notice = format!(
        r#"<div class="flow unsupported">
            <h3>Unsupported authentication stage</h3>
            <p>This homeserver does not offer a web fallback for <code>{auth_type_escaped}</code>.</p>
        </div>"#
    );

    let html = format!(
        r#"<!doctype html>
<html>
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>Authentication - Matrix</title>
    <style>
        body {{
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
            max-width: 600px;
            margin: 50px auto;
            padding: 20px;
        }}
        h1 {{
            color: #333;
        }}
        .flow {{
            margin: 20px 0;
            padding: 20px;
            border: 1px solid #ddd;
            border-radius: 8px;
        }}
        .flow.unsupported {{
            border-color: #e24b4a;
            background: #fcebeb;
        }}
        .flow h3 {{
            margin-top: 0;
        }}
        form div {{
            margin: 10px 0;
        }}
        label {{
            display: inline-block;
            width: 100px;
        }}
        input[type="text"], input[type="password"] {{
            padding: 8px;
            width: 300px;
            border: 1px solid #ddd;
            border-radius: 4px;
        }}
        button, a {{
            display: inline-block;
            padding: 10px 20px;
            background: #0066cc;
            color: white;
            text-decoration: none;
            border: none;
            border-radius: 4px;
            cursor: pointer;
        }}
        button:hover, a:hover {{
            background: #0052a3;
        }}
    </style>
</head>
<body>
    <h1>Authentication ({auth_type_escaped})</h1>
    {stages_html}
    {unsupported_notice}
    <script>
        // Spec contract: clients poll for window.onAuthDone to close the
        // fallback webview. Defined eagerly so a page that finishes early
        // (e.g. SSO redirect back) still signals the waiting client.
        window.onAuthDone = function () {{}};
    </script>
</body>
</html>"#
    );

    Ok(axum::response::Html(html))
}

#[cfg(test)]
mod request_token_anti_enumeration_tests {
    use super::{hides_registration_state, honest_request_token_error, RequestTokenPurpose};
    use synapse_common::{ApiErrorKind, MatrixErrorCode};

    /// The two directions are opposite, and getting one backwards silently
    /// *creates* the leak the flag exists to close: a password reset hides
    /// "unregistered", registration / 3PID-add hide "already taken".
    #[test]
    fn hides_exactly_the_sensitive_registration_state_per_purpose() {
        assert!(
            hides_registration_state(RequestTokenPurpose::PasswordReset, false),
            "a password reset must hide that the address has no account"
        );
        assert!(
            !hides_registration_state(RequestTokenPurpose::PasswordReset, true),
            "an address with an account is not the sensitive fact for a password reset"
        );

        for purpose in [RequestTokenPurpose::Register, RequestTokenPurpose::ThreePidAdd] {
            assert!(hides_registration_state(purpose, true), "{purpose:?} must hide that the address is already taken");
            assert!(!hides_registration_state(purpose, false), "{purpose:?} has nothing to hide for an unused address");
        }
    }

    /// The honest branch must match the Matrix spec's statuses and errcodes:
    /// both are 400, with `M_THREEPID_NOT_FOUND` for password reset and
    /// `M_THREEPID_IN_USE` for registration / 3PID-add (the latter is specified
    /// as mirroring `/register/email/requestToken`).
    #[test]
    fn honest_errors_match_the_spec_statuses_and_codes() {
        let not_found = honest_request_token_error(RequestTokenPurpose::PasswordReset);
        assert_eq!(not_found.kind, ApiErrorKind::BadRequest);
        assert_eq!(not_found.code, MatrixErrorCode::ThreepidNotFound);
        assert_eq!(not_found.message, "Email not found");

        for purpose in [RequestTokenPurpose::Register, RequestTokenPurpose::ThreePidAdd] {
            let in_use = honest_request_token_error(purpose);
            assert_eq!(
                in_use.kind,
                ApiErrorKind::BadRequest,
                "the spec puts M_THREEPID_IN_USE under 400 for these endpoints, not 409"
            );
            assert_eq!(in_use.code, MatrixErrorCode::ThreepidInUse);
            assert_eq!(in_use.message, "Email is already in use");
        }
    }

    /// `session_data.purpose` is matched literally by the 3PID-add completion
    /// path, so the markers are wire contract, not cosmetics.
    #[test]
    fn session_purpose_markers_are_stable() {
        assert_eq!(RequestTokenPurpose::PasswordReset.as_str(), "password_reset");
        assert_eq!(RequestTokenPurpose::Register.as_str(), "register");
        assert_eq!(RequestTokenPurpose::ThreePidAdd.as_str(), "3pid_add");
    }
}
