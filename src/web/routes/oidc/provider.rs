// OIDC Provider endpoints: authorize, token, userinfo, logout.

use crate::common::error::ApiError;
use crate::web::routes::context::SsoContext;
use crate::web::routes::AuthenticatedUser;
use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use synapse_services::oidc_service::OidcService;
use validator::Validate;

use super::{current_unix_ts, store_oidc_auth_session};

/// OIDC Token Request
#[derive(Debug, Deserialize, Validate)]
pub(crate) struct OidcTokenRequest {
    #[validate(length(min = 1, max = 100))]
    pub grant_type: String,
    #[validate(length(min = 1, max = 2048))]
    pub code: Option<String>,
    #[validate(length(max = 2048))]
    pub redirect_uri: Option<String>,
    #[validate(length(max = 2048))]
    pub refresh_token: Option<String>,
    #[validate(length(max = 1024))]
    pub scope: Option<String>,
    #[validate(length(max = 255))]
    pub client_id: Option<String>,
    #[validate(length(min = 43, max = 128))]
    pub code_verifier: Option<String>,
}

/// OIDC Token Response
#[derive(Debug, Serialize)]
pub(crate) struct OidcTokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub refresh_token: Option<String>,
    pub scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matrix_user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
}

/// OIDC UserInfo Response
#[derive(Debug, Serialize)]
pub(crate) struct OidcUserInfoResponse {
    pub sub: String,
    pub name: Option<String>,
    pub picture: Option<String>,
    pub email: Option<String>,
}

/// OIDC Logout Request
#[derive(Debug, Deserialize)]
pub(crate) struct OidcLogoutRequest {
    pub refresh_token: Option<String>,
    pub device_id: Option<String>,
}

/// OIDC Authorize Request
#[derive(Debug, Deserialize, Validate)]
pub(crate) struct OidcAuthorizeRequest {
    #[validate(length(min = 1, max = 50))]
    pub response_type: String,
    #[validate(length(min = 1, max = 255))]
    pub client_id: String,
    #[validate(length(min = 1, max = 2048))]
    pub redirect_uri: String,
    #[validate(length(max = 1024))]
    pub scope: Option<String>,
    #[validate(length(max = 512))]
    pub state: Option<String>,
    #[validate(length(max = 512))]
    pub nonce: Option<String>,
}

/// OIDC Token endpoint — handles authorization code exchange and refresh token
pub(crate) async fn oidc_token(
    State(ctx): State<SsoContext>,
    Json(body): Json<OidcTokenRequest>,
) -> Result<Json<OidcTokenResponse>, ApiError> {
    // Validate input
    body.validate().map_err(|e| ApiError::bad_request(format!("Validation error: {e}")))?;

    // Prefer the built-in OIDC Provider when available
    #[cfg(feature = "builtin-oidc")]
    if let Some(builtin_provider) = &ctx.builtin_oidc_provider {
        use synapse_services::builtin_oidc_provider::OidcTokenRequest as BuiltinOidcTokenRequest;
        let request = BuiltinOidcTokenRequest {
            grant_type: body.grant_type.clone(),
            code: body.code.clone(),
            redirect_uri: body.redirect_uri.clone(),
            client_id: body.client_id.clone(),
            code_verifier: body.code_verifier.clone(),
            refresh_token: body.refresh_token.clone(),
            scope: body.scope.clone(),
        };

        let token_response = builtin_provider
            .token(request)
            .await
            .map_err(|e| ApiError::internal_with_log("Builtin OIDC token failed", &e))?;

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
                .exchange_code(&code, &redirect_uri, code_verifier.as_deref())
                .await
                .map_err(|e| ApiError::internal_with_log("Token exchange failed", &e))?;

            // Fetch user info
            let user_info: synapse_services::oidc_service::OidcUserInfo = oidc_service
                .get_user_info(&token_response.access_token)
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to get user info", &e))?;

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
            let bound_user_id: Option<String> = ctx
                .oidc_mapping_storage
                .get_bound_user_id(&issuer, &subject)
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to query OIDC user mapping", &e))?;

            let matrix_user_id: String = if let Some(existing) = bound_user_id {
                // Subsequent login: ignore IdP's current localpart, use the first binding
                ctx.oidc_mapping_storage
                    .update_last_authenticated(&issuer, &subject, now_ts)
                    .await
                    .map_err(|e| ApiError::internal_with_log("Failed to update OIDC user mapping", &e))?;
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
                    .map_err(|e| ApiError::internal_with_log("Failed to register OIDC user", &e))?;

                ctx.oidc_mapping_storage
                    .insert_mapping(&issuer, &subject, &matrix_user_id, now_ts)
                    .await
                    .map_err(|e| ApiError::internal_with_log("Failed to insert OIDC user mapping", &e))?;
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
                .map_err(|e| ApiError::internal_with_log("Token refresh failed", &e))?;

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
    query: axum::extract::Query<OidcAuthorizeRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let oidc_service: &synapse_services::oidc_service::OidcService =
        ctx.oidc_service.as_ref().ok_or_else(|| ApiError::bad_request("OIDC is not enabled".to_string()))?;

    let OidcAuthorizeRequest { response_type, client_id: _, redirect_uri, scope: _, state: auth_state, nonce } =
        query.0;

    // Validate response_type
    if response_type != "code" {
        return Err(ApiError::bad_request("Only 'code' response type is supported".to_string()));
    }

    // Generate state and nonce
    let state_value: String = auth_state.unwrap_or_else(OidcService::generate_state);
    let nonce_value: String = nonce.unwrap_or_else(OidcService::generate_state);

    // Generate PKCE code_verifier and code_challenge
    let (code_verifier, code_challenge): (String, String) = OidcService::generate_pkce();
    store_oidc_auth_session(&state_value, &nonce_value, &code_verifier, &code_challenge, "S256", &redirect_uri)?;

    // Generate authorization URL (with PKCE)
    let authorization_url: String = oidc_service
        .get_authorization_url(&state_value, &redirect_uri, Some(&code_challenge), Some("S256"))
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to generate authorization URL", &e))?;

    tracing::info!("OIDC authorization redirect_uri: {}, using PKCE", redirect_uri);

    Ok(Json(serde_json::json!({
        "authorization_url": authorization_url,
        "state": state_value,
        "nonce": nonce_value,
        "code_verifier": code_verifier,  // returned to client for later verification
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
        if let Ok(loc) = crate::common::MediaLocator::parse(s) {
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

/// OIDC Logout — deletes device and/or revokes refresh token
pub(crate) async fn oidc_logout(
    State(ctx): State<SsoContext>,
    auth_user: AuthenticatedUser,
    Json(body): Json<OidcLogoutRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // If a device ID is provided, delete that device
    if let Some(device_id) = body.device_id {
        ctx.account_device_list_service.delete_device(&device_id).await?;
    }

    // If a refresh token is provided, revoke it
    if let Some(refresh_token) = body.refresh_token {
        ctx.refresh_token_service.revoke_token(&refresh_token, "OIDC logout").await?;
    }

    tracing::info!("OIDC logout for user: {}", auth_user.user_id);

    Ok(Json(serde_json::json!({
        "success": true
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
}
