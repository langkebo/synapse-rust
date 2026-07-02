use crate::web::routes::{
    get_joined_rooms, get_my_rooms,
    handlers::sync::{get_events, sync},
    AppState,
};
use axum::{
    body::Body,
    extract::State,
    http::{HeaderValue, Request},
    middleware::{self, Next},
    response::Response,
    routing::get,
    Router,
};

const ROUTE_OWNER_HEADER: &str = "x-synapse-route-owner";

async fn sync_route_owner_header_middleware(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let mut response = next.run(request).await;
    let route_owner =
        synapse_services::worker::topology_validator::current_instance_worker_type(&state.services.core.config.worker);
    response.headers_mut().insert(ROUTE_OWNER_HEADER, HeaderValue::from_static(route_owner.as_str()));
    response
}

fn create_sync_compat_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/sync", get(sync))
        .route("/events", get(get_events))
        .route_layer(middleware::from_fn_with_state(state, sync_route_owner_header_middleware))
}

fn create_sync_r0_router(state: AppState) -> Router<AppState> {
    create_sync_compat_router(state).route("/joined_rooms", get(get_joined_rooms))
}

fn create_sync_v3_router(state: AppState) -> Router<AppState> {
    create_sync_compat_router(state).route("/joined_rooms", get(get_joined_rooms)).route("/my_rooms", get(get_my_rooms))
}

pub fn create_sync_router(state: AppState) -> Router<AppState> {
    Router::new()
        .nest("/_matrix/client/r0", create_sync_r0_router(state.clone()))
        .nest("/_matrix/client/v3", create_sync_v3_router(state))
}

/// Manifest of every `(method, absolute_path)` tuple `create_sync_router`
/// registers. Each version has a distinct inner router (r0 has `/sync`,
/// `/events`, `/joined_rooms`; v3 has all of the above plus `/my_rooms`) so
/// the entries are enumerated per-prefix rather than expanded uniformly.
pub fn sync_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::expand_under_prefixes;
    use axum::http::Method;

    const MODULE: &str = "sync";

    let mut out = Vec::new();
    out.extend(expand_under_prefixes(
        MODULE,
        &["/_matrix/client/r0"],
        &[(Method::GET, "/sync"), (Method::GET, "/events"), (Method::GET, "/joined_rooms")],
    ));
    out.extend(expand_under_prefixes(
        MODULE,
        &["/_matrix/client/v3"],
        &[(Method::GET, "/sync"), (Method::GET, "/events"), (Method::GET, "/joined_rooms"), (Method::GET, "/my_rooms")],
    ));
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_sync_routes_structure() {
        let routes = [
            "/_matrix/client/r0/sync",
            "/_matrix/client/r0/events",
            "/_matrix/client/r0/joined_rooms",
            "/_matrix/client/v3/sync",
            "/_matrix/client/v3/events",
            "/_matrix/client/v3/joined_rooms",
            "/_matrix/client/v3/my_rooms",
        ];

        assert!(routes.iter().all(|route| route.starts_with("/_matrix/client/")));
    }

    #[test]
    fn test_sync_router_version_boundaries() {
        let v3_only = ["/_matrix/client/v3/joined_rooms", "/_matrix/client/v3/my_rooms"];

        assert!(v3_only.iter().all(|route| route.starts_with("/_matrix/client/v3/")));
    }
}
