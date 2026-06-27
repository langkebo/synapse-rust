//! MSC4143 — RTC Transports handler
//!
//! Provides ICE server transport information (STUN/TURN) for MatrixRTC calls
//! as specified in MSC4403, wrapped in an MSC4143 response envelope.

use crate::web::routes::ApiError;
use crate::web::routes::AuthenticatedUser;
use crate::web::AppState;
use axum::{extract::State, Json};
use serde_json::json;

/// MSC4143 — `org.matrix.msc4143/rtc/transports`. Element calls this when a
/// VoIP focus call is started. We return standard ICE server transport (MSC4403)
/// based on our VoIP/TURN/STUN configuration so clients can use them for
/// MatrixRTC calls even without a dedicated SFU.
pub async fn get_rtc_transports(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    let voip_service = &state.services.extensions.rtc_domain_service.infra;

    if !voip_service.is_enabled() {
        return Ok(Json(json!({ "transports": [] })));
    }

    let mut ice_servers = Vec::new();

    // Add STUN servers
    let stun_uris = voip_service.get_stun_uris();
    if !stun_uris.is_empty() {
        ice_servers.push(json!({
            "urls": stun_uris,
        }));
    }

    // Add TURN servers with credentials
    if let Ok(creds) = voip_service.generate_turn_credentials(&auth_user.user_id) {
        ice_servers.push(json!({
            "urls": creds.uris,
            "username": creds.username,
            "credential": creds.password,
        }));
    }

    if ice_servers.is_empty() {
        return Ok(Json(json!({ "transports": [] })));
    }

    Ok(Json(json!({
        "transports": [
            {
                "type": "org.matrix.msc4403.ice-server-transport",
                "ice_servers": ice_servers,
            }
        ]
    })))
}
