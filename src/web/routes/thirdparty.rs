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
        .route(
            "/_matrix/client/r0/thirdparty/location",
            get(get_location_by_alias),
        )
        .route("/_matrix/client/r0/thirdparty/user", get(get_user_by_id))
        .with_state(state)
}

const THIRDPARTY_COMPAT_PREFIXES: &[&str] = &["/_matrix/client/v3", "/_matrix/client/r0"];

fn thirdparty_compat_relative_routes() -> Vec<(axum::http::Method, &'static str)> {
    use axum::http::Method;
    vec![
        (Method::GET, "/thirdparty/protocols"),
        (Method::GET, "/thirdparty/protocol/{protocol}"),
        (Method::GET, "/thirdparty/location/{protocol}"),
        (Method::GET, "/thirdparty/user/{protocol}"),
    ]
}

pub fn thirdparty_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::{expand_under_prefixes, RouteEntry};
    use axum::http::Method;

    let mut out = expand_under_prefixes(
        "thirdparty",
        THIRDPARTY_COMPAT_PREFIXES,
        &thirdparty_compat_relative_routes(),
    );
    out.extend(
        [
            (Method::GET, "/_matrix/client/v3/thirdparty/location"),
            (Method::GET, "/_matrix/client/v3/thirdparty/user"),
            (Method::GET, "/_matrix/client/r0/thirdparty/location"),
            (Method::GET, "/_matrix/client/r0/thirdparty/user"),
        ]
        .into_iter()
        .map(|(m, p)| RouteEntry::new(m, p, "thirdparty")),
    );
    out
}

async fn get_protocols(
    State(_state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(serde_json::json!({})))
}

async fn get_protocol(
    State(_state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(_protocol): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(serde_json::json!({
        "instances": [],
        "user_fields": [],
        "location_fields": []
    })))
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
    Path(_protocol): Path<String>,
    Query(_query): Query<LocationQuery>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    Err(ApiError::unrecognized(
        "No third-party location bridges configured",
    ))
}

async fn get_location_by_alias(
    State(_state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Query(_query): Query<LocationQuery>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    Err(ApiError::unrecognized(
        "No third-party location bridges configured",
    ))
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
    Path(_protocol): Path<String>,
    Query(_query): Query<UserQuery>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    Err(ApiError::unrecognized(
        "No third-party user bridges configured",
    ))
}

async fn get_user_by_id(
    State(_state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Query(_query): Query<UserQuery>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    Err(ApiError::unrecognized(
        "No third-party user bridges configured",
    ))
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
