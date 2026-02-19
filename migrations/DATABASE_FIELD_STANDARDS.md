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

### 2.2 access_tokens 表

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
