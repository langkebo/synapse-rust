use crate::utils::auth::resolve_request_id;
use axum::http::{HeaderMap, Method};
use hmac::{Hmac, KeyInit, Mac};
use serde_json::json;
use sha1::Sha1;
use std::time::{SystemTime, UNIX_EPOCH};
use synapse_common::config::SecurityConfig;
use synapse_common::crypto::secure_compare;
use synapse_common::ApiError;
use synapse_services::account::UserService;
use synapse_services::admin_audit_service::CreateAuditEventRequest;
use synapse_services::user_service::User;

mod allowlist;
mod privileged;

use privileged::is_super_admin_only_endpoint;

type HmacSha1 = Hmac<Sha1>;

/// The `AuthorizedAdmin` struct.
#[derive(Clone, Debug)]
pub(crate) struct AuthorizedAdmin {
    /// The `user_id` field.
    pub user_id: String,
    /// The `device_id` field.
    pub device_id: Option<String>,
    /// The `access_token` field.
    pub access_token: String,
    /// The `role` field.
    pub role: String,
}

/// Variant of `authorize_admin_request` that works with individual service
/// references instead of `&AppState`. Used by typed-context `FromRequestParts`
/// implementations.
pub(crate) async fn authorize_admin_from_services(
    auth_service: &(dyn synapse_services::auth::TokenAuth + Send + Sync),
    user_service: &UserService,
    security: &synapse_common::config::SecurityConfig,
    admin_audit_service: Option<&synapse_services::admin::AdminAuditService>,
    headers: &HeaderMap,
    method: &Method,
    path: &str,
) -> Result<AuthorizedAdmin, ApiError> {
    let access_token = super::auth::bearer_token(headers)?;
    let (user_id, device_id, is_admin, _, _, _): (String, Option<String>, bool, bool, bool, Option<String>) =
        auth_service.validate_token(&access_token).await?;

    if !is_admin {
        return Err(ApiError::forbidden("Admin access required".to_string()));
    }

    let user = user_service
        .get_user(&user_id)
        .await?
        .ok_or_else(|| ApiError::unauthorized("Admin user not found".to_string()))?;

    if !user.is_admin {
        return Err(ApiError::forbidden("Admin access has been revoked".to_string()));
    }

    let role = normalize_admin_role(user.user_type.as_deref());
    let allowed = is_role_allowed(&role, method, path);

    let rbac_enabled = security.admin_rbac_enabled;
    let rbac_allowed = !rbac_enabled || allowed;

    // OBS-04 (P2): RBAC 日志需携带 request_id，便于与 audit 落库和 span 链路串联。
    let request_id = resolve_request_id(headers);
    ::tracing::info!(
        target: "security_audit",
        request_id = %request_id,
        role = %role,
        method = %method,
        path = %path,
        allowed = %allowed,
        rbac_enabled = %rbac_enabled,
        rbac_allowed = %rbac_allowed,
        "RBAC check result"
    );

    if let Some(audit_svc) = admin_audit_service {
        let audit_request = CreateAuditEventRequest {
            actor_id: user_id.clone(),
            action: format!("admin.{}", method.as_str().to_lowercase()),
            resource_type: "admin_api".to_string(),
            resource_id: path.to_string(),
            result: if rbac_allowed { "success".to_string() } else { "denied".to_string() },
            request_id,
            details: Some(json!({
                "role": role,
                "path": path,
                "method": method.as_str(),
            })),
        };
        if let Err(e) = audit_svc.create_event(audit_request).await {
            ::tracing::error!(target: "security_audit", "Failed to create audit event: {}", e);
        }
    }

    if !rbac_allowed {
        return Err(ApiError::forbidden(format!("Admin role '{role}' is not allowed to access this resource")));
    }

    if should_require_admin_mfa(security, method, path) {
        let mfa_code = headers
            .get("x-admin-mfa-code")
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                ApiError::forbidden(
                "Sensitive admin operation requires MFA code: pass a 6-digit TOTP code in the 'x-admin-mfa-code' header"
                    .to_string(),
            )
            })?;

        verify_totp_code(security, mfa_code, Some(&user))?;
    }

    Ok(AuthorizedAdmin { user_id, device_id, access_token, role })
}

/// Variant that works with individual service references instead of &AppState.
pub(crate) async fn enforce_admin_login_mfa_svc(
    security: &SecurityConfig,
    user_service: &UserService,
    username: &str,
    mfa_code: Option<&str>,
) -> Result<(), ApiError> {
    enforce_admin_login_mfa_impl(security, user_service, username, mfa_code).await
}

async fn enforce_admin_login_mfa_impl(
    security: &SecurityConfig,
    user_service: &UserService,
    username: &str,
    mfa_code: Option<&str>,
) -> Result<(), ApiError> {
    if !security.admin_mfa_required {
        return Ok(());
    }

    let Some(user) = user_service.get_user_by_identifier(username).await? else {
        return Ok(());
    };

    if !user.is_admin {
        return Ok(());
    }

    let mfa_code = mfa_code.map(str::trim).filter(|value| !value.is_empty()).ok_or_else(|| {
        ApiError::forbidden(
            "Admin login requires MFA code: pass a 6-digit TOTP code in the 'mfa_code' field of the login request body"
                .to_string(),
        )
    })?;

    verify_totp_code(security, mfa_code, Some(&user))
}

/// C10：管理员未配置 `user_type` 时使用的角色。
///
/// 该值**不是**任何已识别角色，因此 [`is_role_allowed`] 会走 `_ => false` 分支，
/// 对所有受 RBAC 保护的端点一律拒绝（最小权限）。此前缺失 `user_type` 会静默降级
/// 为宽权限的 `"admin"`，等于给未分配角色的管理员隐式授权。
const NO_ADMIN_ROLE: &str = "none";

/// See [`normalize_admin_role`].
pub(crate) fn normalize_admin_role(user_type: Option<&str>) -> String {
    match user_type.map(str::trim).filter(|value| !value.is_empty()) {
        None => {
            ::tracing::warn!(
                target: "security_audit",
                event = "admin_role_missing",
                "Admin user has no user_type set - granting no RBAC role (least privilege). \
                 All RBAC-gated admin endpoints will be denied. Set user_type explicitly \
                 (e.g. 'admin' or 'super_admin') to assign privileges."
            );
            NO_ADMIN_ROLE.to_string()
        }
        Some("admin") => "admin".to_string(),
        Some("super_admin") => "super_admin".to_string(),
        Some(value) => value.to_ascii_lowercase(),
    }
}

/// See [`should_require_admin_mfa`].
pub(crate) fn should_require_admin_mfa(security: &SecurityConfig, method: &Method, path: &str) -> bool {
    security.admin_mfa_required
        && !security.admin_mfa_shared_secret.trim().is_empty()
        && is_sensitive_admin_request(method, path)
}

fn is_role_allowed(role: &str, method: &Method, path: &str) -> bool {
    if role == "super_admin" {
        return true;
    }

    // 黑名单**先于**角色匹配：所有角色（含 auditor / security_admin）都受它约束。
    if is_super_admin_only_endpoint(method, path) {
        return false;
    }

    allowlist::role_allows(role, method, path)
}

fn is_sensitive_admin_request(method: &Method, path: &str) -> bool {
    if matches!(*method, Method::POST | Method::PUT | Method::PATCH | Method::DELETE) {
        return true;
    }

    path.starts_with("/_synapse/admin/v1/security")
        || path.starts_with("/_synapse/admin/v1/server")
        || path.starts_with("/_synapse/admin/v1/media/quarantine")
}

fn verify_totp_code(security: &SecurityConfig, provided_code: &str, user: Option<&User>) -> Result<(), ApiError> {
    if !security.admin_mfa_required {
        return Ok(());
    }

    let secret = decode_secret(&security.admin_mfa_shared_secret).ok_or_else(|| {
        ApiError::forbidden("Admin MFA is enabled but no valid TOTP secret is configured".to_string())
    })?;

    let provided_code = provided_code.trim();
    if provided_code.len() != 6 || !provided_code.chars().all(|ch| ch.is_ascii_digit()) {
        return Err(ApiError::forbidden("Invalid admin MFA code format".to_string()));
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| ApiError::internal_with_cause("System time error", e))?
        .as_secs();
    let current_step = now / 30;
    let drift = security.admin_mfa_allowed_drift_steps as i64;

    for offset in -drift..=drift {
        let Some(step) = current_step.checked_add_signed(offset) else {
            continue;
        };

        if let Some(expected) = generate_totp_code(&secret, step) {
            if secure_compare(&expected, provided_code) {
                return Ok(());
            }
        }
    }

    let user_id = user.map_or("unknown", |value| value.user_id.as_str());
    tracing::warn!(target: "admin_auth", user_id, "Admin MFA verification failed");
    Err(ApiError::forbidden("Invalid admin MFA code".to_string()))
}

fn generate_totp_code(secret: &[u8], step: u64) -> Option<String> {
    let mut mac = HmacSha1::new_from_slice(secret).ok()?;
    mac.update(&step.to_be_bytes());
    let hash = mac.finalize().into_bytes();
    let offset = (hash[19] & 0x0f) as usize;
    let binary = ((u32::from(hash[offset]) & 0x7f) << 24)
        | (u32::from(hash[offset + 1]) << 16)
        | (u32::from(hash[offset + 2]) << 8)
        | u32::from(hash[offset + 3]);
    Some(format!("{:06}", binary % 1_000_000))
}

fn decode_secret(secret: &str) -> Option<Vec<u8>> {
    let trimmed = secret.trim();
    if trimmed.is_empty() {
        return None;
    }

    decode_base32_secret(trimmed).or_else(|| Some(trimmed.as_bytes().to_vec()))
}

fn decode_base32_secret(secret: &str) -> Option<Vec<u8>> {
    let mut bits = 0u32;
    let mut bit_count = 0u8;
    let mut output = Vec::new();

    for ch in secret.chars().filter(|ch| !matches!(ch, ' ' | '-')) {
        if ch == '=' {
            break;
        }

        let value = match ch.to_ascii_uppercase() {
            'A'..='Z' => ch.to_ascii_uppercase() as u8 - b'A',
            '2'..='7' => (ch as u8 - b'2') + 26,
            _ => return None,
        };

        bits = (bits << 5) | u32::from(value);
        bit_count += 5;
        while bit_count >= 8 {
            bit_count -= 8;
            output.push(((bits >> bit_count) & 0xff) as u8);
            bits &= (1 << bit_count) - 1;
        }
    }

    if output.is_empty() {
        None
    } else {
        Some(output)
    }
}

#[cfg(test)]
mod admin_rbac_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_without_user_type_gets_no_role_and_is_denied() {
        // C10: missing user_type must NOT silently grant the broad "admin" role.
        let role = normalize_admin_role(None);
        assert_eq!(role, NO_ADMIN_ROLE);
        assert_ne!(role, "admin");
        assert_ne!(role, "super_admin");

        // Least privilege: no role => every RBAC-gated endpoint is denied.
        assert!(!is_role_allowed(&role, &Method::GET, "/_synapse/admin/v1/users"));
        assert!(!is_role_allowed(&role, &Method::GET, "/_synapse/admin/v1/rooms"));
        assert!(!is_role_allowed(&role, &Method::GET, "/_synapse/admin/v1/audit"));
        assert!(!is_role_allowed(&role, &Method::POST, "/_synapse/admin/v1/users/@u:localhost/deactivate"));
        assert!(!is_role_allowed(&role, &Method::POST, "/_synapse/admin/v1/shutdown_room"));
    }

    #[test]
    fn admin_with_blank_user_type_gets_no_role() {
        assert_eq!(normalize_admin_role(Some("")), NO_ADMIN_ROLE);
        assert_eq!(normalize_admin_role(Some("   ")), NO_ADMIN_ROLE);
    }

    #[test]
    fn admin_role_restricted_endpoints_denied() {
        // Super admin only endpoints should be denied for admin role
        assert!(!is_role_allowed("admin", &Method::PUT, "/_synapse/admin/v1/users/@u:localhost/admin"));
        assert!(!is_role_allowed("admin", &Method::POST, "/_synapse/admin/v1/registration_tokens"));
        assert!(!is_role_allowed("admin", &Method::GET, "/_synapse/admin/info"));
        assert!(!is_role_allowed("admin", &Method::POST, "/_synapse/admin/v1/users/batch"));
        assert!(!is_role_allowed("admin", &Method::POST, "/_synapse/admin/v1/users/@u:localhost/devices/delete"));
        assert!(!is_role_allowed("admin", &Method::POST, "/_synapse/admin/v1/rooms/!room:localhost/purge"));
        assert!(!is_role_allowed("admin", &Method::PUT, "/_synapse/admin/v1/rooms/!room:localhost/retention"));
        assert!(!is_role_allowed("admin", &Method::POST, "/_synapse/admin/v1/rooms/!room:localhost/make_admin"));
        assert!(!is_role_allowed("admin", &Method::POST, "/_synapse/admin/v1/send_server_notice"));

        // Sensitive federation operations
        assert!(!is_role_allowed("admin", &Method::POST, "/_synapse/admin/v1/federation/resolve"));
        assert!(!is_role_allowed("admin", &Method::POST, "/_synapse/admin/v1/federation/blacklist/server.example.com"));
        assert!(!is_role_allowed("admin", &Method::POST, "/_synapse/admin/v1/federation/cache/clear"));
    }

    #[test]
    fn admin_role_allowed_management_endpoints() {
        // Admin should be able to deactivate users
        assert!(is_role_allowed("admin", &Method::POST, "/_synapse/admin/v1/users/@u:localhost/deactivate"));
        // Admin should be able to login as user
        assert!(is_role_allowed("admin", &Method::POST, "/_synapse/admin/v1/users/@u:localhost/login"));
        // Admin should be able to logout user
        assert!(is_role_allowed("admin", &Method::POST, "/_synapse/admin/v1/users/@u:localhost/logout"));
        // Admin should be able to shutdown rooms
        assert!(is_role_allowed("admin", &Method::POST, "/_synapse/admin/v1/shutdown_room"));
        // Admin should be able to delete rooms
        assert!(is_role_allowed("admin", &Method::DELETE, "/_synapse/admin/v1/rooms/!room:localhost"));
        // Admin should be able to reset user password
        assert!(is_role_allowed("admin", &Method::POST, "/_synapse/admin/v1/users/@u:localhost/password"));
        // Admin should be able to reset federation connection
        assert!(is_role_allowed(
            "admin",
            &Method::POST,
            "/_synapse/admin/v1/federation/destinations/server.example.com/reset_connection"
        ));
    }

    #[test]
    fn admin_role_allowed_endpoints() {
        // Admin should be able to read registration tokens
        assert!(is_role_allowed("admin", &Method::GET, "/_synapse/admin/v1/registration_tokens"));
        assert!(is_role_allowed("admin", &Method::GET, "/_synapse/admin/v1/federation/destinations"));
        assert!(is_role_allowed("admin", &Method::GET, "/_synapse/admin/v1/users"));
        assert!(is_role_allowed("admin", &Method::GET, "/_synapse/admin/v1/rooms"));
        // Admin should be able to access statistics
        assert!(is_role_allowed("admin", &Method::GET, "/_synapse/admin/v1/statistics"));
        assert!(is_role_allowed("admin", &Method::GET, "/_synapse/admin/v1/stats/users"));
        // Admin should be able to access spaces
        assert!(is_role_allowed("admin", &Method::GET, "/_synapse/admin/v1/spaces"));
        // Admin should be able to access event reports
        assert!(is_role_allowed("admin", &Method::GET, "/_synapse/admin/v1/event_reports"));
        // Admin should be able to block/unblock rooms
        assert!(is_role_allowed("admin", &Method::POST, "/_synapse/admin/v1/rooms/!room:localhost/block"));
    }

    #[test]
    fn admin_role_non_sensitive_read_allowed() {
        assert!(is_role_allowed("admin", &Method::GET, "/_synapse/admin/v1/users"));
        assert!(is_role_allowed("admin", &Method::GET, "/_synapse/admin/v1/rooms"));
    }

    #[test]
    fn super_admin_always_allowed() {
        assert!(is_role_allowed("super_admin", &Method::POST, "/_synapse/admin/v1/users/@u:localhost/deactivate"));
        assert!(is_role_allowed("super_admin", &Method::POST, "/_synapse/admin/v1/federation/cache/clear"));
        assert!(is_role_allowed("super_admin", &Method::POST, "/_synapse/admin/v1/registration_tokens"));
        assert!(is_role_allowed("super_admin", &Method::POST, "/_synapse/admin/v1/shutdown_room"));
    }

    #[test]
    fn admin_role_shadow_ban_allowed() {
        assert!(is_role_allowed("admin", &Method::POST, "/_synapse/admin/v1/users/@user:localhost/shadow_ban"));
        assert!(is_role_allowed("admin", &Method::DELETE, "/_synapse/admin/v1/users/@user:localhost/shadow_ban"));
    }

    #[test]
    fn admin_role_delete_single_device_allowed() {
        assert!(is_role_allowed("admin", &Method::DELETE, "/_synapse/admin/v1/users/@user:localhost/devices/DEVICEID"));
        assert!(is_role_allowed("admin", &Method::GET, "/_synapse/admin/v1/users/@user:localhost/devices"));
    }

    #[test]
    fn admin_role_batch_delete_devices_denied() {
        // P1-1: 使用真实注册路由断言（原先用不存在的 /delete_devices 路径，测不出绕过）。
        assert!(!is_role_allowed("admin", &Method::POST, "/_synapse/admin/v1/users/@user:localhost/devices/delete"));
        assert!(!is_role_allowed(
            "admin",
            &Method::POST,
            "/_synapse/admin/v1/users/@user:localhost/devices/DEVICEID/delete"
        ));
    }

    #[test]
    fn admin_role_evict_user_allowed() {
        assert!(is_role_allowed("admin", &Method::POST, "/_synapse/admin/v1/users/@user:localhost/evict"));
    }

    #[test]
    fn admin_role_rate_limit_allowed() {
        assert!(is_role_allowed("admin", &Method::GET, "/_synapse/admin/v1/users/@user:localhost/rate_limit"));
        assert!(is_role_allowed("admin", &Method::PUT, "/_synapse/admin/v1/users/@user:localhost/rate_limit"));
        assert!(is_role_allowed("admin", &Method::DELETE, "/_synapse/admin/v1/users/@user:localhost/rate_limit"));
    }

    /// RFC 6238 附录 B 的 TOTP 向量（HMAC-SHA1，密钥是 ASCII 的
    /// `12345678901234567890`）。规范值只取后 6 位，与 `generate_totp_code`
    /// 的输出对齐。
    ///
    /// 钉这条是因为 `hmac` 0.13 / `digest` 0.11 升级会同时改变两处：
    /// `new_from_slice` 从 `Mac` 移到 `KeyInit`，`finalize()` 的返回类型换成
    /// `hybrid_array::Array`。下面 `hash[19]` / `hash[offset]` 这类**按下标取值**
    /// 的写法如果被顺手改成 `.as_slice()` 的某种变形，编译能过、码值会错 ——
    /// 而这里算错就等于 2FA 拦不住人（或把管理员锁在门外）。
    #[test]
    fn totp_matches_rfc6238_vectors() {
        const SECRET: &[u8] = b"12345678901234567890";
        // (unix 时间, RFC 6238 的 8 位期望值, 本函数应输出的 6 位)
        //
        // ⚠️ RFC 6238 附录 B 最后一行把标签写成 `20000000000`，但它给的
        // `T = 0x0000000003F940AA`（= 66_666_666）实际对应 **2_000_000_000**：
        // 2e10/30 = 666_666_666 = 0x27BC86AA，是另一个 T。这里按 T 的**真实取值**
        // 取 unix 时间，并额外补一条 2e10 的行（该行 8 位值 65353130 已用
        // Python 的 hmac+sha1 独立复算过，与本实现一致）。照抄标签会让这条 KAT
        // 变成"记录错误实现"，正是它要防的东西。
        let cases = [
            (59u64, "94287082", "287082"),
            (1_111_111_109, "07081804", "081804"),
            (1_111_111_111, "14050471", "050471"),
            (1_234_567_890, "89005924", "005924"),
            (2_000_000_000, "69279037", "279037"),
            (20_000_000_000, "65353130", "353130"),
        ];
        for (unix_time, eight_digit, six_digit) in cases {
            // RFC 6238 的 T 是"30 秒步数"。
            let step = unix_time / 30;
            let code = generate_totp_code(SECRET, step).expect("HMAC-SHA1 accepts any key length");
            assert_eq!(code, six_digit, "TOTP at T={unix_time} (8-digit reference: {eight_digit})");
        }
    }
}
