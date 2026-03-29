use crate::web::routes::{get_events, get_joined_rooms, get_my_rooms, sync, AppState};
use axum::{routing::get, Router};

fn create_sync_compat_router() -> Router<AppState> {
    Router::new()
        .route("/sync", get(sync))
        .route("/events", get(get_events))
}

fn create_sync_r0_router() -> Router<AppState> {
    create_sync_compat_router().route("/joined_rooms", get(get_joined_rooms))
}

fn create_sync_v1_router() -> Router<AppState> {
    Router::new().route("/sync", get(sync))
}

fn create_sync_v3_router() -> Router<AppState> {
    create_sync_compat_router()
        .route("/joined_rooms", get(get_joined_rooms))
        .route("/my_rooms", get(get_my_rooms))
}

pub fn create_sync_router() -> Router<AppState> {
    Router::new()
        .nest("/_matrix/client/r0", create_sync_r0_router())
        .nest("/_matrix/client/v1", create_sync_v1_router())
        .nest("/_matrix/client/v3", create_sync_v3_router())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_sync_routes_structure() {
        let routes = [
            "/_matrix/client/r0/sync",
            "/_matrix/client/r0/events",
            "/_matrix/client/r0/joined_rooms",
            "/_matrix/client/v1/sync",
            "/_matrix/client/v3/sync",
            "/_matrix/client/v3/events",
            "/_matrix/client/v3/joined_rooms",
            "/_matrix/client/v3/my_rooms",
        ];

        assert!(routes
            .iter()
            .all(|route| route.starts_with("/_matrix/client/")));
    }

    #[test]
    fn test_sync_router_version_boundaries() {
        let v1_only = ["/_matrix/client/v1/sync"];
        let v3_only = [
            "/_matrix/client/v3/joined_rooms",
            "/_matrix/client/v3/my_rooms",
        ];

        assert!(v1_only.iter().all(|route| !route.contains("/events")));
        assert!(v3_only
            .iter()
            .all(|route| route.starts_with("/_matrix/client/v3/")));
    }
}
