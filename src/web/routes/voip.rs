use crate::common::error::ApiError;
use crate::services::VoipService;
use crate::web::routes::AppState;
use crate::web::routes::AuthenticatedUser;
use axum::{extract::Path, extract::State, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnServerResponse {
    pub username: String,
    pub password: String,
    pub uris: Vec<String>,
    pub ttl: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct VoipConfigResponse {
    pub turn_servers: Option<Vec<TurnServerResponse>>,
    pub stun_servers: Option<Vec<String>>,
}

#[axum::debug_handler]
pub async fn get_turn_server(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<TurnServerResponse>, ApiError> {
    let voip_service = VoipService::new(Arc::new(state.services.config.voip.clone()));

    if !voip_service.is_enabled() {
        return Err(ApiError::not_found("VoIP/TURN service is not configured"));
    }

    let creds = voip_service.generate_turn_credentials(&auth_user.user_id)?;

    Ok(Json(TurnServerResponse {
        username: creds.username,
        password: creds.password,
        uris: creds.uris,
        ttl: creds.ttl,
    }))
}

#[axum::debug_handler]
pub async fn get_voip_config(
    State(state): State<AppState>,
) -> Result<Json<VoipConfigResponse>, ApiError> {
    let voip_service = VoipService::new(Arc::new(state.services.config.voip.clone()));

    if !voip_service.is_enabled() {
        return Ok(Json(VoipConfigResponse {
            turn_servers: None,
            stun_servers: None,
        }));
    }

    let settings = voip_service.get_settings();
    let turn_servers = if !settings.turn_uris.is_empty() {
        if let (Some(username), Some(password)) = (settings.turn_username, settings.turn_password) {
            Some(vec![TurnServerResponse {
                username,
                password,
                uris: settings.turn_uris,
                ttl: 86400,
            }])
        } else {
            None
        }
    } else {
        None
    };

    Ok(Json(VoipConfigResponse {
        turn_servers,
        stun_servers: if !settings.stun_uris.is_empty() {
            Some(settings.stun_uris)
        } else {
            None
        },
    }))
}

#[axum::debug_handler]
pub async fn get_turn_credentials_guest(
    State(state): State<AppState>,
) -> Result<Json<TurnServerResponse>, ApiError> {
    let voip_service = VoipService::new(Arc::new(state.services.config.voip.clone()));

    if !voip_service.is_enabled() {
        return Err(ApiError::not_found("VoIP/TURN service is not configured"));
    }

    if !voip_service.can_guest_use_turn() {
        return Err(ApiError::forbidden(
            "Guest access to TURN server is disabled",
        ));
    }

    let creds = voip_service.generate_turn_credentials("@guest:anonymous")?;

    Ok(Json(TurnServerResponse {
        username: creds.username,
        password: creds.password,
        uris: creds.uris,
        ttl: creds.ttl,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_turn_server_response_serialization() {
        let response = TurnServerResponse {
            username: "test_user".to_string(),
            password: "test_password".to_string(),
            uris: vec!["turn:turn.example.com:3478".to_string()],
            ttl: 3600,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("test_user"));
        assert!(json.contains("test_password"));
        assert!(json.contains("3600"));
    }

    #[test]
    fn test_voip_config_response() {
        let response = VoipConfigResponse {
            turn_servers: Some(vec![TurnServerResponse {
                username: "user".to_string(),
                password: "pass".to_string(),
                uris: vec!["turn:example.com:3478".to_string()],
                ttl: 3600,
            }]),
            stun_servers: Some(vec!["stun:stun.example.com:3478".to_string()]),
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("turn_servers"));
        assert!(json.contains("stun_servers"));
    }
}

// ============================================================================
// VOIP Call Event Handlers (MSC3079)
// ============================================================================

use crate::services::call_service::{
    CallAnswerEvent, CallCandidatesEvent, CallHangupEvent, CallInviteEvent,
};

/// Call invite event
#[axum::debug_handler]
pub async fn call_invite(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(content): Json<CallInviteEvent>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let session = state
        .services
        .call_service
        .handle_invite(&room_id, &auth_user.user_id, content)
        .await?;

    Ok(Json(serde_json::json!({
        "call_id": session.call_id,
        "state": session.state
    })))
}

/// Call candidates event
#[axum::debug_handler]
pub async fn call_candidates(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(content): Json<CallCandidatesEvent>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .services
        .call_service
        .handle_candidates(&room_id, &auth_user.user_id, content)
        .await?;

    Ok(Json(serde_json::json!({})))
}

/// Call answer event
#[axum::debug_handler]
pub async fn call_answer(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(content): Json<CallAnswerEvent>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let session = state
        .services
        .call_service
        .handle_answer(&room_id, &auth_user.user_id, content)
        .await?;

    Ok(Json(serde_json::json!({
        "call_id": session.call_id,
        "state": session.state
    })))
}

/// Call hangup event
#[axum::debug_handler]
pub async fn call_hangup(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(content): Json<CallHangupEvent>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .services
        .call_service
        .handle_hangup(&room_id, &auth_user.user_id, content)
        .await?;

    Ok(Json(serde_json::json!({})))
}

/// Get call session
#[axum::debug_handler]
pub async fn get_call_session(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path((room_id, call_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let session = state
        .services
        .call_service
        .get_session(&call_id, &room_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Call session not found"))?;

    let candidates = state
        .services
        .call_service
        .get_candidates(&call_id, &room_id)
        .await?;

    Ok(Json(serde_json::json!({
        "call_id": session.call_id,
        "room_id": session.room_id,
        "caller_id": session.caller_id,
        "callee_id": session.callee_id,
        "state": session.state,
        "candidates": candidates
    })))
}
