// OIDC Provider endpoints: authorize, token, userinfo, logout.

use crate::routes::context::SsoContext;
use crate::routes::AuthenticatedUser;
use axum::http::{header, HeaderMap};
use axum::{extract::State, Json};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde::{Deserialize, Serialize};
use synapse_common::error::ApiError;
use synapse_services::oidc_service::OidcService;
use validator::Validate;

use super::current_unix_ts;
use crate::routes::MatrixJson;

/// OIDC Token Request
#[derive(Debug, Deserialize, Validate)]
pub(crate) struct OidcTokenRequest {
    #[validate(length(min = 1, max = 100))]
    /// The `grant_type` field.
    pub grant_type: String,
    #[validate(length(min = 1, max = 2048))]
    /// The `code` field.
    pub code: Option<String>,
    #[validate(length(max = 2048))]
    /// The `redirect_uri` field.
    pub redirect_uri: Option<String>,
    #[validate(length(max = 2048))]
    /// The `refresh_token` field.
    pub refresh_token: Option<String>,
    #[validate(length(max = 1024))]
    /// The `scope` field.
    pub scope: Option<String>,
    #[validate(length(max = 255))]
    /// The `client_id` field.
    pub client_id: Option<String>,
    #[validate(length(min = 43, max = 128))]
    /// The `code_verifier` field.
    pub code_verifier: Option<String>,
    #[validate(length(max = 1024))]
    /// The `client_secret` field (`client_secret_post` auth, RFC 6749 §2.3.1).
    pub client_secret: Option<String>,
}

/// OIDC Token Response
#[derive(Debug, Serialize)]
pub(crate) struct OidcTokenResponse {
    /// The `access_token` field.
    pub access_token: String,
    /// The `token_type` field.
    pub token_type: String,
    /// The `expires_in` field.
    pub expires_in: i64,
    /// The `refresh_token` field.
    pub refresh_token: Option<String>,
    /// The `scope` field.
    pub scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `matrix_user_id` field.
    pub matrix_user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `device_id` field.
    pub device_id: Option<String>,
}

/// OIDC UserInfo Response
#[derive(Debug, Serialize)]
pub(crate) struct OidcUserInfoResponse {
    /// The `sub` field.
    pub sub: String,
    /// The `name` field.
    pub name: Option<String>,
    /// The `picture` field.
    pub picture: Option<String>,
    /// The `email` field.
    pub email: Option<String>,
}

/// OIDC Logout Request
///
/// Accepts two families of fields:
///
/// * `refresh_token` / `device_id` — the revocation targets this server can act on.
///   At least one of them is required (400 `M_MISSING_PARAM` otherwise): the
///   endpoint must not answer `{"success": true}` without revoking anything.
/// * `client_id` / `id_token_hint` / `post_logout_redirect_uri` / `state` — the
///   OpenID Connect RP-Initiated Logout 1.0 payload. They are accepted for wire
///   compatibility (clients that follow the RP spec send them) but cannot be
///   resolved to a server-side session: there is no `id_token_hint` → session
///   index, and this endpoint is a JSON API rather than a browser redirect flow,
///   so `post_logout_redirect_uri` is not used for redirecting either.
#[derive(Debug, Deserialize)]
pub(crate) struct OidcLogoutRequest {
    /// The `refresh_token` field.
    pub refresh_token: Option<String>,
    /// The `device_id` field.
    pub device_id: Option<String>,
    /// The `client_id` field (RP-Initiated Logout).
    pub client_id: Option<String>,
    /// The `id_token_hint` field (RP-Initiated Logout).
    pub id_token_hint: Option<String>,
    /// The `post_logout_redirect_uri` field (RP-Initiated Logout).
    pub post_logout_redirect_uri: Option<String>,
    /// The `state` field (RP-Initiated Logout).
    pub state: Option<String>,
}

/// OIDC Authorize Request
#[derive(Debug, Deserialize, Validate)]
pub(crate) struct OidcAuthorizeRequest {
    #[validate(length(min = 1, max = 50))]
    /// The `response_type` field.
    pub response_type: String,
    #[validate(length(min = 1, max = 255))]
    /// The `client_id` field.
    pub client_id: String,
    #[validate(length(min = 1, max = 2048))]
    /// The `redirect_uri` field.
    pub redirect_uri: String,
    #[validate(length(max = 1024))]
    /// The `scope` field.
    pub scope: Option<String>,
    #[validate(length(max = 512))]
    /// The `state` field.
    pub state: Option<String>,
    #[validate(length(max = 512))]
    /// The `nonce` field.
    pub nonce: Option<String>,
    #[validate(length(max = 128))]
    /// PKCE `code_challenge` supplied by the client (RFC 7636 §4.2).
    ///
    /// When present the server forwards it to the IdP and never issues a
    /// `code_verifier`: only the client that generated the verifier can
    /// complete the code exchange.
    pub code_challenge: Option<String>,
    #[validate(length(max = 10))]
    /// PKCE `code_challenge_method`. Only `S256` is accepted.
    pub code_challenge_method: Option<String>,
}

/// Validate the client-supplied PKCE challenge (RFC 7636 §4.2).
///
/// Returns `Ok(None)` when the client did not send a challenge — that is a
/// confidential client, for which the server generates the PKCE pair itself.
/// Returns the (normalised) challenge method otherwise.
fn validate_client_pkce(
    code_challenge: Option<&str>,
    code_challenge_method: Option<&str>,
) -> Result<Option<String>, ApiError> {
    let Some(challenge) = code_challenge.map(str::trim).filter(|c| !c.is_empty()) else {
        return Ok(None);
    };
    // RFC 7636 §4.3: an absent method means `plain`, which this server does not
    // advertise. Reject it explicitly instead of silently downgrading.
    let method = code_challenge_method.unwrap_or("plain");
    if method != "S256" {
        return Err(ApiError::invalid_param("Unsupported code_challenge_method: only 'S256' is supported"));
    }
    let well_formed = (43..=128).contains(&challenge.len())
        && challenge.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~'));
    if !well_formed {
        return Err(ApiError::invalid_param("Invalid code_challenge: expected 43-128 unreserved characters"));
    }
    Ok(Some(method.to_string()))
}

/// Validate `redirect_uri` against the registered client (open-redirect guard).
///
/// * registered client → the URI must be one of its `redirect_uris`;
/// * unknown client → only the server's own callback or an allowlisted origin
///   is accepted (same rule as `/_matrix/client/v3/login/sso/redirect`).
async fn validate_authorize_redirect_uri(
    ctx: &SsoContext,
    client_id: &str,
    redirect_uri: &str,
) -> Result<(), ApiError> {
    let registered = ctx
        .oauth_client_storage
        .get_client(client_id)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to look up OAuth client", e))?;

    match registered {
        Some(client) => {
            if client.redirect_uris_vec().iter().any(|uri| uri == redirect_uri) {
                Ok(())
            } else {
                Err(ApiError::invalid_param("redirect_uri is not registered for this client_id"))
            }
        }
        None => {
            let own_callback = format!("{}/_matrix/client/v3/oidc/callback", ctx.config.server.get_public_baseurl());
            let configured_callback = ctx.oidc_service.as_ref().and_then(|s| s.get_config().callback_url.clone());
            if redirect_uri == own_callback
                || configured_callback.as_deref() == Some(redirect_uri)
                || super::sso::is_safe_redirect_url(redirect_uri, &ctx.config.sso_redirect_allowlist)
            {
                Ok(())
            } else {
                Err(ApiError::invalid_param("redirect_uri is not registered for this client_id"))
            }
        }
    }
}

/// Requested `scope` must be a subset of the scopes the homeserver is configured for.
fn validate_authorize_scope(requested: Option<&str>, configured: &[String]) -> Result<(), ApiError> {
    let Some(requested) = requested.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(());
    };
    if configured.is_empty() {
        return Ok(());
    }
    if let Some(unknown) = requested.split_whitespace().find(|scope| !configured.iter().any(|allowed| allowed == scope))
    {
        return Err(ApiError::invalid_param(format!("Unsupported scope '{unknown}'")));
    }
    Ok(())
}

/// Parse `Authorization: Basic <base64(client_id:client_secret)>`.
///
/// Returns `Ok(None)` when no Basic credentials are present. Malformed headers
/// are rejected per RFC 6749 §2.3.1.
fn basic_client_credentials(headers: &HeaderMap) -> Result<Option<(String, String)>, ApiError> {
    let Some(value) = headers.get(header::AUTHORIZATION) else {
        return Ok(None);
    };
    let value = value.to_str().map_err(|_| ApiError::bad_request("Malformed Authorization header".to_string()))?;
    let Some(encoded) = value.strip_prefix("Basic ") else {
        return Ok(None);
    };
    let decoded =
        BASE64.decode(encoded.trim()).map_err(|_| ApiError::bad_request("Malformed Basic credentials".to_string()))?;
    let decoded =
        String::from_utf8(decoded).map_err(|_| ApiError::bad_request("Malformed Basic credentials".to_string()))?;
    let (client_id, client_secret) =
        decoded.split_once(':').ok_or_else(|| ApiError::bad_request("Malformed Basic credentials".to_string()))?;
    Ok(Some((client_id.to_string(), client_secret.to_string())))
}

/// OIDC Token endpoint — handles authorization code exchange and refresh token
pub(crate) async fn oidc_token(
    State(ctx): State<SsoContext>,
    headers: HeaderMap,
    MatrixJson(body): MatrixJson<OidcTokenRequest>,
) -> Result<Json<OidcTokenResponse>, ApiError> {
    // Validate input
    body.validate().map_err(|e| ApiError::bad_request(format!("Validation error: {e}")))?;

    // RFC 6749 §2.3.1: client credentials may arrive via HTTP Basic or as body
    // parameters. Basic takes precedence when both are supplied.
    #[cfg_attr(not(feature = "builtin-oidc"), allow(unused_variables))]
    let (client_id, client_secret) = match basic_client_credentials(&headers)? {
        Some((id, secret)) => (Some(id), Some(secret)),
        None => (body.client_id.clone(), body.client_secret.clone()),
    };

    // Prefer the built-in OIDC Provider when available
    #[cfg(feature = "builtin-oidc")]
    if let Some(builtin_provider) = &ctx.builtin_oidc_provider {
        use synapse_services::builtin_oidc_provider::OidcTokenRequest as BuiltinOidcTokenRequest;
        let request = BuiltinOidcTokenRequest {
            grant_type: body.grant_type.clone(),
            code: body.code.clone(),
            redirect_uri: body.redirect_uri.clone(),
            client_id,
            code_verifier: body.code_verifier.clone(),
            refresh_token: body.refresh_token.clone(),
            scope: body.scope.clone(),
            client_secret,
        };

        let token_response = builtin_provider
            .token(request)
            .await
            .map_err(|e| ApiError::internal_with_cause("Builtin OIDC token failed", e))?;

        return Ok(Json(OidcTokenResponse {
            access_token: token_response.access_token,
            token_type: token_response.token_type,
            expires_in: token_response.expires_in,
            refresh_token: token_response.refresh_token,
            scope: token_response.scope.unwrap_or_default(),
            matrix_user_id: None,
            device_id: None,
        }));
    }

    // Fall back to external OIDC service
    let oidc_service: &synapse_services::oidc_service::OidcService =
        ctx.oidc_service.as_ref().ok_or_else(|| ApiError::bad_request("OIDC is not enabled".to_string()))?;

    let OidcTokenRequest { grant_type, code, redirect_uri, refresh_token, scope, code_verifier, .. } = body;

    match grant_type.as_str() {
        "authorization_code" => {
            // Authorization code flow
            let code: String = code.ok_or_else(|| ApiError::bad_request("Missing 'code' parameter".to_string()))?;
            let redirect_uri: String = redirect_uri.unwrap_or_default();

            // Exchange code for tokens via OIDC service
            let token_response: synapse_services::oidc_service::OidcTokenResponse = oidc_service
                .exchange_code(&code, &redirect_uri, code_verifier.as_deref(), None)
                .await
                .map_err(|e| ApiError::internal_with_cause("Token exchange failed", e))?;

            // Fetch user info
            let user_info: synapse_services::oidc_service::OidcUserInfo = oidc_service
                .get_user_info(&token_response.access_token)
                .await
                .map_err(|e| ApiError::internal_with_cause("Failed to get user info", e))?;

            // Map to Matrix user
            let oidc_user: synapse_services::oidc_service::OidcUser = oidc_service.map_user(&user_info);

            let localpart: String = oidc_user.localpart.clone();
            let issuer: String = oidc_service.get_config().issuer.clone();
            let subject: String = oidc_user.subject.clone();
            let server_name: String = ctx.config.server.server_name.clone().unwrap_or_else(|| "localhost".to_string());
            let matrix_user_id: String = format!("@{localpart}:{server_name}");
            let displayname: String = oidc_user.displayname.clone().unwrap_or(localpart.clone());
            let now_ts: i64 = current_unix_ts() as i64;

            // Check OIDC binding record
            let bound_user_id: Option<String> =
                ctx.oidc_user_mapping_service.get_bound_user_id(&issuer, &subject).await?;

            let matrix_user_id: String = if let Some(existing) = bound_user_id {
                // Subsequent login: ignore IdP's current localpart, use the first binding
                ctx.oidc_user_mapping_service.update_last_authenticated(&issuer, &subject, now_ts).await?;
                existing
            } else {
                // First login: if local user exists without OIDC binding, reject to prevent account takeover
                let existing_user = ctx.account_identity_service.get_user_by_id(&matrix_user_id).await.unwrap_or(None);
                if existing_user.is_some() {
                    ::tracing::warn!(
                        target: "security_audit",
                        event = "oidc_localpart_collision_refused",
                        issuer = %issuer,
                        subject = %subject,
                        matrix_user_id = %matrix_user_id,
                        "Refusing OIDC token: localpart already taken by a non-OIDC-bound account",
                    );
                    return Err(ApiError::unauthorized(
                        "OIDC subject is not authorized for this Matrix user".to_string(),
                    ));
                }

                tracing::info!("Creating new Matrix user from OIDC: {}", matrix_user_id);
                let random_password: String = uuid::Uuid::new_v4().to_string();
                ctx.registration_service
                    .register_user(&localpart, &random_password, Some(&displayname), None)
                    .await
                    .map_err(|e| ApiError::internal_with_cause("Failed to register OIDC user", e))?;

                ctx.oidc_user_mapping_service.insert_mapping(&issuer, &subject, &matrix_user_id, now_ts).await?;
                matrix_user_id
            };

            let localpart: String =
                matrix_user_id.strip_prefix('@').and_then(|s| s.split(':').next()).unwrap_or(&localpart).to_string();

            // Generate device ID
            let device_id: String = format!("OIDC{}", &uuid::Uuid::new_v4().to_string().replace("-", "")[..8]);

            // Register the device
            ctx.account_device_list_service.create_device(&device_id, &matrix_user_id, Some("OIDC Device")).await?;

            // Generate Matrix access token
            let user_info = ctx.account_identity_service.get_user_by_username(&localpart).await?;

            let is_admin: bool = user_info.is_some_and(|u| u.is_admin);

            let matrix_token: String =
                ctx.token_auth.generate_access_token(&matrix_user_id, &device_id, is_admin).await?;

            tracing::info!(
                "OIDC token exchange successful for sub: {}, mapped to Matrix user: {}, device_id: {}",
                oidc_user.subject,
                matrix_user_id,
                device_id
            );

            Ok(Json(OidcTokenResponse {
                access_token: matrix_token,
                token_type: "Bearer".to_string(),
                expires_in: 3600,
                refresh_token: token_response.refresh_token,
                scope: scope.unwrap_or_else(|| "openid profile email".to_string()),
                matrix_user_id: Some(matrix_user_id),
                device_id: Some(device_id),
            }))
        }
        "refresh_token" => {
            // Refresh token flow
            let refresh_token: String =
                refresh_token.ok_or_else(|| ApiError::bad_request("Missing 'refresh_token' parameter".to_string()))?;

            let token_response: synapse_services::oidc_service::OidcTokenResponse = oidc_service
                .refresh_token(&refresh_token)
                .await
                .map_err(|e| ApiError::internal_with_cause("Token refresh failed", e))?;

            tracing::info!("OIDC token refresh successful");

            Ok(Json(OidcTokenResponse {
                access_token: token_response.access_token,
                token_type: token_response.token_type,
                expires_in: token_response.expires_in.unwrap_or(3600),
                refresh_token: token_response.refresh_token,
                scope: scope.unwrap_or_else(|| "openid profile email".to_string()),
                matrix_user_id: None,
                device_id: None,
            }))
        }
        _ => Err(ApiError::bad_request(format!(
            "Unsupported grant_type: {grant_type}. Supported: authorization_code, refresh_token"
        ))),
    }
}

/// OIDC Authorization handler
///
/// Note: This endpoint does NOT require authentication — it's the first step in OIDC login
pub(crate) async fn oidc_authorize(
    State(ctx): State<SsoContext>,
    query: crate::routes::extractors::MatrixQuery<OidcAuthorizeRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let oidc_service: &synapse_services::oidc_service::OidcService =
        ctx.oidc_service.as_ref().ok_or_else(|| ApiError::bad_request("OIDC is not enabled".to_string()))?;

    let OidcAuthorizeRequest {
        response_type,
        client_id,
        redirect_uri,
        scope,
        state: auth_state,
        nonce,
        code_challenge,
        code_challenge_method,
    } = query.0;

    // Validate response_type
    if response_type != "code" {
        return Err(ApiError::bad_request("Only 'code' response type is supported".to_string()));
    }

    // RFC 6749 §3.1.2 / OIDC Core §3.1.2.1: `redirect_uri` must belong to the
    // client. Without this check the endpoint is an open redirector.
    validate_authorize_redirect_uri(&ctx, &client_id, &redirect_uri).await?;
    validate_authorize_scope(scope.as_deref(), &oidc_service.get_config().scopes)?;

    // RFC 7636: a public client brings its own challenge and later its own
    // verifier. Only a confidential client (no challenge) may let the server
    // generate the pair — and even then the verifier never leaves the server.
    let client_challenge_method = validate_client_pkce(code_challenge.as_deref(), code_challenge_method.as_deref())?;
    let (code_verifier, code_challenge, code_challenge_method) =
        match (code_challenge.map(|c| c.trim().to_string()).filter(|c| !c.is_empty()), client_challenge_method) {
            (Some(challenge), Some(method)) => (String::new(), challenge, method),
            _ => {
                let (verifier, challenge) = OidcService::generate_pkce();
                (verifier, challenge, "S256".to_string())
            }
        };

    // Generate state and nonce
    let state_value: String = auth_state.unwrap_or_else(OidcService::generate_state);
    let nonce_value: String = nonce.unwrap_or_else(OidcService::generate_state);

    ctx.oidc_session_service
        .store(&state_value, &nonce_value, &code_verifier, &code_challenge, &code_challenge_method, &redirect_uri)
        .await?;

    // Generate authorization URL (with PKCE)
    let authorization_url: String = oidc_service
        .get_authorization_url(
            &state_value,
            &redirect_uri,
            Some(&code_challenge),
            Some(&code_challenge_method),
            Some(&nonce_value),
        )
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to generate authorization URL", e))?;

    tracing::info!("OIDC authorization redirect_uri: {}, using PKCE", redirect_uri);

    // Never return the verifier: doing so would let any caller impersonate the
    // client at the token endpoint and defeat PKCE entirely.
    Ok(Json(serde_json::json!({
        "authorization_url": authorization_url,
        "state": state_value,
        "nonce": nonce_value,
    })))
}

/// OIDC UserInfo — returns the user profile for an authenticated Matrix user
pub(crate) async fn oidc_userinfo(
    State(ctx): State<SsoContext>,
    auth_user: AuthenticatedUser,
) -> Result<Json<OidcUserInfoResponse>, ApiError> {
    let user_id = &auth_user.user_id;

    // Fetch user profile
    let profile_res: Result<serde_json::Value, ApiError> = ctx.registration_service.get_profile(user_id).await;
    let profile_val = profile_res?;
    let profile = profile_val.as_object().ok_or_else(|| ApiError::internal("Profile is not an object"))?;

    let name: Option<String> = profile.get("displayname").and_then(|v| v.as_str()).map(String::from);

    let picture: Option<String> = profile.get("avatar_url").and_then(|v| v.as_str()).map(|s| {
        if let Ok(loc) = synapse_common::MediaLocator::parse(s) {
            loc.to_mxc_url()
        } else if s.starts_with("mxc://") {
            s.to_string()
        } else {
            format!("mxc://{s}")
        }
    });

    let email: Option<String> = profile.get("email").and_then(|v| v.as_str()).map(String::from);

    Ok(Json(OidcUserInfoResponse { sub: user_id.clone(), name, picture, email }))
}

/// OIDC Logout — revokes the named device and/or refresh token
///
/// Semantics (see [`OidcLogoutRequest`]): the endpoint acts on `device_id` and/or
/// `refresh_token`; with neither field present it answers 400 `M_MISSING_PARAM`
/// instead of a success that did nothing. Both revocations are scoped to the
/// authenticated user, so a token/device belonging to somebody else is 403.
/// Revoking an already-dead refresh token is an idempotent success, mirroring
/// `POST /_matrix/client/v3/logout`.
pub(crate) async fn oidc_logout(
    State(ctx): State<SsoContext>,
    auth_user: AuthenticatedUser,
    MatrixJson(body): MatrixJson<OidcLogoutRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let OidcLogoutRequest { refresh_token, device_id, client_id, id_token_hint, post_logout_redirect_uri, state } =
        body;

    if refresh_token.is_none() && device_id.is_none() {
        return Err(ApiError::missing_param(
            "oidc logout requires `refresh_token` or `device_id`; the RP-Initiated Logout \
             fields (`client_id` / `id_token_hint` / `post_logout_redirect_uri` / `state`) \
             alone do not identify a session on this server",
        ));
    }

    let mut device_revoked = false;
    if let Some(device_id) = device_id {
        // Owned-device revocation: deletes the device, its access tokens, its
        // refresh tokens and its device keys, and writes a security audit entry.
        device_revoked = ctx.token_auth.revoke_device(&auth_user.user_id, &device_id).await? > 0;
    }

    let mut refresh_token_revoked = false;
    if let Some(refresh_token) = refresh_token {
        match ctx.refresh_token_service.validate_token(&refresh_token).await {
            Ok(record) => {
                if record.user_id != auth_user.user_id {
                    return Err(ApiError::forbidden("refresh_token does not belong to the authenticated user"));
                }
                ctx.refresh_token_service.revoke_token(&refresh_token, "OIDC logout").await?;
                refresh_token_revoked = true;
            }
            // Already expired / revoked / blacklisted: logout is idempotent.
            Err(err) if err.is_unauthorized() => {}
            Err(err) => return Err(err),
        }
    }

    tracing::info!(
        user_id = %auth_user.user_id,
        device_revoked = device_revoked,
        refresh_token_revoked = refresh_token_revoked,
        // Accepted for RP-Initiated Logout wire compatibility; recorded so the
        // fields are visibly consumed rather than silently dropped.
        client_id = ?client_id,
        has_id_token_hint = id_token_hint.is_some(),
        post_logout_redirect_uri = ?post_logout_redirect_uri,
        state = ?state,
        "OIDC logout"
    );

    Ok(Json(serde_json::json!({
        "success": true,
        "device_revoked": device_revoked,
        "refresh_token_revoked": refresh_token_revoked
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_oidc_userinfo_response() {
        let response = OidcUserInfoResponse {
            sub: "@test:example.com".to_string(),
            name: Some("Test User".to_string()),
            picture: Some("mxc://example.com/avatar".to_string()),
            email: None,
        };

        assert_eq!(response.sub, "@test:example.com");
        assert_eq!(response.name, Some("Test User".to_string()));
    }

    #[test]
    fn client_pkce_accepts_s256_challenge() {
        let challenge = "a".repeat(43);
        assert_eq!(validate_client_pkce(Some(&challenge), Some("S256")).unwrap(), Some("S256".to_string()));
    }

    #[test]
    fn client_pkce_absent_challenge_means_confidential_client() {
        assert_eq!(validate_client_pkce(None, None).unwrap(), None);
        assert_eq!(validate_client_pkce(Some("   "), Some("S256")).unwrap(), None);
    }

    #[test]
    fn client_pkce_rejects_plain_and_malformed_challenges() {
        let challenge = "a".repeat(43);
        // RFC 7636 §4.3: an absent method defaults to `plain`, which is not advertised.
        assert!(validate_client_pkce(Some(&challenge), None).is_err());
        assert!(validate_client_pkce(Some(&challenge), Some("plain")).is_err());
        assert!(validate_client_pkce(Some("short"), Some("S256")).is_err());
        assert!(validate_client_pkce(Some(&"a".repeat(129)), Some("S256")).is_err());
        assert!(validate_client_pkce(Some(&format!("{}!", "a".repeat(42))), Some("S256")).is_err());
    }

    #[test]
    fn authorize_scope_must_be_subset_of_configured_scopes() {
        let configured = vec!["openid".to_string(), "profile".to_string(), "email".to_string()];
        assert!(validate_authorize_scope(None, &configured).is_ok());
        assert!(validate_authorize_scope(Some("openid profile"), &configured).is_ok());
        assert!(validate_authorize_scope(Some("openid admin"), &configured).is_err());
        // No configured scopes ⇒ nothing to constrain against.
        assert!(validate_authorize_scope(Some("anything"), &[]).is_ok());
    }
}
