# Synapse Rust 数据库字段使用规范

## 1. 命名规范

### 1.1 通用命名规则

| 规则 | 说明 | 示例 |
|------|------|------|
| 使用snake_case | 所有字段名使用小写字母和下划线 | `user_id`, `created_ts` |
| 避免缩写 | 除非是广泛认知的缩写 | `access_token` 而非 `acc_tok` |
| 布尔字段使用is_/has_前缀 | 明确表示布尔类型 | `is_revoked`, `is_admin` |
| 时间戳字段使用_ts后缀 | 毫秒级时间戳 | `created_ts`, `expires_ts` |
| 可选时间戳使用_at后缀 | 可为空的时间戳 | `expires_at`, `revoked_at` |

### 1.2 时间字段规范

| 字段类型 | 后缀 | 数据类型 | 说明 |
|----------|------|----------|------|
| 创建时间 | `created_ts` | BIGINT | 毫秒级时间戳，NOT NULL |
| 更新时间 | `updated_ts` | BIGINT | 毫秒级时间戳，可为NULL |
| 过期时间 | `expires_at` | BIGINT | 毫秒级时间戳，可为NULL |
| 撤销时间 | `revoked_ts` | BIGINT | 毫秒级时间戳，可为NULL |
| 最后使用时间 | `last_used_ts` | BIGINT | 毫秒级时间戳，可为NULL |

### 1.3 禁止使用的冗余字段

| 禁止字段 | 替代字段 | 原因 |
|----------|----------|------|
| `invalidated` | `is_revoked` | 语义重复 |
| `invalidated_ts` | `revoked_ts` | 命名不一致 |
| `expires_ts` (在refresh_tokens中) | `expires_at` | 统一使用`expires_at`表示可选过期时间 |

## 2. 核心表字段规范

### 2.1 users 表

```sql
CREATE TABLE users (
    user_id VARCHAR(255) PRIMARY KEY,        -- 用户唯一标识，格式: @username:server
    username VARCHAR(255) UNIQUE NOT NULL,   -- 用户名
    password_hash TEXT NOT NULL,             -- 密码哈希
    displayname VARCHAR(255),                -- 显示名称
    avatar_url TEXT,                         -- 头像URL
    is_admin BOOLEAN DEFAULT FALSE,          -- 是否管理员
    is_guest BOOLEAN DEFAULT FALSE,          -- 是否访客
    user_type VARCHAR(50),                   -- 用户类型
    deactivated BOOLEAN DEFAULT FALSE,       -- 是否停用
    creation_ts BIGINT NOT NULL,             -- 创建时间戳
    updated_ts BIGINT,                       -- 更新时间戳
    generation BIGINT DEFAULT 1              -- 代数
);
```

### 2.2 devices 表

```sql
CREATE TABLE devices (
    device_id VARCHAR(255) PRIMARY KEY,      -- 设备唯一标识
    user_id VARCHAR(255) NOT NULL,           -- 用户ID
    display_name VARCHAR(255),               -- 设备显示名称
    device_key JSONB,                        -- 设备密钥信息
    last_seen_ts BIGINT,                     -- 最后活跃时间戳（可为空）
    last_seen_ip VARCHAR(45),                -- 最后活跃IP地址
    created_ts BIGINT NOT NULL,              -- 创建时间戳（不为空）
    first_seen_ts BIGINT NOT NULL,           -- 首次出现时间戳（不为空）
    appservice_id VARCHAR(255),              -- 应用服务ID
    ignored_user_list TEXT                   -- 忽略用户列表
);

CREATE INDEX idx_devices_user ON devices(user_id);
CREATE INDEX idx_devices_last_seen ON devices(last_seen_ts DESC);
```

**重要说明**：
- `created_ts` 和 `first_seen_ts` 必须存在且不为空
- **禁止使用** `created_at` 字段，统一使用 `created_ts`
- `last_seen_ts` 可为空，表示设备从未活跃过

### 2.3 access_tokens 表

```sql
CREATE TABLE access_tokens (
    id BIGSERIAL PRIMARY KEY,
    token TEXT UNIQUE NOT NULL,              -- 访问令牌
    user_id VARCHAR(255) NOT NULL,           -- 用户ID
    device_id VARCHAR(255),                  -- 设备ID
    created_ts BIGINT NOT NULL,              -- 创建时间戳
    expires_ts BIGINT NOT NULL,              -- 过期时间戳
    last_used_ts BIGINT,                     -- 最后使用时间戳
    user_agent TEXT,                         -- 用户代理
    ip_address VARCHAR(45),                  -- IP地址
    is_valid BOOLEAN DEFAULT TRUE,           -- 是否有效
    invalidated_ts BIGINT                    -- 失效时间戳
);
```

### 2.3 refresh_tokens 表

```sql
CREATE TABLE refresh_tokens (
    id BIGSERIAL PRIMARY KEY,
    token_hash VARCHAR(255) UNIQUE NOT NULL, -- 令牌哈希（SHA256）
    user_id VARCHAR(255) NOT NULL,           -- 用户ID
    device_id VARCHAR(255),                  -- 设备ID
    access_token_id VARCHAR(255),            -- 关联的访问令牌ID
    scope VARCHAR(255),                      -- 权限范围
    created_ts BIGINT NOT NULL,              -- 创建时间戳
    expires_at BIGINT,                       -- 过期时间戳（可为空）
    last_used_ts BIGINT,                     -- 最后使用时间戳
    use_count INTEGER DEFAULT 0,             -- 使用次数
    is_revoked BOOLEAN DEFAULT FALSE,        -- 是否已撤销
    revoked_ts BIGINT,                       -- 撤销时间戳
    revoked_reason TEXT,                     -- 撤销原因
    client_info JSONB,                       -- 客户端信息
    ip_address VARCHAR(45),                  -- IP地址
    user_agent TEXT                          -- 用户代理
);
```

## 3. Rust 代码规范

### 3.1 结构体字段定义

```rust
// 正确示例
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RefreshToken {
    pub id: i64,
    pub token_hash: String,
    pub user_id: String,
    pub device_id: Option<String>,
    pub expires_at: Option<i64>,  // 可为空的时间戳
    pub created_ts: i64,          // 不为空的时间戳
    pub is_revoked: bool,         // 布尔字段使用is_前缀
}

// 错误示例
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RefreshToken {
    pub invalidated: bool,        // 错误：应使用is_revoked
    pub expires_ts: i64,          // 错误：应为Option<i64>类型
    pub invalidated_ts: Option<i64>, // 错误：应使用revoked_ts
}
```

### 3.2 SQL查询规范

```rust
// 正确示例：使用标准字段名
sqlx::query_as::<_, RefreshToken>(
    r#"
    SELECT id, token_hash, user_id, device_id, created_ts, expires_at, is_revoked
    FROM refresh_tokens WHERE token_hash = $1
    "#
)

// 错误示例：使用非标准字段名
sqlx::query_as::<_, RefreshToken>(
    r#"
    SELECT id, token, user_id, device_id, created_ts, expires_ts, invalidated
    FROM refresh_tokens WHERE token = $1
    "#
)
```

### 3.3 时间戳处理规范

```rust
// 创建时间戳
let now = chrono::Utc::now().timestamp_millis();

// 检查过期时间
if let Some(expires_at) = token.expires_at {
    if expires_at < chrono::Utc::now().timestamp_millis() {
        return Err(ApiError::unauthorized("Token expired"));
    }
}
```

## 4. 常见错误案例

### 4.1 字段名称不一致

| 错误 | 正确 | 说明 |
|------|------|------|
| `invalidated` | `is_revoked` | 布尔字段应使用is_前缀 |
| `invalidated_ts` | `revoked_ts` | 撤销时间应使用revoked_ts |
| `expires_ts` (refresh_tokens) | `expires_at` | 可选过期时间应使用expires_at |

### 4.2 数据类型不匹配

| 错误 | 正确 | 说明 |
|------|------|------|
| `expires_at: i64` | `expires_at: Option<i64>` | 可为空的字段应使用Option |
| `is_revoked: Option<bool>` | `is_revoked: bool` | 有默认值的布尔字段不需要Option |

### 4.3 重复结构体定义

禁止在不同文件中定义相同功能的结构体。例如：
- 禁止在`storage/token.rs`和`storage/refresh_token.rs`中同时定义`RefreshToken`
- 统一使用`storage/refresh_token.rs`中的定义

## 5. 代码审查要点

### 5.1 新增字段检查清单

- [ ] 字段名是否符合snake_case规范
- [ ] 布尔字段是否使用is_/has_前缀
- [ ] 时间戳字段是否使用正确的后缀（_ts或_at）
- [ ] 可为空的字段是否使用Option类型
- [ ] 是否存在重复或冗余字段

### 5.2 SQL查询检查清单

- [ ] SELECT语句中的字段名是否与结构体匹配
- [ ] INSERT语句中的字段名是否正确
- [ ] UPDATE语句中的字段名是否正确
- [ ] WHERE条件中的字段名是否正确

## 6. 版本历史

| 版本 | 日期 | 变更说明 |
|------|------|----------|
| 1.0.0 | 2026-02-19 | 初始版本，统一字段命名规范 |
| 1.1.0 | 2026-02-20 | 新增 devices 表规范，明确禁止使用 created_at 字段 |
| 1.2.0 | 2026-02-22 | 新增 pushers, cross_signing_keys, device_keys, device_signatures, push_rule, push_device, push_notification_queue 表规范 |

## 7. 常见问题修复

### 7.1 devices 表 created_at 字段问题

**问题描述**：代码中使用 `created_at` 字段，但数据库 schema 定义的是 `created_ts`。

**解决方案**：
1. 运行迁移脚本 `20260220000003_fix_devices_table.sql`
2. 确保代码中使用 `created_ts` 而非 `created_at`

**代码修复示例**：
```rust
// 错误
pub struct Device {
    pub created_at: i64,  // 错误：数据库中没有此字段
}

// 正确
pub struct Device {
    pub created_ts: i64,  // 正确：与数据库字段匹配
}
```

## 8. 扩展表字段规范

### 8.1 pushers 表

```sql
CREATE TABLE pushers (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255),
    pushkey VARCHAR(255) NOT NULL,
    kind VARCHAR(50),
    app_id VARCHAR(255),
    app_display_name VARCHAR(255),
    device_display_name VARCHAR(255),
    profile_tag VARCHAR(255),
    lang VARCHAR(10),
    data JSONB,
    is_enabled BOOLEAN DEFAULT TRUE,
    created_ts BIGINT NOT NULL,           -- 使用 created_ts，禁止 created_at
    last_updated_ts BIGINT,               -- 使用 last_updated_ts
    last_success_ts BIGINT,
    last_failure_ts BIGINT,
    failure_count INTEGER DEFAULT 0
);

CREATE UNIQUE INDEX idx_pushers_user_pushkey ON pushers(user_id, pushkey);
CREATE INDEX idx_pushers_user ON pushers(user_id);
```

**重要说明**：
- **禁止使用** `created_at` 字段，统一使用 `created_ts`
- **禁止使用** `updated_at` 字段，统一使用 `last_updated_ts`

### 8.2 cross_signing_keys 表

```sql
CREATE TABLE cross_signing_keys (
    user_id VARCHAR(255) NOT NULL,
    key_type VARCHAR(50) NOT NULL,
    key_data TEXT NOT NULL,
    added_ts BIGINT NOT NULL,             -- 使用 added_ts，禁止 created_at/updated_at
    PRIMARY KEY (user_id, key_type)
);

CREATE INDEX idx_cross_signing_keys_user ON cross_signing_keys(user_id);
```

**重要说明**：
- **禁止使用** `created_at`、`updated_at` 字段
- 使用 `added_ts` 表示密钥添加时间

### 8.3 device_keys 表

```sql
CREATE TABLE device_keys (
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255) NOT NULL,
    algorithm VARCHAR(255) NOT NULL,
    key_data TEXT NOT NULL,
    added_ts BIGINT NOT NULL,             -- 使用 added_ts，禁止 created_at
    last_seen_ts BIGINT,
    is_verified BOOLEAN DEFAULT FALSE,
    ts_updated_ms BIGINT,                 -- 使用 ts_updated_ms，禁止 updated_at
    PRIMARY KEY (user_id, device_id, algorithm)
);

CREATE INDEX idx_device_keys_user ON device_keys(user_id);
CREATE INDEX idx_device_keys_device ON device_keys(device_id);
```

**重要说明**：
- **禁止使用** `created_at` 字段，统一使用 `added_ts`
- **禁止使用** `updated_at` 字段，统一使用 `ts_updated_ms`

### 8.4 device_signatures 表

```sql
CREATE TABLE device_signatures (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255) NOT NULL,
    target_user_id VARCHAR(255) NOT NULL,
    target_device_id VARCHAR(255) NOT NULL,
    algorithm VARCHAR(255) NOT NULL,
    signature TEXT NOT NULL,
    created_ts BIGINT NOT NULL,           -- 使用 created_ts，禁止 created_at
    UNIQUE (user_id, device_id, target_user_id, target_device_id, algorithm)
);

CREATE INDEX idx_device_signatures_user ON device_signatures(user_id);
CREATE INDEX idx_device_signatures_target ON device_signatures(target_user_id, target_device_id);
```

**重要说明**：
- **禁止使用** `created_at` 字段，统一使用 `created_ts`
- **禁止使用** `signing_key_id`、`target_key_id` 字段，使用 `algorithm` 字段

### 8.5 push_rule 表

```sql
CREATE TABLE push_rule (
    id SERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    rule_id VARCHAR(255) NOT NULL,
    scope VARCHAR(50) NOT NULL DEFAULT 'global',
    kind VARCHAR(50) NOT NULL,
    priority INTEGER DEFAULT 0,
    conditions JSONB DEFAULT '[]',
    actions JSONB DEFAULT '[]',
    is_enabled BOOLEAN DEFAULT TRUE,      -- 使用 is_enabled，禁止 enabled
    is_default BOOLEAN DEFAULT FALSE,
    created_at BIGINT,                    -- 注意：此表使用 created_at（历史原因）
    updated_at BIGINT,
    UNIQUE (user_id, scope, kind, rule_id)
);

CREATE INDEX idx_push_rule_user ON push_rule(user_id);
```

**重要说明**：
- 此表使用 `is_enabled` 而非 `enabled`
- 代码中需要使用 `#[sqlx(rename = "is_enabled")]` 映射

### 8.6 push_device 表

```sql
CREATE TABLE push_device (
    id SERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255) NOT NULL,
    push_token TEXT NOT NULL,
    push_type VARCHAR(20) NOT NULL,
    app_id VARCHAR(255),
    platform VARCHAR(50),
    platform_version VARCHAR(50),
    app_version VARCHAR(50),
    locale VARCHAR(20),
    timezone VARCHAR(50),
    is_enabled BOOLEAN DEFAULT TRUE,      -- 使用 is_enabled，禁止 enabled
    created_at BIGINT,
    updated_at BIGINT,
    last_used_at TIMESTAMP WITH TIME ZONE,
    last_error TEXT,
    error_count INTEGER DEFAULT 0,
    metadata JSONB DEFAULT '{}',
    UNIQUE (user_id, device_id)
);

CREATE INDEX idx_push_device_user ON push_device(user_id);
```

**重要说明**：
- 此表使用 `is_enabled` 而非 `enabled`
- `last_used_at` 使用 TIMESTAMP 类型（非 BIGINT）

### 8.7 push_notification_queue 表

```sql
CREATE TABLE push_notification_queue (
    id SERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255) NOT NULL,
    event_id VARCHAR(255),
    room_id VARCHAR(255),
    notification_type VARCHAR(50),
    content JSONB NOT NULL,
    priority INTEGER DEFAULT 5,
    status VARCHAR(20) DEFAULT 'pending',
    attempts INTEGER DEFAULT 0,
    max_attempts INTEGER DEFAULT 5,
    next_attempt_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    created_at BIGINT,
    sent_at TIMESTAMP WITH TIME ZONE,
    error_message TEXT
);

CREATE INDEX idx_push_notification_queue_status ON push_notification_queue(status);
CREATE INDEX idx_push_notification_queue_next_attempt ON push_notification_queue(next_attempt_at);
```

**重要说明**：
- `created_at` 使用 BIGINT 类型
- `next_attempt_at`、`sent_at` 使用 TIMESTAMP 类型

## 9. 字段映射速查表

### 9.1 时间戳字段映射

| 数据库字段 | Rust 类型 | 说明 |
|------------|-----------|------|
| `created_ts` (NOT NULL) | `i64` | 毫秒时间戳 |
| `created_ts` (NULLABLE) | `Option<i64>` | 可空毫秒时间戳 |
| `created_at` (BIGINT) | `i64` | 毫秒时间戳 |
| `added_ts` | `i64` | 毫秒时间戳 |
| `updated_ts` | `Option<i64>` | 可空毫秒时间戳 |
| `ts_updated_ms` | `Option<i64>` | 可空毫秒时间戳 |
| `last_used_at` (TIMESTAMP) | `Option<DateTime<Utc>>` | 时区时间戳 |
| `next_attempt_at` (TIMESTAMP) | `DateTime<Utc>` | 时区时间戳 |

### 9.2 布尔字段映射

| 数据库字段 | Rust 字段名 | 映射方式 |
|------------|-------------|----------|
| `is_enabled` | `enabled` | `#[sqlx(rename = "is_enabled")]` |
| `is_admin` | `is_admin` | 直接映射 |
| `is_revoked` | `is_revoked` | 直接映射 |
| `is_default` | `is_default` | 直接映射 |
| `enabled` (错误) | - | 禁止使用，改为 `is_enabled` |
