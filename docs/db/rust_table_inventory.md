# Rust 表定义统计文档

> **项目**: synapse-rust 数据库全面排查与优化
> **版本**: v1.0.0
> **生成日期**: 2026-03-20
> **源文件**: `src/services/database_initializer.rs`

---

## 统计概览

| 指标 | 数量 |
|------|------|
| Rust 中定义的表数 | 23+ |
| 动态创建的表数 | 20+ |
| 总计 | 43+ |

---

## 第一部分：E2EE 加密表

### 1.1 device_keys (设备密钥表)

**位置**: `database_initializer.rs` - `step_create_e2ee_tables()`

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

**索引**:
- `idx_device_keys_user_device` ON (user_id, device_id)

**与 SQL 差异**: 与 SQL 一致

---

## 第二部分：动态创建的表

### 2.1 typing (输入状态表)

**位置**: `database_initializer.rs` - `step_ensure_additional_tables()`

```sql
CREATE TABLE IF NOT EXISTS typing (
    user_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    typing BOOLEAN DEFAULT FALSE,
    last_active_ts BIGINT NOT NULL,
    UNIQUE (user_id, room_id)
)
```

**索引**: UNIQUE (user_id, room_id)

**与 SQL 差异**: 在 unified_schema_v6.sql 中未定义（动态创建）

---

### 2.2 search_index (消息搜索索引表)

**位置**: `database_initializer.rs` - `step_ensure_additional_tables()`

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
)
```

**索引**:
- `idx_search_index_room` ON room_id
- `idx_search_index_user` ON user_id
- `idx_search_index_type` ON event_type

**与 SQL 差异**: 在 unified_schema_v6.sql 中使用 `VARCHAR(255)` 类型，Rust 代码中定义一致

---

### 2.3 user_directory (用户目录表)

**位置**: `database_initializer.rs` - `step_ensure_additional_tables()`

```sql
CREATE TABLE IF NOT EXISTS user_directory (
    user_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    visibility TEXT NOT NULL DEFAULT 'private',
    added_by TEXT,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    CONSTRAINT pk_user_directory PRIMARY KEY (user_id, room_id)
)
```

**索引**:
- `idx_user_directory_user` ON user_id
- `idx_user_directory_visibility` ON visibility

**与 SQL 差异**:
- SQL 定义中无 `updated_ts` 列
- SQL 定义中有 `updated_ts` 列 (该列是后添加的)

---

### 2.4 user_privacy_settings (用户隐私设置表)

**位置**: `database_initializer.rs` - `step_ensure_additional_tables()`

```sql
CREATE TABLE IF NOT EXISTS user_privacy_settings (
    user_id VARCHAR(255) PRIMARY KEY,
    allow_presence_lookup BOOLEAN DEFAULT TRUE,
    allow_profile_lookup BOOLEAN DEFAULT TRUE,
    allow_room_invites BOOLEAN DEFAULT TRUE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT
)
```

**索引**: 无

**与 SQL 差异**: 在 unified_schema_v6.sql 中定义为 TEXT 类型主键，Rust 中使用 VARCHAR(255)

---

### 2.5 pushers (推送器表)

**位置**: `database_initializer.rs` - `step_ensure_additional_tables()`

```sql
CREATE TABLE IF NOT EXISTS pushers (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    pushkey TEXT NOT NULL,
    pushkey_ts BIGINT NOT NULL,
    kind TEXT NOT NULL,
    app_id TEXT NOT NULL,
    app_display_name TEXT NOT NULL,
    device_display_name TEXT NOT NULL,
    profile_tag TEXT,
    lang TEXT DEFAULT 'en',
    data JSONB DEFAULT '{}',
    updated_ts BIGINT,
    created_ts BIGINT NOT NULL,
    is_enabled BOOLEAN DEFAULT TRUE,
    CONSTRAINT uq_pushers_user_device_pushkey UNIQUE (user_id, device_id, pushkey)
)
```

**索引**:
- `idx_pushers_user` ON user_id

**与 SQL 差异**: 与 SQL 定义一致

---

### 2.6 ~~threepids (第三方身份表)~~ ⚠️ 已废弃并清理

**状态**: ✅ 已在 `database_initializer.rs` 中移除

**说明**:
- 此表在 SQL 中已废弃，功能与 `user_threepids` 合并
- Rust 代码中已移除此表的创建代码
- 相关业务代码已确认使用 `user_threepids` 表

---

### 2.7 account_data (账户数据表)

**位置**: `database_initializer.rs` - `step_ensure_additional_tables()`

```sql
CREATE TABLE IF NOT EXISTS account_data (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    data_type TEXT NOT NULL,
    content JSONB NOT NULL DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT uq_account_data_user_type UNIQUE (user_id, data_type)
)
```

**索引**:
- `idx_account_data_user` ON user_id

**与 SQL 差异**: 与 SQL 定义一致

---

### 2.8 key_backups (密钥备份表)

**位置**: `database_initializer.rs` - `step_ensure_additional_tables()`

```sql
CREATE TABLE IF NOT EXISTS key_backups (
    backup_id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    algorithm TEXT NOT NULL,
    auth_data JSONB,
    auth_key TEXT,
    version BIGINT DEFAULT 1,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    CONSTRAINT uq_key_backups_user_version UNIQUE (user_id, version)
)
```

**索引**:
- `idx_key_backups_user` ON user_id

**与 SQL 差异**: 与 SQL 定义一致

---

### 2.9 room_tags (房间标签表)

**位置**: `database_initializer.rs` - `step_ensure_additional_tables()`

```sql
CREATE TABLE IF NOT EXISTS room_tags (
    id SERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    tag VARCHAR(255) NOT NULL,
    order_value DOUBLE PRECISION,
    created_ts BIGINT NOT NULL,
    UNIQUE (user_id, room_id, tag)
)
```

**索引**: 无

**与 SQL 差异**: 在 unified_schema_v6.sql 中定义为 VARCHAR(255)，Rust 中定义一致

---

### 2.10 room_events (房间事件缓存表)

**位置**: `database_initializer.rs` - `step_ensure_additional_tables()`

```sql
CREATE TABLE IF NOT EXISTS room_events (
    id SERIAL PRIMARY KEY,
    event_id VARCHAR(255) UNIQUE NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    sender VARCHAR(255) NOT NULL,
    event_type VARCHAR(255) NOT NULL,
    state_key VARCHAR(255),
    content JSONB NOT NULL DEFAULT '{}',
    prev_event_id VARCHAR(255),
    origin_server_ts BIGINT NOT NULL,
    created_ts BIGINT NOT NULL
)
```

**索引**:
- `idx_room_events_room` ON room_id
- `idx_room_events_event` ON event_id

**与 SQL 差异**: 与 SQL 定义一致

---

### ~~reports (事件举报表 - 废弃)~~ ⚠️ 已废弃并清理

**状态**: ✅ 已在 `database_initializer.rs` 中移除

**说明**:
- 此表在 SQL 中已废弃，功能与 `event_reports` 合并
- Rust 代码中已移除此表的创建代码
- 相关业务代码已确认使用 `event_reports` 表

---

### 2.12 to_device_messages (E2EE To-Device 消息表)

**位置**: `database_initializer.rs` - `step_ensure_additional_tables()`

```sql
CREATE TABLE IF NOT EXISTS to_device_messages (
    id SERIAL PRIMARY KEY,
    sender_user_id VARCHAR(255) NOT NULL,
    sender_device_id VARCHAR(255) NOT NULL,
    recipient_user_id VARCHAR(255) NOT NULL,
    recipient_device_id VARCHAR(255) NOT NULL,
    event_type VARCHAR(255) NOT NULL,
    content JSONB NOT NULL DEFAULT '{}',
    message_id VARCHAR(255),
    stream_id BIGINT NOT NULL,
    created_ts BIGINT NOT NULL
)
```

**索引**:
- `idx_to_device_recipient` ON (recipient_user_id, recipient_device_id)
- `idx_to_device_stream` ON (recipient_user_id, stream_id)

**与 SQL 差异**: 与 SQL 定义一致

---

### 2.13 device_lists_changes (设备列表变更跟踪表)

**位置**: `database_initializer.rs` - `step_ensure_additional_tables()`

```sql
CREATE TABLE IF NOT EXISTS device_lists_changes (
    id SERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255),
    change_type VARCHAR(50) NOT NULL,
    stream_id BIGINT NOT NULL,
    created_ts BIGINT NOT NULL
)
```

**索引**:
- `idx_device_lists_user` ON user_id
- `idx_device_lists_stream` ON stream_id

**与 SQL 差异**: 与 SQL 定义一致

---

### 2.14 room_ephemeral (房间临时数据表)

**位置**: `database_initializer.rs` - `step_ensure_additional_tables()`

```sql
CREATE TABLE IF NOT EXISTS room_ephemeral (
    id SERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    event_type VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    content JSONB NOT NULL DEFAULT '{}',
    stream_id BIGINT NOT NULL,
    created_ts BIGINT NOT NULL,
    expires_at BIGINT
)
```

**索引**:
- `idx_room_ephemeral_room` ON room_id

**与 SQL 差异**: 与 SQL 定义一致

---

### 2.15 device_lists_stream (设备列表流位置表)

**位置**: `database_initializer.rs` - `step_ensure_additional_tables()`

```sql
CREATE TABLE IF NOT EXISTS device_lists_stream (
    stream_id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255),
    created_ts BIGINT NOT NULL
)
```

**索引**:
- `idx_device_lists_stream_user` ON user_id

**与 SQL 差异**: 与 SQL 定义一致

---

### 2.16 user_filters (用户过滤器持久化表)

**位置**: `database_initializer.rs` - `step_ensure_additional_tables()`

```sql
CREATE TABLE IF NOT EXISTS user_filters (
    id SERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    filter_id VARCHAR(255) NOT NULL,
    filter_json JSONB NOT NULL DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    UNIQUE (user_id, filter_id)
)
```

**索引**: 无

**与 SQL 差异**: 与 SQL 定义一致

---

### 2.17 sync_stream_id (同步流 ID 表)

**位置**: `database_initializer.rs` - `step_ensure_additional_tables()`

```sql
CREATE TABLE IF NOT EXISTS sync_stream_id (
    id BIGSERIAL PRIMARY KEY,
    stream_type TEXT,
    last_id BIGINT DEFAULT 0,
    updated_ts BIGINT,
    CONSTRAINT uq_sync_stream_id_type UNIQUE (stream_type)
)
```

**索引**: UNIQUE (stream_type)

**与 SQL 差异**: 与 SQL 定义一致

---

### 2.18 sliding_sync_rooms (Sliding Sync 房间状态缓存表)

**位置**: `database_initializer.rs` - `step_ensure_additional_tables()`

```sql
CREATE TABLE IF NOT EXISTS sliding_sync_rooms (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    conn_id TEXT,
    list_key TEXT,
    bump_stamp BIGINT DEFAULT 0,
    highlight_count INTEGER DEFAULT 0,
    notification_count INTEGER DEFAULT 0,
    is_dm BOOLEAN DEFAULT FALSE,
    is_encrypted BOOLEAN DEFAULT FALSE,
    is_tombstoned BOOLEAN DEFAULT FALSE,
    invited BOOLEAN DEFAULT FALSE,
    name TEXT,
    avatar TEXT,
    timestamp BIGINT DEFAULT 0,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL
)
```

**索引**:
- UNIQUE: (user_id, device_id, room_id, COALESCE(conn_id, ''))
- `idx_sliding_sync_rooms_user_device` ON (user_id, device_id)
- `idx_sliding_sync_rooms_bump_stamp` ON bump_stamp DESC

**与 SQL 差异**: 与 SQL 定义一致

---

### 2.19 thread_subscriptions (线程订阅表)

**位置**: `database_initializer.rs` - `step_ensure_additional_tables()`

```sql
CREATE TABLE IF NOT EXISTS thread_subscriptions (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    thread_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    notification_level TEXT DEFAULT 'all',
    is_muted BOOLEAN DEFAULT FALSE,
    is_pinned BOOLEAN DEFAULT FALSE,
    subscribed_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    UNIQUE (room_id, thread_id, user_id)
)
```

**索引**:
- `idx_thread_subscriptions_room_thread` ON (room_id, thread_id)

**与 SQL 差异**: 与 SQL 定义一致

---

### 2.20 space_children (Space 子房间表)

**位置**: `database_initializer.rs` - `step_ensure_additional_tables()`

```sql
CREATE TABLE IF NOT EXISTS space_children (
    id BIGSERIAL PRIMARY KEY,
    space_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    sender TEXT NOT NULL,
    is_suggested BOOLEAN DEFAULT FALSE,
    via_servers JSONB DEFAULT '[]',
    added_ts BIGINT NOT NULL,
    CONSTRAINT pk_space_children PRIMARY KEY (id),
    CONSTRAINT uq_space_children_space_room UNIQUE (space_id, room_id)
)
```

**索引**:
- `idx_space_children_space` ON space_id

**与 SQL 差异**: 与 SQL 定义一致

---

### 2.21 space_hierarchy (Space 层级结构表)

**位置**: `database_initializer.rs` - `step_ensure_additional_tables()`

```sql
CREATE TABLE IF NOT EXISTS space_hierarchy (
    id BIGSERIAL PRIMARY KEY,
    space_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    parent_space_id TEXT,
    depth INTEGER DEFAULT 0,
    children TEXT[],
    via_servers TEXT[],
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    UNIQUE (space_id, room_id)
)
```

**索引**:
- `idx_space_hierarchy_space` ON space_id

**与 SQL 差异**: 与 SQL 定义一致

---

## 第三部分：列添加操作

### 3.1 users 表 - is_guest 列

**位置**: `database_initializer.rs` - `step_ensure_additional_tables()`

```sql
ALTER TABLE users ADD COLUMN IF NOT EXISTS is_guest BOOLEAN DEFAULT FALSE
```

**与 SQL 差异**: SQL 定义中已有此列，无差异

---

### 3.2 rooms 表 - guest_access 列

**位置**: `database_initializer.rs` - `step_ensure_additional_tables()`

```sql
ALTER TABLE rooms ADD COLUMN IF NOT EXISTS guest_access VARCHAR(50) DEFAULT 'forbidden'
```

**与 SQL 差异**: SQL 定义中使用 `has_guest_access BOOLEAN` 类型，Rust 代码中添加的是 `VARCHAR(50)` 类型

---

### 3.3 refresh_tokens 表 - expires_at 列

**位置**: `database_initializer.rs` - `step_ensure_additional_tables()`

```sql
ALTER TABLE refresh_tokens ADD COLUMN IF NOT EXISTS expires_at BIGINT
```

**与 SQL 差异**: 与 SQL 定义一致

---

### ~~reports 表~~ ⚠️ 已废弃并清理

**状态**: ✅ 已在 `database_initializer.rs` 中移除

**说明**:
- 此表在 SQL 中已废弃，功能与 `event_reports` 合并
- Rust 代码中已移除此表的创建代码
- 相关业务代码已确认使用 `event_reports` 表

---

## 第四部分：表结构对比汇总

| 表名 | Rust 定义 | SQL 定义 | 一致性 | 问题 |
|------|-----------|----------|--------|------|
| device_keys | ✅ 完整 | ✅ 完整 | ✅ 一致 | 无 |
| typing | ✅ 动态创建 | ❌ 未定义 | ⚠️ 差异 | 动态创建表 |
| search_index | ✅ 完整 | ✅ 完整 | ✅ 一致 | 无 |
| user_directory | ✅ 完整 | ✅ 完整 | ✅ 一致 | 无 |
| user_privacy_settings | ✅ 完整 | ✅ 完整 | ✅ 一致 | 无 |
| pushers | ✅ 完整 | ✅ 完整 | ✅ 一致 | 无 |
| threepids | ❌ 已移除 | ⚠️ 已废弃 | ✅ 已清理 | 无 - 废弃表已清理 |
| account_data | ✅ 完整 | ✅ 完整 | ✅ 一致 | 无 |
| key_backups | ✅ 完整 | ✅ 完整 | ✅ 一致 | 无 |
| room_tags | ✅ 完整 | ✅ 完整 | ✅ 一致 | 无 |
| room_events | ✅ 完整 | ✅ 完整 | ✅ 一致 | 无 |
| reports | ❌ 已移除 | ⚠️ 已废弃 | ✅ 已清理 | 无 - 废弃表已清理 |
| to_device_messages | ✅ 完整 | ✅ 完整 | ✅ 一致 | 无 |
| device_lists_changes | ✅ 完整 | ✅ 完整 | ✅ 一致 | 无 |
| room_ephemeral | ✅ 完整 | ✅ 完整 | ✅ 一致 | 无 |
| device_lists_stream | ✅ 完整 | ✅ 完整 | ✅ 一致 | 无 |
| user_filters | ✅ 完整 | ✅ 完整 | ✅ 一致 | 无 |
| sync_stream_id | ✅ 完整 | ✅ 完整 | ✅ 一致 | 无 |
| sliding_sync_rooms | ✅ 完整 | ✅ 完整 | ✅ 一致 | 无 |
| thread_subscriptions | ✅ 完整 | ✅ 完整 | ✅ 一致 | 无 |
| space_children | ✅ 完整 | ✅ 完整 | ✅ 一致 | 无 |
| space_hierarchy | ✅ 完整 | ✅ 完整 | ✅ 一致 | 无 |

---

## 第五部分：问题汇总

### 问题 1: 废弃表已在 Rust 代码中清理 ✅ (P1)

| 表名 | 状态 | 修复操作 |
|------|------|----------|
| threepids | ✅ 已清理 | 已在 database_initializer.rs 中移除废弃表创建代码 |
| reports | ✅ 已清理 | 已在 database_initializer.rs 中移除废弃表创建代码 |

### 问题 2: rooms.guest_access 类型不一致 (P2)

| 位置 | SQL 类型 | Rust 类型 | 建议操作 |
|------|----------|----------|----------|
| rooms.guest_access | BOOLEAN (has_guest_access) | VARCHAR(50) | 统一类型定义 |

---

## 文档更新记录

| 日期 | 版本 | 更新内容 |
|------|------|----------|
| 2026-03-20 | v1.0.0 | 初始版本，基于 database_initializer.rs 生成 |
| 2026-03-20 | v1.1.0 | 更新：废弃表 (threepids, reports) 已从 Rust 代码中清理 |