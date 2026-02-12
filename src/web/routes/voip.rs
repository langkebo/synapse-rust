use crate::common::error::ApiError;
use crate::services::VoipService;
use crate::web::routes::AppState;
use crate::web::routes::AuthenticatedUser;
use axum::{
    extract::State,
    Json,
};
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
        return Err(ApiError::forbidden("Guest access to TURN server is disabled"));
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
