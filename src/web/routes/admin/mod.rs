/// The `audit` module.
pub mod audit;
/// The `cleanup` module.
pub mod cleanup;
/// The `federation` module.
pub mod federation;
/// The `media` module.
pub mod media;
/// The `notification` module.
pub mod notification;
/// The `policy` module.
pub mod policy;
/// The `register` module.
pub mod register;
/// The `report` module.
pub mod report;
/// The `retention` module.
pub mod retention;
/// The `room` module.
pub mod room;
/// The `security` module.
pub mod security;
/// The `server` module.
pub mod server;
/// The `token` module.
pub mod token;
/// The `user` module.
pub mod user;

use crate::web::routes::AppState;
use axum::middleware;
use axum::Router;

pub use audit::create_audit_router;
pub use cleanup::create_cleanup_router;
pub use federation::create_federation_router;
pub use media::create_media_router;
pub use notification::create_notification_router;
pub use policy::create_policy_router;
pub use register::create_register_router;
pub use report::create_report_router;
pub use retention::create_retention_router;
pub use security::create_security_router;
pub use server::create_server_router;
pub use token::create_token_router;
pub use user::create_user_router;

/// See [`ensure_super_admin_for_privilege_change`].
pub(crate) fn ensure_super_admin_for_privilege_change(
    admin: &crate::web::routes::AdminUser,
) -> Result<(), crate::common::ApiError> {
    if admin.role != "super_admin" {
        return Err(crate::common::ApiError::forbidden("Only super_admin can perform this operation".to_string()));
    }
    Ok(())
}

/// See [`create_admin_module_router`].
pub fn create_admin_module_router(state: AppState) -> Router<crate::web::routes::AppState> {
    #[allow(unused_mut)]
    let mut admin_router = Router::new()
        .merge(create_audit_router())
        .merge(create_user_router())
        .merge(create_server_router(state.clone()))
        .merge(create_security_router())
        .merge(create_cleanup_router(state.clone()))
        .merge(create_notification_router());
    let protected =
        admin_router
            .merge(create_token_router())
            .merge(create_federation_router())
            .merge(create_media_router())
            .merge(create_report_router())
            .merge(create_retention_router())
            .merge(create_policy_router())
            .merge(room::create_room_router(state.clone()))
            .route("/_synapse/admin/info", axum::routing::get(server::get_admin_info))
            .route_layer(
                middleware::from_fn_with_state(
                    <crate::web::routes::context::AdminContext as axum::extract::FromRef<
                        crate::web::routes::AppState,
                    >>::from_ref(&state),
                    crate::web::middleware::admin_auth_middleware,
                ),
            );

    Router::new().merge(protected).merge(create_register_router(state))
}

/// See [`admin_module_route_manifest`].
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
    entries.extend(policy::admin_policy_route_manifest());
    entries
}
