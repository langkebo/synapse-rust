# A1: login_as_user 缺独立审计事件

**状态**：✅ DONE
**优先级**：🟡 中
**审计来源**：API 路由安全审计 #A1（2026-09-04）

## 问题描述

`src/web/routes/admin/user.rs` `login_as_user` 函数（line 583-616）让 admin 以任意用户身份生成 access token。

当前审计覆盖情况：
- ✅ `admin_auth_middleware` 会在 HTTP 层面记录该请求（`POST /_synapse/admin/v1/users/{user_id}/login` → `result: success`）
- ❌ 缺少**业务层面独立审计事件**（action = `admin.login_as_user`），导致：
  1. audit log 中 actor 是**目标用户**（因为 middleware 用最终有效的 token 溯源）
  2. 无法区分"admin 正常管理"和"admin 以某用户身份登录"
  3. 合规审计粒度不足

## 修复方案

在 `login_as_user` handler 返回成功 JSON 前，显式调用 `record_audit_event`，参数：
- `actor_id`: admin 自己的 user_id（`admin.user_id`）
- `action`: `"admin.login_as_user"`
- `resource_type`: `"user"`
- `resource_id`: target user_id
- `details`: `json!({ "target_user": &user.user_id, "target_is_admin": user.is_admin })`

参考 `record_audit_event` 签名（`admin/audit.rs:139`）。

## 验收标准

1. [x] `login_as_user` 返回 200 后，`audit_events` 表有 `action = 'admin.login_as_user'` 且 `actor_id` ≠ target user_id 的记录
2. [x] `details` 包含 `admin_role` / `target_user` / `target_is_admin` / `device_id`
3. [x] 审计写入失败不阻塞 API 返回（warn 兜底 + 内部已 warn）
4. [x] `cargo clippy --workspace --all-targets -D warnings` 零警告（49.16s）
5. [x] `cargo test --test integration api_admin_user_lifecycle_tests` 4/4 PASS

## 改动范围

```
src/web/routes/admin/user.rs                                                       # login_as_user 加 audit_event
tests/integration/api_admin_user_lifecycle_tests.rs                                # 新增 test_admin_login_as_user_writes_audit_event
```

## 验证日志

- `cargo build --locked` ✅ 3m57s
- `cargo clippy --workspace --features "test-utils privacy-ext voice-extended voip-tracking beacons server-notifications" --all-targets --locked -- -D warnings` ✅ 49.16s, 零警告
- `cargo test --test integration api_admin_user_lifecycle_tests` ✅ 4/4 (166s)

## 注意事项

- `admin` 参数从 `_admin` 改为 `admin`，因为现在要读 `admin.user_id` / `admin.role`
- handler 签名追加 `headers: HeaderMap` 提取 `request_id`
- `record_audit_event` 内部已 warn-兜底不会传播错误（外层再加 `tracing::warn!` 是 belt-and-suspenders 风格，但符合本仓"失败留 trace"的审计要求）
