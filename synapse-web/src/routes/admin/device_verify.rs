//! 交叉签名单设备验证：`POST /_synapse/admin/v1/users/{user_id}/devices/{device_id}/verify`。
//!
//! 独立成模块（而不是塞进 `admin/user.rs`）有两个原因：
//! * `admin/user.rs` 已贴住行数棘轮上限（1139），塞一个 handler 进去立即判红；
//! * 这条路由的语义是"读设备 + 读签名表 + 写一条审计行"，不是用户管理的基本操作。
//!
//! 为什么是 **POST 而不是 GET**：`E2eeAuditService::verify_device_for_user` 会写一行
//! `e2ee_audit_log`（`operation = "verify_device"`）。做成 GET 的话，管理面板一刷新就多一条
//! 审计记录，审计表会被"读操作"撑大，反而让真正的验证动作淹没在里面。

use axum::{
    extract::{Path, State},
    routing::post,
    Json, Router,
};
use synapse_common::types::DeviceId;
use synapse_common::ApiError;
use synapse_services::e2ee_audit::DeviceVerificationStatus;

use crate::routes::context::AdminContext;
use crate::routes::AdminUser;

/// See [`create_device_verify_router`].
pub fn create_device_verify_router() -> Router<crate::routes::AppState> {
    Router::new().route("/_synapse/admin/v1/users/{user_id}/devices/{device_id}/verify", post(verify_user_device_admin))
}

/// See [`verify_user_device_admin`].
///
/// 返回 [`DeviceVerificationStatus`]：`signature_valid`（设备签名表里有没有这个设备的签名）
/// × `is_cross_signed`（该用户有没有 `self_signing` 交叉签名密钥）⇒ `is_verified`。
///
/// 不额外写 `admin_audit` 行：设备级验证结果已经由 `e2ee_audit_log` 承载（含 `device_id`
/// 与三项布尔细节），再记一条只会让同一动作在两份审计里各出现一次。
#[axum::debug_handler]
pub async fn verify_user_device_admin(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
    Path((user_id, device_id)): Path<(String, DeviceId)>,
) -> Result<Json<DeviceVerificationStatus>, ApiError> {
    // 先按路径里的 `user_id` 解析用户：用户不存在 ⇒ 404，且后续只在该用户的设备表里找设备
    // （设备 id 全局唯一，但不用它直接定位 —— 否则路径与响应里的 owner 可能对不上）。
    let user = ctx.user_service.get_user_or_not_found(&user_id).await?;
    let status = ctx.e2ee_audit_service.verify_device_for_user(&user.user_id, device_id.as_str()).await?;

    Ok(Json(status))
}
