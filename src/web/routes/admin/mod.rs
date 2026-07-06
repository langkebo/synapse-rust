pub mod audit;
pub mod cleanup;
pub mod federation;
pub mod media;
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
pub use cleanup::create_cleanup_router;
pub use federation::create_federation_router;
pub use media::create_media_router;
pub use notification::create_notification_router;
pub use register::create_register_router;
pub use report::create_report_router;
pub use retention::create_retention_router;
pub use security::create_security_router;
pub use server::create_server_router;
pub use token::create_token_router;
pub use user::create_user_router;

pub(crate) fn ensure_super_admin_for_privilege_change(
    admin: &crate::web::routes::AdminUser,
) -> Result<(), crate::common::ApiError> {
    if admin.role != "super_admin" {
        return Err(crate::common::ApiError::forbidden("Only super_admin can perform this operation".to_string()));
    }
    Ok(())
}

pub fn create_admin_module_router(state: AppState) -> Router<crate::web::routes::AppState> {
    #[allow(unused_mut)]
    let mut admin_router = Router::new()
        .merge(create_audit_router())
        .merge(create_user_router())
        .merge(create_server_router(state.clone()))
        .merge(create_security_router())
        .merge(create_cleanup_router(state.clone()))
        .merge(create_notification_router());
    let protected = admin_router
        .merge(create_token_router())
        .merge(create_federation_router())
        .merge(create_media_router())
        .merge(create_report_router())
        .merge(create_retention_router())
        .merge(room::create_room_router(state.clone()))
        .route("/_synapse/admin/info", axum::routing::get(server::get_admin_info))
        .route_layer(middleware::from_fn_with_state(state.clone(), crate::web::middleware::admin_auth_middleware));

    Router::new().merge(protected).merge(create_register_router(state))
}

pub fn admin_module_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    let mut entries = Vec::new();
    entries.extend(audit::admin_audit_route_manifest());
    entries.extend(cleanup::admin_cleanup_route_manifest());
    entries.extend(federation::admin_federation_route_manifest());
    entries.extend(media::admin_media_route_manifest());
    entries.extend(notification::admin_notification_route_manifest());
    entries.extend(register::admin_register_route_manifest());
    entries.extend(report::admin_report_route_manifest());
    entries.extend(retention::admin_retention_route_manifest());
    entries.extend(room::admin_room_route_manifest());
    entries.extend(security::admin_security_route_manifest());
    entries.extend(server::admin_server_route_manifest());
    entries.extend(token::admin_token_route_manifest());
    entries.extend(user::admin_user_route_manifest());
    entries
}
