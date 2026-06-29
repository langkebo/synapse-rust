//! MSC2965 — Auth Discovery handlers
//!
//! Provides endpoints for clients (e.g. Element) to discover whether the
//! homeserver supports OAuth2/OIDC-based login. When OIDC is not configured,
//! returns `M_UNRECOGNIZED` (HTTP 400) so clients fall back to classic password login.

use crate::web::routes::oidc;
use crate::web::routes::ApiError;
use crate::web::AppState;
use axum::{extract::State, Json};
use serde_json::json;

/// MSC2965 — `auth_metadata`. Used by clients (e.g. Element) to discover whether
/// the homeserver supports OAuth2/OIDC-based login. When OIDC is not configured
/// we must return `M_UNRECOGNIZED` (HTTP 400) rather than a generic 404, so clients
/// fall back to the classic password login flow without surfacing a misleading error.
pub async fn get_auth_metadata(State(state): State<AppState>) -> Result<Json<serde_json::Value>, ApiError> {
    if !oidc::oidc_enabled(&state) {
        return Err(ApiError::unrecognized("Authentication metadata is not available because OIDC/SSO is not enabled"));
    }

    let discovery = oidc::openid_discovery(State(state)).await?;
    Ok(Json(serde_json::to_value(discovery.0).map_err(|e| ApiError::internal(e.to_string()))?))
}

/// Legacy MSC2965 issuer discovery used by some Element code paths before
/// fetching the full auth metadata document.
pub async fn get_auth_issuer(State(state): State<AppState>) -> Result<Json<serde_json::Value>, ApiError> {
    if !oidc::oidc_enabled(&state) {
        return Err(ApiError::unrecognized("Authentication issuer is not available because OIDC/SSO is not enabled"));
    }

    let discovery = oidc::openid_discovery(State(state)).await?;
    Ok(Json(json!({
        "issuer": discovery.0.issuer
    })))
}
