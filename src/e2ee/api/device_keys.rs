use super::super::device_keys::{
    DeviceKeyService, KeyClaimRequest, KeyClaimResponse, KeyQueryRequest, KeyQueryResponse,
    KeyUploadRequest, KeyUploadResponse,
};
use crate::error::ApiError;
use crate::web::routes::extractors::auth::AuthenticatedUser;
use axum::{
    extract::{Path, State},
    Json,
};
use std::sync::Arc;

#[deprecated(note = "Use e2ee_routes.rs authenticated handlers instead")]
pub async fn query_keys(
    _auth_user: AuthenticatedUser,
    State(service): State<Arc<DeviceKeyService>>,
    Json(request): Json<KeyQueryRequest>,
) -> Result<Json<KeyQueryResponse>, ApiError> {
    let response = service.query_keys(request).await?;
    Ok(Json(response))
}

#[deprecated(note = "Use e2ee_routes.rs authenticated handlers instead")]
pub async fn upload_keys(
    auth_user: AuthenticatedUser,
    State(service): State<Arc<DeviceKeyService>>,
    Json(request): Json<KeyUploadRequest>,
) -> Result<Json<KeyUploadResponse>, ApiError> {
    if let Some(ref device_keys) = request.device_keys {
        if device_keys.user_id != auth_user.user_id {
            return Err(ApiError::forbidden("Cannot upload keys for another user"));
        }
    }
    let response = service.upload_keys(request).await?;
    Ok(Json(response))
}

#[deprecated(note = "Use e2ee_routes.rs authenticated handlers instead")]
pub async fn claim_keys(
    _auth_user: AuthenticatedUser,
    State(service): State<Arc<DeviceKeyService>>,
    Json(request): Json<KeyClaimRequest>,
) -> Result<Json<KeyClaimResponse>, ApiError> {
    let response = service.claim_keys(request).await?;
    Ok(Json(response))
}

#[deprecated(note = "Use e2ee_routes.rs authenticated handlers instead")]
pub async fn delete_keys(
    auth_user: AuthenticatedUser,
    State(service): State<Arc<DeviceKeyService>>,
    Path((user_id, device_id)): Path<(String, String)>,
) -> Result<Json<()>, ApiError> {
    if auth_user.user_id != user_id {
        return Err(ApiError::forbidden("Cannot delete keys for another user"));
    }
    service.delete_keys(&user_id, &device_id).await?;
    Ok(Json(()))
}
