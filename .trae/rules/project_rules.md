# synapse-rust 项目规则

> 本规则基于项目实际实现，结合 PostgreSQL 最佳实践、数据模型规范和字段标准制定。

---

## 一、项目概述

synapse-rust 是一个使用 Rust 编写的 Matrix homeserver 实现，兼容 Synapse (Python) API，提供高性能、安全的即时通讯服务。

### 核心技术栈

| 技术 | 用途 | 版本/特性 |
|------|------|----------|
| Rust | 主要编程语言 | Edition 2021, async/await | rustc 1.93.0
| Axum | Web框架 | Tower中间件, WebSocket支持 |
| PostgreSQL | 主数据库 | sqlx, 连接池监控 |
| Redis | 缓存层 | 连接池, Token缓存 |
| JWT | 认证机制 | HS256签名 |
| Argon2 | 密码哈希 | 可配置成本参数 |

---

## 二、数据库字段命名规范

### 2.1 通用命名规则

| 规则 | 说明 | 示例 |
|------|------|------|
| 使用 snake_case | 所有字段名使用小写字母和下划线 | `user_id`, `created_ts` |
| 避免缩写 | 除非是广泛认知的缩写 | `access_token` 而非 `acc_tok` |
| 布尔字段使用 is_/has_ 前缀 | 明确表示布尔类型 | `is_revoked`, `is_admin`, `is_enabled` |
| 时间戳字段使用 _ts 后缀 | 毫秒级时间戳 | `created_ts`, `expires_ts` |
| 可选时间戳使用 _at 后缀 | 可为空的时间戳 | `expires_at`, `revoked_at` |

### 2.2 时间字段规范

| 字段类型 | 后缀 | 数据类型 | 说明 |
|----------|------|----------|------|
| 创建时间 | `created_ts` | BIGINT | 毫秒级时间戳，NOT NULL |
| 更新时间 | `updated_ts` | BIGINT | 毫秒级时间戳，可为NULL |
| 过期时间 | `expires_at` | BIGINT | 毫秒级时间戳，可为NULL |
| 撤销时间 | `revoked_ts` | BIGINT | 毫秒级时间戳，可为NULL |
| 最后使用时间 | `last_used_ts` | BIGINT | 毫秒级时间戳，可为NULL |
| 添加时间 | `added_ts` | BIGINT | 毫秒级时间戳，NOT NULL |

### 2.3 禁止使用的冗余字段

| 禁止字段 | 替代字段 | 原因 |
|----------|----------|------|
| `invalidated` | `is_revoked` | 语义重复 |
| `invalidated_ts` | `revoked_ts` | 命名不一致 |
| `created_at` | `created_ts` | 统一使用 _ts 后缀 |
| `updated_at` | `updated_ts` | 统一使用 _ts 后缀 |
| `enabled` | `is_enabled` | 布尔字段应使用 is_ 前缀 |

---

## 三、数据类型映射规范

### 3.1 PostgreSQL 与 Rust 类型映射

| PostgreSQL 类型 | Rust 类型 | 说明 |
|-----------------|-----------|------|
| BIGINT (NOT NULL) | `i64` | 毫秒时间戳、ID |
| BIGINT (NULLABLE) | `Option<i64>` | 可空时间戳 |
| BIGSERIAL | `i64` | 自增主键 |
| TEXT (NOT NULL) | `String` | 字符串 |
| TEXT (NULLABLE) | `Option<String>` | 可空字符串 |
| BOOLEAN (NOT NULL) | `bool` | 布尔值 |
| BOOLEAN (NULLABLE) | `Option<bool>` | 可空布尔值 |
| JSONB | `serde_json::Value` | JSON数据 |
| TIMESTAMPTZ | `DateTime<Utc>` | 时区时间戳 |

### 3.2 主键类型选择

| 场景 | PostgreSQL 类型 | Rust 类型 |
|------|-----------------|-----------|
| 自增主键 | `BIGSERIAL` | `i64` |
| UUID主键 | `UUID` | `uuid::Uuid` |
| 业务主键 | `TEXT PRIMARY KEY` | `String` |

---

## 四、Schema 设计原则

### 4.1 标准 Schema 模板

```sql
CREATE TABLE example (
    id BIGSERIAL PRIMARY KEY,
    name TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    metadata JSONB DEFAULT '{}'
);

CREATE UNIQUE INDEX idx_example_name ON example(name) 
WHERE is_active = TRUE;

CREATE INDEX idx_example_active_created ON example(is_active, created_ts DESC);
```

### 4.2 索引设计原则

1. **主键索引**：自动创建，使用 BIGSERIAL 或 TEXT PRIMARY KEY
2. **唯一索引**：用于唯一约束字段
3. **部分索引**：减少存储空间，如 `WHERE is_active = TRUE`
4. **复合索引**：按查询频率排序字段

```sql
CREATE INDEX idx_room_memberships_room_membership_joined 
ON room_memberships(room_id, membership, joined_ts DESC);

CREATE INDEX idx_room_events_not_redacted 
ON room_events(room_id, origin_server_ts DESC) 
WHERE redacted = FALSE;
```

### 4.3 外键约束

```sql
FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
```

---

## 五、Rust 代码规范

### 5.1 结构体定义

```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RefreshToken {
    pub id: i64,
    pub token_hash: String,
    pub user_id: String,
    pub device_id: Option<String>,
    pub created_ts: i64,
    pub expires_at: Option<i64>,
    pub is_revoked: bool,
}
```

### 5.2 SQL 查询规范

```rust
sqlx::query_as::<_, User>(
    r#"
    SELECT user_id, username, is_admin, created_ts
    FROM users WHERE user_id = $1
    "#
)
.bind(user_id)
.fetch_one(&pool)
.await?;
```

### 5.3 时间戳处理

```rust
let now = chrono::Utc::now().timestamp_millis();

if let Some(expires_at) = token.expires_at {
    if expires_at < chrono::Utc::now().timestamp_millis() {
        return Err(ApiError::unauthorized("Token expired"));
    }
}
```

### 5.4 错误处理

```rust
pub enum ApiError {
    BadRequest(String),
    Unauthorized(String),
    Forbidden(String),
    NotFound(String),
    MethodNotAllowed(String),
    RateLimited(String),
    Internal(String),
}

pub async fn create_room(&self, request: CreateRoomRequest) -> ApiResult<Room> {
    let room = self.room_storage
        .create_room(request)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create room: {}", e)))?;
    Ok(room)
}
```

---

## 六、数据库迁移规范

### 6.1 迁移文件命名

```
YYYYMMDDHHMMSS_description.sql
```

### 6.2 安全迁移模板

```sql
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'table_name' AND column_name = 'column_name'
    ) THEN
        ALTER TABLE table_name ADD COLUMN column_name DATA_TYPE DEFAULT default_value;
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_name ON table_name(column_name);
```

### 6.3 回滚脚本

```sql
ALTER TABLE table_name DROP COLUMN IF EXISTS column_name;
DROP INDEX IF EXISTS idx_name;
```

---

## 七、核心表结构参考

### 7.1 用户表 (users)

```sql
CREATE TABLE users (
    user_id TEXT NOT NULL PRIMARY KEY,
    username TEXT NOT NULL UNIQUE,
    password_hash TEXT,
    is_admin BOOLEAN DEFAULT FALSE,
    is_guest BOOLEAN DEFAULT FALSE,
    creation_ts BIGINT NOT NULL,
    deactivated BOOLEAN DEFAULT FALSE,
    displayname TEXT,
    avatar_url TEXT
);
```

### 7.2 设备表 (devices)

```sql
CREATE TABLE devices (
    device_id TEXT NOT NULL PRIMARY KEY,
    user_id TEXT NOT NULL,
    display_name TEXT,
    last_seen_ts BIGINT,
    last_seen_ip TEXT,
    created_ts BIGINT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);
```

### 7.3 访问令牌表 (access_tokens)

```sql
CREATE TABLE access_tokens (
    id BIGSERIAL PRIMARY KEY,
    token TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL,
    device_id TEXT,
    created_ts BIGINT NOT NULL,
    expires_ts BIGINT,
    is_valid BOOLEAN DEFAULT TRUE,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);
```

### 7.4 刷新令牌表 (refresh_tokens)

```sql
CREATE TABLE refresh_tokens (
    id BIGSERIAL PRIMARY KEY,
    token_hash TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL,
    device_id TEXT,
    created_ts BIGINT NOT NULL,
    expires_at BIGINT,
    is_revoked BOOLEAN DEFAULT FALSE,
    revoked_ts BIGINT,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);
```

---

## 八、缓存策略

### 8.1 缓存 TTL 配置

| 缓存类型 | TTL | 说明 |
|----------|-----|------|
| Token 缓存 | 3600秒 | JWT 令牌验证结果 |
| 用户活跃状态 | 60秒 | 在线状态 |
| 房间摘要 | 300秒 | 房间信息 |
| 用户管理员状态 | 3600秒 | is_admin 状态 |

### 8.2 缓存键命名

```rust
let token_key = format!("token:{}", token);
let user_admin_key = format!("user:admin:{}", user_id);
let room_summary_key = format!("room:summary:{}", room_id);
```

---

## 九、安全最佳实践

### 9.1 密码安全

- 使用 Argon2id 进行密码哈希
- 支持从旧版哈希迁移
- 登录失败锁定机制

### 9.2 Token 安全

- JWT 签名验证
- Token 黑名单机制
- Refresh Token 轮换

### 9.3 SQL 注入防护

```rust
sqlx::query_as::<_, User>("SELECT * FROM users WHERE user_id = $1")
    .bind(user_id)
    .fetch_one(&pool)
    .await?;
```

---

## 十、性能优化

### 10.1 数据库索引策略

```sql
CREATE INDEX idx_events_room_time ON events(room_id, origin_server_ts DESC);
CREATE INDEX idx_users_lower_email ON users(LOWER(email));
CREATE INDEX idx_events_content ON events USING GIN(content);
```

### 10.2 查询优化

```sql
EXPLAIN ANALYZE SELECT * FROM events 
WHERE room_id = $1 ORDER BY origin_server_ts DESC LIMIT 100;

CREATE INDEX idx_events_covering ON events(room_id, origin_server_ts DESC) 
INCLUDE (event_id, type, sender);
```

---

## 十一、测试账户

| 角色 | 用户名 | 密码 | 用途 |
|------|--------|------|------|
| 管理员 | admin | Admin@123 | 管理 API 测试 |
| 用户1 | testuser1 | Test@123 | 基础功能测试 |
| 用户2 | testuser2 | Test@123 | 交互测试 |
| 用户3 | testuser3 | Test@123 | 群组测试 |

---

## 十二、常见错误修复

### 12.1 字段名称不一致

| 错误 | 正确 | 说明 |
|------|------|------|
| `invalidated` | `is_revoked` | 布尔字段应使用 is_ 前缀 |
| `created_at` | `created_ts` | 统一使用 _ts 后缀 |
| `enabled` | `is_enabled` | 布尔字段应使用 is_ 前缀 |

### 12.2 数据类型不匹配

| 错误 | 正确 | 说明 |
|------|------|------|
| `expires_at: i64` | `expires_at: Option<i64>` | 可为空的字段应使用 Option |
| `id: i32` | `id: i64` | BIGSERIAL 对应 i64 |

---

## 十三、相关文档

- 数据模型文档: `docs/synapse-rust/data-models.md`
- 字段标准文档: `migrations/DATABASE_FIELD_STANDARDS.md`
- PostgreSQL 指南: `.trae/pg-aiguide/SKILL.md`
- API 测试文档: `/home/tzd/api-test/api-test.md`

---

## 十四、版本历史

| 版本 | 日期 | 变更说明 |
|------|------|----------|
| 1.0.0 | 2026-03-01 | 初始版本，综合项目规范 |
| 1.1.0 | 2026-03-05 | 更新字段标准化规范，统一使用created_ts |
