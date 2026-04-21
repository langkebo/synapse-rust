pub mod audit;
pub mod federation;
pub mod media;
#[cfg(feature = "server-notifications")]
pub mod notification;
pub mod register;
pub mod report;
pub mod retention;
pub mod room;
pub mod security;
pub mod server;
pub mod token;
pub mod user;

use crate::web::routes::AppState;
use axum::middleware;
use axum::Router;

pub use audit::create_audit_router;
pub use federation::create_federation_router;
pub use media::create_media_router;
#[cfg(feature = "server-notifications")]
pub use notification::create_notification_router;
pub use register::create_register_router;
pub use report::create_report_router;
pub use retention::create_retention_router;
pub use room::create_room_router;
pub use security::create_security_router;
pub use server::create_server_router;
pub use token::create_token_router;
pub use user::create_user_router;

pub(crate) fn ensure_super_admin_for_privilege_change(
    admin: &crate::web::routes::AdminUser,
) -> Result<(), crate::common::ApiError> {
    if admin.role != "super_admin" {
        return Err(crate::common::ApiError::forbidden(
            "Only super_admin can perform this operation".to_string(),
        ));
    }
    Ok(())
}

pub fn create_admin_module_router(state: AppState) -> Router<AppState> {
    let mut admin_router = Router::new()
        .merge(create_audit_router(state.clone()))
        .merge(create_user_router(state.clone()))
        .merge(create_room_router(state.clone()))
        .merge(create_server_router(state.clone()))
        .merge(create_security_router(state.clone()));
    #[cfg(feature = "server-notifications")]
    { admin_router = admin_router.merge(create_notification_router(state.clone())); }
    let protected = admin_router
        .merge(create_token_router(state.clone()))
        .merge(create_federation_router(state.clone()))
        .merge(create_media_router(state.clone()))
        .merge(create_report_router(state.clone()))
        .merge(create_retention_router(state.clone()))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            crate::web::middleware::admin_auth_middleware,
        ));

    Router::new()
        .route(
            "/_synapse/admin/info",
            axum::routing::get(server::get_admin_info),
        )
        .merge(protected)
        .merge(create_register_router(state.clone()))
}
