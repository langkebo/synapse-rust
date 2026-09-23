// OIDC SSO redirect and callback handlers.

use crate::routes::context::SsoContext;
use crate::routes::formatting::format_token_response;
use axum::{
    extract::{Query, State},
    response::Redirect,
    Json,
};
use serde::Deserialize;
use synapse_common::error::ApiError;
use synapse_services::oidc_service::OidcService;

use super::validate_state_pkce_binding;
use synapse_services::oidc_session_service::OidcAuthSession;

/// The `SsoRedirectQuery` struct.
#[derive(Debug, Deserialize)]
pub(crate) struct SsoRedirectQuery {
    #[serde(rename = "redirectUrl")]
    redirect_url: Option<String>,
    #[serde(rename = "redirect_url")]
    redirect_url_compat: Option<String>,
}

fn is_safe_redirect_url(url: &str, allowlist: &[String]) -> bool {
    // Block dangerous URL schemes
    if url.starts_with("javascript:") || url.starts_with("data:") {
        return false;
    }
    // Block protocol-relative URLs (e.g. //evil.com/callback)
    if url.starts_with("//") {
        return false;
    }
    // Same-origin path is always safe
    if url.starts_with('/') {
        return true;
    }

    // For absolute URLs, validate against the allowlist
    if let Ok(parsed) = url::Url::parse(url) {
        // Only allow http and https schemes
        if parsed.scheme() != "http" && parsed.scheme() != "https" {
            return false;
        }
        if let Some(host) = parsed.host() {
            // Block localhost and loopback addresses
            match host {
                url::Host::Domain("localhost") => return false,
                url::Host::Ipv4(ip) => {
                    if ip.is_loopback() || ip.is_unspecified() {
                        return false;
                    }
                    // Block all raw IPv4 addresses
                    return false;
                }
                url::Host::Ipv6(ip) => {
                    if ip.is_loopback() || ip.is_unspecified() {
                        return false;
                    }
                    // Block all raw IPv6 addresses
                    return false;
                }
                _ => {}
            }

            // Allowlist check for cross-origin redirects
            if allowlist.is_empty() {
                // No allowlist configured — only same-origin paths permitted
                return false;
            }
            // S9: Use structured host comparison instead of starts_with(…)
            // to prevent bypass like https://app.example.com.evil.com matching
            // allowlist entry https://app.example.com.
            let parsed_host = parsed.host_str().unwrap_or("");
            return allowlist.iter().any(|allowed| {
                if let Ok(allowed_url) = url::Url::parse(allowed) {
                    if let Some(allowed_host) = allowed_url.host_str() {
                        return parsed_host == allowed_host;
                    }
                }
                // Fallback: if allowed entry is not a valid URL, treat as literal
                // hostname (e.g. "app.example.com" without scheme)
                parsed_host == allowed.as_str()
            });
        }
    }

    false
}

fn resolve_sso_redirect_url(ctx: &SsoContext, query: &SsoRedirectQuery) -> String {
    let url = query
        .redirect_url
        .clone()
        .or_else(|| query.redirect_url_compat.clone())
        .unwrap_or_else(|| format!("{}/_matrix/client/v3/oidc/callback", ctx.config.server.get_public_baseurl()));

    if !url.is_empty() && !is_safe_redirect_url(&url, &ctx.config.sso_redirect_allowlist) {
        tracing::warn!("Blocked unsafe SSO redirect URL: {}", &url[..url.len().min(64)]);
        return format!("{}/_matrix/client/v3/oidc/callback", ctx.config.server.get_public_baseurl());
    }

    url
}

/// See [`sso_redirect`].
pub(crate) async fn sso_redirect(
    State(ctx): State<SsoContext>,
    Query(query): Query<SsoRedirectQuery>,
) -> Result<Redirect, ApiError> {
    let redirect_uri: String = resolve_sso_redirect_url(&ctx, &query);

    if let Some(oidc_service) = ctx.oidc_service.as_ref() {
        let state_value: String = OidcService::generate_state();
        let nonce_value: String = OidcService::generate_state();
        let (code_verifier, code_challenge): (String, String) = OidcService::generate_pkce();

        ctx.oidc_session_service
            .store(&state_value, &nonce_value, &code_verifier, &code_challenge, "S256", &redirect_uri)
            .await?;

        let authorization_url: String = oidc_service
            .get_authorization_url(&state_value, &redirect_uri, Some(&code_challenge), Some("S256"), Some(&nonce_value))
            .await?;

        return Ok(Redirect::temporary(&authorization_url));
    }

    #[cfg(feature = "saml-sso")]
    if ctx.saml_service.is_enabled() {
        let auth_request: synapse_services::saml_service::SamlAuthRequest =
            ctx.saml_service.get_auth_redirect(Some(&redirect_uri)).await?;
        return Ok(Redirect::temporary(&auth_request.redirect_url));
    }

    Err(ApiError::bad_request("SSO is not enabled".to_string()))
}

/// OIDC Callback Request - handles OIDC authorization callback
#[derive(Debug, Deserialize)]
pub(crate) struct OidcCallbackRequest {
    /// The `code` field.
    pub code: Option<String>,
    /// The `state` field.
    pub state: Option<String>,
    /// The `error` field.
    pub error: Option<String>,
    /// The `error_description` field.
    pub error_description: Option<String>,
}

/// OIDC Callback handler - processes the callback from the OIDC provider after user authorization
pub(crate) async fn oidc_callback(
    State(ctx): State<SsoContext>,
    query: axum::extract::Query<OidcCallbackRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Check that OIDC service is enabled
    let oidc_service: &synapse_services::oidc_service::OidcService =
        ctx.oidc_service.as_ref().ok_or_else(|| ApiError::bad_request("OIDC is not enabled".to_string()))?;

    let OidcCallbackRequest { code, state: callback_state, error, error_description } = query.0;

    // Check for upstream error
    if let Some(err) = error {
        return Err(ApiError::bad_request(format!(
            "OIDC authorization failed: {} - {}",
            err,
            error_description.unwrap_or_default()
        )));
    }

    // Authorization code is required
    let code: String =
        code.ok_or_else(|| ApiError::bad_request("Missing 'code' parameter in OIDC callback".to_string()))?;
    let callback_state: String = callback_state
        .ok_or_else(|| ApiError::bad_request("Missing 'state' parameter in OIDC callback".to_string()))?;
    let auth_session: OidcAuthSession = ctx.oidc_session_service.consume(&callback_state).await?;
    validate_state_pkce_binding(&auth_session)?;

    // Resolve the callback URL
    let callback_url: String = if auth_session.redirect_uri.is_empty() {
        oidc_service
            .get_config()
            .callback_url
            .clone()
            .unwrap_or_else(|| format!("https://{}/_matrix/client/v3/oidc/callback", ctx.server_name))
    } else {
        auth_session.redirect_uri.clone()
    };

    // Exchange code for tokens
    let token_response: synapse_services::oidc_service::OidcTokenResponse = oidc_service
        .exchange_code(&code, &callback_url, Some(auth_session.code_verifier.as_str()), Some(&auth_session.nonce))
        .await
        .map_err(|e| ApiError::internal_with_cause("Token exchange failed", e))?;

    // Fetch user info
    let user_info: synapse_services::oidc_service::OidcUserInfo = oidc_service
        .get_user_info(&token_response.access_token)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to get user info", e))?;

    // Map to Matrix user
    let oidc_user: synapse_services::oidc_service::OidcUser = oidc_service.map_user(&user_info);

    tracing::info!(
        "OIDC callback successful for sub: {}, localpart: {}, email_present: {}, nonce_len: {}",
        oidc_user.subject,
        oidc_user.localpart,
        oidc_user.email.is_some(),
        auth_session.nonce.len()
    );

    // 账号接管防护（fail-closed）—— 对齐 `provider.rs:187-201` 的判定，用 issuer+subject 绑定查询
    // （同一个 `oidc_user_mapping_service`）：若该 OIDC subject **未绑定**任何 Matrix 用户，
    // 而 `localpart` 已被本地账号占用，就拒绝签发令牌。否则任何能让 IdP 断言某个已存在
    // localpart（含 admin）的人都能直接拿到该账号的令牌 —— 即 v1.4 复核指出的 P0。
    let issuer: String = oidc_service.get_config().issuer.clone();
    let subject: String = oidc_user.subject.clone();
    let now_ts: i64 = synapse_common::current_timestamp_millis() / 1000;

    let bound_user_id: Option<String> = ctx.oidc_user_mapping_service.get_bound_user_id(&issuer, &subject).await?;
    if bound_user_id.is_none() {
        // 只有"未绑定"时才需要付出这次 DB 查询；判定本身由纯函数给出（可单测）。
        let localpart_taken = ctx.account_identity_service.get_user_by_username(&oidc_user.localpart).await?.is_some();
        if let Err(rejection) = oidc_login_binding_decision(bound_user_id.as_deref(), localpart_taken) {
            ::tracing::warn!(
                target: "security_audit",
                event = "oidc_localpart_collision_refused",
                issuer = %issuer,
                subject = %subject,
                localpart = %oidc_user.localpart,
                "Refusing OIDC callback: localpart already taken by a non-OIDC-bound account",
            );
            return Err(rejection);
        }
    }

    // 已绑定 → 一律沿用**绑定记录**里的 user_id（忽略 IdP 当前下发的 localpart），并刷新最近登录时间；
    // 未绑定（首次登录，且上面已确认同名未被占用）→ 用 IdP 的 localpart 建号，随后写入绑定。
    let user_id: String = match bound_user_id {
        Some(bound) => {
            ctx.oidc_user_mapping_service.update_last_authenticated(&issuer, &subject, now_ts).await?;
            bound
        }
        None => format!("@{}:{}", oidc_user.localpart, ctx.server_name),
    };

    let existing_user = ctx.account_identity_service.get_user_by_username(&oidc_user.localpart).await?;

    let (user, access_token, refresh_token, device_id) = if let Some(existing) = existing_user {
        // User exists, generate tokens for them
        let device_id: String = uuid::Uuid::new_v4().to_string()[..8].to_string();
        let access_token: String = ctx
            .token_auth
            .generate_access_token(&user_id, &device_id, existing.is_admin)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to generate access token", e))?;
        let refresh_token: String = ctx
            .token_auth
            .generate_refresh_token(&user_id, &device_id, &access_token)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to generate refresh token", e))?;
        (existing, access_token, refresh_token, device_id)
    } else {
        // Create new user — use a random password since auth is by OIDC provider
        let random_password: String = OidcService::generate_state();
        let displayname: Option<&str> = oidc_user.displayname.as_deref();

        match ctx.credential_auth.register(&oidc_user.localpart, &random_password, false, displayname).await {
            Ok(result) => {
                // 首次登录：写入 `(issuer, subject) → user_id` 绑定，使后续登录由**绑定**授权，
                // 而不是由 IdP 每次下发的 localpart 决定（与 `provider.rs:210` 同一顺序）。
                ctx.oidc_user_mapping_service.insert_mapping(&issuer, &subject, &user_id, now_ts).await?;
                result
            }
            Err(e) => {
                // Check if user was created by another request (race condition)
                let error_msg: String = e.to_string();
                if error_msg.contains("already taken") || error_msg.contains("in use") || error_msg.contains("conflict")
                {
                    // User was created by another request, try to get them
                    let existing = ctx
                        .account_identity_service
                        .get_user_by_username(&oidc_user.localpart)
                        .await?
                        .ok_or_else(|| ApiError::internal("User creation failed".to_string()))?;

                    let device_id: String = uuid::Uuid::new_v4().to_string()[..8].to_string();
                    let access_token: String = ctx
                        .token_auth
                        .generate_access_token(&user_id, &device_id, existing.is_admin)
                        .await
                        .map_err(|e| ApiError::internal_with_cause("Failed to generate access token", e))?;
                    let refresh_token: String = ctx
                        .token_auth
                        .generate_refresh_token(&user_id, &device_id, &access_token)
                        .await
                        .map_err(|e| ApiError::internal_with_cause("Failed to generate refresh token", e))?;
                    (existing, access_token, refresh_token, device_id)
                } else {
                    return Err(e);
                }
            }
        }
    };

    let user_id_for_log: String = user.user_id();
    tracing::info!("OIDC user logged in: {}, device_id: {}", user_id_for_log, device_id);

    Ok(Json(format_token_response(
        &access_token,
        &refresh_token,
        ctx.token_auth.token_expiry(),
        &device_id,
        &user_id_for_log,
        &ctx.config.server.get_public_baseurl(),
    )))
}

/// 账号接管防护的**纯判定**：该 OIDC subject 未绑定任何 Matrix 用户（`bound_user_id == None`）
/// 而目标 localpart 已被本地账号占用时，必须拒绝签发令牌。
///
/// 抽成纯函数是为了让这条安全规则**可被单测直接证明**（handler 侧只负责提供两个事实：
/// issuer+subject 的绑定查询结果、以及同名本地账号是否存在），见 `mod tests` 的判定表用例。
fn oidc_login_binding_decision(bound_user_id: Option<&str>, localpart_taken: bool) -> Result<(), ApiError> {
    if bound_user_id.is_none() && localpart_taken {
        return Err(ApiError::unauthorized("OIDC subject is not authorized for this Matrix user".to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_safe_redirect_url_accepts_relative_paths() {
        assert!(is_safe_redirect_url("/_matrix/client/v3/oidc/callback", &[]));
        assert!(is_safe_redirect_url("/home", &[]));
    }

    /// 账号接管防护的判定表（v1.4 复核 P0）。
    ///
    /// 只有"该 OIDC subject 未绑定 + 目标 localpart 已被占用"才拒绝；其余三种都放行
    /// （未绑定且同名可用 = 首次登录，随后写入绑定；已绑定 = 沿用绑定 user_id）。
    #[test]
    fn oidc_callback_binding_decision_table() {
        let denied = oidc_login_binding_decision(None, true);
        assert!(denied.is_err(), "未绑定 + 同名已占用 必须拒绝");
        assert!(denied.unwrap_err().to_string().contains("not authorized"), "拒绝原因必须是该 OIDC subject 未获授权");
        assert!(oidc_login_binding_decision(None, false).is_ok(), "未绑定 + 同名可用 必须放行（首次登录）");
        assert!(oidc_login_binding_decision(Some("@bound:example.com"), true).is_ok(), "已绑定必须放行");
        assert!(oidc_login_binding_decision(Some("@bound:example.com"), false).is_ok(), "已绑定必须放行");
    }

    #[test]
    fn test_is_safe_redirect_url_rejects_cross_origin_without_allowlist() {
        assert!(!is_safe_redirect_url("https://example.com/callback", &[]));
        assert!(!is_safe_redirect_url("https://matrix.example.org/_matrix/client/v3/oidc/callback", &[]));
    }

    #[test]
    fn test_is_safe_redirect_url_accepts_cross_origin_with_allowlist() {
        let allowlist = vec!["https://example.com/".to_string()];
        assert!(is_safe_redirect_url("https://example.com/callback", &allowlist));
        assert!(is_safe_redirect_url("https://example.com/home", &allowlist));
        assert!(!is_safe_redirect_url("https://other.example.com/callback", &allowlist));
    }

    #[test]
    fn test_is_safe_redirect_url_rejects_dangerous_schemes() {
        assert!(!is_safe_redirect_url("javascript:alert(1)", &[]));
        assert!(!is_safe_redirect_url("data:text/html,<script>alert(1)</script>", &[]));
    }

    #[test]
    fn test_is_safe_redirect_url_rejects_localhost_and_loopback() {
        assert!(!is_safe_redirect_url("http://localhost/callback", &[]));
        assert!(!is_safe_redirect_url("http://127.0.0.1/callback", &[]));
        assert!(!is_safe_redirect_url("http://[::1]/callback", &[]));
        assert!(!is_safe_redirect_url("http://0.0.0.0/callback", &[]));
    }

    #[test]
    fn test_is_safe_redirect_url_rejects_raw_ips() {
        assert!(!is_safe_redirect_url("http://192.168.1.1/callback", &[]));
        assert!(!is_safe_redirect_url("https://10.0.0.1/callback", &[]));
    }

    #[test]
    fn test_is_safe_redirect_url_rejects_protocol_relative_urls() {
        assert!(!is_safe_redirect_url("//evil.com/callback", &[]));
    }

    #[test]
    fn test_is_safe_redirect_url_rejects_empty_and_unknown_schemes() {
        assert!(!is_safe_redirect_url("", &[]));
        assert!(!is_safe_redirect_url("ftp://example.com/file", &[]));
        assert!(!is_safe_redirect_url("mailto:test@example.com", &[]));
    }
}
