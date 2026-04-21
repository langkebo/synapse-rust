# Rust 模型与 SQL Schema 差异分析报告

> 生成日期: 2026-03-26
> 项目: synapse-rust
> SQL Schema: migrations/00000000_unified_schema_v6.sql

---

## 摘要

本报告详细比较 `src/storage/models/*.rs` 中的 Rust 结构体定义与 `migrations/00000000_unified_schema_v6.sql` 中的 SQL 表定义，识别字段名称、数据类型、约束条件等不匹配问题。

**严重程度等级:**
- **P0**: 关键问题 - 会导致编译错误或运行时崩溃
- **P1**: 高优先级 - 数据丢失或功能异常
- **P2**: 中优先级 - 潜在问题或命名不一致

---

## 1. users 表

### SQL 定义 (users)
```sql
CREATE TABLE users (
    user_id TEXT NOT NULL,
    username TEXT NOT NULL,
    password_hash TEXT,
    is_admin BOOLEAN DEFAULT FALSE,
    is_guest BOOLEAN DEFAULT FALSE,
    is_shadow_banned BOOLEAN DEFAULT FALSE,
    is_deactivated BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    displayname TEXT,
    avatar_url TEXT,
    email TEXT,
    phone TEXT,
    generation BIGINT DEFAULT 0,
    consent_version TEXT,
    appservice_id TEXT,
    user_type TEXT,
    invalid_update_at BIGINT,
    migration_state TEXT,
    password_changed_ts BIGINT,
    is_password_change_required BOOLEAN DEFAULT FALSE,
    must_change_password BOOLEAN DEFAULT FALSE,
    password_expires_at BIGINT,
    failed_login_attempts INTEGER DEFAULT 0,
    locked_until BIGINT,
    PRIMARY KEY (user_id)
);
```

### Rust 定义 (User)
```rust
pub struct User {
    pub user_id: String,
    pub username: String,
    pub password_hash: Option<String>,
    pub is_admin: bool,
    pub is_guest: bool,
    pub is_shadow_banned: bool,
    pub is_deactivated: bool,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
    pub displayname: Option<String>,
    pub avatar_url: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub generation: i64,
    pub consent_version: Option<String>,
    pub appservice_id: Option<String>,
    pub user_type: Option<String>,
    pub invalid_update_at: Option<i64>,
    pub migration_state: Option<String>,
    pub password_changed_ts: Option<i64>,
    pub is_password_change_required: bool,
    pub password_expires_at: Option<i64>,
    pub failed_login_attempts: i32,
    pub locked_until: Option<i64>,
}
```

### 字段对比

| 字段名 | SQL 类型 | Rust 类型 | 匹配 | 备注 |
|--------|----------|-----------|------|------|
| user_id | TEXT NOT NULL | String | ✅ | |
| username | TEXT NOT NULL | String | ✅ | |
| password_hash | TEXT | Option<String> | ✅ | |
| is_admin | BOOLEAN | bool | ✅ | |
| is_guest | BOOLEAN | bool | ✅ | |
| is_shadow_banned | BOOLEAN | bool | ✅ | |
| is_deactivated | BOOLEAN | bool | ✅ | |
| created_ts | BIGINT NOT NULL | i64 | ✅ | |
| updated_ts | BIGINT | Option<i64> | ✅ | |
| displayname | TEXT | Option<String> | ✅ | |
| avatar_url | TEXT | Option<String> | ✅ | |
| email | TEXT | Option<String> | ✅ | |
| phone | TEXT | Option<String> | ✅ | |
| generation | BIGINT | i64 | ✅ | |
| consent_version | TEXT | Option<String> | ✅ | |
| appservice_id | TEXT | Option<String> | ✅ | |
| user_type | TEXT | Option<String> | ✅ | |
| invalid_update_at | BIGINT | Option<i64> | ✅ | |
| migration_state | TEXT | Option<String> | ✅ | |
| password_changed_ts | BIGINT | Option<i64> | ✅ | |
| is_password_change_required | BOOLEAN | bool | ✅ | |
| **must_change_password** | **BOOLEAN** | **❌ 缺失** | **P1** | SQL 有此字段，Rust 缺失 |
| password_expires_at | BIGINT | Option<i64> | ✅ | |
| failed_login_attempts | INTEGER | i32 | ✅ | |
| locked_until | BIGINT | Option<i64> | ✅ | |

### 问题列表

| 严重程度 | 问题 | 位置 | 建议修复 |
|----------|------|------|----------|
| P1 | `must_change_password` 字段在 SQL 存在但 Rust 模型缺失 | user.rs | 添加 `pub must_change_password: bool` 字段 |

---

## 2. user_threepids 表

### SQL 定义
```sql
CREATE TABLE user_threepids (
    id BIGSERIAL,
    user_id TEXT NOT NULL,
    medium TEXT NOT NULL,
    address TEXT NOT NULL,
    validated_ts BIGINT,
    added_ts BIGINT NOT NULL,
    is_verified BOOLEAN DEFAULT FALSE,
    verification_token TEXT,
    verification_expires_ts BIGINT,
    PRIMARY KEY (id)
);
```

### Rust 定义 (UserThreepid)
```rust
pub struct UserThreepid {
    pub id: i64,
    pub user_id: String,
    pub medium: String,
    pub address: String,
    pub validated_ts: Option<i64>,
    pub added_ts: i64,
    pub is_verified: bool,
    pub verification_token: Option<String>,
    pub verification_expires_ts: Option<i64>,
}
```

### 字段对比

| 字段名 | SQL 类型 | Rust 类型 | 匹配 | 备注 |
|--------|----------|-----------|------|------|
| id | BIGSERIAL | i64 | ✅ | |
| user_id | TEXT NOT NULL | String | ✅ | |
| medium | TEXT NOT NULL | String | ✅ | |
| address | TEXT NOT NULL | String | ✅ | |
| **validated_ts** | BIGINT | Option<i64> | ⚠️ | 应为 `validated_at` (可选时间戳) |
| added_ts | BIGINT NOT NULL | i64 | ✅ | |
| is_verified | BOOLEAN | bool | ✅ | |
| verification_token | TEXT | Option<String> | ✅ | |
| **verification_expires_ts** | BIGINT | Option<i64> | ⚠️ | 应为 `verification_expires_at` (可选时间戳) |

### 问题列表

| 严重程度 | 问题 | 位置 | 建议修复 |
|----------|------|------|----------|
| P1 | `validated_ts` 应为 `validated_at` | user.rs:43 | 重命名字段为 `validated_at` |
| P1 | `verification_expires_ts` 应为 `verification_expires_at` | user.rs:47 | 重命名字段为 `verification_expires_at` |

**注意**: 根据项目规范 `DATABASE_FIELD_STANDARDS.md`，可选时间戳应使用 `_at` 后缀。

---

## 3. devices 表

### SQL 定义
```sql
CREATE TABLE devices (
    device_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    display_name TEXT,
    device_key JSONB,
    last_seen_ts BIGINT,
    last_seen_ip TEXT,
    created_ts BIGINT NOT NULL,
    first_seen_ts BIGINT NOT NULL,
    user_agent TEXT,
    appservice_id TEXT,
    ignored_user_list TEXT,
    PRIMARY KEY (device_id)
);
```

### Rust 定义 (Device)
```rust
pub struct Device {
    pub device_id: String,
    pub user_id: String,
    pub display_name: Option<String>,
    pub device_key: Option<serde_json::Value>,
    pub last_seen_ts: Option<i64>,
    pub last_seen_ip: Option<String>,
    pub created_ts: i64,
    pub first_seen_ts: i64,
    pub user_agent: Option<String>,
    pub appservice_id: Option<String>,
    pub ignored_user_list: Option<String>,
}
```

### 字段对比

| 字段名 | SQL 类型 | Rust 类型 | 匹配 | 备注 |
|--------|----------|-----------|------|------|
| device_id | TEXT NOT NULL | String | ✅ | |
| user_id | TEXT NOT NULL | String | ✅ | |
| display_name | TEXT | Option<String> | ✅ | |
| device_key | JSONB | Option<serde_json::Value> | ✅ | |
| last_seen_ts | BIGINT | Option<i64> | ✅ | |
| last_seen_ip | TEXT | Option<String> | ✅ | |
| created_ts | BIGINT NOT NULL | i64 | ✅ | |
| first_seen_ts | BIGINT NOT NULL | i64 | ✅ | |
| user_agent | TEXT | Option<String> | ✅ | |
| appservice_id | TEXT | Option<String> | ✅ | |
| ignored_user_list | TEXT | Option<String> | ✅ | |

**状态**: ✅ 完全匹配

---

## 4. access_tokens 表

### SQL 定义
```sql
CREATE TABLE access_tokens (
    id BIGSERIAL,
    token TEXT NOT NULL,
    user_id TEXT NOT NULL,
    device_id TEXT,
    created_ts BIGINT NOT NULL,
    expires_at BIGINT,
    last_used_ts BIGINT,
    user_agent TEXT,
    ip_address TEXT,
    is_revoked BOOLEAN DEFAULT FALSE,
    PRIMARY KEY (id)
);
```

### Rust 定义 (AccessToken)
```rust
pub struct AccessToken {
    pub id: i64,
    pub token: String,
    pub user_id: String,
    pub device_id: Option<String>,
    pub created_ts: i64,
    pub expires_at: Option<i64>,
    pub last_used_ts: Option<i64>,
    pub user_agent: Option<String>,
    pub ip_address: Option<String>,
    pub is_revoked: bool,
}
```

### 字段对比

| 字段名 | SQL 类型 | Rust 类型 | 匹配 | 备注 |
|--------|----------|-----------|------|------|
| id | BIGSERIAL | i64 | ✅ | |
| token | TEXT NOT NULL | String | ✅ | |
| user_id | TEXT NOT NULL | String | ✅ | |
| device_id | TEXT | Option<String> | ✅ | |
| created_ts | BIGINT NOT NULL | i64 | ✅ | |
| expires_at | BIGINT | Option<i64> | ✅ | |
| last_used_ts | BIGINT | Option<i64> | ✅ | |
| user_agent | TEXT | Option<String> | ✅ | |
| ip_address | TEXT | Option<String> | ✅ | |
| is_revoked | BOOLEAN | bool | ✅ | |

**状态**: ✅ 完全匹配

---

## 5. refresh_tokens 表

### SQL 定义
```sql
CREATE TABLE refresh_tokens (
    id BIGSERIAL,
    token_hash TEXT NOT NULL,
    user_id TEXT NOT NULL,
    device_id TEXT,
    access_token_id TEXT,
    scope TEXT,
    created_ts BIGINT NOT NULL,
    expires_at BIGINT,
    last_used_ts BIGINT,
    use_count INTEGER DEFAULT 0,
    is_revoked BOOLEAN DEFAULT FALSE,
    revoked_reason TEXT,
    client_info JSONB,
    ip_address TEXT,
    user_agent TEXT,
    PRIMARY KEY (id)
);
```

### Rust 定义 (RefreshToken)
```rust
pub struct RefreshToken {
    pub id: i64,
    pub token_hash: String,
    pub user_id: String,
    pub device_id: Option<String>,
    pub access_token_id: Option<String>,
    pub scope: Option<String>,
    pub created_ts: i64,
    pub expires_at: Option<i64>,
    pub last_used_ts: Option<i64>,
    pub use_count: i32,
    pub is_revoked: bool,
    pub revoked_reason: Option<String>,
    pub client_info: Option<serde_json::Value>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
}
```

### 字段对比

| 字段名 | SQL 类型 | Rust 类型 | 匹配 | 备注 |
|--------|----------|-----------|------|------|
| id | BIGSERIAL | i64 | ✅ | |
| token_hash | TEXT NOT NULL | String | ✅ | |
| user_id | TEXT NOT NULL | String | ✅ | |
| device_id | TEXT | Option<String> | ✅ | |
| access_token_id | TEXT | Option<String> | ✅ | |
| scope | TEXT | Option<String> | ✅ | |
| created_ts | BIGINT NOT NULL | i64 | ✅ | |
| expires_at | BIGINT | Option<i64> | ✅ | |
| last_used_ts | BIGINT | Option<i64> | ✅ | |
| use_count | INTEGER | i32 | ✅ | |
| is_revoked | BOOLEAN | bool | ✅ | |
| revoked_reason | TEXT | Option<String> | ✅ | |
| client_info | JSONB | Option<serde_json::Value> | ✅ | |
| ip_address | TEXT | Option<String> | ✅ | |
| user_agent | TEXT | Option<String> | ✅ | |

**状态**: ✅ 完全匹配

---

## 6. token_blacklist 表

### SQL 定义
```sql
CREATE TABLE token_blacklist (
    id BIGSERIAL,
    token_hash TEXT NOT NULL,
    token TEXT,
    token_type TEXT DEFAULT 'access',
    user_id TEXT,
    is_revoked BOOLEAN DEFAULT TRUE,
    reason TEXT,
    expires_at BIGINT,
    PRIMARY KEY (id)
);
```

### Rust 定义 (TokenBlacklistEntry)
```rust
pub struct TokenBlacklistEntry {
    pub id: i64,
    pub token_hash: String,
    pub token: Option<String>,
    pub token_type: String,
    pub user_id: Option<String>,
    pub is_revoked: bool,
    pub reason: Option<String>,
    pub expires_at: Option<i64>,
}
```

### 字段对比

| 字段名 | SQL 类型 | Rust 类型 | 匹配 | 备注 |
|--------|----------|-----------|------|------|
| id | BIGSERIAL | i64 | ✅ | |
| token_hash | TEXT NOT NULL | String | ✅ | |
| token | TEXT | Option<String> | ✅ | |
| token_type | TEXT | String | ✅ | |
| user_id | TEXT | Option<String> | ✅ | |
| is_revoked | BOOLEAN | bool | ✅ | |
| reason | TEXT | Option<String> | ✅ | |
| expires_at | BIGINT | Option<i64> | ✅ | |

**状态**: ✅ 完全匹配

---

## 7. rooms 表

### SQL 定义
```sql
CREATE TABLE rooms (
    room_id TEXT NOT NULL,
    creator TEXT,
    is_public BOOLEAN DEFAULT FALSE,
    room_version TEXT DEFAULT '6',
    created_ts BIGINT NOT NULL,
    last_activity_ts BIGINT,
    is_federated BOOLEAN DEFAULT TRUE,
    has_guest_access BOOLEAN DEFAULT FALSE,
    join_rules TEXT DEFAULT 'invite',
    history_visibility TEXT DEFAULT 'shared',
    name TEXT,
    topic TEXT,
    avatar_url TEXT,
    canonical_alias TEXT,
    visibility TEXT DEFAULT 'private',
    PRIMARY KEY (room_id)
);
```

### Rust 定义 (Room)
```rust
pub struct Room {
    pub room_id: String,
    pub creator: Option<String>,
    pub is_public: bool,
    pub room_version: String,
    pub created_ts: i64,
    pub last_activity_ts: Option<i64>,
    pub is_federated: bool,
    pub has_guest_access: bool,
    pub join_rules: String,
    pub history_visibility: String,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub avatar_url: Option<String>,
    pub canonical_alias: Option<String>,
    pub visibility: String,
}
```

### 字段对比

| 字段名 | SQL 类型 | Rust 类型 | 匹配 | 备注 |
|--------|----------|-----------|------|------|
| room_id | TEXT NOT NULL | String | ✅ | |
| creator | TEXT | Option<String> | ✅ | |
| is_public | BOOLEAN | bool | ✅ | |
| room_version | TEXT | String | ✅ | |
| created_ts | BIGINT NOT NULL | i64 | ✅ | |
| last_activity_ts | BIGINT | Option<i64> | ✅ | |
| is_federated | BOOLEAN | bool | ✅ | |
| has_guest_access | BOOLEAN | bool | ✅ | |
| join_rules | TEXT | String | ✅ | |
| history_visibility | TEXT | String | ✅ | |
| name | TEXT | Option<String> | ✅ | |
| topic | TEXT | Option<String> | ✅ | |
| avatar_url | TEXT | Option<String> | ✅ | |
| canonical_alias | TEXT | Option<String> | ✅ | |
| visibility | TEXT | String | ✅ | |

**状态**: ✅ 完全匹配

---

## 8. room_memberships 表

### SQL 定义
```sql
CREATE TABLE room_memberships (
    id BIGSERIAL,
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    membership TEXT NOT NULL,
    joined_ts BIGINT,
    invited_ts BIGINT,
    left_ts BIGINT,
    banned_ts BIGINT,
    sender TEXT,
    reason TEXT,
    event_id TEXT,
    event_type TEXT,
    display_name TEXT,
    avatar_url TEXT,
    is_banned BOOLEAN DEFAULT FALSE,
    invite_token TEXT,
    updated_ts BIGINT,
    join_reason TEXT,
    banned_by TEXT,
    ban_reason TEXT,
    PRIMARY KEY (id)
);
```

### Rust 定义 (RoomMembership)
```rust
pub struct RoomMembership {
    pub id: i64,
    pub room_id: String,
    pub user_id: String,
    pub membership: String,
    pub joined_ts: Option<i64>,
    pub invited_ts: Option<i64>,
    pub left_ts: Option<i64>,
    pub banned_ts: Option<i64>,
    pub sender: Option<String>,
    pub reason: Option<String>,
    pub event_id: Option<String>,
    pub event_type: Option<String>,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub is_banned: bool,
    pub invite_token: Option<String>,
    pub updated_ts: Option<i64>,
    pub join_reason: Option<String>,
    pub banned_by: Option<String>,
    pub ban_reason: Option<String>,
}
```

### 字段对比

| 字段名 | SQL 类型 | Rust 类型 | 匹配 | 备注 |
|--------|----------|-----------|------|------|
| id | BIGSERIAL | i64 | ✅ | |
| room_id | TEXT NOT NULL | String | ✅ | |
| user_id | TEXT NOT NULL | String | ✅ | |
| membership | TEXT NOT NULL | String | ✅ | |
| joined_ts | BIGINT | Option<i64> | ✅ | |
| invited_ts | BIGINT | Option<i64> | ✅ | |
| left_ts | BIGINT | Option<i64> | ✅ | |
| banned_ts | BIGINT | Option<i64> | ✅ | |
| sender | TEXT | Option<String> | ✅ | |
| reason | TEXT | Option<String> | ✅ | |
| event_id | TEXT | Option<String> | ✅ | |
| event_type | TEXT | Option<String> | ✅ | |
| display_name | TEXT | Option<String> | ✅ | |
| avatar_url | TEXT | Option<String> | ✅ | |
| is_banned | BOOLEAN | bool | ✅ | |
| invite_token | TEXT | Option<String> | ✅ | |
| updated_ts | BIGINT | Option<i64> | ✅ | |
| join_reason | TEXT | Option<String> | ✅ | |
| banned_by | TEXT | Option<String> | ✅ | |
| ban_reason | TEXT | Option<String> | ✅ | |

**状态**: ✅ 完全匹配

---

## 9. events 表

### SQL 定义
```sql
CREATE TABLE events (
    event_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    sender TEXT NOT NULL,
    event_type TEXT NOT NULL,
    content JSONB NOT NULL,
    origin_server_ts BIGINT NOT NULL,
    state_key TEXT,
    is_redacted BOOLEAN DEFAULT FALSE,
    redacted_at BIGINT,
    redacted_by TEXT,
    transaction_id TEXT,
    depth BIGINT,
    prev_events JSONB,
    auth_events JSONB,
    signatures JSONB,
    hashes JSONB,
    unsigned JSONB DEFAULT '{}',
    processed_at BIGINT,
    not_before BIGINT DEFAULT 0,
    status TEXT,
    reference_image TEXT,
    origin TEXT,
    user_id TEXT,
    PRIMARY KEY (event_id)
);
```

### Rust 定义 (Event)
```rust
pub struct Event {
    pub event_id: String,
    pub room_id: String,
    pub sender: String,
    pub event_type: String,
    pub content: serde_json::Value,
    pub origin_server_ts: i64,
    pub state_key: Option<String>,
    pub is_redacted: bool,
    pub redacted_at: Option<i64>,
    pub redacted_by: Option<String>,
    pub transaction_id: Option<String>,
    pub depth: Option<i64>,
    pub prev_events: Option<serde_json::Value>,
    pub auth_events: Option<serde_json::Value>,
    pub signatures: Option<serde_json::Value>,
    pub hashes: Option<serde_json::Value>,
    pub unsigned: Option<serde_json::Value>,
    pub processed_at: Option<i64>,
    pub not_before: i64,
    pub status: Option<String>,
    pub reference_image: Option<String>,
    pub origin: Option<String>,
    pub user_id: Option<String>,
}
```

### 字段对比

| 字段名 | SQL 类型 | Rust 类型 | 匹配 | 备注 |
|--------|----------|-----------|------|------|
| event_id | TEXT NOT NULL | String | ✅ | |
| room_id | TEXT NOT NULL | String | ✅ | |
| sender | TEXT NOT NULL | String | ✅ | |
| event_type | TEXT NOT NULL | String | ✅ | |
| content | JSONB NOT NULL | serde_json::Value | ✅ | |
| origin_server_ts | BIGINT NOT NULL | i64 | ✅ | |
| state_key | TEXT | Option<String> | ✅ | |
| is_redacted | BOOLEAN | bool | ✅ | |
| redacted_at | BIGINT | Option<i64> | ✅ | |
| redacted_by | TEXT | Option<String> | ✅ | |
| transaction_id | TEXT | Option<String> | ✅ | |
| depth | BIGINT | Option<i64> | ✅ | |
| prev_events | JSONB | Option<serde_json::Value> | ✅ | |
| auth_events | JSONB | Option<serde_json::Value> | ✅ | |
| signatures | JSONB | Option<serde_json::Value> | ✅ | |
| hashes | JSONB | Option<serde_json::Value> | ✅ | |
| unsigned | JSONB | Option<serde_json::Value> | ✅ | |
| processed_at | BIGINT | Option<i64> | ✅ | |
| not_before | BIGINT | i64 | ✅ | |
| status | TEXT | Option<String> | ✅ | |
| reference_image | TEXT | Option<String> | ✅ | |
| origin | TEXT | Option<String> | ✅ | |
| user_id | TEXT | Option<String> | ✅ | |

**状态**: ✅ 完全匹配

---

## 10. room_summaries 表

### SQL 定义
```sql
CREATE TABLE room_summaries (
    room_id TEXT NOT NULL,
    name TEXT,
    topic TEXT,
    canonical_alias TEXT,
    member_count BIGINT DEFAULT 0,
    joined_members BIGINT DEFAULT 0,
    invited_members BIGINT DEFAULT 0,
    hero_users JSONB,
    is_world_readable BOOLEAN DEFAULT FALSE,
    can_guest_join BOOLEAN DEFAULT FALSE,
    is_federated BOOLEAN DEFAULT TRUE,
    encryption_state TEXT,
    updated_ts BIGINT,
    PRIMARY KEY (room_id)
);
```

### Rust 定义 (RoomSummary)
```rust
pub struct RoomSummary {
    pub room_id: String,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub canonical_alias: Option<String>,
    pub joined_members: i64,
    pub invited_members: i64,
    pub hero_users: Option<serde_json::Value>,
    pub is_world_readable: bool,
    pub can_guest_join: bool,
    pub is_federated: bool,
    pub encryption_state: Option<String>,
    pub updated_ts: Option<i64>,
}
```

### 字段对比

| 字段名 | SQL 类型 | Rust 类型 | 匹配 | 备注 |
|--------|----------|-----------|------|------|
| room_id | TEXT NOT NULL | String | ✅ | |
| name | TEXT | Option<String> | ✅ | |
| topic | TEXT | Option<String> | ✅ | |
| canonical_alias | TEXT | Option<String> | ✅ | |
| **member_count** | BIGINT | **❌ 缺失** | **P2** | Rust 缺少此字段 |
| joined_members | BIGINT | i64 | ✅ | |
| invited_members | BIGINT | i64 | ✅ | |
| hero_users | JSONB | Option<serde_json::Value> | ✅ | |
| is_world_readable | BOOLEAN | bool | ✅ | |
| can_guest_join | BOOLEAN | bool | ✅ | |
| is_federated | BOOLEAN | bool | ✅ | |
| encryption_state | TEXT | Option<String> | ✅ | |
| updated_ts | BIGINT | Option<i64> | ✅ | |

### 问题列表

| 严重程度 | 问题 | 位置 | 建议修复 |
|----------|------|------|----------|
| P2 | `member_count` 字段在 SQL 存在但 Rust 模型缺失 | room.rs | 添加 `pub member_count: i64` 字段 |

---

## 11. device_keys 表

### SQL 定义
```sql
CREATE TABLE device_keys (
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
    PRIMARY KEY (id)
);
```

### Rust 定义 (DeviceKey)
```rust
pub struct DeviceKey {
    pub id: i64,
    pub user_id: String,
    pub device_id: String,
    pub algorithm: String,
    pub key_id: String,
    pub public_key: String,
    pub key_data: Option<String>,
    pub signatures: Option<serde_json::Value>,
    pub added_ts: i64,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
    pub is_verified: bool,
    pub is_blocked: bool,
    pub display_name: Option<String>,
}
```

### 字段对比

| 字段名 | SQL 类型 | Rust 类型 | 匹配 | 备注 |
|--------|----------|-----------|------|------|
| id | BIGSERIAL | i64 | ✅ | |
| user_id | TEXT NOT NULL | String | ✅ | |
| device_id | TEXT NOT NULL | String | ✅ | |
| algorithm | TEXT NOT NULL | String | ✅ | |
| key_id | TEXT NOT NULL | String | ✅ | |
| public_key | TEXT NOT NULL | String | ✅ | |
| key_data | TEXT | Option<String> | ✅ | |
| signatures | JSONB | Option<serde_json::Value> | ✅ | |
| added_ts | BIGINT NOT NULL | i64 | ✅ | |
| created_ts | BIGINT NOT NULL | i64 | ✅ | |
| updated_ts | BIGINT | Option<i64> | ✅ | |
| **ts_updated_ms** | **BIGINT** | **❌ 缺失** | **P1** | SQL 有此字段，Rust 缺失 |
| is_verified | BOOLEAN | bool | ✅ | |
| is_blocked | BOOLEAN | bool | ✅ | |
| display_name | TEXT | Option<String> | ✅ | |

### 问题列表

| 严重程度 | 问题 | 位置 | 建议修复 |
|----------|------|------|----------|
| P1 | `ts_updated_ms` 字段在 SQL 存在但 Rust 模型缺失 | crypto.rs | 添加 `pub ts_updated_ms: Option<i64>` 字段 |

---

## 12. key_backups 表

### SQL 定义
```sql
CREATE TABLE key_backups (
    backup_id BIGSERIAL,
    user_id TEXT NOT NULL,
    algorithm TEXT NOT NULL,
    auth_data JSONB,
    auth_key TEXT,
    mgmt_key TEXT,
    version BIGINT DEFAULT 1,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    PRIMARY KEY (backup_id)
);
```

### Rust 定义 (KeyBackup)
```rust
pub struct KeyBackup {
    pub id: i64,
    pub user_id: String,
    pub algorithm: String,
    pub auth_data: serde_json::Value,
    pub auth_key: Option<String>,
    pub version: i64,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
}
```

### 字段对比

| 字段名 | SQL 类型 | Rust 类型 | 匹配 | 备注 |
|--------|----------|-----------|------|------|
| **backup_id** | **BIGSERIAL** | **i64** | ⚠️ | Rust 用 `id`，SQL 用 `backup_id` |
| user_id | TEXT NOT NULL | String | ✅ | |
| algorithm | TEXT NOT NULL | String | ✅ | |
| auth_data | JSONB | serde_json::Value | ✅ | |
| auth_key | TEXT | Option<String> | ✅ | |
| **mgmt_key** | **TEXT** | **❌ 缺失** | **P1** | SQL 有此字段，Rust 缺失 |
| version | BIGINT | i64 | ✅ | |
| created_ts | BIGINT NOT NULL | i64 | ✅ | |
| updated_ts | BIGINT | Option<i64> | ✅ | |

### 问题列表

| 严重程度 | 问题 | 位置 | 建议修复 |
|----------|------|------|----------|
| P1 | `mgmt_key` 字段在 SQL 存在但 Rust 模型缺失 | crypto.rs | 添加 `pub mgmt_key: Option<String>` 字段 |
| P2 | SQL 主键为 `backup_id`，Rust 模型用 `id` | crypto.rs | 考虑统一命名 |

---

## 13. backup_keys 表

### SQL 定义
```sql
CREATE TABLE backup_keys (
    id BIGSERIAL,
    backup_id BIGINT NOT NULL,
    room_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    session_data JSONB NOT NULL,
    created_ts BIGINT NOT NULL,
    PRIMARY KEY (id)
);
```

### Rust 定义 (BackupKey)
```rust
pub struct BackupKey {
    pub id: i64,
    pub backup_id: i64,
    pub room_id: String,
    pub session_id: String,
    pub session_data: serde_json::Value,
    pub created_ts: i64,
}
```

### 字段对比

| 字段名 | SQL 类型 | Rust 类型 | 匹配 | 备注 |
|--------|----------|-----------|------|------|
| id | BIGSERIAL | i64 | ✅ | |
| backup_id | BIGINT NOT NULL | i64 | ✅ | |
| room_id | TEXT NOT NULL | String | ✅ | |
| session_id | TEXT NOT NULL | String | ✅ | |
| session_data | JSONB NOT NULL | serde_json::Value | ✅ | |
| created_ts | BIGINT NOT NULL | i64 | ✅ | |

**状态**: ✅ 完全匹配

---

## 14. megolm_sessions 表

### SQL 定义
```sql
CREATE TABLE megolm_sessions (
    id UUID DEFAULT gen_random_uuid(),
    session_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    sender_key TEXT NOT NULL,
    session_key TEXT NOT NULL,
    algorithm TEXT NOT NULL,
    message_index BIGINT DEFAULT 0,
    created_ts BIGINT NOT NULL,
    last_used_ts BIGINT,
    expires_at BIGINT,
    PRIMARY KEY (id)
);
```

### Rust 定义 (MegolmSession)
```rust
pub struct MegolmSession {
    pub id: uuid::Uuid,
    pub session_id: String,
    pub room_id: String,
    pub sender_key: String,
    pub session_key: String,
    pub algorithm: String,
    pub message_index: i64,
    pub created_ts: i64,
    pub last_used_ts: Option<i64>,
    pub expires_at: Option<i64>,
}
```

### 字段对比

| 字段名 | SQL 类型 | Rust 类型 | 匹配 | 备注 |
|--------|----------|-----------|------|------|
| id | UUID | uuid::Uuid | ✅ | |
| session_id | TEXT NOT NULL | String | ✅ | |
| room_id | TEXT NOT NULL | String | ✅ | |
| sender_key | TEXT NOT NULL | String | ✅ | |
| session_key | TEXT NOT NULL | String | ✅ | |
| algorithm | TEXT NOT NULL | String | ✅ | |
| message_index | BIGINT | i64 | ✅ | |
| created_ts | BIGINT NOT NULL | i64 | ✅ | |
| last_used_ts | BIGINT | Option<i64> | ✅ | |
| expires_at | BIGINT | Option<i64> | ✅ | |

**状态**: ✅ 完全匹配

---

## 15. olm_sessions 表

### SQL 定义
```sql
CREATE TABLE olm_sessions (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    sender_key TEXT NOT NULL,
    receiver_key TEXT NOT NULL,
    serialized_state TEXT NOT NULL,
    message_index INTEGER DEFAULT 0,
    created_ts BIGINT NOT NULL,
    last_used_ts BIGINT NOT NULL,
    expires_at BIGINT,
    UNIQUE (session_id)
);
```

### Rust 模型

**Rust 中没有 OlmSession 模型定义！**

### 问题列表

| 严重程度 | 问题 | 位置 | 建议修复 |
|----------|------|------|----------|
| P0 | `olm_sessions` 表没有对应的 Rust 模型 | - | 创建 `OlmSession` 结构体 |

---

## 16. olm_accounts 表

### SQL 定义
```sql
CREATE TABLE olm_accounts (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    identity_key TEXT NOT NULL,
    serialized_account TEXT NOT NULL,
    is_one_time_keys_published BOOLEAN DEFAULT FALSE,
    is_fallback_key_published BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    UNIQUE (user_id, device_id)
);
```

### Rust 模型

**Rust 中没有 OlmAccount 模型定义！**

### 问题列表

| 严重程度 | 问题 | 位置 | 建议修复 |
|----------|------|------|----------|
| P0 | `olm_accounts` 表没有对应的 Rust 模型 | - | 创建 `OlmAccount` 结构体 |

---

## 17. event_signatures 表

### SQL 定义
```sql
CREATE TABLE event_signatures (
    id UUID DEFAULT gen_random_uuid(),
    event_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    signature TEXT NOT NULL,
    key_id TEXT NOT NULL,
    algorithm TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    PRIMARY KEY (id)
);
```

### Rust 定义 (EventSignature)
```rust
pub struct EventSignature {
    pub id: uuid::Uuid,
    pub event_id: String,
    pub user_id: String,
    pub device_id: String,
    pub signature: String,
    pub key_id: String,
    pub algorithm: String,
    pub created_ts: i64,
}
```

### 字段对比

| 字段名 | SQL 类型 | Rust 类型 | 匹配 | 备注 |
|--------|----------|-----------|------|------|
| id | UUID | uuid::Uuid | ✅ | |
| event_id | TEXT NOT NULL | String | ✅ | |
| user_id | TEXT NOT NULL | String | ✅ | |
| device_id | TEXT NOT NULL | String | ✅ | |
| signature | TEXT NOT NULL | String | ✅ | |
| key_id | TEXT NOT NULL | String | ✅ | |
| algorithm | TEXT NOT NULL | String | ✅ | |
| created_ts | BIGINT NOT NULL | i64 | ✅ | |

**状态**: ✅ 完全匹配

---

## 18. device_signatures 表

### SQL 定义
```sql
CREATE TABLE device_signatures (
    id BIGSERIAL,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    target_user_id TEXT NOT NULL,
    target_device_id TEXT NOT NULL,
    algorithm TEXT NOT NULL,
    signature TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    PRIMARY KEY (id)
);
```

### Rust 定义 (DeviceSignature)
```rust
pub struct DeviceSignature {
    pub id: i64,
    pub user_id: String,
    pub device_id: String,
    pub target_user_id: String,
    pub target_device_id: String,
    pub algorithm: String,
    pub signature: String,
    pub created_ts: i64,
}
```

### 字段对比

| 字段名 | SQL 类型 | Rust 类型 | 匹配 | 备注 |
|--------|----------|-----------|------|------|
| id | BIGSERIAL | i64 | ✅ | |
| user_id | TEXT NOT NULL | String | ✅ | |
| device_id | TEXT NOT NULL | String | ✅ | |
| target_user_id | TEXT NOT NULL | String | ✅ | |
| target_device_id | TEXT NOT NULL | String | ✅ | |
| algorithm | TEXT NOT NULL | String | ✅ | |
| signature | TEXT NOT NULL | String | ✅ | |
| created_ts | BIGINT NOT NULL | i64 | ✅ | |

**状态**: ✅ 完全匹配

---

## 19. media_metadata 表

### SQL 定义
```sql
CREATE TABLE media_metadata (
    media_id TEXT NOT NULL,
    server_name TEXT NOT NULL,
    content_type TEXT NOT NULL,
    file_name TEXT,
    size BIGINT NOT NULL,
    uploader_user_id TEXT,
    created_ts BIGINT NOT NULL,
    last_accessed_at BIGINT,
    quarantine_status TEXT,
    PRIMARY KEY (media_id)
);
```

### Rust 定义 (MediaMetadata)
```rust
pub struct MediaMetadata {
    pub media_id: String,
    pub server_name: String,
    pub content_type: String,
    pub file_name: Option<String>,
    pub size: i64,
    pub uploader_user_id: Option<String>,
    pub created_ts: i64,
    pub last_accessed_at: Option<i64>,
    pub quarantine_status: Option<String>,
}
```

### 字段对比

| 字段名 | SQL 类型 | Rust 类型 | 匹配 | 备注 |
|--------|----------|-----------|------|------|
| media_id | TEXT NOT NULL | String | ✅ | |
| server_name | TEXT NOT NULL | String | ✅ | |
| content_type | TEXT NOT NULL | String | ✅ | |
| file_name | TEXT | Option<String> | ✅ | |
| size | BIGINT NOT NULL | i64 | ✅ | |
| uploader_user_id | TEXT | Option<String> | ✅ | |
| created_ts | BIGINT NOT NULL | i64 | ✅ | |
| last_accessed_at | BIGINT | Option<i64> | ✅ | |
| quarantine_status | TEXT | Option<String> | ✅ | |

**状态**: ✅ 完全匹配

---

## 20. thumbnails 表

### SQL 定义
```sql
CREATE TABLE thumbnails (
    id BIGSERIAL,
    media_id TEXT NOT NULL,
    width INTEGER NOT NULL,
    height INTEGER NOT NULL,
    method TEXT NOT NULL,
    content_type TEXT NOT NULL,
    size BIGINT NOT NULL,
    created_ts BIGINT NOT NULL,
    PRIMARY KEY (id)
);
```

### Rust 定义 (Thumbnail)
```rust
pub struct Thumbnail {
    pub id: i64,
    pub media_id: String,
    pub width: i32,
    pub height: i32,
    pub method: String,
    pub content_type: String,
    pub size: i64,
    pub created_ts: i64,
}
```

### 字段对比

| 字段名 | SQL 类型 | Rust 类型 | 匹配 | 备注 |
|--------|----------|-----------|------|------|
| id | BIGSERIAL | i64 | ✅ | |
| media_id | TEXT NOT NULL | String | ✅ | |
| width | INTEGER | i32 | ✅ | |
| height | INTEGER | i32 | ✅ | |
| method | TEXT NOT NULL | String | ✅ | |
| content_type | TEXT NOT NULL | String | ✅ | |
| size | BIGINT NOT NULL | i64 | ✅ | |
| created_ts | BIGINT NOT NULL | i64 | ✅ | |

**状态**: ✅ 完全匹配

---

## 21. push_devices 表

### SQL 定义
```sql
CREATE TABLE push_devices (
    id BIGSERIAL,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    push_kind TEXT NOT NULL,
    app_id TEXT NOT NULL,
    app_display_name TEXT,
    device_display_name TEXT,
    profile_tag TEXT,
    pushkey TEXT NOT NULL,
    lang TEXT DEFAULT 'en',
    data JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    is_enabled BOOLEAN DEFAULT TRUE,
    PRIMARY KEY (id)
);
```

### Rust 定义 (PushDevice)
```rust
pub struct PushDevice {
    pub id: i64,
    pub user_id: String,
    pub device_id: String,
    pub push_kind: String,
    pub app_id: String,
    pub app_display_name: Option<String>,
    pub device_display_name: Option<String>,
    pub profile_tag: Option<String>,
    pub pushkey: String,
    pub lang: String,
    pub data: Option<serde_json::Value>,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
    pub is_enabled: bool,
}
```

### 字段对比

| 字段名 | SQL 类型 | Rust 类型 | 匹配 | 备注 |
|--------|----------|-----------|------|------|
| id | BIGSERIAL | i64 | ✅ | |
| user_id | TEXT NOT NULL | String | ✅ | |
| device_id | TEXT NOT NULL | String | ✅ | |
| push_kind | TEXT NOT NULL | String | ✅ | |
| app_id | TEXT NOT NULL | String | ✅ | |
| app_display_name | TEXT | Option<String> | ✅ | |
| device_display_name | TEXT | Option<String> | ✅ | |
| profile_tag | TEXT | Option<String> | ✅ | |
| pushkey | TEXT NOT NULL | String | ✅ | |
| lang | TEXT | String | ✅ | |
| data | JSONB | Option<serde_json::Value> | ✅ | |
| created_ts | BIGINT NOT NULL | i64 | ✅ | |
| updated_ts | BIGINT | Option<i64> | ✅ | |
| is_enabled | BOOLEAN | bool | ✅ | |

**状态**: ✅ 完全匹配

---

## 22. push_rules 表

### SQL 定义
```sql
CREATE TABLE push_rules (
    id BIGSERIAL,
    user_id TEXT NOT NULL,
    scope TEXT NOT NULL,
    rule_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    priority_class INTEGER NOT NULL,
    priority INTEGER DEFAULT 0,
    conditions JSONB DEFAULT '[]',
    actions JSONB DEFAULT '[]',
    pattern TEXT,
    is_default BOOLEAN DEFAULT FALSE,
    is_enabled BOOLEAN DEFAULT TRUE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    PRIMARY KEY (id)
);
```

### Rust 定义 (PushRule)
```rust
pub struct PushRule {
    pub id: i64,
    pub user_id: String,
    pub scope: String,
    pub rule_id: String,
    pub kind: String,
    pub priority_class: i32,
    pub priority: i32,
    pub conditions: Option<serde_json::Value>,
    pub actions: Option<serde_json::Value>,
    pub pattern: Option<String>,
    pub is_default: bool,
    pub is_enabled: bool,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
}
```

### 字段对比

| 字段名 | SQL 类型 | Rust 类型 | 匹配 | 备注 |
|--------|----------|-----------|------|------|
| id | BIGSERIAL | i64 | ✅ | |
| user_id | TEXT NOT NULL | String | ✅ | |
| scope | TEXT NOT NULL | String | ✅ | |
| rule_id | TEXT NOT NULL | String | ✅ | |
| kind | TEXT NOT NULL | String | ✅ | |
| priority_class | INTEGER | i32 | ✅ | |
| priority | INTEGER | i32 | ✅ | |
| conditions | JSONB | Option<serde_json::Value> | ✅ | |
| actions | JSONB | Option<serde_json::Value> | ✅ | |
| pattern | TEXT | Option<String> | ✅ | |
| is_default | BOOLEAN | bool | ✅ | |
| is_enabled | BOOLEAN | bool | ✅ | |
| created_ts | BIGINT NOT NULL | i64 | ✅ | |
| updated_ts | BIGINT | Option<i64> | ✅ | |

**状态**: ✅ 完全匹配

---

## 23. pushers 表

### SQL 定义
```sql
CREATE TABLE pushers (
    id BIGSERIAL,
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
    PRIMARY KEY (id)
);
```

### Rust 定义 (Pusher)
```rust
pub struct Pusher {
    pub id: i64,
    pub user_id: String,
    pub device_id: String,
    pub pushkey: String,
    pub pushkey_ts: i64,
    pub kind: String,
    pub app_id: String,
    pub app_display_name: String,
    pub device_display_name: String,
    pub profile_tag: Option<String>,
    pub lang: String,
    pub data: Option<serde_json::Value>,
    pub last_updated_ts: Option<i64>,
    pub created_ts: i64,
    pub is_enabled: bool,
}
```

### 字段对比

| 字段名 | SQL 类型 | Rust 类型 | 匹配 | 备注 |
|--------|----------|-----------|------|------|
| id | BIGSERIAL | i64 | ✅ | |
| user_id | TEXT NOT NULL | String | ✅ | |
| device_id | TEXT NOT NULL | String | ✅ | |
| pushkey | TEXT NOT NULL | String | ✅ | |
| pushkey_ts | BIGINT NOT NULL | i64 | ✅ | |
| kind | TEXT NOT NULL | String | ✅ | |
| app_id | TEXT NOT NULL | String | ✅ | |
| app_display_name | TEXT NOT NULL | String | ✅ | |
| device_display_name | TEXT NOT NULL | String | ✅ | |
| profile_tag | TEXT | Option<String> | ✅ | |
| lang | TEXT | String | ✅ | |
| data | JSONB | Option<serde_json::Value> | ✅ | |
| **updated_ts** | **BIGINT** | **last_updated_ts** | ⚠️ | 字段名不一致 |
| created_ts | BIGINT NOT NULL | i64 | ✅ | |
| is_enabled | BOOLEAN | bool | ✅ | |

### 问题列表

| 严重程度 | 问题 | 位置 | 建议修复 |
|----------|------|------|----------|
| P2 | SQL 用 `updated_ts`，Rust 用 `last_updated_ts` | push.rs | 统一命名为 `updated_ts` |

---

## 24. notifications 表

### SQL 定义
```sql
CREATE TABLE notifications (
    id BIGSERIAL,
    user_id TEXT NOT NULL,
    event_id TEXT,
    room_id TEXT,
    ts BIGINT NOT NULL,
    notification_type VARCHAR(50) DEFAULT 'message',
    profile_tag VARCHAR(255),
    is_read BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    PRIMARY KEY (id)
);
```

### Rust 定义 (Notification)
```rust
pub struct Notification {
    pub id: i64,
    pub user_id: String,
    pub event_id: Option<String>,
    pub room_id: Option<String>,
    pub ts: i64,
    pub notification_type: String,
    pub profile_tag: Option<String>,
    pub is_read: bool,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
}
```

### 字段对比

| 字段名 | SQL 类型 | Rust 类型 | 匹配 | 备注 |
|--------|----------|-----------|------|------|
| id | BIGSERIAL | i64 | ✅ | |
| user_id | TEXT NOT NULL | String | ✅ | |
| event_id | TEXT | Option<String> | ✅ | |
| room_id | TEXT | Option<String> | ✅ | |
| ts | BIGINT NOT NULL | i64 | ✅ | |
| notification_type | VARCHAR(50) | String | ✅ | |
| profile_tag | VARCHAR(255) | Option<String> | ✅ | |
| is_read | BOOLEAN | bool | ✅ | |
| created_ts | BIGINT NOT NULL | i64 | ✅ | |
| updated_ts | BIGINT | Option<i64> | ✅ | |

**状态**: ✅ 完全匹配

---

## 25. federation_servers 表

### SQL 定义
```sql
CREATE TABLE federation_servers (
    id BIGSERIAL,
    server_name TEXT NOT NULL,
    is_blocked BOOLEAN DEFAULT FALSE,
    blocked_at BIGINT,
    blocked_reason TEXT,
    last_successful_connect_at BIGINT,
    last_failed_connect_at BIGINT,
    failure_count INTEGER DEFAULT 0,
    PRIMARY KEY (id)
);
```

### Rust 定义 (FederationServer)
```rust
pub struct FederationServer {
    pub id: i64,
    pub server_name: String,
    pub is_blocked: bool,
    pub blocked_at: Option<i64>,
    pub blocked_reason: Option<String>,
    pub last_successful_connect_at: Option<i64>,
    pub last_failed_connect_at: Option<i64>,
    pub failure_count: i32,
}
```

### 字段对比

| 字段名 | SQL 类型 | Rust 类型 | 匹配 | 备注 |
|--------|----------|-----------|------|------|
| id | BIGSERIAL | i64 | ✅ | |
| server_name | TEXT NOT NULL | String | ✅ | |
| is_blocked | BOOLEAN | bool | ✅ | |
| blocked_at | BIGINT | Option<i64> | ✅ | |
| blocked_reason | TEXT | Option<String> | ✅ | |
| last_successful_connect_at | BIGINT | Option<i64> | ✅ | |
| last_failed_connect_at | BIGINT | Option<i64> | ✅ | |
| failure_count | INTEGER | i32 | ✅ | |

**状态**: ✅ 完全匹配

---

## 26. federation_blacklist 表

### SQL 定义
```sql
CREATE TABLE federation_blacklist (
    id BIGSERIAL,
    server_name TEXT NOT NULL,
    reason TEXT,
    added_ts BIGINT NOT NULL,
    added_by TEXT,
    updated_ts BIGINT,
    PRIMARY KEY (id)
);
```

### Rust 定义 (FederationBlacklist)
```rust
pub struct FederationBlacklist {
    pub id: i64,
    pub server_name: String,
    pub reason: Option<String>,
    pub added_ts: i64,
    pub added_by: Option<String>,
    pub updated_ts: Option<i64>,
}
```

### 字段对比

| 字段名 | SQL 类型 | Rust 类型 | 匹配 | 备注 |
|--------|----------|-----------|------|------|
| id | BIGSERIAL | i64 | ✅ | |
| server_name | TEXT NOT NULL | String | ✅ | |
| reason | TEXT | Option<String> | ✅ | |
| added_ts | BIGINT NOT NULL | i64 | ✅ | |
| added_by | TEXT | Option<String> | ✅ | |
| updated_ts | BIGINT | Option<i64> | ✅ | |

**状态**: ✅ 完全匹配

---

## 27. federation_queue 表

### SQL 定义
```sql
CREATE TABLE federation_queue (
    id BIGSERIAL,
    destination TEXT NOT NULL,
    event_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    room_id TEXT,
    content JSONB NOT NULL,
    created_ts BIGINT NOT NULL,
    sent_at BIGINT,
    retry_count INTEGER DEFAULT 0,
    status TEXT DEFAULT 'pending',
    PRIMARY KEY (id)
);
```

### Rust 定义 (FederationQueue)
```rust
pub struct FederationQueue {
    pub id: i64,
    pub destination: String,
    pub event_id: String,
    pub event_type: String,
    pub room_id: Option<String>,
    pub content: serde_json::Value,
    pub created_ts: i64,
    pub sent_at: Option<i64>,
    pub retry_count: i32,
    pub status: String,
}
```

### 字段对比

| 字段名 | SQL 类型 | Rust 类型 | 匹配 | 备注 |
|--------|----------|-----------|------|------|
| id | BIGSERIAL | i64 | ✅ | |
| destination | TEXT NOT NULL | String | ✅ | |
| event_id | TEXT NOT NULL | String | ✅ | |
| event_type | TEXT NOT NULL | String | ✅ | |
| room_id | TEXT | Option<String> | ✅ | |
| content | JSONB NOT NULL | serde_json::Value | ✅ | |
| created_ts | BIGINT NOT NULL | i64 | ✅ | |
| sent_at | BIGINT | Option<i64> | ✅ | |
| retry_count | INTEGER | i32 | ✅ | |
| status | TEXT | String | ✅ | |

**状态**: ✅ 完全匹配

---

## 28. application_services 表

### SQL 定义
```sql
CREATE TABLE application_services (
    id BIGSERIAL,
    as_id TEXT NOT NULL,
    url TEXT NOT NULL,
    as_token TEXT NOT NULL,
    hs_token TEXT NOT NULL,
    sender_localpart TEXT NOT NULL,
    is_enabled BOOLEAN DEFAULT FALSE,
    rate_limited BOOLEAN DEFAULT TRUE,
    protocols TEXT[] DEFAULT '{}',
    namespaces JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    description TEXT,
    PRIMARY KEY (id)
);
```

### Rust 定义 (ApplicationService)
```rust
pub struct ApplicationService {
    pub id: i64,
    pub as_id: String,
    pub url: String,
    pub as_token: String,
    pub hs_token: String,
    pub sender_localpart: String,
    pub is_enabled: bool,
    pub rate_limited: bool,
    pub protocols: Option<Vec<String>>,
    pub namespaces: Option<serde_json::Value>,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
    pub description: Option<String>,
}
```

### 字段对比

| 字段名 | SQL 类型 | Rust 类型 | 匹配 | 备注 |
|--------|----------|-----------|------|------|
| id | BIGSERIAL | i64 | ✅ | |
| as_id | TEXT NOT NULL | String | ✅ | |
| url | TEXT NOT NULL | String | ✅ | |
| as_token | TEXT NOT NULL | String | ✅ | |
| hs_token | TEXT NOT NULL | String | ✅ | |
| sender_localpart | TEXT NOT NULL | String | ✅ | |
| is_enabled | BOOLEAN | bool | ✅ | |
| rate_limited | BOOLEAN | bool | ✅ | |
| protocols | TEXT[] | Option<Vec<String>> | ✅ | |
| namespaces | JSONB | Option<serde_json::Value> | ✅ | |
| created_ts | BIGINT NOT NULL | i64 | ✅ | |
| updated_ts | BIGINT | Option<i64> | ✅ | |
| description | TEXT | Option<String> | ✅ | |

**状态**: ✅ 完全匹配

---

## 29. openid_tokens 表

### SQL 定义
```sql
CREATE TABLE openid_tokens (
    id BIGSERIAL,
    token TEXT NOT NULL,
    user_id TEXT NOT NULL,
    device_id TEXT,
    created_ts BIGINT NOT NULL,
    expires_ts BIGINT NOT NULL,
    is_valid BOOLEAN DEFAULT TRUE,
    PRIMARY KEY (id)
);
```

### Rust 定义 (OpenIdToken)
```rust
pub struct OpenIdToken {
    pub id: i64,
    pub token: String,
    pub user_id: String,
    pub device_id: Option<String>,
    pub created_ts: i64,
    pub expires_at: i64,
    pub is_valid: bool,
}
```

### 字段对比

| 字段名 | SQL 类型 | Rust 类型 | 匹配 | 备注 |
|--------|----------|-----------|------|------|
| id | BIGSERIAL | i64 | ✅ | |
| token | TEXT NOT NULL | String | ✅ | |
| user_id | TEXT NOT NULL | String | ✅ | |
| device_id | TEXT | Option<String> | ✅ | |
| created_ts | BIGINT NOT NULL | i64 | ✅ | |
| **expires_ts** | **BIGINT NOT NULL** | **expires_at: i64** | ⚠️ | 命名不一致 |
| **is_valid** | **BOOLEAN** | **bool** | ✅ | |

### 问题列表

| 严重程度 | 问题 | 位置 | 建议修复 |
|----------|------|------|----------|
| P2 | SQL 用 `expires_ts`，Rust 用 `expires_at` | token.rs | 统一命名为 `expires_ts` |

---

## 30. refresh_token_families 表

### SQL 定义
```sql
CREATE TABLE refresh_token_families (
    id BIGSERIAL,
    family_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    device_id TEXT,
    created_ts BIGINT NOT NULL,
    last_refresh_ts BIGINT,
    refresh_count INTEGER DEFAULT 0,
    is_compromised BOOLEAN DEFAULT FALSE,
    compromised_at BIGINT,
    PRIMARY KEY (id)
);
```

### Rust 定义 (RefreshTokenFamily)
```rust
pub struct RefreshTokenFamily {
    pub id: i64,
    pub family_id: String,
    pub user_id: String,
    pub device_id: Option<String>,
    pub created_ts: i64,
    pub last_refresh_ts: Option<i64>,
    pub refresh_count: i32,
    pub is_compromised: bool,
    pub compromised_ts: Option<i64>,
}
```

### 字段对比

| 字段名 | SQL 类型 | Rust 类型 | 匹配 | 备注 |
|--------|----------|-----------|------|------|
| id | BIGSERIAL | i64 | ✅ | |
| family_id | TEXT NOT NULL | String | ✅ | |
| user_id | TEXT NOT NULL | String | ✅ | |
| device_id | TEXT | Option<String> | ✅ | |
| created_ts | BIGINT NOT NULL | i64 | ✅ | |
| last_refresh_ts | BIGINT | Option<i64> | ✅ | |
| refresh_count | INTEGER | i32 | ✅ | |
| is_compromised | BOOLEAN | bool | ✅ | |
| **compromised_at** | BIGINT | **compromised_ts** | ⚠️ | 命名不一致 |

### 问题列表

| 严重程度 | 问题 | 位置 | 建议修复 |
|----------|------|------|----------|
| P2 | SQL 用 `compromised_at`，Rust 用 `compromised_ts` | token.rs | 统一命名为 `compromised_at` |

---

## 31. thread_roots 表

### SQL 定义
```sql
CREATE TABLE thread_roots (
    id BIGSERIAL,
    room_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    sender TEXT NOT NULL,
    thread_id TEXT,
    reply_count BIGINT DEFAULT 0,
    last_reply_event_id TEXT,
    last_reply_sender TEXT,
    last_reply_ts BIGINT,
    participants JSONB DEFAULT '[]',
    is_fetched BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    PRIMARY KEY (id)
);
```

### Rust 定义 (ThreadRoot)
```rust
pub struct ThreadRoot {
    pub id: i64,
    pub room_id: String,
    pub event_id: String,
    pub sender: String,
    pub thread_id: Option<String>,
    pub reply_count: i64,
    pub last_reply_event_id: Option<String>,
    pub last_reply_sender: Option<String>,
    pub last_reply_ts: Option<i64>,
    pub participants: Option<serde_json::Value>,
    pub is_fetched: bool,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
}
```

### 字段对比

| 字段名 | SQL 类型 | Rust 类型 | 匹配 | 备注 |
|--------|----------|-----------|------|------|
| id | BIGSERIAL | i64 | ✅ | |
| room_id | TEXT NOT NULL | String | ✅ | |
| event_id | TEXT NOT NULL | String | ✅ | |
| sender | TEXT NOT NULL | String | ✅ | |
| thread_id | TEXT | Option<String> | ✅ | |
| reply_count | BIGINT | i64 | ✅ | |
| last_reply_event_id | TEXT | Option<String> | ✅ | |
| last_reply_sender | TEXT | Option<String> | ✅ | |
| last_reply_ts | BIGINT | Option<i64> | ✅ | |
| participants | JSONB | Option<serde_json::Value> | ✅ | |
| is_fetched | BOOLEAN | bool | ✅ | |
| created_ts | BIGINT NOT NULL | i64 | ✅ | |
| updated_ts | BIGINT | Option<i64> | ✅ | |

**状态**: ✅ 完全匹配

---

## 32. space_children 表

### SQL 定义
```sql
CREATE TABLE space_children (
    id BIGSERIAL,
    space_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    sender TEXT NOT NULL,
    is_suggested BOOLEAN DEFAULT FALSE,
    via_servers JSONB DEFAULT '[]',
    added_ts BIGINT NOT NULL,
    PRIMARY KEY (id)
);
```

### Rust 定义 (SpaceChild)
```rust
pub struct SpaceChild {
    pub id: i64,
    pub space_id: String,
    pub room_id: String,
    pub sender: String,
    pub is_suggested: bool,
    pub via_servers: Option<serde_json::Value>,
    pub added_ts: i64,
}
```

### 字段对比

| 字段名 | SQL 类型 | Rust 类型 | 匹配 | 备注 |
|--------|----------|-----------|------|------|
| id | BIGSERIAL | i64 | ✅ | |
| space_id | TEXT NOT NULL | String | ✅ | |
| room_id | TEXT NOT NULL | String | ✅ | |
| sender | TEXT NOT NULL | String | ✅ | |
| is_suggested | BOOLEAN | bool | ✅ | |
| via_servers | JSONB | Option<serde_json::Value> | ✅ | |
| added_ts | BIGINT NOT NULL | i64 | ✅ | |

**状态**: ✅ 完全匹配

---

## 33. room_parents 表

### SQL 定义
```sql
CREATE TABLE room_parents (
    id BIGSERIAL,
    room_id TEXT NOT NULL,
    parent_room_id TEXT NOT NULL,
    sender TEXT NOT NULL,
    is_suggested BOOLEAN DEFAULT FALSE,
    via_servers JSONB DEFAULT '[]',
    added_ts BIGINT NOT NULL,
    PRIMARY KEY (id)
);
```

### Rust 定义 (RoomParent)
```rust
pub struct RoomParent {
    pub id: i64,
    pub room_id: String,
    pub parent_room_id: String,
    pub sender: String,
    pub is_suggested: bool,
    pub via_servers: Option<serde_json::Value>,
    pub added_ts: i64,
}
```

### 字段对比

| 字段名 | SQL 类型 | Rust 类型 | 匹配 | 备注 |
|--------|----------|-----------|------|------|
| id | BIGSERIAL | i64 | ✅ | |
| room_id | TEXT NOT NULL | String | ✅ | |
| parent_room_id | TEXT NOT NULL | String | ✅ | |
| sender | TEXT NOT NULL | String | ✅ | |
| is_suggested | BOOLEAN | bool | ✅ | |
| via_servers | JSONB | Option<serde_json::Value> | ✅ | |
| added_ts | BIGINT NOT NULL | i64 | ✅ | |

**状态**: ✅ 完全匹配

---

## 34. room_aliases 表

### SQL 定义
```sql
CREATE TABLE room_aliases (
    room_alias TEXT NOT NULL,
    room_id TEXT NOT NULL,
    server_name TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    PRIMARY KEY (room_alias)
);
```

### Rust 定义 (RoomAlias)
```rust
pub struct RoomAlias {
    pub room_alias: String,
    pub room_id: String,
    pub server_name: String,
    pub created_ts: i64,
}
```

### 字段对比

| 字段名 | SQL 类型 | Rust 类型 | 匹配 | 备注 |
|--------|----------|-----------|------|------|
| room_alias | TEXT NOT NULL | String | ✅ | |
| room_id | TEXT NOT NULL | String | ✅ | |
| server_name | TEXT NOT NULL | String | ✅ | |
| created_ts | BIGINT NOT NULL | i64 | ✅ | |

**状态**: ✅ 完全匹配

---

## 35. event_receipts 表

### SQL 定义
```sql
CREATE TABLE event_receipts (
    id BIGSERIAL,
    event_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    receipt_type TEXT NOT NULL,
    ts BIGINT NOT NULL,
    data JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    PRIMARY KEY (id)
);
```

### Rust 定义 (EventReceipt)
```rust
pub struct EventReceipt {
    pub id: i64,
    pub event_id: String,
    pub room_id: String,
    pub user_id: String,
    pub receipt_type: String,
    pub ts: i64,
    pub data: Option<serde_json::Value>,
    pub created_ts: i64,
    pub updated_ts: i64,
}
```

### 字段对比

| 字段名 | SQL 类型 | Rust 类型 | 匹配 | 备注 |
|--------|----------|-----------|------|------|
| id | BIGSERIAL | i64 | ✅ | |
| event_id | TEXT NOT NULL | String | ✅ | |
| room_id | TEXT NOT NULL | String | ✅ | |
| user_id | TEXT NOT NULL | String | ✅ | |
| receipt_type | TEXT NOT NULL | String | ✅ | |
| ts | BIGINT NOT NULL | i64 | ✅ | |
| data | JSONB | Option<serde_json::Value> | ✅ | |
| created_ts | BIGINT NOT NULL | i64 | ✅ | |
| updated_ts | BIGINT NOT NULL | i64 | ✅ | |

**状态**: ✅ 完全匹配

---

## 36. event_reports 表

### SQL 定义
```sql
CREATE TABLE event_reports (
    id BIGSERIAL,
    event_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    reporter_user_id TEXT NOT NULL,
    reported_user_id TEXT,
    event_json JSONB,
    reason TEXT,
    description TEXT,
    status TEXT DEFAULT 'open',
    score INTEGER DEFAULT 0,
    received_ts BIGINT NOT NULL,
    resolved_at BIGINT,
    resolved_by TEXT,
    resolution_reason TEXT,
    PRIMARY KEY (id)
);
```

### Rust 定义 (EventReport)
```rust
pub struct EventReport {
    pub id: i64,
    pub event_id: String,
    pub room_id: String,
    pub reporter_user_id: String,
    pub reported_user_id: Option<String>,
    pub event_json: Option<serde_json::Value>,
    pub reason: Option<String>,
    pub description: Option<String>,
    pub status: String,
    pub score: i32,
    pub received_ts: i64,
    pub resolved_at: Option<i64>,
    pub resolved_by: Option<String>,
    pub resolution_reason: Option<String>,
}
```

### 字段对比

| 字段名 | SQL 类型 | Rust 类型 | 匹配 | 备注 |
|--------|----------|-----------|------|------|
| id | BIGSERIAL | i64 | ✅ | |
| event_id | TEXT NOT NULL | String | ✅ | |
| room_id | TEXT NOT NULL | String | ✅ | |
| reporter_user_id | TEXT NOT NULL | String | ✅ | |
| reported_user_id | TEXT | Option<String> | ✅ | |
| event_json | JSONB | Option<serde_json::Value> | ✅ | |
| reason | TEXT | Option<String> | ✅ | |
| description | TEXT | Option<String> | ✅ | |
| status | TEXT | String | ✅ | |
| score | INTEGER | i32 | ✅ | |
| received_ts | BIGINT NOT NULL | i64 | ✅ | |
| resolved_at | BIGINT | Option<i64> | ✅ | |
| resolved_by | TEXT | Option<String> | ✅ | |
| resolution_reason | TEXT | Option<String> | ✅ | |

**状态**: ✅ 完全匹配

---

## 37. voice_messages 表

### SQL 定义
```sql
CREATE TABLE voice_messages (
    id BIGSERIAL,
    event_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    room_id TEXT,
    media_id TEXT,
    duration_ms INT NOT NULL,
    waveform TEXT,
    mime_type VARCHAR(100),
    file_size BIGINT,
    transcription TEXT,
    encryption JSONB,
    is_processed BOOLEAN DEFAULT FALSE,
    processed_at BIGINT,
    created_ts BIGINT NOT NULL,
    PRIMARY KEY (id)
);
```

### Rust 定义 (VoiceMessage)
```rust
pub struct VoiceMessage {
    pub id: i64,
    pub event_id: String,
    pub user_id: String,
    pub room_id: Option<String>,
    pub media_id: Option<String>,
    pub duration_ms: i32,
    pub waveform: Option<String>,
    pub mime_type: Option<String>,
    pub file_size: Option<i64>,
    pub transcription: Option<String>,
    pub encryption: Option<serde_json::Value>,
    pub is_processed: bool,
    pub processed_at: Option<i64>,
    pub created_ts: i64,
}
```

### 字段对比

| 字段名 | SQL 类型 | Rust 类型 | 匹配 | 备注 |
|--------|----------|-----------|------|------|
| id | BIGSERIAL | i64 | ✅ | |
| event_id | TEXT NOT NULL | String | ✅ | |
| user_id | TEXT NOT NULL | String | ✅ | |
| room_id | TEXT | Option<String> | ✅ | |
| media_id | TEXT | Option<String> | ✅ | |
| duration_ms | INT | i32 | ✅ | |
| waveform | TEXT | Option<String> | ✅ | |
| mime_type | VARCHAR(100) | Option<String> | ✅ | |
| file_size | BIGINT | Option<i64> | ✅ | |
| transcription | TEXT | Option<String> | ✅ | |
| encryption | JSONB | Option<serde_json::Value> | ✅ | |
| is_processed | BOOLEAN | bool | ✅ | |
| processed_at | BIGINT | Option<i64> | ✅ | |
| created_ts | BIGINT NOT NULL | i64 | ✅ | |

**状态**: ✅ 完全匹配

---

## 38. private_sessions 表

### SQL 定义
```sql
CREATE TABLE private_sessions (
    id VARCHAR(255) NOT NULL,
    user_id_1 VARCHAR(255) NOT NULL,
    user_id_2 VARCHAR(255) NOT NULL,
    session_type VARCHAR(50) DEFAULT 'direct',
    encryption_key VARCHAR(255),
    created_ts BIGINT NOT NULL,
    last_activity_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    unread_count INTEGER DEFAULT 0,
    encrypted_content TEXT,
    PRIMARY KEY (id)
);
```

### Rust 定义 (PrivateSession)
```rust
pub struct PrivateSession {
    pub id: String,
    pub user_id_1: String,
    pub user_id_2: String,
    pub session_type: String,
    pub encryption_key: Option<String>,
    pub created_ts: i64,
    pub last_activity_ts: i64,
    pub updated_ts: Option<i64>,
    pub unread_count: i32,
    pub encrypted_content: Option<String>,
}
```

### 字段对比

| 字段名 | SQL 类型 | Rust 类型 | 匹配 | 备注 |
|--------|----------|-----------|------|------|
| id | VARCHAR(255) | String | ✅ | |
| user_id_1 | VARCHAR(255) | String | ✅ | |
| user_id_2 | VARCHAR(255) | String | ✅ | |
| session_type | VARCHAR(50) | String | ✅ | |
| encryption_key | VARCHAR(255) | Option<String> | ✅ | |
| created_ts | BIGINT NOT NULL | i64 | ✅ | |
| last_activity_ts | BIGINT NOT NULL | i64 | ✅ | |
| updated_ts | BIGINT | Option<i64> | ✅ | |
| unread_count | INTEGER | i32 | ✅ | |
| encrypted_content | TEXT | Option<String> | ✅ | |

**状态**: ✅ 完全匹配

---

## 39. private_messages 表

### SQL 定义
```sql
CREATE TABLE private_messages (
    id BIGSERIAL,
    session_id VARCHAR(255) NOT NULL,
    sender_id VARCHAR(255) NOT NULL,
    content TEXT NOT NULL,
    encrypted_content TEXT,
    created_ts BIGINT NOT NULL,
    message_type VARCHAR(50) DEFAULT 'm.text',
    is_read BOOLEAN DEFAULT FALSE,
    read_by_receiver BOOLEAN DEFAULT FALSE,
    read_ts BIGINT,
    edit_history JSONB,
    is_deleted BOOLEAN DEFAULT FALSE,
    deleted_at BIGINT,
    is_edited BOOLEAN DEFAULT FALSE,
    unread_count INTEGER DEFAULT 0,
    PRIMARY KEY (id)
);
```

### Rust 定义 (PrivateMessage)
```rust
pub struct PrivateMessage {
    pub id: i64,
    pub session_id: String,
    pub sender_id: String,
    pub content: String,
    pub encrypted_content: Option<String>,
    pub created_ts: i64,
    pub message_type: String,
    pub is_read: bool,
    pub read_by_receiver: bool,
    pub read_ts: Option<i64>,
    pub edit_history: Option<serde_json::Value>,
    pub is_deleted: bool,
    pub deleted_at: Option<i64>,
    pub is_edited: bool,
    pub unread_count: i32,
}
```

### 字段对比

| 字段名 | SQL 类型 | Rust 类型 | 匹配 | 备注 |
|--------|----------|-----------|------|------|
| id | BIGSERIAL | i64 | ✅ | |
| session_id | VARCHAR(255) | String | ✅ | |
| sender_id | VARCHAR(255) | String | ✅ | |
| content | TEXT NOT NULL | String | ✅ | |
| encrypted_content | TEXT | Option<String> | ✅ | |
| created_ts | BIGINT NOT NULL | i64 | ✅ | |
| message_type | VARCHAR(50) | String | ✅ | |
| is_read | BOOLEAN | bool | ✅ | |
| read_by_receiver | BOOLEAN | bool | ✅ | |
| read_ts | BIGINT | Option<i64> | ✅ | |
| edit_history | JSONB | Option<serde_json::Value> | ✅ | |
| is_deleted | BOOLEAN | bool | ✅ | |
| deleted_at | BIGINT | Option<i64> | ✅ | |
| is_edited | BOOLEAN | bool | ✅ | |
| unread_count | INTEGER | i32 | ✅ | |

**状态**: ✅ 完全匹配

---

## 40. presence 表

### SQL 定义
```sql
CREATE TABLE presence (
    user_id TEXT NOT NULL,
    status_msg TEXT,
    presence TEXT NOT NULL DEFAULT 'offline',
    last_active_ts BIGINT NOT NULL DEFAULT 0,
    status_from TEXT,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    PRIMARY KEY (user_id)
);
```

### Rust 定义 (Presence)
```rust
pub struct Presence {
    pub user_id: String,
    pub status_msg: Option<String>,
    pub presence: String,
    pub last_active_ts: i64,
    pub status_from: Option<String>,
    pub created_ts: i64,
    pub updated_ts: i64,
}
```

### 字段对比

| 字段名 | SQL 类型 | Rust 类型 | 匹配 | 备注 |
|--------|----------|-----------|------|------|
| user_id | TEXT NOT NULL | String | ✅ | |
| status_msg | TEXT | Option<String> | ✅ | |
| presence | TEXT | String | ✅ | |
| last_active_ts | BIGINT | i64 | ✅ | |
| status_from | TEXT | Option<String> | ✅ | |
| created_ts | BIGINT NOT NULL | i64 | ✅ | |
| updated_ts | BIGINT NOT NULL | i64 | ✅ | |

**状态**: ✅ 完全匹配

---

## 41. friends 表

### SQL 定义
```sql
CREATE TABLE friends (
    id BIGSERIAL,
    user_id TEXT NOT NULL,
    friend_id TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    PRIMARY KEY (id)
);
```

### Rust 定义 (Friend)
```rust
pub struct Friend {
    pub id: i64,
    pub user_id: String,
    pub friend_id: String,
    pub created_ts: i64,
}
```

### 字段对比

| 字段名 | SQL 类型 | Rust 类型 | 匹配 | 备注 |
|--------|----------|-----------|------|------|
| id | BIGSERIAL | i64 | ✅ | |
| user_id | TEXT NOT NULL | String | ✅ | |
| friend_id | TEXT NOT NULL | String | ✅ | |
| created_ts | BIGINT NOT NULL | i64 | ✅ | |

**状态**: ✅ 完全匹配

---

## 42. friend_requests 表

### SQL 定义
```sql
CREATE TABLE friend_requests (
    id BIGSERIAL,
    sender_id TEXT NOT NULL,
    receiver_id TEXT NOT NULL,
    message TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    PRIMARY KEY (id)
);
```

### Rust 定义 (FriendRequest)
```rust
pub struct FriendRequest {
    pub id: i64,
    pub sender_id: String,
    pub receiver_id: String,
    pub message: Option<String>,
    pub status: String,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
}
```

### 字段对比

| 字段名 | SQL 类型 | Rust 类型 | 匹配 | 备注 |
|--------|----------|-----------|------|------|
| id | BIGSERIAL | i64 | ✅ | |
| sender_id | TEXT NOT NULL | String | ✅ | |
| receiver_id | TEXT NOT NULL | String | ✅ | |
| message | TEXT | Option<String> | ✅ | |
| status | TEXT | String | ✅ | |
| created_ts | BIGINT NOT NULL | i64 | ✅ | |
| updated_ts | BIGINT | Option<i64> | ✅ | |

**状态**: ✅ 完全匹配

---

## 问题汇总

### P0 - 关键问题 (会导致编译错误或运行时崩溃)

| # | 表名 | 问题 | 建议修复 |
|---|------|------|----------|
| 1 | olm_sessions | 表没有对应的 Rust 模型定义 | 创建 `OlmSession` 结构体 |
| 2 | olm_accounts | 表没有对应的 Rust 模型定义 | 创建 `OlmAccount` 结构体 |

### P1 - 高优先级问题 (数据丢失或功能异常)

| # | 表名 | 问题 | 建议修复 |
|---|------|------|----------|
| 1 | user_threepids | `validated_ts` 应为 `validated_at` | 重命名为 `validated_at` |
| 2 | user_threepids | `verification_expires_ts` 应为 `verification_expires_at` | 重命名为 `verification_expires_at` |
| 3 | users | `must_change_password` 字段在 SQL 存在但 Rust 缺失 | 添加 `pub must_change_password: bool` |
| 4 | device_keys | `ts_updated_ms` 字段在 SQL 存在但 Rust 缺失 | 添加 `pub ts_updated_ms: Option<i64>` |
| 5 | key_backups | `mgmt_key` 字段在 SQL 存在但 Rust 缺失 | 添加 `pub mgmt_key: Option<String>` |
| 6 | room_summaries | `member_count` 字段在 SQL 存在但 Rust 缺失 | 添加 `pub member_count: i64` |

### P2 - 中优先级问题 (潜在问题或命名不一致)

| # | 表名 | 问题 | 建议修复 |
|---|------|------|----------|
| 1 | pushers | SQL 用 `updated_ts`，Rust 用 `last_updated_ts` | 统一命名为 `updated_ts` |
| 2 | openid_tokens | SQL 用 `expires_ts`，Rust 用 `expires_at` | 统一命名为 `expires_ts` |
| 3 | refresh_token_families | SQL 用 `compromised_at`，Rust 用 `compromised_ts` | 统一命名为 `compromised_at` |
| 4 | key_backups | SQL 主键为 `backup_id`，Rust 模型用 `id` | 考虑统一命名 |

---

## 命名规范一致性分析

### 时间戳字段命名问题

根据项目规范 `DATABASE_FIELD_STANDARDS.md`:
- `_ts` 后缀: 用于 NOT NULL 的毫秒级时间戳（创建/更新/活跃时间）
- `_at` 后缀: 用于可空的时间戳（过期/撤销/验证等可选操作）

**违反规范的情况:**

| 当前名称 | 规范名称 | 原因 |
|----------|----------|------|
| `validated_ts` (user_threepids) | `validated_at` | 可空验证时间 |
| `verification_expires_ts` (user_threepids) | `verification_expires_at` | 可空过期时间 |

---

## 缺失的 Rust 模型

以下 SQL 表没有对应的 Rust 模型定义:

| # | 表名 | 严重程度 |
|---|------|----------|
| 1 | olm_sessions | P0 |
| 2 | olm_accounts | P0 |
| 3 | cross_signing_keys | P2 |
| 4 | one_time_keys | P2 |
| 5 | e2ee_key_requests | P2 |
| 6 | room_invites | P2 |
| 7 | read_markers | P2 |
| 8 | room_tags | P2 |
| 9 | room_state_events | P2 |
| 10 | filters | P2 |
| 11 | account_data | P2 |
| 12 | room_account_data | P2 |
| 13 | user_account_data | P2 |
| 14 | background_updates | P2 |
| 15 | workers | P2 |
| 16 | worker_commands | P2 |
| 17 | worker_events | P2 |
| 18 | worker_statistics | P2 |
| 19 | sync_stream_id | P2 |
| 20 | modules | P2 |
| 21 | module_execution_logs | P2 |
| 22 | spam_check_results | P2 |
| 23 | third_party_rule_results | P2 |
| 24 | account_validity | P2 |
| 25 | password_auth_providers | P2 |
| 26 | presence_routes | P2 |
| 27 | media_callbacks | P2 |
| 28 | rate_limit_callbacks | P2 |
| 29 | account_data_callbacks | P2 |
| 30 | registration_tokens | P2 |
| 31 | registration_token_usage | P2 |
| 32 | event_report_history | P2 |
| 33 | report_rate_limits | P2 |
| 34 | event_report_stats | P2 |
| 35 | push_notification_queue | P2 |
| 36 | push_notification_log | P2 |
| 37 | push_config | P2 |
| 38 | key_rotation_history | P2 |
| 39 | blocked_rooms | P2 |
| 40 | security_events | P2 |
| 41 | ip_blocks | P2 |
| 42 | ip_reputation | P2 |
| 43 | delayed_events | P2 |
| 44 | to_device_messages | P2 |
| 45 | device_lists_changes | P2 |
| 46 | device_lists_stream | P2 |
| 47 | sliding_sync_rooms | P2 |
| 48 | thread_subscriptions | P2 |
| 49 | space_hierarchy | P2 |
| 50 | password_history | P2 |
| 51 | password_policy | P2 |
| 52 | schema_migrations | P2 |
| 53 | db_metadata | P2 |
| 54 | server_retention_policy | P2 |
| 55 | user_media_quota | P2 |
| 56 | media_quota_config | P2 |
| 57 | rendezvous_session | P2 |
| 58 | application_service_state | P2 |
| 59 | application_service_transactions | P2 |
| 60 | application_service_events | P2 |
| 61 | application_service_user_namespaces | P2 |
| 62 | application_service_room_alias_namespaces | P2 |
| 63 | application_service_room_namespaces | P2 |
| 64 | user_privacy_settings | P2 |
| 65 | search_index | P2 |
| 66 | room_ephemeral | P2 |
| 67 | user_filters | P2 |
| 68 | cas_tickets | P2 |
| 69 | cas_proxy_tickets | P2 |
| 70 | cas_proxy_granting_tickets | P2 |
| 71 | cas_services | P2 |
| 72 | cas_user_attributes | P2 |
| 73 | cas_slo_sessions | P2 |
| 74 | saml_sessions | P2 |
| 75 | saml_user_mapping | P2 |
| 76 | saml_identity_providers | P2 |
| 77 | saml_auth_events | P2 |
| 78 | saml_logout_requests | P2 |
| 79 | registration_captcha | P2 |
| 80 | captcha_send_log | P2 |
| 81 | captcha_template | P2 |
| 82 | captcha_config | P2 |
| 83 | room_directory | P2 |
| 84 | thread_statistics (废弃) | P2 |

---

## 修复优先级建议

### 第一阶段: P0 修复 (立即)

1. 创建 `OlmSession` Rust 模型
2. 创建 `OlmAccount` Rust 模型

### 第二阶段: P1 修复 (本周)

1. 修复 `UserThreepid` 字段命名
2. 添加 `users.must_change_password` 到 Rust 模型
3. 添加 `device_keys.ts_updated_ms` 到 Rust 模型
4. 添加 `key_backups.mgmt_key` 到 Rust 模型
5. 添加 `room_summaries.member_count` 到 Rust 模型

### 第三阶段: P2 修复 (下个迭代)

1. 统一 `pusher.updated_ts` / `last_updated_ts` 命名
2. 统一 `openid_tokens.expires_ts` / `expires_at` 命名
3. 统一 `refresh_token_families.compromised_at` / `compromised_ts` 命名
4. 为缺失的表创建 Rust 模型

---

## 附录: SQL 与 Rust 类型映射参考

| PostgreSQL 类型 | Rust 类型 |
|-----------------|-----------|
| BIGSERIAL | i64 |
| BIGINT | i64 |
| INTEGER | i32 |
| TEXT | String |
| TEXT NOT NULL | String |
| BOOLEAN | bool |
| JSONB | serde_json::Value |
| UUID | uuid::Uuid |
| VARCHAR(n) | String |
| DATE | chrono::NaiveDate |
| TIMESTAMP | chrono::DateTime<chrono::Utc> |
| TEXT[] | Vec<String> |
