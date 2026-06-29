//! MSC2965 — Auth Discovery handlers
//!
//! Provides endpoints for clients (e.g. Element) to discover whether the
//! homeserver supports OAuth2/OIDC-based login. When OIDC is not configured,
//! returns `M_UNRECOGNIZED` (HTTP 400) so clients fall back to classic password login.

use crate::web::routes::context::AuthContext;
use crate::web::routes::ApiError;
use axum::{extract::State, Json};
use serde_json::json;

/// Check whether OIDC/SAML is enabled via configuration.
fn oidc_available(config: &synapse_common::config::Config) -> bool {
    config.oidc.is_enabled() || config.builtin_oidc.is_enabled() || config.saml.enabled
}

/// Build an OpenID Connect discovery response from the server config.
fn build_oidc_discovery(config: &synapse_common::config::Config) -> serde_json::Value {
    let issuer = config.server.get_public_baseurl();
    json!({
        "issuer": issuer,
        "authorization_endpoint": format!("{issuer}/_matrix/client/v3/oidc/authorize"),
        "token_endpoint": format!("{issuer}/_matrix/client/v3/oidc/token"),
        "userinfo_endpoint": format!("{issuer}/_matrix/client/v3/oidc/userinfo"),
        "jwks_uri": format!("{issuer}/.well-known/jwks.json"),
        "registration_endpoint": config.oidc.registration_endpoint,
        "revocation_endpoint": format!("{issuer}/_matrix/client/v3/oidc/revoke"),
        "end_session_endpoint": format!("{issuer}/_matrix/client/v3/oidc/logout"),
        "scopes_supported": ["openid", "profile", "email"],
        "response_types_supported": ["code"],
        "response_modes_supported": ["query", "fragment"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "token_endpoint_auth_methods_supported": ["client_secret_basic"],
        "claims_supported": ["sub", "name", "picture", "email"],
        "ui_locales_supported": ["en"],
        "subject_types_supported": ["public"],
        "id_token_signing_alg_values_supported": ["RS256"],
        "code_challenge_methods_supported": ["S256"]
    })
}

/// MSC2965 — `auth_metadata`. Used by clients (e.g. Element) to discover whether
/// the homeserver supports OAuth2/OIDC-based login. When OIDC is not configured
/// we must return `M_UNRECOGNIZED` (HTTP 400) rather than a generic 404, so clients
/// fall back to the classic password login flow without surfacing a misleading error.
pub async fn get_auth_metadata(State(ctx): State<AuthContext>) -> Result<Json<serde_json::Value>, ApiError> {
    if !oidc_available(&ctx.config) {
        return Err(ApiError::unrecognized("Authentication metadata is not available because OIDC/SSO is not enabled"));
    }

    Ok(Json(build_oidc_discovery(&ctx.config)))
}

/// Legacy MSC2965 issuer discovery used by some Element code paths before
/// fetching the full auth metadata document.
pub async fn get_auth_issuer(State(ctx): State<AuthContext>) -> Result<Json<serde_json::Value>, ApiError> {
    if !oidc_available(&ctx.config) {
        return Err(ApiError::unrecognized("Authentication issuer is not available because OIDC/SSO is not enabled"));
    }

    Ok(Json(json!({
        "issuer": ctx.config.server.get_public_baseurl()
    })))
}
