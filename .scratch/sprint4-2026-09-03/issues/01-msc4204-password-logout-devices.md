# 01: MSC4204 改密默认吊销全部设备 (P0, 1d)

**What to build:** 实现 Matrix v1.3 规范的 `logout_devices` 字段语义：POST /account/password 成功改密后**默认**吊销用户所有 device access token 与 refresh token。客户端可通过 `auth.logout_devices: false` 保留当前设备（向后兼容 Element Web 旧版本期望）。

**Blocked by:** None

**Status:** ready-for-agent

**Spec reference:** https://spec.matrix.org/v1.3/client-server-api/#post_matrixclientv3accountpassword
> `logout_devices` — Defaults to `true`. Whether the user should be logged out of all their other sessions. The current session is included as one of the sessions to log out unless `logout_devices` is `false`.

**现状（已调研）:**
- 路由：`POST /account/password` → `change_password_uia` (`src/web/routes/account_compat.rs:230`)
- service: `registration_service.change_password` (`synapse-services/src/registration_service.rs:179`) → `credential_auth.change_password` → `AuthService::change_password` (`synapse-services/src/auth/account.rs:7`)
- **缺口**: `account_compat.rs:230-300` 完全没有解析 `auth.logout_devices` 字段，行为是"传 `device_id` 就保留当前设备，否则全吊销"——语义与 v1.3 规范**颠倒**。
- 现有吊销逻辑 (`account.rs:41-61`) 正确但**没接到 spec 字段**上。
- `m.change_password` capability boolean (`src/web/routes/handlers/versions.rs:118`) 已实现。

**实现计划（acceptance criteria）:**
- [ ] `account_compat.rs:change_password_uia` 解析 `auth.logout_devices: bool`（默认 `true`）
- [ ] `AuthService::change_password` 签名加 `logout_devices: bool` 参数（破坏性变更：所有 caller 必须更新）
- [ ] `logout_devices=true` → 走 `delete_user_tokens` + `revoke_all_user_tokens`（现有 line 52-60 路径）
- [ ] `logout_devices=false` → 走 `delete_user_tokens_except_device` + `revoke_all_user_tokens_except_device`（现有 line 41-50 路径，**current_device_id 必传**）
- [ ] `invalidate_revocation_ok_for_user` 在两条路径都执行（line 72 已正确）
- [ ] `api_doc/auth.rs:change_password_doc` 更新 OpenAPI 文档补 `auth.logout_devices` 字段
- [ ] 现有 caller 全部更新（grep 找 `\.change_password\(`）：
  - `src/web/routes/account_compat.rs:296` (UIA 流)
  - `src/web/routes/admin/user.rs:475` (admin 重置密码场景，**必须 logout_devices=true** 强制)
- [ ] 集成测试（`tests/integration/auth_service_coverage_tests.rs` 或新建）：
  - [ ] 改密默认吊销全部（用 A 设备登录改密，B 设备 token 失效）
  - [ ] 改密 + `logout_devices=false` 保留当前设备
  - [ ] 改密 + `logout_devices=true` 吊销当前设备
- [ ] `cargo build --locked` 通过
- [ ] `cargo clippy --all-features --locked -- -D warnings` 零警告
- [ ] 提交 commit `feat(auth): MSC4204 default logout all devices on password change`

**风险点:**
- ⚠️ 行为反转可能影响旧 Element Web 客户端（旧版期望默认保留当前设备）。Mitigation: 默认 true 是 v1.3 标准，旧客户端要么已更新，要么用户主动传 `logout_devices: false`。
- ⚠️ Admin 改密场景 (`admin/user.rs:475`) 当前**没有**传 `device_id` 走全吊销路径——这次改完仍是 logout_devices=true 全吊销，行为不变，只是显式。
- ⚠️ `current_device_id` 为 None + `logout_devices=false` 的矛盾组合需要明确报错（spec 没明说，但 400 比较合理）。

**估算:** 1 人天
