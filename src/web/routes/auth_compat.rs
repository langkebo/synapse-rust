use crate::common::ApiError;
use crate::web::extractors::{AuthenticatedUser, MatrixJson};
use crate::web::routes::context::AuthContext;
use crate::web::routes::formatting::format_token_response;
use crate::web::utils::admin_auth::enforce_admin_login_mfa_svc;
use crate::web::utils::auth::resolve_request_id;
use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};
/// See [`register`].
pub(crate) async fn register(
    State(ctx): State<AuthContext>,
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

    let user_id = format!("@{}:{}", username, ctx.server_name);
    let exists = ctx.account_identity_service.user_exists(&user_id).await?;

    Ok(Json(json!({
        "available": !exists,
        "username": username
    })))
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
        "register",
        &request_id,
    )
    .await
}

/// See [`request_email_verification_with_submit_path`].
pub(crate) async fn request_email_verification_with_submit_path(
    ctx: &AuthContext,
    body: &Value,
    submit_path: &str,
    user_id: Option<&str>,
    purpose: &str,
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

    let token = ctx.credential_auth.generate_email_verification_token().map_err(|e| {
        ::tracing::error!(
            request_id = %request_id,
            purpose = %purpose,
            error = %e,
            "Failed to generate email verification token"
        );
        e
    })?;

    let session_data = serde_json::json!({
        "client_secret": client_secret,
        "purpose": purpose,
    });

    let token_id = ctx
        .email_verification_storage
        .create_verification_token(email, &token, 3600, user_id, Some(session_data))
        .await
        .map_err(|e| {
            ::tracing::error!(
                request_id = %request_id,
                purpose = %purpose,
                email = %email,
                user_id = ?user_id,
                error = %e,
                "Failed to store email verification token"
            );
            ApiError::internal("Failed to store verification token. Please try again later.".to_string())
        })?;

    let sid = format!("{token_id}");

    let submit_url = format!("{}{}", ctx.config.server.get_public_baseurl(), submit_path);

    ::tracing::info!(
        request_id = %request_id,
        purpose = %purpose,
        email = %email,
        user_id = ?user_id,
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

    ctx.email_verification_storage.validate_and_consume_token(sid_int, token, client_secret).await?;

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

// ---------------------------------------------------------------------------
// D8: Login failure lockout (Redis-backed, fail-open)
// ---------------------------------------------------------------------------

/// Maximum failed login attempts before lockout triggers.
const LOGIN_MAX_ATTEMPTS: u32 = 5;
/// Lockout duration in seconds (15 minutes).
const LOGIN_LOCKOUT_TTL_SECS: u64 = 900;

fn extract_login_client_ip(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .or_else(|| headers.get("x-real-ip").and_then(|v| v.to_str().ok()).map(|s| s.trim().to_string()))
        .unwrap_or_else(|| "unknown".to_string())
}

/// Check if the user is locked out due to too many failed login attempts.
///
/// Failure behavior when Redis is unavailable depends on
/// `config.security.login_lockout_fail_open_on_redis_error`:
/// - `true` (default, backward compatible): skip the lockout check, login proceeds
/// - `false` (recommended for production): return 503 Service Unavailable to
///   block all login attempts while Redis is down. This closes the brute-force
///   window that would otherwise be open during a Redis outage.
async fn check_login_lockout(
    cache: &crate::cache::CacheManager,
    config: &synapse_common::config::Config,
    ip: &str,
    username: &str,
) -> Result<(), ApiError> {
    if !cache.is_redis_enabled() {
        if config.security.login_lockout_fail_open_on_redis_error {
            tracing::warn!(
                ip = %ip,
                username = %username,
                "Login lockout check skipped: Redis unavailable (fail_open_on_redis_error=true)"
            );
            return Ok(());
        }
        tracing::error!(
            ip = %ip,
            username = %username,
            "Login refused: Redis unavailable and fail_open_on_redis_error=false"
        );
        return Err(ApiError::service_unavailable(
            "Login temporarily unavailable: account lockout backend offline".to_string(),
        ));
    }
    let key = format!("login_fail:{ip}:{username}");
    match cache.get::<u32>(&key).await {
        Ok(Some(count)) if count >= LOGIN_MAX_ATTEMPTS => {
            tracing::warn!(ip = %ip, username = %username, count, "Login locked out due to too many failures");
            Err(ApiError::rate_limited_with_retry(LOGIN_LOCKOUT_TTL_SECS * 1000))
        }
        _ => Ok(()),
    }
}

/// Record a failed login attempt. Fail-open on Redis errors when
/// `config.security.login_lockout_fail_open_on_redis_error` is true (default);
/// otherwise silently drop (counter is gone anyway since Redis is down).
async fn record_login_failure(
    cache: &crate::cache::CacheManager,
    config: &synapse_common::config::Config,
    ip: &str,
    username: &str,
) {
    if !cache.is_redis_enabled() {
        if !config.security.login_lockout_fail_open_on_redis_error {
            tracing::error!(
                ip = %ip,
                username = %username,
                "Cannot record login failure: Redis unavailable and fail_open_on_redis_error=false"
            );
        }
        return;
    }
    let key = format!("login_fail:{ip}:{username}");
    let current = cache.get::<u32>(&key).await.ok().flatten().unwrap_or(0);
    let _ = cache.set(&key, current + 1, LOGIN_LOCKOUT_TTL_SECS).await;
}

/// Clear login failure counter on successful login.
async fn clear_login_failures(cache: &crate::cache::CacheManager, ip: &str, username: &str) {
    if !cache.is_redis_enabled() {
        return;
    }
    let key = format!("login_fail:{ip}:{username}");
    cache.delete(&key).await;
}

/// See [`login`].
pub(crate) async fn login(
    State(ctx): State<AuthContext>,
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

        let (user_id, _existing_device_id) =
            crate::web::routes::qr_login_token::consume_login_token(&ctx.login_token_storage, token)
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
    let client_ip = extract_login_client_ip(&headers);
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
    let token = crate::web::routes::qr_login_token::generate_login_token(
        &ctx.login_token_storage,
        &auth_user.user_id,
        auth_user.device_id.as_deref(),
    )
    .await?;

    Ok(Json(json!({
        "login_token": token,
        "expires_in_ms": 60000
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

// ---------------------------------------------------------------------------
// P1 login-lockout hardening — tests
// ---------------------------------------------------------------------------
//
// Background: `docs/audit/P1_security_2026-09-10.md`.
//
// The audit initially suspected that `check_login_lockout`'s `_ => Ok(())` arm
// let a Redis outage disable the brute-force lock. Direct measurement showed
// otherwise: `CacheManager::get()` **never** surfaces backend failures (it logs
// an L1 miss and returns `Ok(None)`), so that arm is unreachable and the local
// (L1) tier is what keeps the lock alive. These tests pin that real behaviour —
// and the `CacheManager::set` TTL defect it exposed, which is guarded separately
// in `synapse-cache`.
#[cfg(test)]
mod lockout_degradation_tests {
    use super::*;
    use deadpool_redis::{Config as RedisPoolConfig, Runtime as RedisRuntime};

    /// Build a `CacheManager` whose Redis backend is unreachable.
    ///
    /// Port 1 is reserved (tcpmux) and never listening, so connect attempts fail
    /// immediately instead of hanging.
    fn cache_with_unreachable_redis() -> crate::cache::CacheManager {
        const BROKEN_URL: &str = "redis://127.0.0.1:1";
        let pool = RedisPoolConfig::from_url(BROKEN_URL)
            .create_pool(Some(RedisRuntime::Tokio1))
            .expect("failed to build Redis pool for degraded-lockout test");
        crate::cache::CacheManager::with_redis_pool_and_url(pool, &crate::cache::CacheConfig::default(), BROKEN_URL)
    }

    /// Build a `Config` whose lockout policy is fail-closed.
    fn fail_closed_config() -> crate::common::config::Config {
        let mut config = crate::common::config::Config::default();
        config.security.login_lockout_fail_open_on_redis_error = false;
        config
    }

    /// Regression guard: the lockout must keep working while Redis is down.
    ///
    /// It survives on the L1 tier rather than through the `_ => Ok(())` arm that
    /// the audit originally flagged. If a refactor ever routes the counter away
    /// from L1 without adding an equivalent fallback, this fails.
    #[tokio::test]
    async fn lockout_engages_while_redis_is_down() {
        let cache = cache_with_unreachable_redis();
        let config = fail_closed_config();
        let (ip, username) = ("203.0.113.20", "local-tier-victim");

        assert!(
            check_login_lockout(&cache, &config, ip, username).await.is_ok(),
            "a fresh identifier must not be locked out"
        );

        for _ in 0..LOGIN_MAX_ATTEMPTS {
            record_login_failure(&cache, &config, ip, username).await;
        }

        let err = check_login_lockout(&cache, &config, ip, username)
            .await
            .expect_err("threshold reached ⇒ lock must engage even without Redis");
        assert_eq!(err.kind, crate::common::ApiErrorKind::RateLimited);
    }

    /// The counter is scoped per `(ip, username)`: one victim's failures must not
    /// lock out unrelated identifiers.
    #[tokio::test]
    async fn lockout_counter_is_scoped_per_ip_and_username() {
        let cache = cache_with_unreachable_redis();
        let config = fail_closed_config();

        for _ in 0..LOGIN_MAX_ATTEMPTS {
            record_login_failure(&cache, &config, "203.0.113.8", "scoped-victim").await;
        }

        assert!(
            check_login_lockout(&cache, &config, "203.0.113.8", "scoped-victim").await.is_err(),
            "the targeted identifier must be locked"
        );
        assert!(
            check_login_lockout(&cache, &config, "203.0.113.8", "other-user").await.is_ok(),
            "a different username from the same IP must not inherit the lock"
        );
        assert!(
            check_login_lockout(&cache, &config, "198.51.100.9", "scoped-victim").await.is_ok(),
            "the same username from a different IP must not inherit the lock"
        );
    }

    /// When Redis is simply not configured at all, the pre-existing
    /// `login_lockout_fail_open_on_redis_error` semantics stay in force — this is
    /// the one path that flag actually governs.
    #[tokio::test]
    async fn lockout_is_skipped_when_redis_is_unconfigured() {
        let cache = crate::cache::CacheManager::new(&crate::cache::CacheConfig::default());
        assert!(!cache.is_redis_enabled(), "in-memory cache must report Redis as disabled");

        let mut lenient = crate::common::config::Config::default();
        lenient.security.login_lockout_fail_open_on_redis_error = true;

        for _ in 0..(LOGIN_MAX_ATTEMPTS * 3) {
            record_login_failure(&cache, &lenient, "203.0.113.10", "no-redis").await;
        }
        assert!(
            check_login_lockout(&cache, &lenient, "203.0.113.10", "no-redis").await.is_ok(),
            "fail_open_on_redis_error=true keeps the documented allow-through behaviour"
        );

        let mut strict = crate::common::config::Config::default();
        strict.security.login_lockout_fail_open_on_redis_error = false;
        assert!(
            check_login_lockout(&cache, &strict, "203.0.113.11", "no-redis-strict").await.is_err(),
            "fail_open_on_redis_error=false must refuse login while the backend is unavailable"
        );
    }
}
