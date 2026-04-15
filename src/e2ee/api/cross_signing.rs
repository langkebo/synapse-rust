use super::super::cross_signing::{
    BulkSignatureUpload, CrossSigningService, CrossSigningSetupRequest, CrossSigningUpload,
    SignatureVerificationRequest,
};
use crate::error::ApiError;
use crate::web::routes::extractors::auth::AuthenticatedUser;
use axum::{
    extract::{Path, State},
    Json,
};
use std::sync::Arc;

#[deprecated(
    note = "Unauthenticated handler - do not register as route. Use e2ee_routes.rs handlers instead."
)]
pub async fn upload_cross_signing_keys(
    _auth_user: AuthenticatedUser,
    State(service): State<Arc<CrossSigningService>>,
    Json(upload): Json<CrossSigningUpload>,
) -> Result<Json<()>, ApiError> {
    service.upload_cross_signing_keys(upload).await?;
    Ok(Json(()))
}

#[deprecated(
    note = "Unauthenticated handler - do not register as route. Use e2ee_routes.rs handlers instead."
)]
pub async fn get_cross_signing_keys(
    auth_user: AuthenticatedUser,
    State(service): State<Arc<CrossSigningService>>,
    Path(user_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if auth_user.user_id != user_id {
        return Err(ApiError::forbidden(
            "Cannot access cross-signing keys for another user",
        ));
    }
    let keys = service.get_cross_signing_keys(&user_id).await?;
    Ok(Json(serde_json::json!({
        "user_id": user_id,
        "master_key": keys.master_key,
        "self_signing_key": keys.self_signing_key,
        "user_signing_key": keys.user_signing_key,
    })))
}

#[deprecated(
    note = "Unauthenticated handler - do not register as route. Use e2ee_routes.rs handlers instead."
)]
pub async fn upload_signatures(
    auth_user: AuthenticatedUser,
    State(service): State<Arc<CrossSigningService>>,
    Path(user_id): Path<String>,
    Json(signatures): Json<BulkSignatureUpload>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if auth_user.user_id != user_id {
        return Err(ApiError::forbidden(
            "Cannot upload signatures for another user",
        ));
    }
    let response = service.upload_signatures(&user_id, &signatures).await?;
    Ok(Json(serde_json::to_value(response)?))
}

#[deprecated(
    note = "Unauthenticated handler - do not register as route. Use e2ee_routes.rs handlers instead."
)]
pub async fn get_user_signatures(
    auth_user: AuthenticatedUser,
    State(service): State<Arc<CrossSigningService>>,
    Path(user_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if auth_user.user_id != user_id {
        return Err(ApiError::forbidden(
            "Cannot access signatures for another user",
        ));
    }
    let signatures = service.get_user_signatures(&user_id).await?;
    Ok(Json(serde_json::to_value(signatures)?))
}

#[deprecated(
    note = "Unauthenticated handler - do not register as route. Use e2ee_routes.rs handlers instead."
)]
pub async fn verify_signature(
    _auth_user: AuthenticatedUser,
    State(service): State<Arc<CrossSigningService>>,
    Json(request): Json<SignatureVerificationRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let response = service.verify_signature(&request).await?;
    Ok(Json(serde_json::to_value(response)?))
}

#[deprecated(
    note = "Unauthenticated handler - do not register as route. Use e2ee_routes.rs handlers instead."
)]
pub async fn setup_cross_signing(
    auth_user: AuthenticatedUser,
    State(service): State<Arc<CrossSigningService>>,
    Path(user_id): Path<String>,
    Json(request): Json<CrossSigningSetupRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if auth_user.user_id != user_id {
        return Err(ApiError::forbidden(
            "Cannot set up cross-signing for another user",
        ));
    }
    let response = service.setup_cross_signing(&user_id, &request).await?;
    Ok(Json(serde_json::to_value(response)?))
}

#[deprecated(
    note = "Unauthenticated handler - do not register as route. Use e2ee_routes.rs handlers instead."
)]
pub async fn get_device_signatures(
    auth_user: AuthenticatedUser,
    State(service): State<Arc<CrossSigningService>>,
    Path((user_id, device_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if auth_user.user_id != user_id {
        return Err(ApiError::forbidden(
            "Cannot access device signatures for another user",
        ));
    }
    let signatures = service.get_device_signatures(&user_id, &device_id).await?;
    Ok(Json(serde_json::json!({
        "user_id": user_id,
        "device_id": device_id,
        "signatures": signatures,
    })))
}

#[deprecated(
    note = "Unauthenticated handler - do not register as route. Use e2ee_routes.rs handlers instead."
)]
pub async fn delete_cross_signing_keys(
    auth_user: AuthenticatedUser,
    State(service): State<Arc<CrossSigningService>>,
    Path(user_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if auth_user.user_id != user_id {
        return Err(ApiError::forbidden(
            "Cannot delete cross-signing keys for another user",
        ));
    }
    service.delete_cross_signing_keys(&user_id).await?;
    Ok(Json(serde_json::json!({
        "deleted": true,
        "user_id": user_id,
    })))
}

#[deprecated(
    note = "Unauthenticated handler - do not register as route. Use e2ee_routes.rs handlers instead."
)]
pub async fn sign_device(
    auth_user: AuthenticatedUser,
    State(service): State<Arc<CrossSigningService>>,
    Path((user_id, device_id)): Path<(String, String)>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if auth_user.user_id != user_id {
        return Err(ApiError::forbidden("Cannot sign device for another user"));
    }
    let signing_key_id = body["signing_key_id"].as_str().unwrap_or("");
    let signature = body["signature"].as_str().unwrap_or("");

    service
        .sign_device(&user_id, &device_id, signing_key_id, signature)
        .await?;

    Ok(Json(serde_json::json!({
        "signed": true,
        "user_id": user_id,
        "device_id": device_id,
    })))
}

#[deprecated(
    note = "Unauthenticated handler - do not register as route. Use e2ee_routes.rs handlers instead."
)]
pub async fn sign_user(
    auth_user: AuthenticatedUser,
    State(service): State<Arc<CrossSigningService>>,
    Path((user_id, target_user_id)): Path<(String, String)>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if auth_user.user_id != user_id {
        return Err(ApiError::forbidden("Cannot sign user as another user"));
    }
    let signing_key_id = body["signing_key_id"].as_str().unwrap_or("");
    let signature = body["signature"].as_str().unwrap_or("");

    service
        .sign_user(&user_id, &target_user_id, signing_key_id, signature)
        .await?;

    Ok(Json(serde_json::json!({
        "signed": true,
        "user_id": user_id,
        "target_user_id": target_user_id,
    })))
}
