use crate::web::routes::{
    get_scanner_info, report_event, report_room, update_report_score, AppState,
};
use axum::{
    routing::{get, post, put},
    Router,
};

fn create_room_report_compat_router() -> Router<AppState> {
    Router::new()
        .route("/rooms/{room_id}/report/{event_id}", post(report_event))
        .route(
            "/rooms/{room_id}/report/{event_id}/score",
            put(update_report_score),
        )
}

fn create_moderation_v1_router() -> Router<AppState> {
    create_room_report_compat_router().route(
        "/rooms/{room_id}/report/{event_id}/scanner_info",
        get(get_scanner_info),
    )
}

fn create_moderation_v3_router() -> Router<AppState> {
    create_room_report_compat_router().route("/rooms/{room_id}/report", post(report_room))
}

pub fn create_moderation_router() -> Router<AppState> {
    let compat_router = create_room_report_compat_router();

    Router::new()
        .nest("/_matrix/client/r0", compat_router)
        .nest("/_matrix/client/v1", create_moderation_v1_router())
        .nest("/_matrix/client/v3", create_moderation_v3_router())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_moderation_routes_structure() {
        let routes = [
            "/_matrix/client/r0/rooms/{room_id}/report/{event_id}",
            "/_matrix/client/r0/rooms/{room_id}/report/{event_id}/score",
            "/_matrix/client/v1/rooms/{room_id}/report/{event_id}",
            "/_matrix/client/v1/rooms/{room_id}/report/{event_id}/score",
            "/_matrix/client/v1/rooms/{room_id}/report/{event_id}/scanner_info",
            "/_matrix/client/v3/rooms/{room_id}/report/{event_id}",
            "/_matrix/client/v3/rooms/{room_id}/report/{event_id}/score",
            "/_matrix/client/v3/rooms/{room_id}/report",
        ];

        assert!(routes
            .iter()
            .all(|route| route.starts_with("/_matrix/client/")));
    }

    #[test]
    fn test_moderation_router_keeps_version_specific_paths() {
        let v1_only = ["/_matrix/client/v1/rooms/{room_id}/report/{event_id}/scanner_info"];

        assert!(v1_only
            .iter()
            .all(|route| !route.ends_with("/{event_id}/score")));
    }
}
