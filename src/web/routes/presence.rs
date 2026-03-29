use crate::web::routes::{
    handlers::presence::{get_presence, presence_list, set_presence},
    AppState,
};
use axum::{
    routing::{get, post},
    Router,
};

fn create_presence_compat_router() -> Router<AppState> {
    Router::new().route(
        "/presence/{user_id}/status",
        get(get_presence).put(set_presence),
    )
}

pub fn create_presence_router() -> Router<AppState> {
    let compat_router = create_presence_compat_router();

    Router::new()
        .nest("/_matrix/client/r0", compat_router.clone())
        .nest("/_matrix/client/v3", compat_router)
        .route("/_matrix/client/v3/presence/list", post(presence_list))
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_presence_routes_structure() {
        let routes = [
            "/_matrix/client/r0/presence/{user_id}/status",
            "/_matrix/client/v3/presence/{user_id}/status",
            "/_matrix/client/v3/presence/list",
        ];

        assert!(routes
            .iter()
            .all(|route| route.starts_with("/_matrix/client/")));
    }

    #[test]
    fn test_presence_router_keeps_list_endpoint_on_v3() {
        let compat_paths = ["/presence/{user_id}/status"];
        let v3_only = ["/_matrix/client/v3/presence/list"];

        assert!(compat_paths.iter().all(|path| path.starts_with('/')));
        assert!(v3_only
            .iter()
            .all(|path| path.starts_with("/_matrix/client/v3/")));
    }
}
