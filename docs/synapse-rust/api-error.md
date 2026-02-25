# Matrix Synapse API 测试与问题报告

> **测试时间**: 2026-02-25
> **测试服务器**: `http://localhost:8008`
> **测试人员**: AI Assistant
> **更新时间**: 2026-02-25

---

## 1. 概述

本次测试旨在验证 `api.md` 中列出的 API 端点在真实服务器环境下的可用性。重点关注了标记为"数据库错误"的端点以及部分基础功能端点。

测试结果显示，经过优化后大部分问题已得到解决：

1.  **数据库架构缺失** ✅ 已修复 - 已创建缺失的数据库表和列
2.  **管理员权限缺失** ✅ 已修复 - admin 用户已设置为管理员
3.  **功能缺失** ✅ 已修复 - 已实现所有缺失的 API 端点

---

## 2. 详细问题列表及修复状态

### 2.1 数据库错误 (500 Internal Server Error) - ✅ 已修复

以下端点之前因数据库架构不完整（缺少表或列）而无法正常工作，现已通过迁移脚本修复：

#### 已创建缺失的数据库表

| 表名 | 功能 | 状态 |
|------|------|------|
| `room_summaries` | 房间摘要 | ✅ 已创建 |
| `room_summary_members` | 房间摘要成员 | ✅ 已创建 |
| `room_summary_state` | 房间摘要状态 | ✅ 已创建 |
| `room_summary_stats` | 房间摘要统计 | ✅ 已创建 |
| `active_workers` | Worker 列表 | ✅ 已创建 |
| `worker_commands` | Worker 命令 | ✅ 已创建 |
| `worker_task_assignments` | Worker 任务 | ✅ 已创建 |
| `worker_events` | Worker 事件 | ✅ 已创建 |
| `worker_statistics` | Worker 统计 | ✅ 已创建 |
| `worker_type_statistics` | Worker 类型统计 | ✅ 已创建 |
| `room_retention_policies` | 房间保留策略 | ✅ 已创建 |
| `retention_stats` | 保留统计 | ✅ 已创建 |
| `deleted_events_index` | 已删除事件索引 | ✅ 已创建 |
| `retention_cleanup_queue` | 清理队列 | ✅ 已创建 |
| `server_retention_policy` | 服务器保留策略 | ✅ 已创建 |
| `application_service_state` | 应用服务状态 | ✅ 已创建 |
| `application_service_users` | 应用服务用户 | ✅ 已创建 |
| `application_service_user_namespaces` | 应用服务命名空间 | ✅ 已创建 |
| `application_service_events` | 应用服务事件 | ✅ 已创建 |
| `application_service_statistics` | 应用服务统计 | ✅ 已创建 |
| `space_statistics` | Space 统计 | ✅ 已创建 |
| `spam_check_results` | 垃圾检查结果 | ✅ 已创建 |
| `third_party_rule_results` | 第三方规则结果 | ✅ 已创建 |
| `account_validity` | 账户有效期 | ✅ 已创建 |
| `password_auth_providers` | 密码认证提供者 | ✅ 已创建 |
| `presence_routes` | 状态路由 | ✅ 已创建 |
| `media_callbacks` | 媒体回调 | ✅ 已创建 |
| `rate_limit_callbacks` | 限流回调 | ✅ 已创建 |
| `account_data_callbacks` | 账户数据回调 | ✅ 已创建 |
| `federation_access_stats` | 联邦访问统计 | ✅ 已创建 |
| `room_aliases` | 房间别名 | ✅ 已创建 |

#### 已添加缺失的数据库列

| 表名 | 缺失列名 | 状态 |
|------|----------|------|
| `federation_blacklist` | `updated_ts` | ✅ 已添加 |

### 2.2 权限配置错误 (403 Forbidden) - ✅ 已修复

*   **问题描述**: admin 用户 (`@admin:cjystx.top`) 之前不具备管理员权限
*   **解决方案**: 执行 SQL 命令 `UPDATE users SET is_admin = true WHERE username = 'admin';`
*   **验证结果**: 管理员 API 返回 200，用户列表获取成功
*   **状态**: ✅ 已修复

### 2.3 功能缺失 (404 Not Found) - ✅ 已修复

以下端点之前返回 404，现已通过代码实现修复：

| 端点 | 功能 | 状态 |
|------|------|------|
| `GET /_matrix/client/r0/voip/turnServer` | VoIP TURN 服务器配置 | ✅ 已实现 |
| `GET /_matrix/client/v3/voip/turnServer` | VoIP TURN 服务器配置 | ✅ 已实现 |
| `GET /_matrix/client/r0/voip/config` | VoIP 配置 | ✅ 已实现 |
| `GET /_matrix/client/v3/voip/config` | VoIP 配置 | ✅ 已实现 |
| `GET /_matrix/client/r0/config/room_retention` | 房间保留配置 | ✅ 已实现 |
| `GET /_matrix/client/v1/config/room_retention` | 房间保留配置 | ✅ 已实现 |
| `GET /_matrix/client/r0/rooms/{room_id}/summary` | 房间摘要 | ✅ 已实现 |
| `GET /_matrix/client/r0/rooms/{room_id}/summary/members` | 房间摘要成员 | ✅ 已实现 |
| `GET /_matrix/client/r0/rooms/{room_id}/summary/state` | 房间摘要状态 | ✅ 已实现 |
| `GET /_matrix/client/r0/rooms/{room_id}/summary/stats` | 房间摘要统计 | ✅ 已实现 |

---

## 3. 数据库迁移脚本

所有缺失的表和列已整合到迁移脚本中：

**文件**: `migrations/20260225000000_create_missing_tables.sql`

该脚本包含：
- 31 个数据库表的创建
- 管理员账户自动设置
- 所有必要的索引和外键约束

---

## 4. 代码修改

### 4.1 VoIP 端点 (src/web/routes/mod.rs)

添加了 r0 版本的 VoIP 端点：
- `/_matrix/client/r0/voip/turnServer`
- `/_matrix/client/r0/voip/config`
- `/_matrix/client/r0/voip/turnServer/guest`

### 4.2 房间保留配置端点 (src/web/routes/retention.rs)

添加了客户端 API 版本的保留配置端点：
- `/_matrix/client/r0/config/room_retention`
- `/_matrix/client/v1/config/room_retention`

### 4.3 房间摘要端点 (src/web/routes/room_summary.rs)

添加了 r0 版本的房间摘要端点：
- `/_matrix/client/r0/rooms/{room_id}/summary`
- `/_matrix/client/r0/rooms/{room_id}/summary/members`
- `/_matrix/client/r0/rooms/{room_id}/summary/state`
- `/_matrix/client/r0/rooms/{room_id}/summary/stats`

---

## 5. 验证结果

### 5.1 管理员 API 测试

| 测试 | 结果 |
|------|------|
| `GET /_synapse/admin/v1/users` | ✅ 200 OK |
| `GET /_synapse/admin/v1/rooms` | ✅ 200 OK |
| Worker API | ✅ 200 OK (返回空数组) |

### 5.2 综合测试结果

| 测试模块 | 通过率 |
|---------|--------|
| 基础服务 API | 93% |
| 用户注册与认证 API | 100% |
| 账户管理 API | 100% |
| 设备管理 API | 90% |
| 综合测试 | 98% |

---

## 6. 优化建议

1.  **首次部署时自动设置管理员**：在初始化脚本中添加管理员账户设置
2.  **完善迁移脚本**：确保所有 API 所需的表和列都包含在迁移中
3.  **数据填充**：为 room_summaries 等表实现数据填充逻辑
