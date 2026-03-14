pub mod user;
pub mod room;
pub mod server;
pub mod security;
pub mod notification;
pub mod token;
pub mod federation;
pub mod media;
pub mod report;
pub mod retention;

use axum::Router;
use crate::web::routes::AppState;

pub use user::create_user_router;
pub use room::create_room_router;
pub use server::create_server_router;
pub use security::create_security_router;
pub use notification::create_notification_router;
pub use token::create_token_router;
pub use federation::create_federation_router;
pub use media::create_media_router;
pub use report::create_report_router;
pub use retention::create_retention_router;

pub fn create_admin_module_router(state: AppState) -> Router<AppState> {
    Router::new()
        .merge(create_user_router(state.clone()))
        .merge(create_room_router(state.clone()))
        .merge(create_server_router(state.clone()))
        .merge(create_security_router(state.clone()))
        .merge(create_notification_router(state.clone()))
        .merge(create_token_router(state.clone()))
        .merge(create_federation_router(state.clone()))
        .merge(create_media_router(state.clone()))
        .merge(create_report_router(state.clone()))
        .merge(create_retention_router(state.clone()))
}
