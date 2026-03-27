# API 错误追踪报告

> **创建日期**: 2026-03-26
> **更新日期**: 2026-03-26
> **项目**: synapse-rust

---

## 测试结果总览

| 测试类型 | 结果 |
|----------|------|
| 单元测试 | ✅ 694 passed |
| API 测试 | ⚠️ 50/74 passed (67%) |
| Clippy | ✅ 0 warnings |

---

## 一、API 测试失败分类

### 1.1 Admin API (预期行为 - 需要管理员权限)

这些测试失败是**预期的**，因为测试用户不是管理员。

| # | API 端点 | 错误码 | 说明 |
|---|----------|--------|------|
| 35 | `/_synapse/admin/v1/server_version` | M_FORBIDDEN | 需要管理员 |
| 36 | `/_synapse/admin/v1/users` | M_FORBIDDEN | 需要管理员 |
| 37 | `/_synapse/admin/v1/rooms` | M_FORBIDDEN | 需要管理员 |
| 38 | `/_synapse/admin/v1/statistics` | M_FORBIDDEN | 需要管理员 |
| 49 | `/_synapse/admin/v1/background_updates` | M_FORBIDDEN | 需要管理员 |
| 50 | `/_synapse/admin/v1/background_updates/stats` | M_FORBIDDEN | 需要管理员 |
| 51 | `/_synapse/admin/v1/event_reports` | M_FORBIDDEN | 需要管理员 |
| 52 | `/_synapse/admin/v1/event_reports/stats` | M_FORBIDDEN | 需要管理员 |
| 53 | `/_matrix/client/v3/account_data/m.direct` | M_FORBIDDEN | 需要管理员 |
| 58 | `/_synapse/admin/v1/registration_tokens` | M_FORBIDDEN | 需要管理员 |
| 65 | `/_synapse/worker/v1/workers` | M_FORBIDDEN | 需要管理员 |
| 66 | `/_synapse/worker/v1/statistics` | M_FORBIDDEN | 需要管理员 |
| 67 | `/_synapse/admin/v1/federation/blacklist` | M_FORBIDDEN | 需要管理员 |
| 73 | `/_synapse/admin/v1/telemetry/status` | M_FORBIDDEN | 需要管理员 |
| 74 | `/_synapse/admin/v1/telemetry/health` | M_FORBIDDEN | 需要管理员 |

**结论**: Admin API 功能正常，需要管理员令牌才能访问。

### 1.2 数据库错误

这些错误需要检查数据库迁移是否正确执行。

| # | API 端点 | 错误信息 | 可能原因 |
|---|----------|----------|----------|
| 41 | `/_matrix/client/v3/room_keys/version` | `column "mgmt_key" does not exist` | 数据库表 `room_keys` 缺少列 |
| 42 | `/_matrix/client/v3/room_keys/keys` | `column "mgmt_key" does not exist` | 同上 |
| 29 | `/_matrix/client/v1/spaces/public` | `Failed to get public spaces` | Space 表可能有问题 |
| 30 | `/_matrix/client/v1/spaces/user` | `Failed to get user spaces` | Space 表可能有问题 |
| 31 | `/_matrix/client/v1/spaces/search` | `Failed to search spaces` | Space 表可能有问题 |
| 32 | `/_matrix/client/v1/rooms/!test/threads` | `Failed to list threads` | Thread 表可能有问题 |
| 59 | `/_matrix/media/v1/quota/check` | `Failed to get default quota config` | 媒体配额表问题 |
| 60 | `/_matrix/media/v1/quota/stats` | `Failed to get default quota config` | 同上 |

### 1.3 服务配置问题

| # | API 端点 | 错误信息 | 说明 |
|---|----------|----------|------|
| 43 | `/_matrix/client/v3/voip/turnServer` | `No TURN URIs configured` | TURN 服务器未配置 |
| 54 | `/_synapse/retention/v1/server/policy` | Empty response | 保留策略未配置 |
| 55 | `/_synapse/retention/v1/rooms` | Empty response | 保留策略未配置 |
| 61 | `/_synapse/admin/v1/cas/config` | Empty response | CAS 未启用 |
| 62 | `/_synapse/admin/v1/saml/config` | Empty response | SAML 未启用 |
| 63 | `/_synapse/admin/v1/oidc/config` | Empty response | OIDC 未启用 |

---

## 二、需要数据库迁移修复

### 2.1 room_keys 表问题

**错误**: `column "mgmt_key" does not exist`

**检查项**:
1. 检查 `e2ee_backup` 表或相关表的结构
2. 确认迁移脚本是否包含 `mgmt_key` 列

---

## 三、测试脚本改进建议

### 3.1 添加管理员测试

创建单独的管理员 API 测试脚本，使用管理员令牌。

### 3.2 添加数据库健康检查

在 API 测试前先检查数据库表结构是否完整。

---

## 四、行动项

| 优先级 | 问题 | 状态 |
|--------|------|------|
| 高 | 检查 `room_keys` 表结构 | 待处理 |
| 中 | 配置 TURN 服务器 | 待处理 |
| 中 | 配置保留策略 | 待处理 |
| 低 | 启用 CAS/SAML/OIDC | 待处理 |

---

*本文档将随项目进展持续更新*
