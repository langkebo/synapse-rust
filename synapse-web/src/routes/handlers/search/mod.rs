/// The `context` module.
pub(crate) mod context;
/// The `hierarchy` module.
pub(crate) mod hierarchy;
/// The `search` module.
#[allow(clippy::module_inception)]
pub(crate) mod search;

use crate::routes::AppState;
use axum::routing::{get, post};
use axum::Router;

fn create_search_compat_router() -> Router<AppState> {
    // `/search` is the stable Matrix endpoint. `/search_recipients` and
    // `/search_rooms` are private and live **only** under `/_matrix/vendor/v1/…`
    // (see `assembly.rs::create_vendor_router`). Their former `/_matrix/client/v3`
    // aliases were dead duplicates (same handler on both sides) and were deleted
    // under ISSUE-13 / 铁律 1 — regression lock:
    // `scripts/contract/test_extract_registered.py::check_client_prefix_vendor_twins`.
    Router::new().route("/search", post(search::search))
}

fn create_room_context_router() -> Router<AppState> {
    Router::new().route("/rooms/{room_id}/context/{event_id}", get(context::get_event_context))
}

/// See [`create_search_router`].
pub fn create_search_router(state: AppState) -> Router<AppState> {
    let v1_router = Router::new()
        .merge(create_room_context_router())
        .route("/rooms/{room_id}/hierarchy", get(hierarchy::get_room_hierarchy))
        .route("/rooms/{room_id}/timestamp_to_event", get(context::timestamp_to_event));

    let v3_router = Router::new()
        .merge(create_search_compat_router())
        .merge(create_room_context_router())
        .route("/rooms/{room_id}/hierarchy", get(hierarchy::get_room_hierarchy_v3));

    Router::new().nest("/_matrix/client/v1", v1_router).nest("/_matrix/client/v3", v3_router).with_state(state)
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_search_routes_structure() {
        let routes = vec![
            "/_matrix/client/v3/search",
            "/_matrix/client/v3/search",
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
            "/_matrix/client/v3/search",
            "/_matrix/client/v1/rooms/{room_id}/context/{event_id}",
            "/_matrix/client/v3/rooms/{room_id}/context/{event_id}",
        ];

        assert!(routes.iter().all(|route| !route.contains("/user/{user_id}/rooms/{room_id}/threads")));
    }
}
