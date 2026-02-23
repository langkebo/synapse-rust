use super::super::device_keys::{
    DeviceKeyService, KeyClaimRequest, KeyClaimResponse, KeyQueryRequest, KeyQueryResponse,
    KeyUploadRequest, KeyUploadResponse,
};
use crate::error::ApiError;
use axum::{
    extract::{Path, State},
    Json,
};
use std::sync::Arc;

pub async fn query_keys(
    State(service): State<Arc<DeviceKeyService>>,
    Json(request): Json<KeyQueryRequest>,
) -> Result<Json<KeyQueryResponse>, ApiError> {
    let response = service.query_keys(request).await?;
    Ok(Json(response))
}

pub async fn upload_keys(
    State(service): State<Arc<DeviceKeyService>>,
    Json(request): Json<KeyUploadRequest>,
) -> Result<Json<KeyUploadResponse>, ApiError> {
    let response = service.upload_keys(request).await?;
    Ok(Json(response))
}

pub async fn claim_keys(
    State(service): State<Arc<DeviceKeyService>>,
    Json(request): Json<KeyClaimRequest>,
) -> Result<Json<KeyClaimResponse>, ApiError> {
    let response = service.claim_keys(request).await?;
    Ok(Json(response))
}

pub async fn delete_keys(
    State(service): State<Arc<DeviceKeyService>>,
    Path((user_id, device_id)): Path<(String, String)>,
) -> Result<Json<()>, ApiError> {
    service.delete_keys(&user_id, &device_id).await?;
    Ok(Json(()))
}
