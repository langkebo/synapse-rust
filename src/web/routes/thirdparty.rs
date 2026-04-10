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

#[allow(dead_code)]
fn default_irc_protocol() -> ThirdPartyProtocol {
    let mut field_types = HashMap::new();
    field_types.insert(
        "domain".to_string(),
        FieldType {
            regexp: "^[A-Za-z0-9.-]+$".to_string(),
            placeholder: "irc.example.com".to_string(),
        },
    );
    field_types.insert(
        "channel".to_string(),
        FieldType {
            regexp: "^#?[A-Za-z0-9._-]+$".to_string(),
            placeholder: "#synapse".to_string(),
        },
    );
    field_types.insert(
        "nick".to_string(),
        FieldType {
            regexp: "^[A-Za-z0-9_\\-\\[\\]\\\\`^{}|]+$".to_string(),
            placeholder: "synapsebot".to_string(),
        },
    );

    let mut fields = HashMap::new();
    fields.insert("domain".to_string(), "irc.example.com".to_string());
    fields.insert("network".to_string(), "Synapse IRC".to_string());

    ThirdPartyProtocol {
        user_fields: vec!["domain".to_string(), "nick".to_string()],
        location_fields: vec!["domain".to_string(), "channel".to_string()],
        icon: "mxc://synapse-rust/thirdparty/irc".to_string(),
        field_types,
        instances: vec![ProtocolInstance {
            network_id: "irc".to_string(),
            desc: "Built-in IRC bridge compatibility profile".to_string(),
            icon: "mxc://synapse-rust/thirdparty/irc".to_string(),
            fields,
        }],
    }
}

#[allow(dead_code)]
fn supported_protocol(protocol: &str) -> Option<ThirdPartyProtocol> {
    match protocol {
        "irc" => Some(default_irc_protocol()),
        _ => None,
    }
}

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

fn create_thirdparty_compat_router() -> Router<AppState> {
    Router::new()
        .route("/thirdparty/protocols", get(get_protocols))
        .route("/thirdparty/protocol/{protocol}", get(get_protocol))
        .route("/thirdparty/location/{protocol}", get(get_location))
        .route("/thirdparty/user/{protocol}", get(get_user))
}

pub fn create_thirdparty_router(state: AppState) -> Router<AppState> {
    let compat_router = create_thirdparty_compat_router();

    Router::new()
        .nest("/_matrix/client/v3", compat_router.clone())
        .nest("/_matrix/client/r0", compat_router)
        .route(
            "/_matrix/client/v3/thirdparty/location",
            get(get_location_by_alias),
        )
        .route("/_matrix/client/v3/thirdparty/user", get(get_user_by_id))
        .with_state(state)
}

async fn get_protocols(
    State(_state): State<AppState>,
    _auth_user: AuthenticatedUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    let irc = default_irc_protocol();
    let mut protocols = serde_json::Map::new();
    protocols.insert(
        "irc".to_string(),
        serde_json::to_value(irc).unwrap_or_default(),
    );
    Ok(Json(serde_json::Value::Object(protocols)))
}

async fn get_protocol(
    State(_state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(protocol): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    match supported_protocol(&protocol) {
        Some(p) => Ok(Json(serde_json::to_value(p).unwrap_or_default())),
        None => Err(ApiError::not_found(format!(
            "Protocol {} not found",
            protocol
        ))),
    }
}

#[derive(Debug, Deserialize)]
pub struct LocationQuery {
    pub alias: Option<String>,
    pub search: Option<String>,
    pub server: Option<String>,
    pub channel: Option<String>,
}

async fn get_location(
    State(_state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(protocol): Path<String>,
    Query(_query): Query<LocationQuery>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    let _ = protocol;
    Ok(Json(vec![]))
}

async fn get_location_by_alias(
    State(_state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Query(_query): Query<LocationQuery>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    Ok(Json(vec![]))
}

#[derive(Debug, Deserialize)]
pub struct UserQuery {
    pub userid: Option<String>,
    pub search: Option<String>,
    pub nickname: Option<String>,
    pub server: Option<String>,
}

async fn get_user(
    State(_state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(protocol): Path<String>,
    Query(_query): Query<UserQuery>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    let _ = protocol;
    Ok(Json(vec![]))
}

async fn get_user_by_id(
    State(_state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Query(_query): Query<UserQuery>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    Ok(Json(vec![]))
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_thirdparty_routes_structure() {
        let compat_routes = [
            "/_matrix/client/v3/thirdparty/protocols",
            "/_matrix/client/r0/thirdparty/protocol/{protocol}",
            "/_matrix/client/v3/thirdparty/location/{protocol}",
            "/_matrix/client/r0/thirdparty/user/{protocol}",
        ];
        let v3_only_routes = [
            "/_matrix/client/v3/thirdparty/location",
            "/_matrix/client/v3/thirdparty/user",
        ];

        assert!(compat_routes
            .iter()
            .all(|route| route.starts_with("/_matrix/client/")));
        assert!(v3_only_routes
            .iter()
            .all(|route| route.starts_with("/_matrix/client/v3/")));
    }

    #[test]
    fn test_thirdparty_compat_router_contains_shared_paths() {
        let shared_paths = [
            "/thirdparty/protocols",
            "/thirdparty/protocol/{protocol}",
            "/thirdparty/location/{protocol}",
            "/thirdparty/user/{protocol}",
        ];

        assert_eq!(shared_paths.len(), 4);
        assert!(shared_paths
            .iter()
            .all(|path| path.starts_with("/thirdparty/")));
    }

    #[test]
    fn test_thirdparty_router_keeps_query_endpoints_outside_compat_scope() {
        let compat_paths = ["/thirdparty/protocols", "/thirdparty/location/{protocol}"];
        let v3_only_paths = [
            "/_matrix/client/v3/thirdparty/location",
            "/_matrix/client/v3/thirdparty/user",
        ];
        let absent_r0_paths = [
            "/_matrix/client/r0/thirdparty/location",
            "/_matrix/client/r0/thirdparty/user",
        ];

        assert!(compat_paths
            .iter()
            .all(|path| !path.ends_with("/thirdparty/location")
                && !path.ends_with("/thirdparty/user")));
        assert!(v3_only_paths
            .iter()
            .all(|path| path.starts_with("/_matrix/client/v3/")));
        assert!(absent_r0_paths
            .iter()
            .all(|path| path.starts_with("/_matrix/client/r0/")));
    }

    #[test]
    fn test_supported_protocol_returns_irc_profile() {
        let protocol = super::supported_protocol("irc").expect("irc protocol should exist");

        assert!(protocol.user_fields.iter().any(|field| field == "nick"));
        assert!(protocol
            .location_fields
            .iter()
            .any(|field| field == "channel"));
        assert_eq!(protocol.instances.len(), 1);
        assert_eq!(protocol.instances[0].network_id, "irc");
    }
}
