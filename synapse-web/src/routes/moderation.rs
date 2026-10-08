use crate::routes::{get_scanner_info, report_event, report_room, report_user, update_report_score, AppState};
use axum::{
    routing::{get, post, put},
    Router,
};

/// 标准端点：`POST /rooms/{room_id}/report/{event_id}`（CS 规范内），v1 与 v3 共用。
fn create_room_report_standard_router() -> Router<AppState> {
    Router::new().route("/rooms/{room_id}/report/{event_id}", post(report_event))
}

/// 私有扩展：唯一规范位置是 `/_matrix/vendor/v1`（ISSUE-13）。
///
/// 此前这两条挂在 `/_matrix/client/{v1,v3}` 下（`score` 在 v1/v3 各挂一份），
/// 已按 AGENTS.md 铁律 1 迁移，**不留 client 别名**。注意 `/score` 的 v1/v3
/// 两份合为 vendor 下的一条（vendor 只有一个版本），故注册条目净 −1。
fn create_moderation_vendor_router() -> Router<AppState> {
    Router::new()
        .route("/rooms/{room_id}/report/{event_id}/score", put(update_report_score))
        .route("/rooms/{room_id}/report/{event_id}/scanner_info", get(get_scanner_info))
}

fn create_moderation_v1_router() -> Router<AppState> {
    create_room_report_standard_router()
}

fn create_moderation_v3_router() -> Router<AppState> {
    create_room_report_standard_router()
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
        .nest("/_matrix/vendor/v1", create_moderation_vendor_router())
}
