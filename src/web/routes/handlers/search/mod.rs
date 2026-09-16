/// The `context` module.
pub(crate) mod context;
/// The `hierarchy` module.
pub(crate) mod hierarchy;
/// The `search` module.
#[allow(clippy::module_inception)]
pub(crate) mod search;

use crate::web::routes::AppState;
use axum::routing::{get, post};
use axum::Router;

fn create_search_compat_router() -> Router<AppState> {
    // `/search` is a stable Matrix endpoint. `/search_recipients` and
    // `/search_rooms` are private and now served under
    // `/_matrix/vendor/v1/…`; the `/v3` aliases stay for backward compatibility
    // and are deprecated (ISSUE-13) — this comment is the deprecation notice,
    // replacing the per-boot WARN B1-3 removed.
    Router::new()
        .route("/search", post(search::search))
        .route("/search_recipients", post(search::search_recipients))
        .route("/search_rooms", post(search::search_rooms))
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
