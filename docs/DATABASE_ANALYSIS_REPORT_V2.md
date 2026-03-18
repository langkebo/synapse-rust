# 数据库架构全面分析和优化方案

> 版本: v2.0.0
> 更新日期: 2026-03-14
> 项目: synapse-rust

---

## 一、数据库架构评估

### 1.1 当前状态

| 指标 | 数值 | 评估 |
|------|------|------|
| 数据库表总数 | 129 | ✅ 完整 |
| 外键约束数 | 49 | ⚠️ 不足 (覆盖率 38%) |
| 迁移文件数 | 30+ | ⚠️ 冗余 |
| 代码模块数 | 58 | ✅ 完整 |

### 1.2 识别的问题

#### 问题 1: 外键约束严重缺失

**现状**: 129 个表中只有 49 个有外键约束
**风险**: 
- 孤立数据无法自动清理
- 数据完整性依赖应用层保证
- 级联删除失效

**受影响的主要表**:
- devices → users (缺失)
- room_memberships → users, rooms (缺失)
- events → rooms, users (缺失)
- notifications → users, rooms (缺失)
- device_keys → users, devices (缺失)
- pushers → users (缺失)
- filters → users (缺失)

#### 问题 2: 迁移脚本冗余

| 问题 | 数量 |
|------|------|
| 可合并的迁移文件 | 8+ |
| 重复字段定义 | 12+ |
| 命名冲突文件 | 2 |

#### 问题 3: 字段命名不一致风险

根据 `DATABASE_FIELD_STANDARDS.md` 规范:

| 规范字段 | 禁止变体 | 检查状态 |
|----------|----------|----------|
| `created_ts` | `created_at` | ✅ 已统一 |
| `updated_ts` | `updated_at` | ✅ 已统一 |
| `expires_at` | `expires_ts` | ✅ 已统一 |
| `is_revoked` | `revoked`, `invalidated` | ✅ 已统一 |
| `is_enabled` | `enabled` | ⚠️ 待检查 |

---

## 二、迁移脚本整合优化

### 2.1 当前迁移文件分析

```
migrations/
├── 核心架构
│   └── 00000000_unified_schema_v6.sql (99KB)
│
├── 增量迁移 (30个文件)
│   ├── 20260309* (密码安全)
│   ├── 20260310* (E2EE + 优化)
│   ├── 20260311* (Space + 修复)
│   ├── 20260313* (Room Tags + QR)
│   ├── 20260314* (Widget + 索引)
│   └── 20260315* (字段 + 外键 + 性能)
│
├── 统一迁移
│   └── 202603150000_unified_migration.sql (14KB)
│
└── archive/
    └── 20260313000000_unified_migration_optimized.sql
```

### 2.2 已完成优化

✅ 删除冗余备份文件 `00000000_unified_schema_v6.sql.bak`
✅ 归档重复文件 `20260313000000_unified_migration_optimized.sql`
✅ 创建统一迁移脚本 `202603150000_unified_migration.sql`

### 2.3 待整合的迁移文件

以下文件可以合并或归档:

| 文件 | 建议操作 |
|------|----------|
| 202603150000_unified_migration.sql | 保留 (已合并) |
| 20260315000007_add_foreign_key_constraints.sql | 已合并到统一迁移 |
| 20260315000008_performance_optimization.sql | 已合并到统一迁移 |

---

## 三、完整数据库表结构清单

### 3.1 核心表 (Core) - 8个

| 表名 | 主键 | 外键 | 索引 | 说明 |
|------|------|------|------|------|
| `users` | user_id | - | username, is_admin | 用户主表 |
| `devices` | device_id | user_id ✅ | user_id | 设备表 |
| `access_tokens` | id | user_id, device_id | token | 访问令牌 |
| `refresh_tokens` | id | user_id | token_hash | 刷新令牌 |
| `user_account_data` | (user_id, type) | user_id | user_id | 用户数据 |
| `user_filters` | id | user_id | user_id | 过滤器 |
| `user_directory` | user_id | - | user_id | 用户目录 |
| `user_threepids` | id | user_id | user_id | 第三方ID |

### 3.2 房间表 (Room) - 11个

| 表名 | 主键 | 外键 | 索引 | 说明 |
|------|------|------|------|------|
| `rooms` | room_id | - | creator | 房间主表 |
| `room_memberships` | (room_id, user_id) | user_id, room_id | user_id, membership | 成员关系 |
| `room_state_events` | (room_id, type, state_key) | room_id | room_id, type | 状态事件 |
| `room_events` | (room_id, event_id) | room_id | room_id, ts | 房间事件 |
| `room_aliases` | (room_id, alias) | room_id | room_id, alias | 房间别名 |
| `room_directory` | room_id | - | room_id, is_public | 房间目录 |
| `room_tags` | (room_id, user_id, tag) | room_id, user_id | user_id, tag | 房间标签 |
| `room_account_data` | (room_id, user_id, type) | room_id, user_id | user_id | 房间数据 |
| `room_summaries` | room_id | - | room_id | 房间摘要 |
| `room_parents` | (room_id, parent_id) | room_id, parent_id | parent_id | 房间层级 |
| `room_invites` | (room_id, user_id) | user_id | user_id | 邀请 |

### 3.3 事件表 (Event) - 7个

| 表名 | 主键 | 外键 | 索引 | 说明 |
|------|------|------|------|------|
| `events` | event_id | room_id, sender | room_id, stream | 事件主表 |
| `event_receipts` | 复合 | room_id, event_id | room_id, user_id | 收据 |
| `read_markers` | 复合 | room_id, user_id | user_id | 已读标记 |
| `typing` | (room_id, user_id) | room_id, user_id | room_id | 打字提示 |
| `event_reports` | id | room_id, user_id | room_id | 事件报告 |
| `event_signatures` | id | - | - | 事件签名 |
| `event_report_stats` | id | - | - | 报告统计 |

### 3.4 设备与安全表 (Device & Security) - 12个

| 表名 | 主键 | 外键 | 索引 | 说明 |
|------|------|------|------|------|
| `device_keys` | 复合 | user_id, device_id | user_id | 设备密钥 |
| `device_signatures` | id | user_id, device_id | user_id | 设备签名 |
| `cross_signing_keys` | (user_id, key_type) | user_id | user_id | 交叉签名 |
| `olm_accounts` | account_id | user_id | user_id | Olm账户 |
| `olm_sessions` | session_id | account_id | account_id | Olm会话 |
| `one_time_keys` | 复合 | session_id | session_id | 一次性密钥 |
| `key_backups` | backup_id | user_id | user_id | 密钥备份 |
| `backup_keys` | 复合 | backup_id | backup_id | 备份密钥 |
| `e2ee_key_requests` | request_id | user_id | user_id | E2EE请求 |
| `account_validity` | user_id | user_id | user_id | 账户有效期 |
| `password_history` | id | user_id | user_id | 密码历史 |
| `password_policy` | id | - | - | 密码策略 |

### 3.5 推送表 (Push) - 5个

| 表名 | 主键 | 外键 | 索引 | 说明 |
|------|------|------|------|------|
| `pushers` | id | user_id | user_id, pushkey | 推送器 |
| `push_devices` | id | user_id, device_id | user_id | 推送设备 |
| `push_notification_queue` | id | user_id, device_id | status | 推送队列 |
| `push_notification_log` | id | - | - | 推送日志 |
| `push_config` | id | - | - | 推送配置 |

### 3.6 认证表 (Auth) - 8个

| 表名 | 主键 | 外键 | 索引 | 说明 |
|------|------|------|------|------|
| `saml_sessions` | id | user_id | user_id | SAML会话 |
| `cas_sessions` | id | user_id | user_id | CAS会话 |
| `oidc_sessions` | id | user_id | user_id | OIDC会话 |
| `openid_tokens` | id | user_id | user_id | OpenID令牌 |
| `registration_tokens` | token | - | token | 注册令牌 |
| `registration_token_usage` | id | token | token | 令牌使用 |
| `captcha_config` | id | - | - | 验证码配置 |
| `captcha_send_log` | id | - | - | 发送日志 |

### 3.7 应用服务表 (AS) - 6个

| 表名 | 主键 | 外键 | 索引 | 说明 |
|------|------|------|------|------|
| `application_services` | id | - | as_token | 应用服务 |
| `application_service_room_namespaces` | id | as_id | as_id | 房间命名空间 |
| `application_service_user_namespaces` | id | as_id | as_id | 用户命名空间 |
| `application_service_transactions` | id | as_id | as_id | 事务 |
| `application_service_events` | id | - | - | 服务事件 |
| `application_service_state` | id | - | - | 服务状态 |

### 3.8 联邦与同步表 (Federation) - 8个

| 表名 | 主键 | 外键 | 索引 | 说明 |
|------|------|------|------|------|
| `federation_servers` | server_name | - | server_name | 联邦服务器 |
| `federation_queue` | id | - | destination | 联邦队列 |
| `federation_blacklist` | id | - | server_name | 黑名单 |
| `sliding_sync_rooms` | room_id | user_id | user_id | 滑动同步 |
| `sync_stream_id` | id | - | - | 同步流 |
| `device_lists_changes` | id | - | user_id | 设备列表变更 |
| `device_lists_stream` | id | - | - | 设备列表流 |
| `filters` | id | user_id | user_id | 过滤器 |

### 3.9 Space 表 - 3个

| 表名 | 主键 | 外键 | 索引 | 说明 |
|------|------|------|------|------|
| `spaces` | room_id | room_id | room_id | Space主表 |
| `space_children` | 复合 | parent_id, child_id | parent_id | Space子项 |
| `space_hierarchy` | 复合 | room_id | room_id | Space层级 |

### 3.10 线程表 - 2个

| 表名 | 主键 | 外键 | 索引 | 说明 |
|------|------|------|------|------|
| `thread_roots` | 复合 | room_id, event_id | room_id | 线程根 |
| `thread_subscriptions` | 复合 | room_id, user_id | user_id | 线程订阅 |

### 3.11 媒体表 - 6个

| 表名 | 主键 | 外键 | 索引 | 说明 |
|------|------|------|------|------|
| `media_metadata` | media_id | - | media_id | 媒体元数据 |
| `thumbnails` | media_id | - | media_id | 缩略图 |
| `media_quota` | server_name | - | server_name | 服务器配额 |
| `media_quota_config` | id | - | - | 配额配置 |
| `user_media_quota` | user_id | user_id | user_id | 用户配额 |
| `media_callbacks` | id | - | - | 媒体回调 |

### 3.12 好友表 - 3个

| 表名 | 主键 | 外键 | 索引 | 说明 |
|------|------|------|------|------|
| `friends` | id | user_id, friend_user_id | user_id | 好友 |
| `friend_requests` | id | user_id, requester_id | user_id | 好友请求 |
| `friend_categories` | id | user_id | user_id | 好友分类 |

### 3.13 管理表 - 8个

| 表名 | 主键 | 外键 | 索引 | 说明 |
|------|------|------|------|------|
| `server_notices` | user_id | user_id | user_id | 服务器通知 |
| `shadow_bans` | user_id | user_id | user_id | 影子封禁 |
| `rate_limits` | id | user_id | user_id | 速率限制 |
| `blocked_users` | user_id | user_id | user_id | 封禁用户 |
| `blocked_rooms` | room_id | room_id | room_id | 封禁房间 |
| `report_rate_limits` | id | - | - | 报告限制 |
| `event_report_history` | id | - | - | 报告历史 |
| `spam_check_results` | id | - | - | 垃圾检查 |

### 3.14 保留策略表 - 3个

| 表名 | 主键 | 外键 | 索引 | 说明 |
|------|------|------|------|------|
| `server_retention_policy` | server_name | - | server_name | 服务器策略 |
| `room_retention_policy` | room_id | room_id | room_id | 房间策略 |
| `retention_events` | room_id | - | room_id | 保留事件 |

### 3.15 Presence 表 - 2个

| 表名 | 主键 | 外键 | 索引 | 说明 |
|------|------|------|------|------|
| `presence` | user_id | - | user_id | 在线状态 |
| `presence_routes` | id | - | - | 状态路由 |
| `presence_subscriptions` | 复合 | - | observer_user_id | 状态订阅 |

### 3.16 Worker 表 - 4个

| 表名 | 主键 | 外键 | 索引 | 说明 |
|------|------|------|------|------|
| `workers` | id | - | - | Worker信息 |
| `worker_commands` | id | - | - | Worker命令 |
| `worker_events` | id | - | - | Worker事件 |
| `worker_statistics` | id | - | - | Worker统计 |

### 3.17 其他表 - 15个

| 表名 | 说明 |
|------|------|
| `account_data` | 账户数据 |
| `background_updates` | 后台更新 |
| `search_index` | 搜索索引 |
| `modules` | 模块 |
| `module_execution_logs` | 模块执行日志 |
| `schema_migrations` | 架构迁移 |
| `security_events` | 安全事件 |
| `user_privacy_settings` | 隐私设置 |
| `rendezvous_session` | 会议会话 |
| `delayed_events` | 延迟事件 |
| `third_party_rule_results` | 第三方规则 |
| `ip_blocks` | IP封禁 |
| `ip_reputation` | IP信誉 |
| `connection_monitor` | 连接监控 |
| `private_messages` | 私信 |

---

## 四、数据库优化技能

### 4.1 索引优化策略

#### 4.1.1 必需复合索引

```sql
-- 用户设备查询
CREATE INDEX idx_devices_user_last_seen ON devices(user_id, last_seen_ts DESC);

-- 房间成员查询 (按加入时间)
CREATE INDEX idx_room_memberships_joined ON room_memberships(room_id, membership, joined_ts DESC)
    WHERE membership = 'join';

-- 事件时间线查询
CREATE INDEX idx_events_room_time_type ON events(room_id, origin_server_ts DESC, type);

-- 通知查询优化
CREATE INDEX idx_notifications_user_room ON notifications(user_id, room_id, stream_ordering DESC);
```

#### 4.1.2 部分索引

```sql
-- 仅索引有效Token
CREATE INDEX idx_access_tokens_valid ON access_tokens(user_id, created_ts DESC)
    WHERE is_valid = TRUE;

-- 仅索引未撤销刷新Token
CREATE INDEX idx_refresh_tokens_active ON refresh_tokens(user_id, created_ts DESC)
    WHERE is_revoked = FALSE;

-- 仅索引公开房间
CREATE INDEX idx_rooms_public ON rooms(room_id, created_ts DESC)
    WHERE is_public = TRUE;
```

### 4.2 查询性能提升

#### 4.2.1 分页优化

```sql
-- 游标分页 (优于 OFFSET)
SELECT * FROM events 
WHERE room_id = $1 
AND origin_server_ts < $cursor_ts
ORDER BY origin_server_ts DESC 
LIMIT 100;
```

#### 4.2.2 批量操作

```sql
-- 批量插入
INSERT INTO device_keys (user_id, device_id, key_data) VALUES 
    ($1, $2, $3),
    ($4, $5, $6),
    ($7, $8, $9);
```

### 4.3 事务管理

```sql
-- 死锁预防: 固定访问顺序
BEGIN;
UPDATE room_memberships WHERE user_id = $2;
UPDATE rooms SET ... WHERE room_id = $1;
COMMIT;
```

### 4.4 备份策略

| 类型 | 频率 | 保留 |
|------|------|------|
| 全量 | 每天 | 30天 |
| 增量 | 每小时 | 7天 |
| WAL | 实时 | 3天 |

```bash
pg_dump -Fc synapse > backup_$(date +%Y%m%d).dump
pg_basebackup -D backup_incr -X stream -Ft -z -z6
```

### 4.5 审计日志

```sql
CREATE TABLE audit_log (
    id BIGSERIAL PRIMARY KEY,
    table_name TEXT,
    operation TEXT,
    old_data JSONB,
    new_data JSONB,
    user_id TEXT,
    created_ts BIGINT NOT NULL
);

-- 创建触发器函数
CREATE OR REPLACE FUNCTION audit_trigger() RETURNS TRIGGER AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        INSERT INTO audit_log VALUES (TG_TABLE_NAME, 'INSERT', NULL, row_to_json(NEW), ...);
    END IF;
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;
```

---

## 五、重构方案

### 5.1 重构目标

| 目标 | 现状 | 目标 |
|------|------|------|
| 外键覆盖率 | 38% | 95% |
| 索引完整性 | 缺失20+ | 完整 |
| 迁移冗余 | 8+文件 | 0 |

### 5.2 实施步骤

#### 阶段1: 准备 (Day 1)
- [x] 数据库备份
- [x] 创建检查脚本
- [x] 分析现有问题

#### 阶段2: 字段标准化 (Day 2)
- [x] 创建统一迁移脚本
- [x] 验证字段命名一致性

#### 阶段3: 外键补全 (Day 3)
- [x] 创建外键迁移
- [x] 执行外键添加

#### 阶段4: 索引优化 (Day 4)
- [x] 创建性能优化迁移
- [x] 添加必需索引

#### 阶段5: 清理 (Day 5)
- [x] 删除冗余文件
- [x] 更新文档

### 5.3 验证清单

```sql
-- 验证外键数量
SELECT COUNT(*) FROM information_schema.table_constraints 
WHERE constraint_type = 'FOREIGN KEY';

-- 验证索引数量
SELECT COUNT(*) FROM pg_indexes WHERE schemaname = 'public';

-- 验证数据完整性
SELECT COUNT(*) FROM devices WHERE user_id NOT IN (SELECT user_id FROM users);
```

---

## 六、数据库一致性保障

### 6.1 代码与数据库对齐

| 检查项 | 方法 |
|--------|------|
| 字段名匹配 | SQL查询 vs Rust结构体 |
| 类型匹配 | PostgreSQL vs Rust类型 |
| 约束匹配 | 索引 vs 查询模式 |

### 6.2 自动化检查

```bash
# 字段一致性检查
psql -d synapse -f scripts/check_field_consistency.sql

# 外键完整性检查
psql -d synapse -c "SELECT * FROM foreign_key_check()"

# 索引完整性检查  
psql -d synapse -c "SELECT * FROM index_coverage_check()"
```

### 6.3 文档同步

- 字段变更 → 更新 `DATABASE_FIELD_STANDARDS.md`
- 新表 → 更新 `MIGRATION_INDEX.md`
- 重大变更 → 更新 `project_rules.md`

---

## 七、变更日志

### 2026-03-14
- 创建完整数据库分析报告
- 创建统一迁移脚本
- 添加外键约束迁移
- 添加性能优化迁移
- 归档冗余文件
- 创建字段一致性检查脚本

### 2026-03-13
- 初始版本
- 统一Schema创建
