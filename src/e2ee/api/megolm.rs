use super::super::megolm::MegolmService;
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
pub async fn enable_encryption(
    _auth_user: AuthenticatedUser,
    State(service): State<Arc<MegolmService>>,
    Path(room_id): Path<String>,
    Json(request): Json<serde_json::Value>,
) -> Result<Json<()>, ApiError> {
    let algorithm = request["algorithm"]
        .as_str()
        .ok_or_else(|| ApiError::bad_request("Missing or invalid 'algorithm' field".to_string()))?;
    let _algorithm = algorithm;
    let sender_key = request["sender_key"].as_str().ok_or_else(|| {
        ApiError::bad_request("Missing or invalid 'sender_key' field".to_string())
    })?;

    service.create_session(&room_id, sender_key).await?;

    Ok(Json(()))
}

#[deprecated(
    note = "Unauthenticated handler - do not register as route. Use e2ee_routes.rs handlers instead."
)]
pub async fn disable_encryption(
    _auth_user: AuthenticatedUser,
    State(service): State<Arc<MegolmService>>,
    Path(room_id): Path<String>,
) -> Result<Json<()>, ApiError> {

    let sessions = service.get_room_sessions(&room_id).await?;

    for session in sessions {
        service.delete_session(&session.session_id).await?;
    }

    Ok(Json(()))
}
