// Device Trust API Endpoints
// E2EE Phase 1: Device trust and verification endpoints

use crate::e2ee::device_trust::{
    DeviceTrustListResponse, DeviceTrustService, SecuritySummaryResponse, VerificationMethod,
    VerificationRequestResponse, VerificationRespondResponse,
};
use crate::error::ApiError;
use axum::{
    extract::{Path, State},
    Json,
};
use serde::Deserialize;
use std::sync::Arc;

/// Request body for device verification
#[derive(Debug, Deserialize)]
pub struct RequestVerificationJson {
    pub new_device_id: String,
    pub method: Option<String>,
}

/// Request body for verification response
#[derive(Debug, Deserialize)]
pub struct RespondVerificationJson {
    pub request_token: String,
    pub approved: bool,
}

/// Query params for verification status
#[derive(Debug, Deserialize)]
pub struct VerificationStatusQuery {
    pub token: String,
}

/// Request verification for a new device
pub async fn request_verification(
    State(service): State<Arc<DeviceTrustService>>,
    Path(user_id): Path<String>,
    Json(body): Json<RequestVerificationJson>,
) -> Result<Json<VerificationRequestResponse>, ApiError> {
    let method = match body.method.as_deref() {
        Some("qr") => VerificationMethod::Qr,
        Some("emoji") => VerificationMethod::Emoji,
        _ => VerificationMethod::Sas,
    };

    let response = service
        .request_device_verification(&user_id, &body.new_device_id, method, None)
        .await?;

    Ok(Json(response))
}

/// Respond to a verification request
pub async fn respond_verification(
    State(service): State<Arc<DeviceTrustService>>,
    Path(user_id): Path<String>,
    Json(body): Json<RespondVerificationJson>,
) -> Result<Json<VerificationRespondResponse>, ApiError> {
    let response = service
        .respond_to_verification(&user_id, &body.request_token, body.approved)
        .await?;

    Ok(Json(response))
}

/// Get verification request status
pub async fn get_verification_status(
    State(service): State<Arc<DeviceTrustService>>,
    Path((user_id, token)): Path<(String, String)>,
) -> Result<Json<Option<VerificationRequestResponse>>, ApiError> {
    let response = service.get_verification_status(&user_id, &token).await?;

    Ok(Json(response))
}

/// Get all devices with trust status
pub async fn get_device_trust_list(
    State(service): State<Arc<DeviceTrustService>>,
    Path(user_id): Path<String>,
) -> Result<Json<DeviceTrustListResponse>, ApiError> {
    let devices = service.get_all_devices_with_trust(&user_id).await?;

    Ok(Json(DeviceTrustListResponse { devices }))
}

/// Get security summary for a user
pub async fn get_security_summary(
    State(service): State<Arc<DeviceTrustService>>,
    Path(user_id): Path<String>,
) -> Result<Json<SecuritySummaryResponse>, ApiError> {
    let response = service.get_security_summary(&user_id).await?;

    Ok(Json(response))
}

/// Check if device can access message history
pub async fn can_access_history(
    State(service): State<Arc<DeviceTrustService>>,
    Path((user_id, device_id)): Path<(String, String)>,
) -> Result<Json<bool>, ApiError> {
    let can_access = service.can_access_history(&user_id, &device_id).await?;

    Ok(Json(can_access))
}

/// Check if device can decrypt messages
pub async fn can_decrypt_messages(
    State(service): State<Arc<DeviceTrustService>>,
    Path((user_id, device_id)): Path<(String, String)>,
) -> Result<Json<bool>, ApiError> {
    let can_decrypt = service.can_decrypt_messages(&user_id, &device_id).await?;

    Ok(Json(can_decrypt))
}
