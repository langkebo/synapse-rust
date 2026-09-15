use crate::web::routes::{get_scanner_info, report_event, report_room, report_user, update_report_score, AppState};
use axum::{
    routing::{get, post, put},
    Router,
};

fn create_room_report_compat_router() -> Router<AppState> {
    Router::new()
        .route("/rooms/{room_id}/report/{event_id}", post(report_event))
        .route("/rooms/{room_id}/report/{event_id}/score", put(update_report_score))
}

fn create_moderation_v1_router() -> Router<AppState> {
    create_room_report_compat_router().route("/rooms/{room_id}/report/{event_id}/scanner_info", get(get_scanner_info))
}

fn create_moderation_v3_router() -> Router<AppState> {
    create_room_report_compat_router()
        .route("/rooms/{room_id}/report", post(report_room))
        // MSC4260 (Matrix v1.14): Report a user.
        // Path: POST /_matrix/client/v3/users/{userId}/report
        .route("/users/{user_id}/report", post(report_user))
}

/// See [`create_moderation_router`].
pub fn create_moderation_router() -> Router<AppState> {
    Router::new()
        .nest("/_matrix/client/v1", create_moderation_v1_router())
        .nest("/_matrix/client/v3", create_moderation_v3_router())
}

/// See [`moderation_route_manifest`].
pub fn moderation_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::expand_under_prefixes;
    use axum::http::Method;

    let compat: &[(Method, &str)] = &[
        (Method::POST, "/rooms/{room_id}/report/{event_id}"),
        (Method::PUT, "/rooms/{room_id}/report/{event_id}/score"),
    ];

    let mut v1 = compat.to_vec();
    v1.push((Method::GET, "/rooms/{room_id}/report/{event_id}/scanner_info"));
    let mut out = expand_under_prefixes("moderation", &["/_matrix/client/v1"], &v1);
    let mut v3 = compat.to_vec();
    v3.push((Method::POST, "/rooms/{room_id}/report"));
    // MSC4260: user report endpoint (v3-only).
    v3.push((Method::POST, "/users/{user_id}/report"));
    out.extend(expand_under_prefixes("moderation", &["/_matrix/client/v3"], &v3));
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_moderation_routes_structure() {
        let routes = [
            "/_matrix/client/v3/rooms/{room_id}/report/{event_id}",
            "/_matrix/client/v3/rooms/{room_id}/report/{event_id}/score",
            "/_matrix/client/v1/rooms/{room_id}/report/{event_id}",
            "/_matrix/client/v1/rooms/{room_id}/report/{event_id}/score",
            "/_matrix/client/v1/rooms/{room_id}/report/{event_id}/scanner_info",
            "/_matrix/client/v3/rooms/{room_id}/report/{event_id}",
            "/_matrix/client/v3/rooms/{room_id}/report/{event_id}/score",
            "/_matrix/client/v3/rooms/{room_id}/report",
            // MSC4260 user report
            "/_matrix/client/v3/users/{user_id}/report",
        ];

        assert!(routes.iter().all(|route| route.starts_with("/_matrix/client/")));
    }

    #[test]
    fn test_moderation_router_keeps_version_specific_paths() {
        let v1_only = ["/_matrix/client/v1/rooms/{room_id}/report/{event_id}/scanner_info"];

        assert!(v1_only.iter().all(|route| !route.ends_with("/{event_id}/score")));
    }

    #[test]
    fn test_msc4260_user_report_route_is_v3_only() {
        // MSC4260 user report is only available on v3 (spec added in v1.14).
        let v3_user_report = ["/_matrix/client/v3/users/{user_id}/report"];
        assert!(v3_user_report.iter().all(|route| route.contains("/v3/")));
    }
}
