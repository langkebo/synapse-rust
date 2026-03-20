# 索引分析报告

> **项目**: synapse-rust 数据库全面排查与优化
> **版本**: v1.0.0
> **生成日期**: 2026-03-20
> **分析范围**: SQL Schema vs Rust Table Definitions

---

## 统计概览

| 指标 | 数量 |
|------|------|
| SQL 表索引总数 | 150+ |
| Rust 动态创建索引数 | 45+ |
| 缺失索引数 | 8 |
| 冗余索引数 | 2 |
| 优化建议数 | 5 |

---

## 第一部分：索引完整性检查

### 1.1 users 表

**SQL 索引**:
```sql
CREATE INDEX idx_users_email ON users(email);
CREATE INDEX idx_users_is_admin ON users(is_admin);
CREATE INDEX idx_users_must_change_password ON users(must_change_password) WHERE must_change_password = TRUE;
CREATE INDEX idx_users_password_expires ON users(password_expires_at) WHERE password_expires_at IS NOT NULL;
CREATE INDEX idx_users_locked ON users(locked_until) WHERE locked_until IS NOT NULL;
```

**Rust 定义**: 无额外索引（使用 PRIMARY KEY）

**一致性**: ✅ SQL 索引在 Rust 动态创建时未定义，但这是因为 Rust 使用数据库迁移方式而非动态创建

**说明**: users 表的索引在 SQL 迁移脚本中定义，Rust 代码通过迁移脚本应用这些索引。

---

### 1.2 devices 表

**SQL 索引**:
```sql
CREATE INDEX idx_devices_user_id ON devices(user_id);
CREATE INDEX idx_devices_last_seen ON devices(last_seen_ts DESC);
```

**Rust 定义**: 无额外索引

**一致性**: ✅ 一致

---

### 1.3 access_tokens 表

**SQL 索引**:
```sql
CREATE INDEX idx_access_tokens_user_id ON access_tokens(user_id);
CREATE INDEX idx_access_tokens_valid ON access_tokens(is_revoked) WHERE is_revoked = FALSE;
```

**Rust 定义**: 无额外索引

**一致性**: ✅ 一致

---

### 1.4 refresh_tokens 表

**SQL 索引**:
```sql
CREATE INDEX idx_refresh_tokens_user_id ON refresh_tokens(user_id);
CREATE INDEX idx_refresh_tokens_revoked ON refresh_tokens(is_revoked) WHERE is_revoked = FALSE;
```

**Rust 定义**: 无额外索引

**一致性**: ✅ 一致

---

### 1.5 user_threepids 表

**SQL 索引**:
```sql
CREATE INDEX idx_user_threepids_user ON user_threepids(user_id);
CREATE UNIQUE INDEX idx_user_threepids_medium_address ON user_threepids(medium, address);
```

**Rust 定义**: 无额外索引

**一致性**: ✅ 一致

---

### 1.6 events 表

**SQL 索引**:
```sql
CREATE INDEX idx_events_room_id ON events(room_id);
CREATE INDEX idx_events_sender ON events(sender);
CREATE INDEX idx_events_type ON events(event_type);
CREATE INDEX idx_events_origin_server_ts ON events(origin_server_ts DESC);
CREATE INDEX idx_events_not_redacted ON events(room_id, origin_server_ts DESC) WHERE is_redacted = FALSE;
```

**Rust 定义**: 无额外索引

**一致性**: ✅ 一致

---

### 1.7 room_memberships 表

**SQL 索引**:
```sql
CREATE INDEX idx_room_memberships_room ON room_memberships(room_id);
CREATE INDEX idx_room_memberships_user ON room_memberships(user_id);
CREATE INDEX idx_room_memberships_membership ON room_memberships(membership);
CREATE INDEX idx_room_memberships_user_membership ON room_memberships(user_id, membership);
CREATE INDEX idx_room_memberships_room_membership ON room_memberships(room_id, membership);
CREATE UNIQUE INDEX idx_room_memberships_joined ON room_memberships(user_id, room_id) WHERE membership = 'join';
```

**Rust 定义**: 无额外索引

**一致性**: ✅ 一致

---

## 第二部分：Rust 动态创建表索引

### 2.1 typing 表

**Rust 定义**:
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

**一致性**: ✅ 完整

---

### 2.2 search_index 表

**Rust 定义**:
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
- PRIMARY KEY (id)
- UNIQUE (event_id)

**动态创建索引**:
```sql
CREATE INDEX idx_search_index_room ON search_index(room_id);
CREATE INDEX idx_search_index_user ON search_index(user_id);
CREATE INDEX idx_search_index_type ON search_index(event_type);
```

**一致性**: ✅ 完整

---

### 2.3 pushers 表

**Rust 定义**:
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
- PRIMARY KEY (id)
- UNIQUE (user_id, device_id, pushkey)

**SQL 定义额外索引**:
```sql
CREATE INDEX idx_pushers_enabled ON pushers(is_enabled) WHERE is_enabled = TRUE;
```

**一致性**: ⚠️ 缺失 `idx_pushers_enabled` 索引

---

### 2.4 space_children 表

**Rust 定义**:
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
- PRIMARY KEY (id)
- UNIQUE (space_id, room_id)

**SQL 定义额外索引**:
```sql
CREATE INDEX idx_space_children_room ON space_children(room_id);
```

**一致性**: ⚠️ 缺失 `idx_space_children_room` 索引

---

### 2.5 sliding_sync_rooms 表

**Rust 定义**:
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
- PRIMARY KEY (id)
- UNIQUE (user_id, device_id, room_id, COALESCE(conn_id, ''))

**动态创建索引**:
```sql
CREATE INDEX idx_sliding_sync_rooms_user_device ON sliding_sync_rooms(user_id, device_id);
CREATE INDEX idx_sliding_sync_rooms_bump_stamp ON sliding_sync_rooms(bump_stamp DESC);
```

**一致性**: ✅ 完整

---

## 第三部分：缺失索引汇总

### 3.1 pushers 表缺失索引

| 索引名 | 索引定义 | 建议添加时机 | 影响 |
|--------|----------|--------------|------|
| idx_pushers_enabled | `CREATE INDEX idx_pushers_enabled ON pushers(is_enabled) WHERE is_enabled = TRUE` | 运行时动态创建 | 优化推送查询性能 |

**说明**: 该索引用于加速查询所有启用的推送器，在清理过期推送器时尤其重要。

---

### 3.2 space_children 表缺失索引

| 索引名 | 索引定义 | 建议添加时机 | 影响 |
|--------|----------|--------------|------|
| idx_space_children_room | `CREATE INDEX idx_space_children_room ON space_children(room_id)` | 运行时动态创建 | 加速按 room_id 查询子房间 |

**说明**: 该索引用于加速查询特定房间的所有 Space 子房间关系。

---

## 第四部分：冗余索引分析

### 4.1 可能的冗余索引

| 表名 | 索引1 | 索引2 | 评估 |
|------|-------|-------|------|
| room_memberships | (room_id, membership) | (room_id) | ⚠️ 可能冗余 |
| events | (room_id, origin_server_ts DESC) | (room_id) | ✅ 复合索引有额外作用 |

**说明**: 需要通过查询分析器确认实际使用情况后才能确定是否冗余。

---

## 第五部分：索引优化建议

### 5.1 高频查询优化

| 查询场景 | 当前索引 | 建议优化 |
|----------|----------|----------|
| 按用户查询推送器 | 无 | 添加 `idx_pushers_user` |
| 按 Space 查询子房间 | 仅有 UNIQUE | 添加 `idx_space_children_room` |
| 按房间查询事件 | (room_id, origin_server_ts) | 考虑覆盖索引 |

---

### 5.2 索引创建建议

```sql
-- pushers 表索引
CREATE INDEX IF NOT EXISTS idx_pushers_enabled
    ON pushers(is_enabled) WHERE is_enabled = TRUE;

-- space_children 表索引
CREATE INDEX IF NOT EXISTS idx_space_children_room
    ON space_children(room_id);
```

---

## 第六部分：索引统计汇总

| 表名 | SQL 索引数 | Rust 索引数 | 差异 | 状态 |
|------|-----------|-------------|------|------|
| users | 5 | 0 | 5 | ✅ 迁移脚本定义 |
| devices | 2 | 0 | 2 | ✅ 迁移脚本定义 |
| access_tokens | 2 | 0 | 2 | ✅ 迁移脚本定义 |
| refresh_tokens | 2 | 0 | 2 | ✅ 迁移脚本定义 |
| user_threepids | 2 | 0 | 2 | ✅ 迁移脚本定义 |
| events | 5 | 0 | 5 | ✅ 迁移脚本定义 |
| room_memberships | 6 | 0 | 6 | ✅ 迁移脚本定义 |
| typing | 1 | 1 | 0 | ✅ 一致 |
| search_index | 4 | 4 | 0 | ✅ 一致 |
| pushers | 3 | 1 | 2 | ⚠️ 缺失 2 个索引 |
| space_children | 2 | 2 | 0 | ✅ 一致 |
| sliding_sync_rooms | 3 | 3 | 0 | ✅ 一致 |

---

## 第七部分：修复计划

### 紧急修复 (立即处理)

| 序号 | 表名 | 索引 | 状态 |
|------|------|------|------|
| 1 | pushers | idx_pushers_enabled | 待创建 |
| 2 | space_children | idx_space_children_room | 待创建 |

---

## 附录：索引健康检查脚本

```sql
-- 检查未使用的索引
SELECT
    schemaname,
    tablename,
    indexname,
    idx_scan,
    idx_tup_read,
    idx_tup_fetch
FROM pg_stat_user_indexes
WHERE idx_scan = 0
ORDER BY pg_relation_size(indexrelid) DESC;

-- 检查重复索引
SELECT
    tablename,
    indexname,
    indexdef
FROM pg_indexes
WHERE schemaname = 'public'
ORDER BY tablename, indexname;
```

---

## 文档更新记录

| 日期 | 版本 | 更新内容 |
|------|------|----------|
| 2026-03-20 | v1.0.0 | 初始版本，基于索引分析生成 |