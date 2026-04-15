use crate::e2ee::device_trust::{
    DeviceTrustListResponse, DeviceTrustService, SecuritySummaryResponse, VerificationMethod,
    VerificationRequestResponse, VerificationRespondResponse,
};
use crate::error::ApiError;
use crate::web::routes::extractors::auth::AuthenticatedUser;
use axum::{
    extract::{Path, State},
    Json,
};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct RequestVerificationJson {
    pub new_device_id: String,
    pub method: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RespondVerificationJson {
    pub request_token: String,
    pub approved: bool,
}

#[derive(Debug, Deserialize)]
pub struct VerificationStatusQuery {
    pub token: String,
}

#[deprecated(
    note = "Unauthenticated handler - do not register as route. Use e2ee_routes.rs handlers instead."
)]
pub async fn request_verification(
    auth_user: AuthenticatedUser,
    State(service): State<Arc<DeviceTrustService>>,
    Path(user_id): Path<String>,
    Json(body): Json<RequestVerificationJson>,
) -> Result<Json<VerificationRequestResponse>, ApiError> {
    if auth_user.user_id != user_id {
        return Err(ApiError::forbidden(
            "Cannot request verification for another user",
        ));
    }

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

#[deprecated(
    note = "Unauthenticated handler - do not register as route. Use e2ee_routes.rs handlers instead."
)]
pub async fn respond_verification(
    auth_user: AuthenticatedUser,
    State(service): State<Arc<DeviceTrustService>>,
    Path(user_id): Path<String>,
    Json(body): Json<RespondVerificationJson>,
) -> Result<Json<VerificationRespondResponse>, ApiError> {
    if auth_user.user_id != user_id {
        return Err(ApiError::forbidden(
            "Cannot respond to verification for another user",
        ));
    }

    let response = service
        .respond_to_verification(&user_id, &body.request_token, body.approved)
        .await?;

    Ok(Json(response))
}

#[deprecated(
    note = "Unauthenticated handler - do not register as route. Use e2ee_routes.rs handlers instead."
)]
pub async fn get_verification_status(
    auth_user: AuthenticatedUser,
    State(service): State<Arc<DeviceTrustService>>,
    Path((user_id, token)): Path<(String, String)>,
) -> Result<Json<Option<VerificationRequestResponse>>, ApiError> {
    if auth_user.user_id != user_id {
        return Err(ApiError::forbidden(
            "Cannot access verification status for another user",
        ));
    }

    let response = service.get_verification_status(&user_id, &token).await?;

    Ok(Json(response))
}

#[deprecated(
    note = "Unauthenticated handler - do not register as route. Use e2ee_routes.rs handlers instead."
)]
pub async fn get_device_trust_list(
    auth_user: AuthenticatedUser,
    State(service): State<Arc<DeviceTrustService>>,
    Path(user_id): Path<String>,
) -> Result<Json<DeviceTrustListResponse>, ApiError> {
    if auth_user.user_id != user_id {
        return Err(ApiError::forbidden(
            "Cannot access device trust list for another user",
        ));
    }

    let devices = service.get_all_devices_with_trust(&user_id).await?;

    Ok(Json(DeviceTrustListResponse { devices }))
}

#[deprecated(
    note = "Unauthenticated handler - do not register as route. Use e2ee_routes.rs handlers instead."
)]
pub async fn get_security_summary(
    auth_user: AuthenticatedUser,
    State(service): State<Arc<DeviceTrustService>>,
    Path(user_id): Path<String>,
) -> Result<Json<SecuritySummaryResponse>, ApiError> {
    if auth_user.user_id != user_id {
        return Err(ApiError::forbidden(
            "Cannot access security summary for another user",
        ));
    }

    let response = service.get_security_summary(&user_id).await?;

    Ok(Json(response))
}

#[deprecated(
    note = "Unauthenticated handler - do not register as route. Use e2ee_routes.rs handlers instead."
)]
pub async fn can_access_history(
    auth_user: AuthenticatedUser,
    State(service): State<Arc<DeviceTrustService>>,
    Path((user_id, device_id)): Path<(String, String)>,
) -> Result<Json<bool>, ApiError> {
    if auth_user.user_id != user_id {
        return Err(ApiError::forbidden(
            "Cannot check history access for another user",
        ));
    }

    let can_access = service.can_access_history(&user_id, &device_id).await?;

    Ok(Json(can_access))
}

#[deprecated(
    note = "Unauthenticated handler - do not register as route. Use e2ee_routes.rs handlers instead."
)]
pub async fn can_decrypt_messages(
    auth_user: AuthenticatedUser,
    State(service): State<Arc<DeviceTrustService>>,
    Path((user_id, device_id)): Path<(String, String)>,
) -> Result<Json<bool>, ApiError> {
    if auth_user.user_id != user_id {
        return Err(ApiError::forbidden(
            "Cannot check decrypt capability for another user",
        ));
    }

    let can_decrypt = service.can_decrypt_messages(&user_id, &device_id).await?;

    Ok(Json(can_decrypt))
}
