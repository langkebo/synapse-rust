//! MSC3814 — Dehydrated Device handlers
//!
//! Dehydrated devices allow clients to prepare a device that can be "rehydrated"
//! (restored) later. This is used by Element to maintain E2EE continuity when
//! the user logs in from a new device without having their old device available.

use crate::web::routes::context::RoomContext;
use crate::web::routes::ApiError;
use crate::web::routes::AuthenticatedUser;
use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde_json::json;

/// Return `true` when the user has at least one well-known SSSS account_data
/// event present: `m.secret_storage.default_key` (which names the default
/// key) and any `m.secret_storage.key.<id>` entry. Element/Synapse clients
/// consider SSSS "initialised" once either of those is set, regardless of
/// what the homeserver's internal SSSS table happens to contain.
async fn user_has_secret_storage_account_data(ctx: &RoomContext, user_id: &str) -> Result<bool, ApiError> {
    let account_data = ctx.account_data_service.list_account_data(user_id).await?;

    if account_data.contains_key("m.secret_storage.default_key") {
        return Ok(true);
    }

    Ok(account_data.keys().any(|key| key.starts_with("m.secret_storage.key.")))
}

/// MSC3814 — dehydrated devices. Element probes this on startup. We now expose
/// a minimal persisted implementation so clients can upload and resume the
/// dehydrated device state instead of seeing a permanent 404.
pub async fn get_dehydrated_device(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    let device = ctx.dehydrated_device_service.get_device(&auth_user.user_id).await?;

    match device {
        Some(device) => Ok(Json(device)),
        None => Err(ApiError::not_found("No dehydrated device available")),
    }
}

pub async fn put_dehydrated_device(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // P0: Precondition checks (MSC3814)
    // 1. Check if user has cross-signing keys
    let cs_status = ctx.cross_signing_service.get_user_verification_status(&auth_user.user_id).await?;
    if !cs_status.has_master_key {
        return Err(ApiError::forbidden(
            "Cross-signing keys not found. Please initialize cross-signing before creating a dehydrated device."
                .to_string(),
        ));
    }

    // 2. Check if user has SSSS keys
    //
    // Element actually consults the *standard* account_data event names
    // (`m.secret_storage.default_key` and the `m.secret_storage.key.<id>` keys)
    // to decide whether SSSS is set up — not the homeserver's internal table.
    // We mirror that, and also still accept the internal `ssss_service` rows so
    // server-initiated bootstrap (e.g. tests / admin tools) keeps working.
    let ssss_keys = ctx.ssss_service.get_all_keys(&auth_user.user_id).await?;
    if ssss_keys.is_empty() && !user_has_secret_storage_account_data(&ctx, &auth_user.user_id).await? {
        return Err(ApiError::forbidden(
            "Secret storage keys not found. Please initialize secret storage (SSSS) before creating a dehydrated device."
                .to_string(),
        ));
    }

    let device_id = ctx.dehydrated_device_service.put_device(&auth_user.user_id, body).await?;

    Ok(Json(json!({
        "device_id": device_id
    })))
}

pub async fn get_dehydrated_device_status(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    let status = ctx.dehydrated_device_service.get_status(&auth_user.user_id).await?;
    Ok(Json(status))
}

pub async fn delete_dehydrated_device(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    let device_id = ctx.dehydrated_device_service.delete_device(&auth_user.user_id).await?;
    match device_id {
        Some(device_id) => Ok(Json(json!({ "device_id": device_id }))),
        None => Err(ApiError::not_found("No dehydrated device available")),
    }
}

/// MSC3814 — claim pending to-device events addressed to a dehydrated device.
///
/// Clients call this after restoring a session backed by a dehydrated device,
/// to drain the queue of `m.room_key` (and other) to-device messages that were
/// delivered while the user was offline. We page by `stream_id` of the
/// underlying `to_device_messages` table, returning the cursor as a string in
/// `next_batch`. When the cursor stops advancing the queue is empty and the
/// client is expected to `DELETE` the dehydrated device.
pub async fn post_dehydrated_device_events(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(device_id): Path<String>,
    Query(query): Query<serde_json::Value>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let next_batch = body.get("next_batch").and_then(|v| v.as_str());
    let limit = query.get("limit").and_then(|v| v.as_i64()).unwrap_or(100);
    let response =
        ctx.dehydrated_device_service.claim_events(&auth_user.user_id, &device_id, next_batch, limit).await?;
    Ok(Json(response))
}
