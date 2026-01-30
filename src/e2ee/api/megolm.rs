use super::super::megolm::MegolmService;
use crate::error::ApiError;
use axum::{
    extract::{Path, State},
    Json,
};
use std::sync::Arc;

pub async fn enable_encryption(
    State(service): State<Arc<MegolmService>>,
    Path(room_id): Path<String>,
    Json(request): Json<serde_json::Value>,
) -> Result<Json<()>, ApiError> {
    let _algorithm = request["algorithm"].as_str().unwrap();
    let sender_key = request["sender_key"].as_str().unwrap();

    service.create_session(&room_id, sender_key).await?;

    Ok(Json(()))
}

pub async fn disable_encryption(
    State(service): State<Arc<MegolmService>>,
    Path(room_id): Path<String>,
) -> Result<Json<()>, ApiError> {
    let sessions = service.get_room_sessions(&room_id).await?;

    for session in sessions {
        service.delete_session(&session.session_id).await?;
    }

    Ok(Json(()))
}
