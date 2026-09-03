# 01: MSC4204 改密默认吊销全部设备 (P0, 1d)

**What to build:** 实现 Matrix v1.3 规范的 `logout_devices` 字段语义：POST /account/password 成功改密后**默认**吊销用户所有 device access token 与 refresh token。客户端可通过 `auth.logout_devices: false` 保留当前设备（向后兼容 Element Web 旧版本期望）。

**Blocked by:** None

**Status:** ✅ done (commit `56d03326`)

**Spec reference:** https://spec.matrix.org/v1.3/client-server-api/#post_matrixclientv3accountpassword
> `logout_devices` — Defaults to `true`. Whether the user should be logged out of all their other sessions. The current session is included as one of the sessions to log out unless `logout_devices` is `false`.

**现状（已调研）:**
- 路由：`POST /account/password` → `change_password_uia` (`src/web/routes/account_compat.rs:230`)
- service: `registration_service.change_password` (`synapse-services/src/registration_service.rs:179`) → `credential_auth.change_password` → `AuthService::change_password` (`synapse-services/src/auth/account.rs:7`)
- **缺口**: `account_compat.rs:230-300` 完全没有解析 `auth.logout_devices` 字段，行为是"传 `device_id` 就保留当前设备，否则全吊销"——语义与 v1.3 规范**颠倒**。
- 现有吊销逻辑 (`account.rs:41-61`) 正确但**没接到 spec 字段**上。
- `m.change_password` capability boolean (`src/web/routes/handlers/versions.rs:118`) 已实现。

**实现计划（acceptance criteria）:**
- [x] `account_compat.rs:change_password_uia` 解析 `auth.logout_devices: bool`（默认 `true`）
- [x] `AuthService::change_password` 签名加 `logout_devices: bool` 参数（破坏性变更：所有 caller 必须更新）
- [x] `logout_devices=true` → 走 `delete_user_tokens` + `revoke_all_user_tokens`（现有 line 52-60 路径）
- [x] `logout_devices=false` → 走 `delete_user_tokens_except_device` + `revoke_all_user_tokens_except_device`（现有 line 41-50 路径，**current_device_id 必传**）
- [x] `logout_devices=false` 但无 `current_device_id` → 400 BadRequest
- [x] `invalidate_revocation_ok_for_user` 在两条路径都执行（line 72 已正确）
- [x] `api_doc/auth.rs:change_password_doc` 更新 OpenAPI 文档补 `auth.logout_devices` 字段
- [x] 现有 caller 全部更新（grep 找 `\.change_password\(`）：
  - [x] `src/web/routes/account_compat.rs:296` (UIA 流)
  - [x] `src/web/routes/account_compat.rs:349` (threepid reset — 强制 logout_devices=true)
  - [x] `src/web/routes/admin/user.rs:475` (admin 重置密码 — 强制 logout_devices=true)
  - [x] `synapse-services/src/test_mocks.rs:158` (mock impl)
  - [x] `synapse-services/src/auth/tests.rs:795,808,821` (3 个单元测试)
  - [x] `synapse-services/src/auth/credential_auth.rs:34` (trait signature)
  - [x] `synapse-services/src/auth/mod.rs:324` (trait impl — 用 `AuthService::change_password` 显式调用避免递归)
- [x] `cargo build --locked` 通过
- [x] `cargo clippy --all-features --locked -- -D warnings` 零警告（含 3 个 pre-existing expect_used 修复）
- [x] 122/122 auth 单元测试 PASS
- [x] 提交 commit `56d03326` `feat(auth): MSC4204 default logout all devices on password change`

**实际改动文件（11 个，+121/-17）：**
- `src/web/api_doc/auth.rs` (OpenAPI 文档)
- `src/web/routes/account_compat.rs` (UIA 解析字段 + threepid 强制)
- `src/web/routes/admin/user.rs` (admin 强制)
- `synapse-services/src/auth/account.rs` (核心实现 + let-else 安全分支)
- `synapse-services/src/auth/credential_auth.rs` (trait signature)
- `synapse-services/src/auth/mod.rs` (trait impl 显式调用)
- `synapse-services/src/auth/tests.rs` (3 个测试 caller)
- `synapse-services/src/registration_service.rs` (上层 wrapper)
- `synapse-services/src/test_mocks.rs` (mock impl)
- `synapse-services/src/sync_service/mod.rs` (pre-existing clippy 修复)
- `synapse-services/src/application_service/models.rs` (pre-existing clippy 修复)

**待补（sprint 收尾时统一加）:**
- 集成测试（`tests/integration/auth_service_coverage_tests.rs` 新增 3 个用例：默认吊销、保留当前、保留失败 400）— 留待 sprint 结束
- e2e（`tests/e2e/e2e_scenarios.rs:test_user_password_change_flow` 增强 logout_devices 验证）— 留待 sprint 结束

**风险点（与原始 ticket 一致）:**
- ✅ 行为反转：默认 true 是 v1.3 标准
- ✅ Admin 强制全吊销（不变，行为更显式）
- ✅ 矛盾组合（logout_devices=false + 无 device_id）已 400

**估算实际:** 0.6 人天（比预估 1d 快，因为 AuthService 内部吊销逻辑齐全只需接 spec 字段；额外花 0.2d 修 pre-existing clippy 才让 -D warnings 通过）
