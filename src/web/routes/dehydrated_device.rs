use crate::common::ApiError;
use crate::storage::dehydrated_device::{
    DehydratedDevice, DehydratedDeviceClaimRequest, DehydratedDeviceClaimResponse,
    DehydratedDeviceEvent,
};
use crate::web::routes::{AppState, AuthenticatedUser, MatrixJson};
use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Deserialize)]
struct PutDehydratedDeviceRequest {
    device_data: Value,
    algorithm: String,
    #[serde(default)]
    account: Option<Value>,
    #[serde(default)]
    expires_in_ms: Option<i64>,
}

#[derive(Serialize)]
struct DehydratedDeviceResponse {
    device_id: String,
    device_data: Value,
    algorithm: String,
    account: Option<Value>,
    created_ts: i64,
    updated_ts: i64,
    expires_at: Option<i64>,
}

impl From<DehydratedDevice> for DehydratedDeviceResponse {
    fn from(device: DehydratedDevice) -> Self {
        Self {
            device_id: device.device_id,
            device_data: device.device_data,
            algorithm: device.algorithm,
            account: device.account,
            created_ts: device.created_ts,
            updated_ts: device.updated_ts,
            expires_at: device.expires_at,
        }
    }
}

pub fn create_dehydrated_device_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device",
            get(list_dehydrated_devices),
        )
        .route(
            "/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device/claim",
            post(claim_dehydrated_device),
        )
        .route(
            "/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device/{device_id}",
            get(get_dehydrated_device)
                .put(put_dehydrated_device)
                .delete(delete_dehydrated_device),
        )
        .route(
            "/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device/{device_id}/event",
            get(get_dehydrated_device_event),
        )
        .with_state(state)
}

async fn list_dehydrated_devices(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let devices = state
        .services
        .dehydrated_device_service
        .get_devices_for_user(&auth_user.user_id)
        .await?;

    let response: Vec<DehydratedDeviceResponse> = devices
        .into_iter()
        .map(DehydratedDeviceResponse::from)
        .collect();

    Ok(Json(json!({
        "devices": response
    })))
}

async fn put_dehydrated_device(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(device_id): Path<String>,
    MatrixJson(body): MatrixJson<PutDehydratedDeviceRequest>,
) -> Result<Json<Value>, ApiError> {
    let device = state
        .services
        .dehydrated_device_service
        .create_device(
            &auth_user.user_id,
            &device_id,
            body.device_data,
            &body.algorithm,
            body.account,
            body.expires_in_ms,
        )
        .await?;

    Ok(Json(json!({
        "device_id": device.device_id,
        "created_ts": device.created_ts,
        "updated_ts": device.updated_ts,
        "expires_at": device.expires_at
    })))
}

async fn get_dehydrated_device(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(device_id): Path<String>,
) -> Result<Json<DehydratedDeviceResponse>, ApiError> {
    let device = state
        .services
        .dehydrated_device_service
        .get_device(&auth_user.user_id, &device_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Dehydrated device not found".to_string()))?;

    Ok(Json(DehydratedDeviceResponse::from(device)))
}

async fn get_dehydrated_device_event(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(device_id): Path<String>,
) -> Result<Json<DehydratedDeviceEvent>, ApiError> {
    let event = state
        .services
        .dehydrated_device_service
        .get_device_event(&auth_user.user_id, &device_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Dehydrated device not found".to_string()))?;

    Ok(Json(event))
}

async fn delete_dehydrated_device(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(device_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let deleted = state
        .services
        .dehydrated_device_service
        .delete_device(&auth_user.user_id, &device_id)
        .await?;

    if !deleted {
        return Err(ApiError::not_found(
            "Dehydrated device not found".to_string(),
        ));
    }

    Ok(Json(json!({})))
}

async fn claim_dehydrated_device(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    MatrixJson(body): MatrixJson<DehydratedDeviceClaimRequest>,
) -> Result<Json<DehydratedDeviceClaimResponse>, ApiError> {
    if body.key.algorithm.trim().is_empty()
        || body.key.key_id.trim().is_empty()
        || body.key.key.trim().is_empty()
    {
        return Err(ApiError::bad_request(
            "claim key fields must not be empty".to_string(),
        ));
    }

    let device = state
        .services
        .dehydrated_device_service
        .claim_device(&auth_user.user_id, &body.device_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Dehydrated device not found".to_string()))?;

    Ok(Json(DehydratedDeviceClaimResponse {
        device_id: device.device_id,
        device_data: device.device_data,
        account: device.account,
    }))
}
