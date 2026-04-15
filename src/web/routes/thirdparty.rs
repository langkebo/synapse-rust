use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use serde::Deserialize;

use crate::common::ApiError;
use crate::web::routes::AppState;
use crate::web::routes::AuthenticatedUser;

#[derive(Debug, Deserialize)]
pub struct ProtocolQuery {
    pub search: Option<String>,
}

fn unsupported_thirdparty(operation: &str) -> ApiError {
    ApiError::unrecognized(format!(
        "Third-party network lookup is not implemented for this deployment ({})",
        operation
    ))
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
    Err(unsupported_thirdparty("protocols"))
}

async fn get_protocol(
    State(_state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(protocol): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(unsupported_thirdparty(&format!("protocol={}", protocol)))
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
    Err(unsupported_thirdparty(&format!(
        "location protocol={}",
        protocol
    )))
}

async fn get_location_by_alias(
    State(_state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Query(_query): Query<LocationQuery>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    Err(unsupported_thirdparty("location query"))
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
    Err(unsupported_thirdparty(&format!("user protocol={}", protocol)))
}

async fn get_user_by_id(
    State(_state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Query(_query): Query<UserQuery>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    Err(unsupported_thirdparty("user query"))
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
