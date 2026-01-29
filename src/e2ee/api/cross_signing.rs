use super::super::cross_signing::{CrossSigningService, CrossSigningUpload};
use crate::error::ApiError;
use axum::{
    extract::{Path, State},
    Json,
};
use std::sync::Arc;

pub async fn upload_cross_signing_keys(
    State(service): State<Arc<CrossSigningService>>,
    Json(upload): Json<CrossSigningUpload>,
) -> Result<Json<()>, ApiError> {
    service.upload_cross_signing_keys(upload).await?;
    Ok(Json(()))
}

pub async fn get_cross_signing_keys(
    State(service): State<Arc<CrossSigningService>>,
    Path(user_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let keys = service.get_cross_signing_keys(&user_id).await?;
    Ok(Json(serde_json::json!({
        "user_id": user_id,
        "master_key": keys.master_key,
        "self_signing_key": keys.self_signing_key,
        "user_signing_key": keys.user_signing_key,
    })))
}
