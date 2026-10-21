//! 「提权端点」判定：普通 admin 不得调用的写操作。
//!
//! 从 `admin_auth.rs` 拆出——该文件已压在 625 行的棘轮基线上，规则每加一条就越线。
//! 判定语义不变：仍是 `is_role_allowed` 在 `match role` **之前**调用的黑名单，
//! 命中即「仅 super_admin」。
//!
//! 两批来源：
//! + 2026-10-05 P1-1：路由与匹配串错位导致的漏网（devices 批量删除）；
//! + 2026-10-21 RBAC 审计：原来落在宽前缀 allowlist 里、被普通 admin 顺带获得的
//!   21 条写操作，逐组理由见 `docs/audit/ADMIN_RBAC_SURFACE_2026-10-21.md` §3。

use axum::http::Method;

/// See [`is_role_allowed`](super::is_role_allowed).
pub(super) fn is_super_admin_only_endpoint(method: &Method, path: &str) -> bool {
    let is_write = matches!(*method, Method::POST | Method::PUT | Method::PATCH | Method::DELETE);

    // Setting another user's admin status
    if path.ends_with("/admin") && is_write {
        return true;
    }
    if path.contains("/make_admin") {
        return true;
    }

    // Batch user creation. `batch_deactivate` is deliberately allowed for plain
    // admin, so only the exact `/batch` path is matched.
    if path == "/_synapse/admin/v1/users/batch" {
        return true;
    }

    // Batch device deletion. P1-1 (2026-10-05): the real routes are
    // `…/devices/delete` and `…/devices/{device_id}/delete`; the old
    // `/delete_devices` pattern never matched, so plain admin slipped through.
    if path.ends_with("/devices/delete") || (path.contains("/devices/") && path.ends_with("/delete")) {
        return true;
    }

    if path.contains("/purge") || path.contains("/retention") {
        return true;
    }
    if path == "/_synapse/admin/info" || path.contains("/send_server_notice") {
        return true;
    }
    if path.contains("/registration_tokens") && is_write {
        return true;
    }
    if path == "/_synapse/admin/v1/federation/resolve"
        || path.contains("/federation/blacklist")
        || path == "/_synapse/admin/v1/federation/cache/clear"
    {
        return true;
    }

    // ── 2026-10-21 RBAC 审计收紧：以下写操作以前被宽前缀 allowlist 顺带放行 ──────

    // 应用服务：`as_token` 能以任意用户身份发事件 ⇒ 增删改与代发等同后门。
    // `/ping` 是健康检查（POST 但只读），保留给普通 admin。
    if path.starts_with("/_synapse/admin/v1/appservices") && !path.ends_with("/ping") && is_write {
        return true;
    }

    // 模块：可加载代码并介入消息流；`/check_*` 是只读探测，保留。
    if path.starts_with("/_synapse/admin/v1/modules")
        && !path.ends_with("/check_spam")
        && !path.ends_with("/check_third_party_rule")
        && is_write
    {
        return true;
    }

    // 功能开关：能关掉安全相关特性。
    if path.starts_with("/_synapse/admin/v1/feature-flags") && is_write {
        return true;
    }

    // SSO 配置：改错可致全站登不上，改对可放行未授权身份。
    if path == "/_synapse/admin/v1/saml/config" && is_write {
        return true;
    }

    // 全站账号有效期：到期停用全部账号（单人 `…/{user_id}/renew` 保留）。
    if path == "/_synapse/admin/v1/account_validity" && is_write {
        return true;
    }

    // 不可逆的全局清理 / 批量删除。
    if path == "/_synapse/admin/v1/cleanup/all"
        || path == "/_synapse/admin/v1/media/delete"
        || path == "/_synapse/admin/v1/rooms/cleanup"
    {
        return true;
    }

    // ── 2026-10-21 第二批（A 级 30 条的裁决，见同一文档 §4）──────────────
    // 只收紧「与已收紧的同族」两类：全局集成配置（凭据/放行身份）与不可逆清理。

    // CAS 服务是 SSO 集成配置：改错可致全站登不上，改对可放行未授权身份
    // ——与 `saml/config` 同一类。用户属性（`cas/users/{id}/attributes`）可逆，保留。
    if path.starts_with("/_synapse/admin/v1/cas/services") && is_write {
        return true;
    }

    // 外部服务带 `webhook_url` + `api_key`：可把事件外发到任意地址、也可伪造回调
    // ——与 `appservices` 同一类后门。`/{as_id}/health/check` 只是探测，保留。
    if path.starts_with("/_synapse/admin/v1/external_services") && is_write && !path.ends_with("/health/check") {
        return true;
    }

    // SAML 身份映射：决定 IdP 的 name_id 落到哪个本地账号，改对即可放行他人身份。
    // 同族的 `saml/config` 已收紧，这里保持一致。
    if path.starts_with("/_synapse/admin/v1/saml/mapping/") && is_write {
        return true;
    }

    // `cleanup/all` 收紧后仍留着它的破坏性主体 `cleanup/rooms`（删异常房间数据），
    // 等于没收紧——普通 admin 直接调组成端点即可。`cleanup/tokens` 只清过期令牌，保留。
    if path == "/_synapse/admin/v1/cleanup/rooms" {
        return true;
    }

    // 媒体回调：把媒体事件外发到注册的 URL，与外部服务同类。
    if path == "/_synapse/admin/v1/media_callbacks" && is_write {
        return true;
    }

    // 级联撤回、关闭房间、删房间：不可逆且影响全体成员。
    if path.ends_with("/cascade_redact") || path == "/_synapse/admin/v1/shutdown_room" {
        return true;
    }
    // `rooms/{room_id}` 恰好 5 个斜杠；带子路径的（`/ban`、`/media/…`、`/members/…`）不在此列。
    // `rooms/search` 也是 5 个斜杠，但它是只读检索（POST 只是携带查询体），必须排除——
    // 第一版漏了这条，实测把普通 admin 的搜索能力一起收走了。
    if is_write
        && path.starts_with("/_synapse/admin/v1/rooms/")
        && path.matches('/').count() == 5
        && !path.ends_with("/search")
    {
        return true;
    }

    false
}
