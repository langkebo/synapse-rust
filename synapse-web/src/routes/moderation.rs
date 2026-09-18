use crate::routes::{get_scanner_info, report_event, report_room, report_user, update_report_score, AppState};
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
