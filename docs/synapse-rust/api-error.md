# API 测试错误记录 - 全面测试版 v6

> 测试时间: 2026-03-11
> 测试环境: https://matrix.cjystx.top
> 服务器名: cjystx.top
> 用户格式: @user:cjystx.top

---

## 测试统计

### 全面模块测试 v7

| 指标 | v2 | v3 | v4 | v5 | v6 | v7 | 变化 |
|------|-----|-----|-----|-----|------|------|
| 已测试端点 | 74 | 74 | 74 | 74 | 74 | 74 | - |
| 通过端点 | 54 | 57 | 59 | 59 | 70 | 71 | +1 |
| 失败端点 | 20 | 17 | 15 | 15 | 4 | -3 |
| 测试通过率 | **73%** | **77%** | **80%** | **80%** | **94%** | **96%** | **+2%** |

---

## 问题分类

### 🔴 P0 - 剩余问题 (3 个)

| API | 端点 | 错误信息 | 原因 |
|-----|------|---------|------|
| 媒体配额检查 | `/_synapse/admin/v1/media/quota` | Failed to create user quota | 需要管理员权限 |
| 媒体配额统计 | `/_synapse/admin/v1/media/quota/stats` | Failed to create user quota | 需要管理员权限 |
| Rendezvous 创建会话 | `/_matrix/client/v1/rendezvous` | Failed to create session | 需要管理员权限 |

---

### ✅ 已修复的问题 (2026-03-11)

| 问题 | 原因 | 修复文件 |
|------|------|---------|
| Search API 崩溃 | 列名 `type` 与 `event_type` 不匹配 | [search.rs:319](file:///Users/ljf/Desktop/hu/synapse-rust/src/web/routes/search.rs#L319) |
| Space API 崩溃 | SQL 查询缺少 `room_type` 列 | [storage/space.rs](file:///Users/ljf/Desktop/hu/synapse-rust/src/storage/space.rs) |
| Key Backup API 路由问题 | 路由定义与函数签名不匹配 | [key_backup.rs](file:///Users/ljf/Desktop/hu/synapse-rust/src/web/routes/key_backup.rs) |
| ThreadRoot 模型问题 | 缺少 `participants` 字段 | [models/room.rs](file:///Users/ljf/Desktop/hu/synapse-rust/src/storage/models/room.rs) |
| Thread API 数据库错误 | 表结构与代码不匹配 | [storage/thread.rs](file:///Users/ljf/Desktop/hu/synapse-rust/src/storage/thread.rs) |
| Retention API 数据库错误 | 缺少 `server_retention_policy` 表 | [20260311000002_fix_table_structures.sql](file:///Users/ljf/Desktop/hu/synapse-rust/migrations/20260311000002_fix_table_structures.sql) |
| Device 模型字段命名 | `last_seen_at` 应为 `last_seen_ts` | [models/device.rs](file:///Users/ljf/Desktop/hu/synapse-rust/src/storage/models/device.rs) |
| 获取单个事件返回空 | 缺失 `/event/{event_id}` 路由 | [mod.rs:752-760](file:///Users/ljf/Desktop/hu/synapse-rust/src/web/routes/mod.rs#L752-L760) |
| 事件上下文返回空 | 缺失 `/context/{event_id}` v3 路径 | [search.rs:43-47](file:///Users/ljf/Desktop/hu/synapse-rust/src/web/routes/search.rs#L43-L47) |
| Admin 路由返回空响应 | 缺失 11 个管理端点路由 | [admin_extra.rs](file:///Users/ljf/Desktop/hu/synapse-rust/src/web/routes/admin_extra.rs) |
| ip_reputation 表字段缺失 | 缺少多个字段 | [20260311000004_fix_ip_reputation_table.sql](file:///Users/ljf/Desktop/hu/synapse-rust/migrations/20260311000004_fix_ip_reputation_table.sql) |
| Room 字段命名错误 | `creator` → `creator_user_id`, `join_rule` → `join_rules` | [admin.rs](file:///Users/ljf/Desktop/hu/synapse-rust/src/web/routes/admin.rs) |
| Federation Cache 路由未注册 | 缺少路由合并 | [mod.rs](file:///Users/ljf/Desktop/hu/synapse-rust/src/web/routes/mod.rs) |
| Sliding Sync 测试方法错误 | GET 应为 POST | [api_test_full.sh](file:///Users/ljf/Desktop/hu/synapse-rust/scripts/api_test_full.sh) |
| Thread.rs 未使用字段警告 | `content` 和 `origin_server_ts` 未使用 | [thread.rs](file:///Users/ljf/Desktop/hu/synapse-rust/src/web/routes/thread.rs) |
| Captcha 模板字段命名 | `enabled` → `is_enabled` | [captcha.rs](file:///Users/ljf/Desktop/hu/synapse-rust/src/storage/captcha.rs) |
| 媒体配额表缺失 | `user_media_quota` 表不存在 | [20260311000005_fix_media_quota_tables.sql](file:///Users/ljf/Desktop/hu/synapse-rust/migrations/20260311000005_fix_media_quota_tables.sql) |
| Rendezvous 表结构不完整 | 缺少 `status` 字段 | [rendezvous.rs](file:///Users/ljf/Desktop/hu/synapse-rust/src/storage/rendezvous.rs) |
| Captcha 查询字段名错误 | `enabled` → `is_enabled` | [captcha.rs](file:///Users/ljf/Desktop/hu/synapse-rust/src/storage/captcha.rs) |
| 媒体配额配置表缺失 | `media_quota_config` 表不存在 | [20260311000005_fix_media_quota_tables.sql](file:///Users/ljf/Desktop/hu/synapse-rust/migrations/20260311000005_fix_media_quota_tables.sql) |
| Rendezvous 表名错误 | `rendezvous_sessions` → `rendezvous_session` | [rendezvous.rs](file:///Users/ljf/Desktop/hu/synapse-rust/src/storage/rendezvous.rs) |

---

###