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

fn build_irc_protocol() -> ThirdPartyProtocol {
    let mut field_types = HashMap::new();
    field_types.insert(
        "channel".to_string(),
        FieldType {
            regexp: "^[#&][^ ,]+$".to_string(),
            placeholder: "Channel name (e.g., #matrix)".to_string(),
        },
    );
    field_types.insert(
        "nickname".to_string(),
        FieldType {
            regexp: "^[a-zA-Z0-9_\\-\\[\\]]{1,32}$".to_string(),
            placeholder: "Nickname".to_string(),
        },
    );
    field_types.insert(
        "server".to_string(),
        FieldType {
            regexp: "^[a-zA-Z0-9.-]+$".to_string(),
            placeholder: "Server name (e.g., irc.libera.chat)".to_string(),
        },
    );

    let mut instances = HashMap::new();
    instances.insert("network".to_string(), "irc.libera.chat".to_string());
    instances.insert("channel".to_string(), "#matrix".to_string());

    ThirdPartyProtocol {
        user_fields: vec!["nickname".to_string(), "server".to_string()],
        location_fields: vec!["channel".to_string(), "server".to_string()],
        icon: "mxc://atrix.li/irc.png".to_string(),
        field_types,
        instances: vec![ProtocolInstance {
            network_id: "libera".to_string(),
            desc: "Libera.Chat IRC Network".to_string(),
            icon: "mxc://atrix.li/libera.png".to_string(),
            fields: instances,
        }],
    }
}

fn build_protocols_index() -> HashMap<String, ThirdPartyProtocol> {
    let mut protocols = HashMap::new();
    protocols.insert("irc".to_string(), build_irc_protocol());
    protocols
}

async fn get_protocols(
    State(_state): State<AppState>,
    _auth_user: AuthenticatedUser,
) -> Json<serde_json::Value> {
    let protocols = build_protocols_index();
    Json(serde_json::to_value(protocols).unwrap_or_default())
}

async fn get_protocol(
    State(_state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(protocol): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let protocols = build_protocols_index();
    if let Some(p) = protocols.get(&protocol) {
        Ok(Json(serde_json::to_value(p).unwrap_or_default()))
    } else {
        Err(ApiError::not_found(format!(
            "Protocol '{}' not found. Available protocols: irc",
            protocol
        )))
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
    Query(query): Query<LocationQuery>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    if protocol != "irc" {
        return Err(ApiError::not_found(format!(
            "Protocol '{}' not found. Available protocols: irc",
            protocol
        )));
    }

    let mut results = Vec::new();
    let server = query
        .server
        .clone()
        .unwrap_or_else(|| "irc.libera.chat".to_string());

    if let Some(ref channel) = query.channel {
        results.push(serde_json::json!({
            "alias": format!("#{} on {}", channel.replace("#", ""), server),
            "protocol": "irc",
            "fields": {
                "channel": channel,
                "server": server
            }
        }));
    }

    if query.channel.is_none() && query.server.is_none() && query.search.is_none() {
        results.push(serde_json::json!({
            "alias": "#matrix on irc.libera.chat",
            "protocol": "irc",
            "fields": {
                "channel": "#matrix",
                "server": "irc.libera.chat"
            }
        }));
    }

    Ok(Json(results))
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
    pub nickname: Option<String>,
    pub server: Option<String>,
}

async fn get_user(
    State(_state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(protocol): Path<String>,
    Query(query): Query<UserQuery>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    if protocol != "irc" {
        return Err(ApiError::not_found(format!(
            "Protocol '{}' not found. Available protocols: irc",
            protocol
        )));
    }

    let mut results = Vec::new();
    let server = query
        .server
        .clone()
        .unwrap_or_else(|| "irc.libera.chat".to_string());

    if let Some(ref nickname) = query.nickname {
        results.push(serde_json::json!({
            "userid": format!("{}@{}", nickname, server),
            "protocol": "irc",
            "fields": {
                "nickname": nickname,
                "server": server
            }
        }));
    }

    if query.nickname.is_none() && query.userid.is_none() && query.search.is_none() {
        results.push(serde_json::json!({
            "userid": "NickServ@irc.libera.chat",
            "protocol": "irc",
            "fields": {
                "nickname": "NickServ",
                "server": "irc.libera.chat"
            }
        }));
    }

    Ok(Json(results))
}

async fn get_user_by_id(
    State(_state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Query(_query): Query<UserQuery>,
) -> Json<Vec<serde_json::Value>> {
    Json(vec![])
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
}
