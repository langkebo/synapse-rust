// Built-in OIDC Provider endpoints: login, JWKS, and OpenID Discovery.

use crate::common::error::ApiError;
use crate::web::routes::context::SsoContext;
use axum::{extract::State, Json};
use serde::Serialize;

/// OpenID Connect Discovery document
#[derive(Debug, Serialize)]
pub(crate) struct OpenIdDiscovery {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub userinfo_endpoint: String,
    pub jwks_uri: String,
    pub registration_endpoint: Option<String>,
    pub revocation_endpoint: Option<String>,
    pub end_session_endpoint: Option<String>,
    pub scopes_supported: Vec<String>,
    pub response_types_supported: Vec<String>,
    pub response_modes_supported: Vec<String>,
    pub grant_types_supported: Vec<String>,
    pub token_endpoint_auth_methods_supported: Vec<String>,
    pub claims_supported: Vec<String>,
    pub ui_locales_supported: Vec<String>,
    pub subject_types_supported: Vec<String>,
    pub id_token_signing_alg_values_supported: Vec<String>,
    pub code_challenge_methods_supported: Vec<String>,
}

#[cfg(feature = "builtin-oidc")]
pub(crate) async fn builtin_oidc_login(
    State(ctx): State<SsoContext>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    use synapse_services::builtin_oidc_provider::AuthorizeRequest;

    let provider = ctx
        .builtin_oidc_provider
        .as_ref()
        .ok_or_else(|| ApiError::bad_request("Builtin OIDC provider is not enabled".to_string()))?;

    let client_id = body.get("client_id").and_then(|v| v.as_str()).unwrap_or_default();
    if client_id.is_empty() {
        return Err(ApiError::bad_request("Missing or empty 'client_id' parameter".to_string()));
    }
    let redirect_uri = body.get("redirect_uri").and_then(|v| v.as_str()).unwrap_or_default();
    if redirect_uri.is_empty() {
        return Err(ApiError::bad_request("Missing or empty 'redirect_uri' parameter".to_string()));
    }
    let scope = body.get("scope").and_then(|v| v.as_str()).unwrap_or("openid");
    let state_str = body.get("state").and_then(|v| v.as_str()).unwrap_or_default();
    let nonce = body.get("nonce").and_then(|v| v.as_str());
    let code_verifier = body.get("code_verifier").and_then(|v| v.as_str());
    let username = body.get("username").and_then(|v| v.as_str()).unwrap_or_default();
    let password = body.get("password").and_then(|v| v.as_str()).unwrap_or_default();

    let code = provider
        .authorize(AuthorizeRequest {
            client_id: client_id.to_string(),
            redirect_uri: redirect_uri.to_string(),
            scope: scope.to_string(),
            state: state_str.to_string(),
            nonce: nonce.map(String::from),
            code_verifier: code_verifier.map(String::from),
            username: username.to_string(),
            password: password.to_string(),
        })
        .await
        .map_err(|e| ApiError::unauthorized(format!("Authorization failed: {}", e)))?;

    Ok(Json(serde_json::json!({ "code": code })))
}

#[cfg(feature = "builtin-oidc")]
pub(crate) async fn jwks(State(ctx): State<SsoContext>) -> Result<Json<serde_json::Value>, ApiError> {
    if let Some(provider) = &ctx.builtin_oidc_provider {
        let jwks = provider.get_jwks();
        return Ok(Json(
            serde_json::to_value(jwks).map_err(|e| ApiError::internal_with_log("Failed to serialize JWKS", &e))?,
        ));
    }
    Err(ApiError::bad_request("Builtin OIDC provider is not enabled".to_string()))
}

/// JWKS fallback endpoint — returns an empty JWKS set when OIDC is not enabled
pub(crate) async fn jwks_fallback(State(_ctx): State<SsoContext>) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(serde_json::json!({
        "keys": []
    })))
}

/// OpenID Connect Server Discovery
#[allow(clippy::unused_async)]
pub(crate) async fn openid_discovery(State(ctx): State<SsoContext>) -> Result<Json<OpenIdDiscovery>, ApiError> {
    // If built-in OIDC Provider is enabled, use its discovery document
    #[cfg(feature = "builtin-oidc")]
    if let Some(provider) = &ctx.builtin_oidc_provider {
        let doc = provider.get_discovery_document();
        return Ok(Json(OpenIdDiscovery {
            issuer: doc.issuer,
            authorization_endpoint: doc.authorization_endpoint,
            token_endpoint: doc.token_endpoint,
            userinfo_endpoint: doc.userinfo_endpoint,
            jwks_uri: doc.jwks_uri,
            registration_endpoint: doc.registration_endpoint,
            revocation_endpoint: doc.revocation_endpoint,
            end_session_endpoint: doc.end_session_endpoint,
            scopes_supported: doc.scopes_supported,
            response_types_supported: doc.response_types_supported,
            response_modes_supported: vec!["query".to_string(), "fragment".to_string()],
            grant_types_supported: vec!["authorization_code".to_string(), "refresh_token".to_string()],
            token_endpoint_auth_methods_supported: doc.token_endpoint_auth_methods_supported,
            claims_supported: doc.claims_supported,
            ui_locales_supported: vec!["en".to_string()],
            subject_types_supported: doc.subject_types_supported,
            id_token_signing_alg_values_supported: doc.id_token_signing_alg_values_supported,
            code_challenge_methods_supported: doc.code_challenge_methods_supported,
        }));
    }

    let issuer = ctx.config.server.get_public_baseurl();
    let oidc_config = &ctx.config.oidc;

    Ok(Json(OpenIdDiscovery {
        issuer: issuer.clone(),
        authorization_endpoint: format!("{issuer}/_matrix/client/v3/oidc/authorize"),
        token_endpoint: format!("{issuer}/_matrix/client/v3/oidc/token"),
        userinfo_endpoint: format!("{issuer}/_matrix/client/v3/oidc/userinfo"),
        jwks_uri: format!("{issuer}/.well-known/jwks.json"),
        registration_endpoint: oidc_config.registration_endpoint.clone(),
        revocation_endpoint: Some(format!("{issuer}/_matrix/client/v3/oidc/revoke")),
        end_session_endpoint: Some(format!("{issuer}/_matrix/client/v3/oidc/logout")),
        scopes_supported: vec!["openid".to_string(), "profile".to_string(), "email".to_string()],
        response_types_supported: vec!["code".to_string()],
        response_modes_supported: vec!["query".to_string(), "fragment".to_string()],
        grant_types_supported: vec!["authorization_code".to_string(), "refresh_token".to_string()],
        token_endpoint_auth_methods_supported: vec!["client_secret_basic".to_string()],
        claims_supported: vec!["sub".to_string(), "name".to_string(), "picture".to_string(), "email".to_string()],
        ui_locales_supported: vec!["en".to_string()],
        subject_types_supported: vec!["public".to_string()],
        id_token_signing_alg_values_supported: vec!["RS256".to_string()],
        code_challenge_methods_supported: vec!["S256".to_string()],
    }))
}

/// Convenience alias — delegates to `openid_discovery`
pub(crate) async fn get_openid_configuration(State(ctx): State<SsoContext>) -> Result<Json<OpenIdDiscovery>, ApiError> {
    openid_discovery(State(ctx)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openid_discovery() {
        let discovery = OpenIdDiscovery {
            issuer: "https://example.com".to_string(),
            authorization_endpoint: "https://example.com/_matrix/client/v3/oidc/authorize".to_string(),
            token_endpoint: "https://example.com/_matrix/client/v3/oidc/token".to_string(),
            userinfo_endpoint: "https://example.com/_matrix/client/v3/oidc/userinfo".to_string(),
            jwks_uri: "https://example.com/.well-known/jwks.json".to_string(),
            registration_endpoint: None,
            revocation_endpoint: None,
            end_session_endpoint: None,
            scopes_supported: vec!["openid".to_string()],
            response_types_supported: vec!["code".to_string()],
            response_modes_supported: vec!["query".to_string()],
            grant_types_supported: vec!["authorization_code".to_string()],
            token_endpoint_auth_methods_supported: vec!["client_secret_basic".to_string()],
            claims_supported: vec!["sub".to_string()],
            ui_locales_supported: vec!["en".to_string()],
            subject_types_supported: vec!["public".to_string()],
            id_token_signing_alg_values_supported: vec!["RS256".to_string()],
            code_challenge_methods_supported: vec!["S256".to_string()],
        };

        assert_eq!(discovery.issuer, "https://example.com");
    }
}
