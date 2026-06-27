//! Client Config handler
//!
//! Provides the `/_matrix/client/v1/config/client` endpoint, returning
//! homeserver configuration details (homeserver URL, identity server, feature
//! flags) that clients use to self-configure.

use crate::web::routes::ApiError;
use crate::web::AppState;
use axum::{extract::State, Json};
use serde_json::json;

pub async fn get_client_config(State(state): State<AppState>) -> Result<Json<serde_json::Value>, ApiError> {
    let config = &state.services.core.config;
    let base_url = config.server.get_public_baseurl();

    Ok(Json(json!({
        "homeserver": {
            "base_url": base_url,
            "server_name": config.server.name,
        },
        "identity_server": {
            "base_url": base_url,
        },
        "push": {
            "enabled": true,
        },
        "email": {
            "enabled": false,
        },
        "features": {
            "e2ee": true,
            "voip": true,
            "threads": true,
            "spaces": true,
        },
        "defaults": {
            "client_info": {
                "name": "synapse-rust",
                "version": env!("CARGO_PKG_VERSION"),
            },
        },
    })))
}
