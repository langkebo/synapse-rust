# SQL 迁移文件与 Rust 运行时数据库 Schema 差异报告

> 文档版本: v1.0.0
> 创建日期: 2026-03-26
> 分析文件:
> - SQL迁移: `migrations/00000000_unified_schema_v6.sql` (2747行)
> - Rust初始化: `src/services/database_initializer.rs` (1331行)

---

## 1. 概述

本报告详细比较 SQL 迁移文件定义的数据库 schema 与 Rust 运行时实际创建的 schema 之间的差异。

### 1.1 统计信息

| 项目 | 数量 |
|------|------|
| SQL迁移文件定义表数 | 130+ |
| Rust运行时创建表数 | ~20 (直接创建) |
| Rust运行时添加列数 | 3 |
| 仅存在于SQL迁移的表 | 110+ |
| 仅存在于Rust运行时的表 | 0 |
| 列定义差异 | 2 |
| 主键定义差异 | 1 |

### 1.2 严重程度分级

| 等级 | 说明 |
|------|------|
| P0 | 致命问题 - 会导致数据库操作失败或数据丢失 |
| P1 | 严重问题 - 可能导致功能异常或性能问题 |
| P2 | 中等问题 - 建议修复，不影响基本功能 |
| P3 | 轻微问题 - 代码质量问题 |

---

## 2. 仅存在于 SQL 迁移文件的表

这些表在 SQL 迁移文件中定义，但 Rust 运行时不会主动创建（假设通过迁移文件创建或已存在）。

### 2.1 核心用户与认证表

| 表名 | 严重程度 | 说明 |
|------|----------|------|
| users | P0 | 用户表，Rust代码依赖此表存在 |
| devices | P0 | 设备表，Rust代码依赖此表存在 |
| access_tokens | P0 | 访问令牌表 |
| refresh_tokens | P1 | 刷新令牌表（Rust添加了expires_at列） |
| token_blacklist | P2 | Token黑名单表 |
| user_threepids | P2 | 用户第三方身份表 |

### 2.2 房间与事件表

| 表名 | 严重程度 | 说明 |
|------|----------|------|
| rooms | P0 | 房间表（Rust添加了guest_access列） |
| room_memberships | P0 | 房间成员表 |
| events | P0 | 事件表 |
| room_summaries | P1 | 房间摘要表 |
| room_directory | P2 | 房间目录表 |
| room_aliases | P2 | 房间别名表 |
| thread_roots | P1 | 线程根消息表 |
| room_parents | P2 | 房间父子关系表 |

### 2.3 E2EE加密表

| 表名 | 严重程度 | 说明 |
|------|----------|------|
| cross_signing_keys | P1 | 跨签名密钥表 |
| megolm_sessions | P1 | Megolm会话表 |
| event_signatures | P2 | 事件签名表 |
| device_signatures | P2 | 设备签名表 |
| backup_keys | P1 | 密钥备份数据表 |
| olm_accounts | P1 | Olm账户表 |
| olm_sessions | P1 | Olm会话表 |
| e2ee_key_requests | P2 | E2EE密钥请求表 |
| one_time_keys | P1 | 一次性密钥表 |

### 2.4 媒体存储表

| 表名 | 严重程度 | 说明 |
|------|----------|------|
| media_metadata | P1 | 媒体元数据表 |
| thumbnails | P2 | 缩略图表 |
| media_quota | P2 | 媒体配额表 |

### 2.5 推送通知表

| 表名 | 严重程度 | 说明 |
|------|----------|------|
| push_rules | P1 | 推送规则表 |
| push_devices | P2 | 推送设备表 |
| push_notification_queue | P2 | 推送通知队列表 |
| push_notification_log | P3 | 推送通知日志表 |
| push_config | P3 | 推送配置表 |
| notifications | P2 | 通知表 |

### 2.6 认证与安全表

| 表名 | 严重程度 | 说明 |
|------|----------|------|
| password_history | P2 | 密码历史表 |
| password_policy | P2 | 密码策略表 |
| ip_blocks | P2 | IP封禁表 |
| ip_reputation | P3 | IP信誉表 |
| security_events | P3 | 安全事件表 |

### 2.7 CAS/SAML认证表

| 表名 | 严重程度 | 说明 |
|------|----------|------|
| cas_tickets | P2 | CAS票据表 |
| cas_proxy_tickets | P2 | CAS代理票据表 |
| cas_proxy_granting_tickets | P2 | CAS代理授予票据表 |
| cas_services | P2 | CAS服务表 |
| cas_user_attributes | P2 | CAS用户属性表 |
| cas_slo_sessions | P3 | CAS单点登出会话表 |
| saml_sessions | P2 | SAML会话表 |
| saml_user_mapping | P2 | SAML用户映射表 |
| saml_identity_providers | P2 | SAML身份提供商表 |
| saml_auth_events | P3 | SAML认证事件表 |
| saml_logout_requests | P3 | SAML登出请求表 |

### 2.8 验证码相关表

| 表名 | 严重程度 | 说明 |
|------|----------|------|
| registration_captcha | P2 | 注册验证码表 |
| captcha_send_log | P3 | 验证码发送日志表 |
| captcha_template | P3 | 验证码模板表 |
| captcha_config | P3 | 验证码配置表 |

### 2.9 联邦相关表

| 表名 | 严重程度 | 说明 |
|------|----------|------|
| federation_servers | P1 | 联邦服务器表 |
| federation_blacklist | P2 | 联邦黑名单表 |
| federation_queue | P1 | 联邦队列表 |

### 2.10 后台任务与Worker表

| 表名 | 严重程度 | 说明 |
|------|----------|------|
| background_updates | P1 | 后台更新表 |
| workers | P2 | 工作进程表 |
| worker_commands | P2 | Worker命令表 |
| worker_events | P2 | Worker事件表 |
| worker_statistics | P3 | Worker统计表 |

### 2.11 应用服务表

| 表名 | 严重程度 | 说明 |
|------|----------|------|
| application_services | P2 | 应用服务表 |
| application_service_state | P2 | 应用服务状态表 |
| application_service_transactions | P2 | 应用服务事务表 |
| application_service_events | P2 | 应用服务事件表 |
| application_service_user_namespaces | P2 | 应用服务用户命名空间表 |
| application_service_room_alias_namespaces | P2 | 应用服务房间别名命名空间表 |
| application_service_room_namespaces | P2 | 应用服务房间命名空间表 |

### 2.12 账户数据表

| 表名 | 严重程度 | 说明 |
|------|----------|------|
| filters | P2 | 用户过滤器表 |
| openid_tokens | P2 | OpenID令牌表 |
| room_account_data | P2 | 房间账户数据表 |
| user_account_data | P2 | 用户账户数据表 |

### 2.13 Space相关表

| 表名 | 严重程度 | 说明 |
|------|----------|------|
| spaces | P1 | Space主表 |

### 2.14 举报与审核表

| 表名 | 严重程度 | 说明 |
|------|----------|------|
| event_reports | P2 | 事件举报表 |
| event_report_history | P3 | 事件举报历史表 |
| report_rate_limits | P3 | 举报速率限制表 |
| event_report_stats | P3 | 举报统计表 |

### 2.15 注册令牌表

| 表名 | 严重程度 | 说明 |
|------|----------|------|
| registration_tokens | P2 | 注册令牌表 |
| registration_token_usage | P3 | 注册令牌使用记录表 |

### 2.16 语音消息表

| 表名 | 严重程度 | 说明 |
|------|----------|------|
| voice_messages | P2 | 语音消息表 |
| voice_usage_stats | P3 | 语音使用统计表 |

### 2.17 好友与社交表

| 表名 | 严重程度 | 说明 |
|------|----------|------|
| friends | P2 | 好友表 |
| friend_requests | P2 | 好友请求表 |
| friend_categories | P3 | 好友分类表 |
| blocked_users | P2 | 屏蔽用户表 |

### 2.18 私密消息表

| 表名 | 严重程度 | 说明 |
|------|----------|------|
| private_sessions | P2 | 私密会话表 |
| private_messages | P2 | 私密消息表 |

### 2.19 读标记与回执表

| 表名 | 严重程度 | 说明 |
|------|----------|------|
| read_markers | P2 | 读标记表 |
| event_receipts | P2 | 事件回执表 |
| room_state_events | P2 | 房间状态事件表 |

### 2.20 刷新令牌追踪表

| 表名 | 严重程度 | 说明 |
|------|----------|------|
| refresh_token_usage | P3 | 刷新令牌使用记录表 |
| refresh_token_families | P3 | 刷新令牌家族表 |
| refresh_token_rotations | P3 | 刷新令牌轮换表 |

### 2.21 房间邀请表

| 表名 | 严重程度 | 说明 |
|------|----------|------|
| room_invites | P2 | 房间邀请表 |

### 2.22 Presence表

| 表名 | 严重程度 | 说明 |
|------|----------|------|
| presence | P1 | 在线状态表 |

### 2.23 其他表

| 表名 | 严重程度 | 说明 |
|------|----------|------|
| modules | P3 | 模块管理表 |
| module_execution_logs | P3 | 模块执行日志表 |
| spam_check_results | P3 | 垃圾信息检查结果表 |
| third_party_rule_results | P3 | 第三方规则结果表 |
| account_validity | P2 | 账户有效性表 |
| password_auth_providers | P3 | 密码认证提供者表 |
| presence_routes | P3 | Presence路由表 |
| media_callbacks | P3 | 媒体回调表 |
| rate_limit_callbacks | P3 | 速率限制回调表 |
| account_data_callbacks | P3 | 账户数据回调表 |
| key_rotation_history | P2 | 密钥轮转历史表 |
| blocked_rooms | P2 | 房间封禁表 |
| server_retention_policy | P2 | 保留策略表 |
| user_media_quota | P2 | 用户媒体配额表 |
| media_quota_config | P3 | 媒体配额配置表 |
| rendezvous_session | P2 | Rendezvous会话表 |
| schema_migrations | P1 | 迁移记录表（Rust部分实现） |
| db_metadata | P1 | 数据库元数据表 |

---

## 3. 仅存在于 Rust 运行时的表

**无** - 所有在 Rust 运行时创建的表都已在 SQL 迁移文件中定义。

---

## 4. 列定义差异

### 4.1 device_keys 表 - 主键定义差异

**严重程度: P1**

#### SQL迁移定义 (migrations/00000000_unified_schema_v6.sql:376-394)
```sql
CREATE TABLE IF NOT EXISTS device_keys (
    id BIGSERIAL,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    algorithm TEXT NOT NULL,
    key_id TEXT NOT NULL,
    public_key TEXT NOT NULL,
    key_data TEXT,
    signatures JSONB,
    added_ts BIGINT NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    ts_updated_ms BIGINT,
    is_verified BOOLEAN DEFAULT FALSE,
    is_blocked BOOLEAN DEFAULT FALSE,
    display_name TEXT,
    CONSTRAINT pk_device_keys PRIMARY KEY (id),
    CONSTRAINT uq_device_keys_user_device_key UNIQUE (user_id, device_id, key_id)
);
```

#### Rust运行时定义 (database_initializer.rs:623-646)
```sql
CREATE TABLE IF NOT EXISTS device_keys (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    algorithm TEXT NOT NULL,
    key_id TEXT NOT NULL,
    public_key TEXT NOT NULL,
    key_data TEXT,
    signatures JSONB,
    added_ts BIGINT NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    ts_updated_ms BIGINT,
    is_verified BOOLEAN DEFAULT FALSE,
    is_blocked BOOLEAN DEFAULT FALSE,
    display_name TEXT,
    CONSTRAINT uq_device_keys_user_device_key UNIQUE (user_id, device_id, key_id)
)
```

#### 差异分析

| 项目 | SQL迁移 | Rust运行时 |
|------|---------|------------|
| id列定义 | `id BIGSERIAL` + `CONSTRAINT pk_device_keys PRIMARY KEY (id)` | `id BIGSERIAL PRIMARY KEY` |
| 主键约束命名 | 显式命名 `pk_device_keys` | 隐式未命名 |

**影响**: 虽然功能上等价（都是将 `id` 设置为主键），但SQL迁移多了一个命名的主键约束。这可能导致轻微的维护不一致。

**建议**: 统一使用 `id BIGSERIAL PRIMARY KEY` 格式（Rust风格），或保持SQL迁移的显式命名风格。

---

### 4.2 rooms 表 - guest_access 列类型差异

**严重程度: P2**

#### SQL迁移定义 (migrations/00000000_unified_schema_v6.sql:176-193)
```sql
CREATE TABLE IF NOT EXISTS rooms (
    room_id TEXT NOT NULL,
    creator TEXT,
    is_public BOOLEAN DEFAULT FALSE,
    room_version TEXT DEFAULT '6',
    created_ts BIGINT NOT NULL,
    last_activity_ts BIGINT,
    is_federated BOOLEAN DEFAULT TRUE,
    has_guest_access BOOLEAN DEFAULT FALSE,  -- BOOLEAN类型
    join_rules TEXT DEFAULT 'invite',
    ...
);
```

#### Rust运行时添加 (database_initializer.rs:869-871)
```sql
ALTER TABLE rooms ADD COLUMN IF NOT EXISTS guest_access VARCHAR(50) DEFAULT 'forbidden'
```

#### 差异分析

| 项目 | SQL迁移 | Rust运行时 |
|------|---------|------------|
| 列名 | `has_guest_access` | `guest_access` |
| 数据类型 | `BOOLEAN` | `VARCHAR(50)` |
| 默认值 | `FALSE` | `'forbidden'` |

**影响**: 这两个列名不同，但语义相关。SQL迁移用 `has_guest_access` (布尔) 表示是否有来宾访问权限，而Rust运行时添加的 `guest_access` (VARCHAR) 用于 RoomSummary 兼容性，存储 `'forbidden'`/`'can_join'` 等值。

**建议**: 这是两个不同用途的列，应在代码中明确区分使用场景。

---

## 5. Rust 运行时额外添加的列

### 5.1 users.is_guest 列

**严重程度: P2**

```sql
ALTER TABLE users ADD COLUMN IF NOT EXISTS is_guest BOOLEAN DEFAULT FALSE
```

**分析**:
- SQL迁移的 users 表已定义 `is_guest BOOLEAN DEFAULT FALSE` (第34行)
- Rust代码重复执行此 ALTER TABLE 语句
- 影响: 无害（`IF NOT EXISTS` 保证不会重复添加）

### 5.2 rooms.guest_access 列

**严重程度: P2**

```sql
ALTER TABLE rooms ADD COLUMN IF NOT EXISTS guest_access VARCHAR(50) DEFAULT 'forbidden'
```

**分析**:
- SQL迁移的 rooms 表使用 `has_guest_access BOOLEAN`
- Rust代码添加 `guest_access VARCHAR(50)` 用于 RoomSummary 兼容性
- 这是两个不同的列，用途不同

### 5.3 refresh_tokens.expires_at 列

**严重程度: P2**

```sql
ALTER TABLE refresh_tokens ADD COLUMN IF NOT EXISTS expires_at BIGINT
```

**分析**:
- SQL迁移的 refresh_tokens 表已定义 `expires_at BIGINT` (第136行)
- Rust代码重复执行此 ALTER TABLE 语句
- 影响: 无害（`IF NOT EXISTS` 保证不会重复添加）

---

## 6. 索引定义差异

### 6.1 search_index 表

#### SQL迁移定义 (migrations/00000000_unified_schema_v6.sql:2086-2102)
```sql
CREATE TABLE IF NOT EXISTS search_index (
    id SERIAL PRIMARY KEY,
    event_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    event_type VARCHAR(255) NOT NULL,
    type VARCHAR(255) NOT NULL,
    content TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    CONSTRAINT uq_search_index_event UNIQUE (event_id)
);

CREATE INDEX IF NOT EXISTS idx_search_index_room ON search_index(room_id);
CREATE INDEX IF NOT EXISTS idx_search_index_user ON search_index(user_id);
CREATE INDEX IF NOT EXISTS idx_search_index_type ON search_index(event_type);
```

#### Rust运行时定义 (database_initializer.rs:676-717)
```sql
CREATE TABLE IF NOT EXISTS search_index ( ... )  -- 与SQL迁移一致

CREATE INDEX IF NOT EXISTS idx_search_index_room ON search_index(room_id)
CREATE INDEX IF NOT EXISTS idx_search_index_user ON search_index(user_id)
CREATE INDEX IF NOT EXISTS idx_search_index_type ON search_index(event_type)
```

**差异**: 无

### 6.2 user_directory 表

#### SQL迁移定义 (migrations/00000000_unified_schema_v6.sql:2078-2084)
```sql
CREATE TABLE IF NOT EXISTS typing (
    user_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    typing BOOLEAN DEFAULT FALSE,
    last_active_ts BIGINT NOT NULL,
    UNIQUE (user_id, room_id)
);
```

#### Rust运行时定义 (database_initializer.rs:661-673)
```sql
CREATE TABLE IF NOT EXISTS typing ( ... )  -- 与SQL迁移一致
```

**差异**: 无

---

## 7. 架构流程分析

### 7.1 数据库初始化流程

```
1. step_connection_test()      - 测试数据库连接
2. step_migrations()           - 执行迁移文件
   └── ensure_schema_migrations_table()
   └── run_runtime_migrations()
       └── 读取 migrations/ 目录下的所有 .sql 文件
       └── 执行每个迁移文件中的 SQL 语句
3. step_create_e2ee_tables()    - 创建 E2EE 相关表（device_keys）
4. step_ensure_additional_tables() - 确保附加表存在
   └── 为多个表执行 CREATE TABLE IF NOT EXISTS
   └── 执行 ALTER TABLE 添加可能缺失的列
5. step_schema_validation()     - 验证 schema
6. step_index_validation()     - 验证索引
7. step_create_indexes()       - 创建缺失的索引
```

### 7.2 表创建顺序

| 顺序 | 表名 | 来源 |
|------|------|------|
| 1 | schema_migrations | Rust代码内置 |
| 2 | device_keys | Rust代码 |
| 3-22 | typing, search_index, user_directory, user_privacy_settings, pushers, account_data, key_backups, room_tags, room_events, to_device_messages, device_lists_changes, room_ephemeral, device_lists_stream, user_filters, sync_stream_id, sliding_sync_rooms, thread_subscriptions, space_children, space_hierarchy | Rust代码 |
| 23+ | 所有其他表 | 迁移文件 |

---

## 8. 问题汇总与修复建议

### 8.1 P0 问题（致命）

| # | 问题 | 建议 |
|---|------|------|
| 1 | 核心表(users, devices, rooms, events, room_memberships)依赖迁移文件创建 | 确保迁移文件在首次部署时执行 |
| 2 | 如果迁移文件缺失或执行失败，Rust运行时无法自动创建这些核心表 | 考虑在Rust中添加核心表的创建逻辑 |

### 8.2 P1 问题（严重）

| # | 问题 | 建议 |
|---|------|------|
| 1 | device_keys 表主键定义风格不一致 | 统一为 `id BIGSERIAL PRIMARY KEY` 格式 |
| 2 | 多个重要表(olm_sessions, megolm_sessions等)依赖迁移文件 | 确保迁移文件完整性 |
| 3 | key_backups 表在Rust中缺少 mgmt_key 列 | 检查SQL迁移中的 key_backups 定义 |

### 8.3 P2 问题（中等）

| # | 问题 | 建议 |
|---|------|------|
| 1 | rooms 表存在 has_guest_access 和 guest_access 两个相关列 | 明确区分用途，避免混淆 |
| 2 | Rust 代码重复执行 `ALTER TABLE users ADD COLUMN IF NOT EXISTS is_guest` | 由于 `IF NOT EXISTS`，影响可控 |
| 3 | Rust 代码重复执行 `ALTER TABLE refresh_tokens ADD COLUMN IF NOT EXISTS expires_at` | 由于 `IF NOT EXISTS`，影响可控 |

### 8.4 P3 问题（轻微）

| # | 问题 | 建议 |
|---|------|------|
| 1 | 多个次要表依赖迁移文件 | 如需简化，可移除未使用的表定义 |
| 2 | CAS/SAML相关表可能未被使用 | 根据实际功能启用情况决定保留 |

---

## 9. 最佳实践建议

### 9.1 表创建策略

1. **核心表（users, devices, rooms, events等）**: 应在迁移文件中定义
2. **运行时动态表**: 应在 Rust 代码中创建（使用 `CREATE TABLE IF NOT EXISTS`）
3. **避免重复**: 不要在两个地方都定义相同的表

### 9.2 列添加策略

1. **必填列**: 应在迁移文件中定义
2. **可选列/兼容性列**: 可在运行时添加
3. **使用 `ADD COLUMN IF NOT EXISTS`**: 确保幂等性

### 9.3 索引创建策略

1. **与表一起创建**: 在 `CREATE TABLE` 语句中一起定义索引
2. **单独创建**: 使用 `CREATE INDEX IF NOT EXISTS`
3. **避免重复**: 使用 `IF NOT EXISTS` 子句

---

## 10. 附录

### A. SQL迁移文件表清单 (130+)

```
users, user_threepids, devices, access_tokens, refresh_tokens, token_blacklist,
rooms, room_memberships, events, room_summaries, room_directory, room_aliases,
thread_roots, room_parents, device_keys, cross_signing_keys, megolm_sessions,
event_signatures, device_signatures, key_backups, backup_keys, olm_accounts,
olm_sessions, e2ee_key_requests, media_metadata, thumbnails, media_quota,
cas_tickets, cas_proxy_tickets, cas_proxy_granting_tickets, cas_services,
cas_user_attributes, cas_slo_sessions, saml_sessions, saml_user_mapping,
saml_identity_providers, saml_auth_events, saml_logout_requests,
registration_captcha, captcha_send_log, captcha_template, captcha_config,
push_devices, push_rules, pushers, space_children, spaces,
federation_servers, federation_blacklist, federation_queue,
filters, openid_tokens, account_data, room_account_data, user_account_data,
background_updates, workers, worker_commands, worker_events, worker_statistics,
sync_stream_id, modules, module_execution_logs, spam_check_results,
third_party_rule_results, account_validity, password_auth_providers,
presence_routes, media_callbacks, rate_limit_callbacks, account_data_callbacks,
registration_tokens, registration_token_usage, event_reports,
event_report_history, report_rate_limits, event_report_stats,
room_invites, push_notification_queue, push_notification_log,
push_config, notifications, voice_messages, voice_usage_stats,
presence, user_directory, friends, friend_requests, friend_categories,
blocked_users, key_rotation_history, blocked_rooms,
private_sessions, private_messages, security_events, ip_blocks, ip_reputation,
read_markers, event_receipts, room_state_events, refresh_token_usage,
refresh_token_families, refresh_token_rotations, application_services,
typing, search_index, user_privacy_settings, room_tags, room_events,
to_device_messages, device_lists_changes, room_ephemeral, device_lists_stream,
user_filters, sliding_sync_rooms, thread_subscriptions, space_hierarchy,
password_history, password_policy, schema_migrations, db_metadata,
server_retention_policy, user_media_quota, media_quota_config,
one_time_keys, rendezvous_session, application_service_state,
application_service_transactions, application_service_events,
application_service_user_namespaces, application_service_room_alias_namespaces,
application_service_room_namespaces
```

### B. Rust运行时直接创建的表 (19个)

```
device_keys, typing, search_index, user_directory, user_privacy_settings,
pushers, account_data, key_backups, room_tags, room_events,
to_device_messages, device_lists_changes, room_ephemeral, device_lists_stream,
user_filters, sync_stream_id, sliding_sync_rooms, thread_subscriptions,
space_children, space_hierarchy
```

### C. Rust运行时添加的列 (3个)

```
users.is_guest, rooms.guest_access, refresh_tokens.expires_at
```

---

**文档结束**
