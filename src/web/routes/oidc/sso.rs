// OIDC SSO redirect and callback handlers.

use crate::common::error::ApiError;
use crate::web::routes::context::SsoContext;
use axum::{
    extract::{Query, State},
    response::Redirect,
    Json,
};
use serde::Deserialize;
use synapse_services::oidc_service::OidcService;

use super::{consume_oidc_auth_session, store_oidc_auth_session, validate_state_pkce_binding, OidcAuthSession};

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
            let url_str = parsed.as_str();
            return allowlist.iter().any(|allowed| url_str.starts_with(allowed));
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

pub(crate) async fn sso_redirect(
    State(ctx): State<SsoContext>,
    Query(query): Query<SsoRedirectQuery>,
) -> Result<Redirect, ApiError> {
    let redirect_uri: String = resolve_sso_redirect_url(&ctx, &query);

    if let Some(oidc_service) = ctx.oidc_service.as_ref() {
        let state_value: String = OidcService::generate_state();
        let nonce_value: String = OidcService::generate_state();
        let (code_verifier, code_challenge): (String, String) = OidcService::generate_pkce();

        store_oidc_auth_session(&state_value, &nonce_value, &code_verifier, &code_challenge, "S256", &redirect_uri)?;

        let authorization_url: String = oidc_service
            .get_authorization_url(&state_value, &redirect_uri, Some(&code_challenge), Some("S256"))
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
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
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
    let auth_session: OidcAuthSession = consume_oidc_auth_session(&callback_state)?;
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
        .exchange_code(&code, &callback_url, Some(auth_session.code_verifier.as_str()))
        .await
        .map_err(|e| ApiError::internal_with_log("Token exchange failed", &e))?;

    // Fetch user info
    let user_info: synapse_services::oidc_service::OidcUserInfo = oidc_service
        .get_user_info(&token_response.access_token)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get user info", &e))?;

    // Map to Matrix user
    let oidc_user: synapse_services::oidc_service::OidcUser = oidc_service.map_user(&user_info);

    tracing::info!(
        "OIDC callback successful for sub: {}, localpart: {}, email_present: {}, nonce_len: {}",
        oidc_user.subject,
        oidc_user.localpart,
        oidc_user.email.is_some(),
        auth_session.nonce.len()
    );

    // Create or log in the Matrix user
    let user_id: String = format!("@{}:{}", oidc_user.localpart, ctx.server_name);

    let existing_user = ctx.account_identity_service.get_user_by_username(&oidc_user.localpart).await?;

    let (user, access_token, refresh_token, device_id) = if let Some(existing) = existing_user {
        // User exists, generate tokens for them
        let device_id: String = uuid::Uuid::new_v4().to_string()[..8].to_string();
        let access_token: String = ctx
            .token_auth
            .generate_access_token(&user_id, &device_id, existing.is_admin)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to generate access token", &e))?;
        let refresh_token: String = ctx
            .token_auth
            .generate_refresh_token(&user_id, &device_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to generate refresh token", &e))?;
        (existing, access_token, refresh_token, device_id)
    } else {
        // Create new user — use a random password since auth is by OIDC provider
        let random_password: String = OidcService::generate_state();
        let displayname: Option<&str> = oidc_user.displayname.as_deref();

        match ctx.credential_auth.register(&oidc_user.localpart, &random_password, false, displayname).await {
            Ok(result) => result,
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
                        .map_err(|e| ApiError::internal_with_log("Failed to generate access token", &e))?;
                    let refresh_token: String = ctx
                        .token_auth
                        .generate_refresh_token(&user_id, &device_id)
                        .await
                        .map_err(|e| ApiError::internal_with_log("Failed to generate refresh token", &e))?;
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

// ---------------------------------------------------------------------------
// Shared formatting helpers
// ---------------------------------------------------------------------------

/// Format a token response JSON object — used by SSO callback, login, and
/// other authentication flows that return access/refresh tokens.
pub(crate) fn format_token_response(
    access_token: &str,
    refresh_token: &str,
    expires_in: i64,
    device_id: &str,
    user_id: &str,
    base_url: &str,
) -> serde_json::Value {
    serde_json::json!({
        "access_token": access_token,
        "refresh_token": refresh_token,
        "expires_in": expires_in,
        "device_id": device_id,
        "user_id": user_id,
        "well_known": {
            "m.homeserver": { "base_url": base_url }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_safe_redirect_url_accepts_relative_paths() {
        assert!(is_safe_redirect_url("/_matrix/client/v3/oidc/callback", &[]));
        assert!(is_safe_redirect_url("/home", &[]));
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
