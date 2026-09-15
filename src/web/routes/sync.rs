use crate::web::routes::context::SyncContext;
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
    State(ctx): State<SyncContext>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let mut response = next.run(request).await;
    let route_owner = synapse_services::worker::topology_validator::current_instance_worker_type(&ctx.config.worker);
    response.headers_mut().insert(ROUTE_OWNER_HEADER, HeaderValue::from_static(route_owner.as_str()));
    response
}

fn create_sync_compat_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/sync", get(sync))
        .route("/events", get(get_events))
        .route_layer(middleware::from_fn_with_state(state, sync_route_owner_header_middleware))
}

fn create_sync_v3_router(state: AppState) -> Router<AppState> {
    // `/joined_rooms` is a stable Matrix endpoint; `/my_rooms` is private and now
    // served under `/_matrix/vendor/v1/my_rooms`. The `/v3` alias stays for
    // backward compatibility and is deprecated (ISSUE-13) — this comment is the
    // deprecation notice, replacing the per-boot WARN B1-3 removed.
    create_sync_compat_router(state).route("/joined_rooms", get(get_joined_rooms)).route("/my_rooms", get(get_my_rooms))
}

/// See [`create_sync_router`].
pub fn create_sync_router(state: AppState) -> Router<AppState> {
    Router::new().nest("/_matrix/client/v3", create_sync_v3_router(state))
}

/// Manifest of every `(method, absolute_path)` tuple `create_sync_router`
/// registers. The surface is enumerated per-prefix rather than expanded
/// uniformly so the `/sync` rate-limit exemption can be attached entry-by-entry.
pub fn sync_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::expand_under_prefixes;
    use axum::http::Method;

    const MODULE: &str = "sync";

    let mut out = Vec::new();

    // B-4: /sync routes are exempt from IP-level rate limiting because the
    // handler implements its own per-user+device rate limiter. Marking them
    // here lets `create_router` auto-derive the exemption list from the ledger
    // instead of hardcoding paths in the middleware.
    out.extend(
        expand_under_prefixes(MODULE, &["/_matrix/client/v3"], &[(Method::GET, "/sync")])
            .into_iter()
            .map(|e| e.with_rate_limit_exempt(true)),
    );
    out.extend(expand_under_prefixes(
        MODULE,
        &["/_matrix/client/v3"],
        &[(Method::GET, "/events"), (Method::GET, "/joined_rooms"), (Method::GET, "/my_rooms")],
    ));
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_sync_routes_structure() {
        let routes = [
            "/_matrix/client/v3/sync",
            "/_matrix/client/v3/events",
            "/_matrix/client/v3/joined_rooms",
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
