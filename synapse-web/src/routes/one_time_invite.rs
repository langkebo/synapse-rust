//! One-time invite links (B-9) — vendor extension routes.
//!
//! Both endpoints live under `/_matrix/vendor/v1` because they are a
//! deployment-specific extension, not Matrix C-S API. Claiming is POST-only on
//! purpose: a GET claim would be triggered by link previews and unfurlers,
//! silently burning the token before the invitee ever clicks it.

use crate::routes::context::OneTimeInviteContext;
use crate::routes::{ApiError, AuthenticatedUser, MatrixJson};
use axum::{extract::State, Json};
use serde_json::Value;
use synapse_services::one_time_invite_service::{ClaimOneTimeInviteRequest, CreateOneTimeInviteRequest};

/// Mint a single-use invite token for the authenticated user.
/// POST /_matrix/vendor/v1/invites/one-time
pub async fn create_one_time_invite(
    State(ctx): State<OneTimeInviteContext>,
    auth_user: AuthenticatedUser,
    body: Option<MatrixJson<CreateOneTimeInviteRequest>>,
) -> Result<Json<Value>, ApiError> {
    let request = body.map(|MatrixJson(body)| body).unwrap_or_default();
    Ok(Json(ctx.one_time_invite_service.create(&auth_user.user_id, request).await?))
}

/// Redeem a single-use invite token, creating the deferred room.
/// POST /_matrix/vendor/v1/invites/claim
pub async fn claim_one_time_invite(
    State(ctx): State<OneTimeInviteContext>,
    auth_user: AuthenticatedUser,
    MatrixJson(body): MatrixJson<ClaimOneTimeInviteRequest>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(ctx.one_time_invite_service.claim(&auth_user.user_id, &body.token).await?))
}
