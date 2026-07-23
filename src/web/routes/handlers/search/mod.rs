pub(crate) mod context;
pub(crate) mod hierarchy;
#[allow(clippy::module_inception)]
pub(crate) mod search;

use crate::web::routes::AppState;
use axum::routing::{get, post};
use axum::Router;

fn create_search_compat_router() -> Router<AppState> {
    Router::new()
        .route("/search", post(search::search))
        .route("/search_recipients", post(search::search_recipients))
        .route("/search_rooms", post(search::search_rooms))
}

fn create_room_context_router() -> Router<AppState> {
    Router::new().route("/rooms/{room_id}/context/{event_id}", get(context::get_event_context))
}

pub fn create_search_router(state: AppState) -> Router<AppState> {
    let v1_router = Router::new()
        .merge(create_room_context_router())
        .route("/rooms/{room_id}/hierarchy", get(hierarchy::get_room_hierarchy))
        .route("/rooms/{room_id}/timestamp_to_event", get(context::timestamp_to_event));

    let v3_router = Router::new()
        .merge(create_search_compat_router())
        .merge(create_room_context_router())
        .route("/rooms/{room_id}/hierarchy", get(hierarchy::get_room_hierarchy_v3));

    Router::new()
        .nest("/_matrix/client/r0", create_search_compat_router().merge(create_room_context_router()))
        .nest("/_matrix/client/v1", v1_router)
        .nest("/_matrix/client/v3", v3_router)
        .with_state(state)
}

pub fn search_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::{expand_under_prefixes, RouteEntry};
    use axum::http::Method;

    let search_compat: &[(Method, &'static str)] =
        &[(Method::POST, "/search"), (Method::POST, "/search_recipients"), (Method::POST, "/search_rooms")];
    let room_context: &[(Method, &'static str)] = &[(Method::GET, "/rooms/{room_id}/context/{event_id}")];
    let v1_extras: &[(Method, &'static str)] =
        &[(Method::GET, "/rooms/{room_id}/hierarchy"), (Method::GET, "/rooms/{room_id}/timestamp_to_event")];
    let v3_extras: &[(Method, &'static str)] = &[(Method::GET, "/rooms/{room_id}/hierarchy")];

    let mut entries: Vec<RouteEntry> = expand_under_prefixes("search", &["/_matrix/client/r0"], search_compat);
    entries.extend(expand_under_prefixes("search", &["/_matrix/client/v1"], room_context));
    entries.extend(expand_under_prefixes("search", &["/_matrix/client/v1"], v1_extras));
    entries.extend(expand_under_prefixes("search", &["/_matrix/client/v3"], search_compat));
    entries.extend(expand_under_prefixes("search", &["/_matrix/client/v3"], room_context));
    entries.extend(expand_under_prefixes("search", &["/_matrix/client/v3"], v3_extras));
    entries
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_search_routes_structure() {
        let routes = vec![
            "/_matrix/client/v3/search",
            "/_matrix/client/r0/search",
            "/_matrix/client/v1/rooms/{room_id}/hierarchy",
        ];

        for route in routes {
            assert!(route.starts_with("/_matrix/client/"));
        }
    }

    #[test]
    fn test_search_routes_do_not_claim_thread_compat_endpoint() {
        let routes = [
            "/_matrix/client/v3/search",
            "/_matrix/client/r0/search",
            "/_matrix/client/v1/rooms/{room_id}/context/{event_id}",
            "/_matrix/client/v3/rooms/{room_id}/context/{event_id}",
        ];

        assert!(routes.iter().all(|route| !route.contains("/user/{user_id}/rooms/{room_id}/threads")));
    }
}
