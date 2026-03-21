use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::common::ApiError;
use crate::web::routes::AppState;
use crate::web::routes::AuthenticatedUser;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThirdPartyProtocol {
    pub user_fields: Vec<String>,
    pub location_fields: Vec<String>,
    pub icon: String,
    pub field_types: HashMap<String, FieldType>,
    pub instances: Vec<ProtocolInstance>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldType {
    pub regexp: String,
    pub placeholder: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolInstance {
    pub network_id: String,
    pub desc: String,
    pub icon: String,
    pub fields: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct ProtocolQuery {
    pub search: Option<String>,
}

pub fn create_thirdparty_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/_matrix/client/v3/thirdparty/protocols",
            get(get_protocols),
        )
        .route(
            "/_matrix/client/v3/thirdparty/protocol/{protocol}",
            get(get_protocol),
        )
        .route(
            "/_matrix/client/v3/thirdparty/location/{protocol}",
            get(get_location),
        )
        .route(
            "/_matrix/client/v3/thirdparty/location",
            get(get_location_by_alias),
        )
        .route(
            "/_matrix/client/v3/thirdparty/user/{protocol}",
            get(get_user),
        )
        .route("/_matrix/client/v3/thirdparty/user", get(get_user_by_id))
        .route(
            "/_matrix/client/r0/thirdparty/protocols",
            get(get_protocols),
        )
        .route(
            "/_matrix/client/r0/thirdparty/protocol/{protocol}",
            get(get_protocol),
        )
        .route(
            "/_matrix/client/r0/thirdparty/location/{protocol}",
            get(get_location),
        )
        .route(
            "/_matrix/client/r0/thirdparty/user/{protocol}",
            get(get_user),
        )
        .with_state(state)
}

async fn get_protocols(
    State(_state): State<AppState>,
    _auth_user: AuthenticatedUser,
) -> Json<serde_json::Value> {
    let protocols: HashMap<String, ThirdPartyProtocol> = HashMap::new();
    Json(serde_json::to_value(protocols).unwrap_or_default())
}

async fn get_protocol(
    State(_state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(protocol): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_found(format!(
        "Protocol '{}' not found. No third-party protocols are currently configured.",
        protocol
    )))
}

#[derive(Debug, Deserialize)]
pub struct LocationQuery {
    pub alias: Option<String>,
    pub search: Option<String>,
}

async fn get_location(
    State(_state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(protocol): Path<String>,
    Query(_query): Query<LocationQuery>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    Err(ApiError::not_found(format!(
        "Protocol '{}' not found. No third-party protocols are currently configured.",
        protocol
    )))
}

async fn get_location_by_alias(
    State(_state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Query(_query): Query<LocationQuery>,
) -> Json<Vec<serde_json::Value>> {
    Json(vec![])
}

#[derive(Debug, Deserialize)]
pub struct UserQuery {
    pub userid: Option<String>,
    pub search: Option<String>,
}

async fn get_user(
    State(_state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(protocol): Path<String>,
    Query(_query): Query<UserQuery>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    Err(ApiError::not_found(format!(
        "Protocol '{}' not found. No third-party protocols are currently configured.",
        protocol
    )))
}

async fn get_user_by_id(
    State(_state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Query(_query): Query<UserQuery>,
) -> Json<Vec<serde_json::Value>> {
    Json(vec![])
}
