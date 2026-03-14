# 数据库架构分析与优化方案

> 版本: v1.0.0
> 更新日期: 2026-03-14
> 项目: synapse-rust

---

## 一、数据库架构评估

### 1.1 当前状态概览

| 指标 | 数值 |
|------|------|
| 数据库表总数 | 129 |
| 包含外键的表 | 49 (38%) |
| 缺少外键的表 | 80 (62%) |
| 迁移脚本数量 | 30+ |
| 代码模块数 | 58 |

### 1.2 识别的问题

#### 问题 1: 外键约束缺失

| 问题类型 | 影响范围 | 风险等级 |
|----------|----------|----------|
| 缺少外键约束 | 80 个表 | 🔴 高 |
| 数据完整性无保障 | 所有业务表 | 🔴 高 |
| 级联删除失效 | 依赖关系表 | 🟡 中 |

**详细分析**:
```
现有外键: 49 个
应含外键字段: user_id, room_id, device_id, event_id 等
实际外键覆盖率: 38%
```

#### 问题 2: 字段命名不一致风险

| 规范字段 | 可能错误变体 | 检查状态 |
|----------|--------------|----------|
| `created_ts` | `created_at`, `created` | ✅ 已统一 |
| `updated_ts` | `updated_at`, `modified_at` | ✅ 已统一 |
| `expires_at` | `expires_ts`, `expiry_time` | ⚠️ 待检查 |
| `is_revoked` | `revoked`, `invalidated` | ✅ 已统一 |
| `is_enabled` | `enabled`, `active` | ⚠️ 待检查 |

#### 问题 3: 迁移脚本冗余

| 问题 | 数量 |
|------|------|
| 重复字段定义 | 12 处 |
| 可合并的迁移 | 8 个 |
| 可删除的冗余文件 | 5 个 |

### 1.3 潜在风险评估

| 风险类型 | 描述 | 影响 | 概率 |
|----------|------|------|------|
| 数据不一致 | 代码与数据库字段不匹配 | 运行时错误 | 高 |
| 性能问题 | 缺少必要索引 | 查询慢 | 中 |
| 外键缺失 | 孤立数据无法清理 | 数据垃圾 | 高 |
| 迁移失败 | 脚本依赖混乱 | 部署失败 | 中 |

---

## 二、迁移脚本整合优化

### 2.1 迁移脚本分析

#### 当前迁移文件列表

```
migrations/
├── 核心架构
│   ├── 00000000_unified_schema_v6.sql     (99KB - 主架构)
│   └── 00000000_unified_schema_v6.sql.bak (备份)
│
├── 增量迁移 (按时间排序)
│   ├── 20260309000001_password_security_enhancement.sql
│   ├── 20260310000001_add_missing_e2ee_tables.sql
│   ├── 20260310000002_normalize_fields_and_add_tables.sql
│   ├── 20260310000003_fix_api_test_issues.sql
│   ├── 20260310000004_create_federation_signing_keys.sql
│   ├── 20260311000001_add_space_members_table.sql
│   ├── 20260311000002_fix_table_structures.sql
│   ├── 20260311000003_optimize_database_structure.sql
│   ├── 20260311000004_fix_ip_reputation_table.sql
│   ├── 20260311000005_fix_media_quota_tables.sql
│   ├── 20260311000006_add_e2ee_tables.sql
│   ├── 20260311000007_fix_application_services_tables.sql
│   ├── 20260311000008_fix_key_backups_constraints.sql
│   ├── 20260313000000_create_room_tags_and_password_reset_tokens.sql
│   ├── 20260313000000_unified_migration_optimized.sql  ⚠️ 冲突
│   ├── 20260313000001_qr_login.sql
│   ├── 20260313000002_invite_blocklist.sql
│   ├── 20260313000003_sticky_event.sql
│   ├── 20260314000001_widget_support.sql
│   ├── 20260314000002_add_performance_indexes.sql
│   ├── 20260314000003_fix_updated_at_to_updated_ts.sql
│   ├── 20260314000004_fix_refresh_tokens_fields.sql
│   ├── 20260314000005_fix_refresh_token_families.sql
│   ├── 20260315000001_fix_field_names.sql
│   ├── 20260315000002_create_admin_api_tables.sql
│   ├── 20260315000003_create_feature_tables.sql
│   ├── 20260315000004_fix_field_naming_inconsistencies.sql
│   ├── 20260315000005_fix_room_guest_access.sql
│   ├── 20260315000006_fix_room_summaries.sql
│   └── 20260315000007_add_foreign_key_constraints.sql  🆕 最新
│
└── 文档
    ├── DATABASE_FIELD_STANDARDS.md
    ├── MIGRATION_INDEX.md
    ├── MIGRATION_OPTIMIZATION_REPORT.md
    ├── SCHEMA_OPTIMIZATION_REPORT.md
    └── README.md
```

### 2.2 合并策略

#### 策略 1: 按功能模块合并

| 模块 | 包含迁移 | 合并后 |
|------|----------|--------|
| 字段规范化 | 20260314* | `202603150000_field_normalization.sql` |
| Admin API | 20260315000002 | `202603150001_admin_api.sql` |
| 功能模块 | 20260315000003 | `202603150002_feature_modules.sql` |
| 外键约束 | 20260315000007 | `202603150003_foreign_keys.sql` |

#### 策略 2: 消除冗余

**可删除文件**:
1. `00000000_unified_schema_v6.sql.bak` - 备份文件
2. `20260313000000_unified_migration_optimized.sql` - 与主架构重复
3. 多个字段修复文件可合并

### 2.3 合并后迁移方案

```sql
-- migrations/202603150000_consolidated.sql
-- 合并所有字段规范化迁移

-- 目录结构优化后
migrations/
├── 00000000_unified_schema_v6.sql     -- 基础架构
├── 202603150000_consolidated.sql      -- 合并优化
│   ├── 字段规范化
│   ├── Admin API 表
│   ├── 功能模块表
│   └── 外键约束
└── history/
    └── 原始迁移备份
```

---

## 三、完整数据库表结构清单

### 3.1 核心表 (Core Tables)

#### 3.1.1 用户相关表

| 表名 | 主键 | 外键 | 索引 | 说明 |
|------|------|------|------|------|
| `users` | `user_id` | - | username, is_admin | 用户主表 |
| `devices` | `device_id` | `user_id` | user_id, last_seen_ts | 设备表 |
| `access_tokens` | `id` | `user_id`, `device_id` | token, user_id | 访问令牌 |
| `refresh_tokens` | `id` | `user_id`, `device_id` | token_hash, user_id | 刷新令牌 |
| `user_account_data` | `(user_id, type)` | `user_id` | user_id | 用户账户数据 |
| `user_filters` | `id` | `user_id` | user_id | 用户过滤器 |
| `user_directory` | `user_id` | - | user_id | 用户目录 |
| `user_threepids` | `id` | `user_id` | user_id, medium | 用户第三方ID |

#### 3.1.2 房间相关表

| 表名 | 主键 | 外键 | 索引 | 说明 |
|------|------|------|------|------|
| `rooms` | `room_id` | - | room_id, creator | 房间主表 |
| `room_memberships` | `(room_id, user_id)` | `user_id`, `room_id` | user_id, membership | 房间成员 |
| `room_state_events` | `(room_id, type, state_key)` | `room_id` | room_id, type | 房间状态 |
| `room_events` | `(room_id, event_id)` | `room_id` | room_id, origin_server_ts | 房间事件 |
| `room_aliases` | `(room_id, alias)` | `room_id` | room_id, alias | 房间别名 |
| `room_directory` | `room_id` | - | room_id, is_public | 房间目录 |
| `room_tags` | `(room_id, user_id, tag)` | `room_id`, `user_id` | user_id, tag | 房间标签 |
| `room_account_data` | `(room_id, user_id, type)` | `room_id`, `user_id` | user_id | 房间账户数据 |
| `room_summaries` | `room_id` | - | room_id | 房间摘要 |
| `room_parents` | `(room_id, parent_id)` | `room_id`, `parent_id` | parent_id | 房间层级 |

#### 3.1.3 认证相关表

| 表名 | 主键 | 外键 | 索引 | 说明 |
|------|------|------|------|------|
| `password_policy` | `id` | - | - | 密码策略 |
| `password_history` | `id` | `user_id` | user_id | 密码历史 |
| `account_validity` | `user_id` | `user_id` | user_id | 账户有效期 |
| `saml_sessions` | `id` | `user_id` | user_id | SAML会话 |
| `cas_sessions` | `id` | `user_id` | user_id | CAS会话 |
| `oidc_sessions` | `id` | `user_id` | user_id | OIDC会话 |

### 3.2 消息与事件表

| 表名 | 主键 | 外键 | 索引 | 说明 |
|------|------|------|------|------|
| `events` | `event_id` | `room_id`, `sender` | room_id, stream_ordering | 事件主表 |
| `event_receipts` | `(room_id, event_id, user_id, receipt_type)` | `room_id`, `event_id` | room_id, user_id | 收据 |
| `read_markers` | `(room_id, user_id, name)` | `room_id`, `user_id` | user_id | 已读标记 |
| `typing` | `(room_id, user_id)` | `room_id`, `user_id` | room_id | 打字提示 |
| `notifications` | `id` | `user_id`, `room_id` | user_id, room_id | 通知 |
| `push_rules` | `(user_id, rule_id)` | `user_id` | user_id | 推送规则 |

### 3.3 设备与安全表

| 表名 | 主键 | 外键 | 索引 | 说明 |
|------|------|------|------|------|
| `device_keys` | `(user_id, device_id, algorithm)` | `user_id`, `device_id` | user_id, device_id | 设备密钥 |
| `device_signatures` | `id` | `user_id`, `device_id` | user_id | 设备签名 |
| `cross_signing_keys` | `(user_id, key_type)` | `user_id` | user_id | 交叉签名 |
| `olm_accounts` | `account_id` | `user_id` | user_id | Olm账户 |
| `olm_sessions` | `session_id` | `account_id` | account_id | Olm会话 |
| `one_time_key` | `(session_id, key_id)` | `session_id` | session_id | 一次性密钥 |
| `key_backups` | `backup_id` | `user_id` | user_id | 密钥备份 |
| `backup_keys` | `(backup_id, room_id, session_id)` | `backup_id` | backup_id | 备份密钥 |
| `e2ee_key_requests` | `request_id` | `user_id` | user_id | E2EE密钥请求 |

### 3.4 应用服务表

| 表名 | 主键 | 外键 | 索引 | 说明 |
|------|------|------|------|------|
| `application_services` | `id` | - | as_token | 应用服务 |
| `application_service_room_namespaces` | `id` | `as_id` | as_id | 房间命名空间 |
| `application_service_user_namespaces` | `id` | `as_id` | as_id | 用户命名空间 |
| `application_service_transactions` | `id` | `as_id` | as_id | 事务记录 |
| `application_service_events` | `id` | - | - | 服务事件 |

### 3.5 联邦与同步表

| 表名 | 主键 | 外键 | 索引 | 说明 |
|------|------|------|------|------|
| `federation_servers` | `server_name` | - | server_name | 联邦服务器 |
| `federation_queue` | `id` | - | destination | 联邦队列 |
| `federation_blacklist` | `id` | - | server_name | 联邦黑名单 |
| `sliding_sync_rooms` | `room_id` | `user_id` | user_id | 滑动同步房间 |
| `sync_stream_id` | `id` | - | - | 同步流ID |

### 3.6 Space 相关表

| 表名 | 主键 | 外键 | 索引 | 说明 |
|------|------|------|------|------|
| `spaces` | `room_id` | `room_id` | room_id | Space主表 |
| `space_children` | `(parent_id, child_id)` | `parent_id`, `child_id` | parent_id | Space子项 |
| `space_hierarchy` | `(room_id, child_order)` | `room_id` | room_id | Space层级 |

### 3.7 线程相关表

| 表名 | 主键 | 外键 | 索引 | 说明 |
|------|------|------|------|------|
| `thread_roots` | `(room_id, event_id)` | `room_id`, `event_id` | room_id | 线程根事件 |
| `thread_subscriptions` | `(room_id, user_id)` | `room_id`, `user_id` | user_id | 线程订阅 |

### 3.8 媒体相关表

| 表名 | 主键 | 外键 | 索引 | 说明 |
|------|------|------|------|------|
| `media_metadata` | `media_id` | - | media_id | 媒体元数据 |
| `thumbnails` | `media_id` | - | media_id | 缩略图 |
| `media_quota` | `server_name` | - | server_name | 媒体配额 |
| `user_media_quota` | `user_id` | `user_id` | user_id | 用户媒体配额 |

### 3.9 推送相关表

| 表名 | 主键 | 外键 | 索引 | 说明 |
|------|------|------|------|------|
| `pushers` | `id` | `user_id`, `device_id` | user_id, pushkey | 推送器 |
| `push_devices` | `id` | `user_id`, `device_id` | user_id | 推送设备 |
| `push_notification_queue` | `id` | `user_id`, `device_id` | status | 推送队列 |
| `push_notification_log` | `id` | - | - | 推送日志 |

### 3.10 好友相关表

| 表名 | 主键 | 外键 | 索引 | 说明 |
|------|------|------|------|------|
| `friends` | `id` | `user_id`, `friend_user_id` | user_id | 好友 |
| `friend_requests` | `id` | `user_id`, `requester_id` | user_id | 好友请求 |
| `friend_categories` | `id` | `user_id` | user_id | 好友分类 |

### 3.11 管理相关表

| 表名 | 主键 | 外键 | 索引 | 说明 |
|------|------|------|------|------|
| `registration_tokens` | `token` | - | token | 注册令牌 |
| `server_notices` | `user_id` | `user_id` | user_id | 服务器通知 |
| `shadow_bans` | `user_id` | `user_id` | user_id | 影子封禁 |
| `rate_limits` | `id` | `user_id` | user_id | 速率限制 |
| `blocked_users` | `user_id` | `user_id` | user_id | 封禁用户 |
| `blocked_rooms` | `room_id` | `room_id` | room_id | 封禁房间 |

### 3.12 保留策略表

| 表名 | 主键 | 外键 | 索引 | 说明 |
|------|------|------|------|------|
| `server_retention_policy` | `server_name` | - | server_name | 服务器保留策略 |
| `room_retention_policy` | `room_id` | `room_id` | room_id | 房间保留策略 |
| `retention_events` | `room_id` | - | room_id | 保留事件 |

### 3.13 Worker 相关表

| 表名 | 主键 | 外键 | 索引 | 说明 |
|------|------|------|------|------|
| `workers` | `id` | - | - | Worker信息 |
| `worker_commands` | `id` | - | - | Worker命令 |
| `worker_events` | `id` | - | - | Worker事件 |
| `worker_statistics` | `id` | - | - | Worker统计 |

### 3.14 其他功能表

| 表名 | 主键 | 外键 | 索引 | 说明 |
|------|------|------|------|------|
| `event_reports` | `id` | `room_id`, `user_id` | room_id | 事件报告 |
| `event_report_history` | `id` | - | - | 事件报告历史 |
| `event_report_stats` | `id` | - | - | 事件报告统计 |
| `filters` | `id` | `user_id` | user_id | 过滤器 |
| `presence` | `user_id` | - | user_id | 在线状态 |
| `presence_subscriptions` | `(observer_user_id, present_user_id)` | - | observer_user_id | 状态订阅 |
| `account_data` | `(user_id, type)` | `user_id` | user_id | 账户数据 |
| `openid_tokens` | `id` | `user_id` | user_id | OpenID令牌 |
| `background_updates` | `id` | - | - | 后台更新 |
| `search_index` | `id` | - | - | 搜索索引 |
| `modules` | `id` | - | - | 模块 |
| `schema_migrations` | `id` | - | - | 架构迁移 |

---

## 四、数据库优化技能

### 4.1 索引优化策略

#### 4.1.1 必需索引清单

| 表名 | 索引字段 | 索引类型 | 说明 |
|------|----------|----------|------|
| `users` | `(username)` | UNIQUE | 登录查询 |
| `users` | `(is_admin)` | INDEX | 管理员查询 |
| `devices` | `(user_id)` | INDEX | 用户设备列表 |
| `devices` | `(last_seen_ts)` | INDEX | 设备活跃度 |
| `access_tokens` | `(token)` | UNIQUE | Token验证 |
| `access_tokens` | `(user_id, is_valid)` | INDEX | 用户有效Token |
| `refresh_tokens` | `(token_hash)` | UNIQUE | Token验证 |
| `refresh_tokens` | `(user_id, is_revoked)` | INDEX | 用户Token列表 |
| `room_memberships` | `(user_id, membership)` | INDEX | 用户房间列表 |
| `room_memberships` | `(room_id, membership)` | INDEX | 房间成员列表 |
| `room_events` | `(room_id, origin_server_ts)` | INDEX | 房间事件时间线 |
| `room_events` | `(room_id, type)` | INDEX | 事件类型查询 |
| `events` | `(room_id, stream_ordering)` | INDEX | 事件流顺序 |
| `events` | `(sender, origin_server_ts)` | INDEX | 用户发送事件 |
| `event_receipts` | `(room_id, user_id)` | INDEX | 收据查询 |
| `notifications` | `(user_id, room_id, stream_ordering)` | INDEX | 通知查询 |
| `presence` | `(user_id)` | INDEX | 用户在线状态 |
| `presence_subscriptions` | `(user_id, observed_user_id)` | INDEX | 状态订阅 |

#### 4.1.2 复合索引设计

```sql
-- 用户设备列表查询优化
CREATE INDEX idx_devices_user_last_seen ON devices(user_id, last_seen_ts DESC);

-- 房间成员查询优化
CREATE INDEX idx_room_memberships_joined ON room_memberships(room_id, membership, joined_ts DESC)
    WHERE membership = 'join';

-- 事件时间线查询优化
CREATE INDEX idx_events_room_time_type ON events(room_id, origin_server_ts DESC, type);

-- 通知查询优化
CREATE INDEX idx_notifications_user_room ON notifications(user_id, room_id, stream_ordering DESC);
```

#### 4.1.3 部分索引

```sql
-- 仅索引有效的Token
CREATE INDEX idx_access_tokens_valid ON access_tokens(user_id, created_ts DESC)
    WHERE is_valid = TRUE;

-- 仅索引有效的刷新Token
CREATE INDEX idx_refresh_tokens_active ON refresh_tokens(user_id, created_ts DESC)
    WHERE is_revoked = FALSE;

-- 仅索引公开的房间
CREATE INDEX idx_rooms_public ON rooms(room_id, created_ts DESC)
    WHERE is_public = TRUE;

-- 仅索引非撤销的成员
CREATE INDEX idx_memberships_joined ON room_memberships(room_id, user_id, joined_ts DESC)
    WHERE membership = 'join';
```

### 4.2 查询性能提升方案

#### 4.2.1 分页优化

```sql
-- 低效: OFFSET 大时性能差
SELECT * FROM events 
WHERE room_id = $1 
ORDER BY origin_server_ts DESC 
LIMIT 100 OFFSET 10000;

-- 高效: 使用游标分页
SELECT * FROM events 
WHERE room_id = $1 
AND origin_server_ts < $cursor_ts
ORDER BY origin_server_ts DESC 
LIMIT 100;
```

#### 4.2.2 批量操作优化

```sql
-- 低效: 多次INSERT
INSERT INTO device_keys (user_id, device_id, key_data) VALUES ($1, $2, $3);
INSERT INTO device_keys (user_id, device_id, key_data) VALUES ($4, $5, $6);

-- 高效: 批量INSERT
INSERT INTO device_keys (user_id, device_id, key_data) VALUES 
    ($1, $2, $3),
    ($4, $5, $6);
```

#### 4.2.3 JOIN 优化

```sql
-- 确保有索引
CREATE INDEX idx_room_memberships_user_room ON room_memberships(user_id, room_id);
CREATE INDEX idx_rooms_room ON rooms(room_id);

-- 使用 EXPLAIN ANALYZE 分析
EXPLAIN ANALYZE 
SELECT u.*, r.room_name 
FROM users u 
JOIN room_memberships m ON u.user_id = m.user_id 
JOIN rooms r ON m.room_id = r.room_id 
WHERE m.membership = 'join';
```

### 4.3 事务管理机制

#### 4.3.1 事务隔离级别

```sql
-- 设置隔离级别
SET TRANSACTION ISOLATION LEVEL READ COMMITTED;

-- 或使用锁
BEGIN;
SELECT * FROM rooms WHERE room_id = $1 FOR UPDATE;
-- 执行更新
COMMIT;
```

#### 4.3.2 死锁预防

```sql
-- 始终按相同顺序访问表
-- 错误示例: 可能死锁
BEGIN;
UPDATE rooms SET ... WHERE room_id = $1;
UPDATE room_memberships WHERE user_id = $2;

-- 正确示例: 固定顺序
BEGIN;
UPDATE room_memberships WHERE user_id = $2;
UPDATE rooms SET ... WHERE room_id = $1;
```

#### 4.3.3 长事务处理

```sql
-- 使用保存点
BEGIN;
SAVEPOINT sp1;
-- 可能失败的操作
ROLLBACK TO SAVEPOINT sp1;
COMMIT;
```

### 4.4 数据安全与备份策略

#### 4.4.1 备份策略

| 备份类型 | 频率 | 保留时间 | 存储位置 |
|----------|------|----------|----------|
| 全量备份 | 每天 | 30天 | 远程存储 |
| 增量备份 | 每小时 | 7天 | 本地存储 |
| WAL归档 | 实时 | 3天 | 本地存储 |

```bash
# 全量备份
pg_dump -Fc -f backup_$(date +%Y%m%d).dump synapse

# 增量备份
pg_basebackup -D backup_incremental -X stream -Ft -z -z6

# 恢复
pg_restore -d synapse backup_20260314.dump
```

#### 4.4.2 数据加密

```sql
-- 列级加密示例 (应用层处理)
CREATE TABLE sensitive_data (
    id SERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    encrypted_data BYTEA NOT NULL,  -- 加密数据
    created_ts BIGINT NOT NULL
);

-- 使用 pgcrypto 扩展
CREATE EXTENSION pgcrypto;

-- 加密
INSERT INTO sensitive_data (user_id, encrypted_data) 
VALUES ($1, pgp_sym_encrypt($2, $3));

-- 解密
SELECT pgp_sym_decrypt(encrypted_data::bytea, $1) 
FROM sensitive_data WHERE user_id = $2;
```

#### 4.4.3 审计日志

```sql
-- 创建审计表
CREATE TABLE audit_log (
    id BIGSERIAL PRIMARY KEY,
    table_name TEXT NOT NULL,
    operation TEXT NOT NULL,  -- INSERT, UPDATE, DELETE
    old_data JSONB,
    new_data JSONB,
    user_id TEXT,
    ip_address VARCHAR(45),
    created_ts BIGINT NOT NULL
);

-- 创建触发器
CREATE OR REPLACE FUNCTION audit_trigger() RETURNS TRIGGER AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        INSERT INTO audit_log (table_name, operation, new_data, created_ts)
        VALUES (TG_TABLE_NAME, 'INSERT', row_to_json(NEW), extract(epoch from now())::bigint);
    ELSIF TG_OP = 'UPDATE' THEN
        INSERT INTO audit_log (table_name, operation, old_data, new_data, created_ts)
        VALUES (TG_TABLE_NAME, 'UPDATE', row_to_json(OLD), row_to_json(NEW), extract(epoch from now())::bigint);
    ELSIF TG_OP = 'DELETE' THEN
        INSERT INTO audit_log (table_name, operation, old_data, created_ts)
        VALUES (TG_TABLE_NAME, 'DELETE', row_to_json(OLD), extract(epoch from now())::bigint);
    END IF;
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

-- 应用到敏感表
CREATE TRIGGER audit_users
AFTER INSERT OR UPDATE OR DELETE ON users
FOR EACH ROW EXECUTE FUNCTION audit_trigger();
```

### 4.5 数据库扩展性设计

#### 4.5.1 分表策略

```sql
-- 按时间分表示例
CREATE TABLE events_2026_01 (
    CHECK (origin_server_ts >= 1735689600000 AND origin_server_ts < 1738368000000)
) INHERITS (events);

-- 或使用 pg_pathman
CREATE TABLE events (
    ...
) PARTITION BY RANGE (origin_server_ts);

CREATE TABLE events_2026_01 PARTITION OF events
    FOR VALUES FROM (1735689600000) TO (1738368000000);
```

#### 4.5.2 分库策略

```sql
-- 使用 postgres_fdw 实现分库
CREATE SERVER foreign_server FOREIGN DATA WRAPPER postgres_fdw
    OPTIONS (host '192.168.1.100', port '5432', dbname 'synapse_shard1');

CREATE USER MAPPING FOR CURRENT_USER SERVER foreign_server
    OPTIONS (user 'synapse', password 'password');

-- 创建外部表
CREATE FOREIGN TABLE events_shard1 (
    ...
) SERVER foreign_server OPTIONS (table_name 'events');
```

#### 4.5.3 读写分离

```sql
-- 创建只读副本 (在配置中设置)
-- postgresql.conf
# hot_standby = on

-- 应用层使用连接池分流
-- PgBouncer 配置
[databases]
synapse_ro = host=replica1 port=5432 dbname=synapse
```

---

## 五、数据库重构方案

### 5.1 重构目标与范围

#### 5.1.1 目标

| 目标 | 指标 | 现状 | 目标值 |
|------|------|------|--------|
| 外键覆盖率 | % | 38% | 95% |
| 字段一致性 | 问题数 | 50+ | 0 |
| 索引完整性 | 缺失索引 | 20+ | 0 |
| 迁移脚本 | 冗余文件 | 5 | 0 |

#### 5.1.2 范围

```
重构范围:
├── 数据库架构
│   ├── 外键约束补全
│   ├── 索引优化
│   └── 字段标准化
│
├── 迁移脚本
│   ├── 冗余文件清理
│   ├── 脚本合并
│   └── 文档完善
│
└── 代码一致性
    ├── Rust模型对齐
    └── SQL查询验证
```

### 5.2 实施步骤与时间线

#### 阶段 1: 准备阶段 (Day 1)

| 任务 | 时长 | 输出 |
|------|------|------|
| 数据库备份 | 1h | 完整备份 |
| 当前状态记录 | 2h | 状态报告 |
| 测试环境验证 | 2h | 测试确认 |

#### 阶段 2: 字段标准化 (Day 2)

| 任务 | 时长 | 输出 |
|------|------|------|
| 字段命名检查脚本 | 4h | 检查脚本 |
| 字段修复迁移 | 4h | 修复迁移 |
| 代码模型更新 | 4h | Rust代码 |

#### 阶段 3: 外键补全 (Day 3-4)

| 任务 | 时长 | 输出 |
|------|------|------|
| 外键分析 | 4h | FK分析报告 |
| 外键迁移创建 | 4h | FK迁移 |
| 外键应用 | 4h | 验证完成 |

#### 阶段 4: 索引优化 (Day 5)

| 任务 | 时长 | 输出 |
|------|------|------|
| 索引分析 | 2h | 索引报告 |
| 缺失索引创建 | 4h | 索引迁移 |
| 性能验证 | 2h | 性能报告 |

#### 阶段 5: 脚本整合 (Day 6)

| 任务 | 时长 | 输出 |
|------|------|------|
| 冗余文件识别 | 2h | 冗余列表 |
| 脚本合并 | 4h | 合并脚本 |
| 文档更新 | 2h | 更新文档 |

### 5.3 数据迁移策略

#### 5.3.1 迁移前准备

```bash
# 1. 完整备份
pg_dump -Fc synapse > backup_$(date +%Y%m%d_%H%M%S).dump

# 2. 验证备份
pg_restore --list backup_*.dump | head -20

# 3. 创建检查点
psql -c "CHECKPOINT;";
```

#### 5.3.2 增量迁移

```sql
-- 分批次处理大数据表
BEGIN;

-- 第一批: 修复 user_id 字段
ALTER TABLE some_table 
ALTER COLUMN user_id TYPE TEXT;

-- 验证
SELECT COUNT(*) FROM some_table WHERE user_id IS NULL;

COMMIT;
```

#### 5.3.3 回滚方案

```sql
-- 创建回滚脚本
-- 回滚外键
ALTER TABLE some_table DROP CONSTRAINT IF EXISTS fk_some_constraint;

-- 回滚字段修改
ALTER TABLE some_table RENAME COLUMN new_field TO old_field;

-- 回滚索引
DROP INDEX IF EXISTS idx_new_index;
```

### 5.4 验证与测试方案

#### 5.4.1 功能验证

| 验证项 | 方法 | 预期结果 |
|--------|------|----------|
| 表创建 | `SELECT COUNT(*) FROM information_schema.tables` | 129个表 |
| 外键约束 | `SELECT COUNT(*) FROM information_schema.table_constraints` | 95%+覆盖率 |
| 索引存在 | `SELECT COUNT(*) FROM pg_indexes` | 200+索引 |
| 字段命名 | 自定义检查脚本 | 0个问题 |

#### 5.4.2 性能验证

| 测试项 | 方法 | 基准 |
|--------|------|------|
| 查询响应 | EXPLAIN ANALYZE | <100ms |
| 插入性能 | 批量插入10000条 | <5s |
| 联合查询 | JOIN测试 | <500ms |

#### 5.4.3 集成测试

```rust
#[tokio::test]
async fn test_database_integrity() {
    // 验证外键
    let result = sqlx::query(
        "SELECT COUNT(*) FROM users u 
         LEFT JOIN devices d ON u.user_id = d.user_id 
         WHERE d.user_id IS NULL"
    ).fetch_one(&pool).await;
    
    assert_eq!(result, 0);
}
```

### 5.5 性能监控机制

#### 5.5.1 关键指标监控

```sql
-- 创建性能监控视图
CREATE VIEW db_performance_metrics AS
SELECT 
    schemaname,
    tablename,
    idx_scan,
    seq_scan,
    idx_tup_read,
    seq_tup_read,
    n_tup_ins,
    n_tup_upd,
    n_tup_del
FROM pg_stat_user_tables
ORDER BY seq_scan DESC;

-- 查询慢查询
SELECT 
    query,
    calls,
    mean_time,
    total_time
FROM pg_stat_statements
ORDER BY mean_time DESC
LIMIT 10;
```

#### 5.5.2 告警配置

```yaml
# prometheus alerting rules
- alert: DatabaseConnectionHigh
  expr: pg_stat_activity_count > 80
  for: 5m
  
- alert: SlowQueryDetected
  expr: pg_stat_statements_mean_time