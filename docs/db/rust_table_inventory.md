# Rust 表清单报告

> **项目**: synapse-rust 数据库全面排查
> **版本**: v1.0.0
> **生成日期**: 2026-03-20
> **数据来源**: src/services/database_initializer.rs

---

## 一、执行摘要

| 指标 | 值 |
|------|-----|
| Rust 动态创建表数量 | 21 |
| SQL 迁移脚本表数量 | 154 |
| 数据库实际表数量 | 154 |

---

## 二、Rust 动态创建表清单

### 2.1 Schema 管理表

| 序号 | 表名 | 行号 | 说明 |
|------|------|------|------|
| 1 | schema_migrations | 283 | Schema 迁移记录 |

### 2.2 设备与密钥相关表

| 序号 | 表名 | 行号 | 说明 |
|------|------|------|------|
| 2 | device_keys | 625 | 设备密钥 |
| 3 | device_lists_changes | 978 | 设备列表变更 |
| 4 | device_lists_stream | 1036 | 设备列表流 |

### 2.3 房间相关表

| 序号 | 表名 | 行号 | 说明 |
|------|------|------|------|
| 5 | room_events | 906 | 房间事件 |
| 6 | room_ephemeral | 1010 | 房间临时数据 |
| 7 | room_tags | 881 | 房间标签 |
| 8 | sliding_sync_rooms | 1106 | 滑动同步房间 |
| 9 | space_children | 1159 | 空间子项 |
| 10 | space_hierarchy | 1187 | 空间层级 |
| 11 | thread_subscriptions | 1122 | 线程订阅 |

### 2.4 用户相关表

| 序号 | 表名 | 行号 | 说明 |
|------|------|------|------|
| 12 | user_directory | 722 | 用户目录 |
| 13 | user_filters | 1058 | 用户过滤器 |
| 14 | user_privacy_settings | 760 | 用户隐私设置 |

### 2.5 消息与推送相关表

| 序号 | 表名 | 行号 | 说明 |
|------|------|------|------|
| 15 | account_data | 818 | 账户数据 |
| 16 | key_backups | 843 | 密钥备份 |
| 17 | pushers | 776 | 推送器 |
| 18 | sync_stream_id | 1082 | 同步流ID |
| 19 | to_device_messages | 942 | 设备消息 |
| 20 | typing | 663 | 打字状态 |
| 21 | search_index | 678 | 搜索索引 |

---

## 三、表结构详情

### 3.1 schema_migrations (行号: 283)

```sql
CREATE TABLE IF NOT EXISTS schema_migrations (
    version VARCHAR(255) PRIMARY KEY,
    executed_at TIMESTAMPTZ DEFAULT NOW(),
    success BOOLEAN NOT NULL DEFAULT TRUE
)
```

### 3.2 device_keys (行号: 625)

```sql
CREATE TABLE IF NOT EXISTS device_keys (
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255) NOT NULL,
    key_name VARCHAR(255) NOT NULL,
    key_data TEXT NOT NULL,
    signatures JSONB,
    ts BIGINT NOT NULL,
    PRIMARY KEY (user_id, device_id, key_name)
)
```

### 3.3 typing (行号: 663)

```sql
CREATE TABLE IF NOT EXISTS typing (
    room_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    ts BIGINT NOT NULL,
    PRIMARY KEY (room_id, user_id)
)
```

### 3.4 search_index (行号: 678)

```sql
CREATE TABLE IF NOT EXISTS search_index (
    key VARCHAR(255) NOT NULL,
    value TEXT NOT NULL,
    ts BIGINT NOT NULL,
    PRIMARY KEY (key)
)
```

### 3.5 user_directory (行号: 722)

```sql
CREATE TABLE IF NOT EXISTS user_directory (
    user_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    visibility VARCHAR(50) NOT NULL DEFAULT 'published',
    added_by VARCHAR(255),
    created_ts BIGINT NOT NULL,
    PRIMARY KEY (user_id, room_id)
)
```

### 3.6 user_privacy_settings (行号: 760)

```sql
CREATE TABLE IF NOT EXISTS user_privacy_settings (
    user_id VARCHAR(255) PRIMARY KEY,
    settings JSONB NOT NULL DEFAULT '{}'
)
```

### 3.7 pushers (行号: 776)

```sql
CREATE TABLE IF NOT EXISTS pushers (
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255) NOT NULL,
    pusher_id VARCHAR(255) NOT NULL,
    kind VARCHAR(50) NOT NULL,
    app_id VARCHAR(255) NOT NULL,
    app_display_name VARCHAR(255),
    device_display_name VARCHAR(255),
    profile_tag VARCHAR(255),
    lang VARCHAR(50),
    data JSONB NOT NULL,
    last_token VARCHAR(255),
    last_seq BIGINT,
    last_ts BIGINT,
    PRIMARY KEY (user_id, device_id, pusher_id)
)
```

### 3.8 account_data (行号: 818)

```sql
CREATE TABLE IF NOT EXISTS account_data (
    user_id VARCHAR(255) NOT NULL,
    account_type VARCHAR(255) NOT NULL,
    content JSONB NOT NULL,
    updated_ts BIGINT,
    PRIMARY KEY (user_id, account_type)
)
```

### 3.9 key_backups (行号: 843)

```sql
CREATE TABLE IF NOT EXISTS key_backups (
    user_id VARCHAR(255) NOT NULL,
    version VARCHAR(50) NOT NULL,
    algorithm VARCHAR(50) NOT NULL,
    auth_data JSONB NOT NULL,
    PRIMARY KEY (user_id, version)
)
```

### 3.10 room_tags (行号: 881)

```sql
CREATE TABLE IF NOT EXISTS room_tags (
    user_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    tag VARCHAR(255) NOT NULL,
    content JSONB,
    PRIMARY KEY (user_id, room_id, tag)
)
```

### 3.11 room_events (行号: 906)

```sql
CREATE TABLE IF NOT EXISTS room_events (
    event_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255),
    event_type VARCHAR(255) NOT NULL,
    state_key VARCHAR(255),
    content JSONB NOT NULL,
    created_ts BIGINT NOT NULL,
    PRIMARY KEY (event_id)
)
```

### 3.12 to_device_messages (行号: 942)

```sql
CREATE TABLE IF NOT EXISTS to_device_messages (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255) NOT NULL,
    event_type VARCHAR(255) NOT NULL,
    content JSONB NOT NULL,
    created_ts BIGINT NOT NULL
)
```

### 3.13 device_lists_changes (行号: 978)

```sql
CREATE TABLE IF NOT EXISTS device_lists_changes (
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255) NOT NULL,
    stream_id BIGINT NOT NULL,
    changed BOOLEAN NOT NULL DEFAULT TRUE,
    PRIMARY KEY (user_id, device_id, stream_id)
)
```

### 3.14 room_ephemeral (行号: 1010)

```sql
CREATE TABLE IF NOT EXISTS room_ephemeral (
    room_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    ephemeral_type VARCHAR(255) NOT NULL,
    content JSONB NOT NULL,
    received_ts BIGINT NOT NULL,
    PRIMARY KEY (room_id, user_id, ephemeral_type)
)
```

### 3.15 device_lists_stream (行号: 1036)

```sql
CREATE TABLE IF NOT EXISTS device_lists_stream (
    stream_id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255),
    PRIMARY KEY (stream_id)
)
```

### 3.16 user_filters (行号: 1058)

```sql
CREATE TABLE IF NOT EXISTS user_filters (
    user_id VARCHAR(255) NOT NULL,
    filter_id BIGINT NOT NULL,
    filter_pattern JSONB NOT NULL,
    PRIMARY KEY (user_id, filter_id)
)
```

### 3.17 sync_stream_id (行号: 1082)

```sql
CREATE TABLE IF NOT EXISTS sync_stream_id (
    user_id VARCHAR(255) PRIMARY KEY,
    stream_id BIGINT NOT NULL
)
```

### 3.18 sliding_sync_rooms (行号: 1106)

```sql
CREATE TABLE IF NOT EXISTS sliding_sync_rooms (
    user_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    sync_type VARCHAR(50) NOT NULL,
    state_json JSONB,
    extensions_json JSONB,
    updated_ts BIGINT NOT NULL,
    PRIMARY KEY (user_id, room_id, sync_type)
)
```

### 3.19 thread_subscriptions (行号: 1159)

```sql
CREATE TABLE IF NOT EXISTS thread_subscriptions (
    user_id VARCHAR(255) NOT NULL,
    thread_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    subscription_info JSONB,
    updated_ts BIGINT NOT NULL,
    PRIMARY KEY (user_id, thread_id)
)
```

### 3.20 space_children (行号: 1187)

```sql
CREATE TABLE IF NOT EXISTS space_children (
    parent_id VARCHAR(255) NOT NULL,
    child_id VARCHAR(255) NOT NULL,
    child_type VARCHAR(50) NOT NULL,
    order_val VARCHAR(255),
    PRIMARY KEY (parent_id, child_id, child_type)
)
```

### 3.21 space_hierarchy (行号: 1222)

```sql
CREATE TABLE IF NOT EXISTS space_hierarchy (
    room_id VARCHAR(255) NOT NULL,
    parent_id VARCHAR(255) NOT NULL,
    depth BIGINT NOT NULL,
    display_name VARCHAR(255),
    avatar_url VARCHAR(255),
    PRIMARY KEY (room_id, parent_id)
)
```

---

## 四、索引定义

### 4.1 自动创建的索引

| 表名 | 索引名 | 字段 | 类型 |
|------|--------|------|------|
| device_keys | idx_device_keys_user | user_id | 普通 |
| device_keys | idx_device_keys_device | device_id | 普通 |
| typing | idx_typing_room | room_id | 普通 |
| search_index | idx_search_index_value | value | 普通 |
| user_directory | idx_user_directory_visibility | visibility | 普通 |
| pushers | idx_pushers_user | user_id | 普通 |
| account_data | idx_account_data_user | user_id | 普通 |
| key_backups | idx_key_backups_user | user_id | 普通 |
| room_tags | idx_room_tags_user | user_id | 普通 |
| room_events | idx_room_events_room | room_id | 普通 |
| room_events | idx_room_events_user | user_id | 普通 |
| to_device_messages | idx_to_device_user | user_id, device_id | 普通 |
| device_lists_changes | idx_device_lists_stream | stream_id | 普通 |
| room_ephemeral | idx_room_ephemeral_room | room_id | 普通 |
| device_lists_stream | idx_device_lists_stream_id | stream_id | 普通 |
| user_filters | idx_user_filters_user | user_id | 普通 |
| sync_stream_id | idx_sync_stream_user | user_id | 普通 |
| sliding_sync_rooms | idx_sliding_sync_user | user_id | 普通 |

---

## 五、文档更新记录

| 日期 | 版本 | 更新内容 |
|------|------|----------|
| 2026-03-20 | v1.0.0 | 初始版本，基于 database_initializer.rs 扫描生成 |
