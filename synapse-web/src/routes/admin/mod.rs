/// The `audit` module.
pub mod audit;
/// The `cleanup` module.
pub mod cleanup;
/// The `federation` module.
pub mod federation;
/// The `invite_lists` module.
pub mod invite_lists;
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

#[cfg(test)]
mod dto_guard_tests;

use crate::routes::AppState;
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
    admin: &crate::routes::AdminUser,
) -> Result<(), synapse_common::ApiError> {
    if admin.role != "super_admin" {
        return Err(synapse_common::ApiError::forbidden("Only super_admin can perform this operation".to_string()));
    }
    Ok(())
}

/// 目标是**管理员**时，要求操作者是 super_admin。
///
/// 专用于"改密码"这类跨级敏感操作：普通 admin 给普通用户重置密码是合理运维，但目标
/// 一旦本身是管理员，改密码就等于接管其账号——`/login` 会按目标用户的 `is_admin`
/// 签发 token，而 `should_require_admin_mfa` 只覆盖 `/_synapse/admin/**`，不拦 `/login`。
/// 见 `docs/audit/ADMIN_RBAC_SURFACE_2026-10-21.md` §2。
pub(crate) fn ensure_super_admin_when_target_is_admin(
    target_is_admin: bool,
    admin: &crate::routes::AdminUser,
) -> Result<(), synapse_common::ApiError> {
    if target_is_admin {
        ensure_super_admin_for_privilege_change(admin)
    } else {
        Ok(())
    }
}

/// 同 [`ensure_super_admin_when_target_is_admin`]，但按 `user_id` 自行查一次 `is_admin`。
///
/// 用于只拿到路径参数、没有现成用户记录的 handler（吊销 refresh token、shadow ban）。
/// 目标不存在时返回 `Ok`——"用户不存在"由调用方按自己的语义（404 或幂等成功）处理，
/// 这里只回答"权限够不够"。
pub(crate) async fn ensure_super_admin_when_target_user_is_admin(
    ctx: &crate::routes::context::AdminContext,
    target_user_id: &str,
    admin: &crate::routes::AdminUser,
) -> Result<(), synapse_common::ApiError> {
    let target_is_admin = ctx.user_service.get_user(target_user_id).await?.is_some_and(|user| user.is_admin);
    ensure_super_admin_when_target_is_admin(target_is_admin, admin)
}

/// See [`create_admin_module_router`].
pub fn create_admin_module_router(state: AppState) -> Router<crate::routes::AppState> {
    let admin_router = Router::new()
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
        .merge(create_policy_router())
        .merge(room::create_room_router(state.clone()))
        .route("/_synapse/admin/info", axum::routing::get(server::get_admin_info))
        .route_layer(middleware::from_fn_with_state(
            <crate::routes::context::AdminContext as axum::extract::FromRef<crate::routes::AppState>>::from_ref(&state),
            crate::middleware::admin_auth_middleware,
        ));

    Router::new().merge(protected).merge(create_register_router(state))
}
