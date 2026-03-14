# synapse-rust 项目规则

> 版本: v2.0.0
> 更新日期: 2026-03-12
> 本规则基于项目实际实现，结合 PostgreSQL 最佳实践、数据模型规范和字段标准制定。

---

## 一、项目概述

synapse-rust 是一个使用 Rust 编写的 Matrix homeserver 实现，兼容 Synapse (Python) API，提供高性能、安全的即时通讯服务。

### 1.1 项目信息

| 项目 | 信息 |
|------|------|
| 项目名称 | synapse-rust |
| 项目类型 | Matrix 服务器实现 (Rust) |
| 主要语言 | Rust |
| 数据库 | PostgreSQL |
| 当前版本 | v6.0.4 |
| 代码行数 | ~100,000 行 |
| 源文件数 | ~400 个 |

### 1.2 核心技术栈

| 技术 | 用途 | 版本/特性 |
|------|------|----------|
| Rust | 主要编程语言 | Edition 2021, async/await, rustc 1.93.0 |
| Axum | Web框架 | Tower中间件, WebSocket支持 |
| PostgreSQL | 主数据库 | sqlx, 连接池监控 |
| Redis | 缓存层 | 连接池, Token缓存 |
| JWT | 认证机制 | HS256签名 |
| Argon2 | 密码哈希 | 可配置成本参数 |

### 1.3 目录结构

```
synapse-rust/
├── migrations/              # 数据库迁移文件 (约 50 个)
├── src/
│   ├── storage/             # 数据访问层 (~50 个文件)
│   ├── services/            # 业务逻辑层 (~60 个文件)
│   ├── web/routes/          # API 路由 (~40 个文件)
│   ├── common/              # 公共模块
│   ├── e2ee/                # 端到端加密
│   ├── federation/          # 联邦协议
│   ├── worker/              # 工作进程支持
│   └── cache/               # 缓存层
├── tests/                   # 测试文件
│   ├── unit/                # 单元测试 (25 个文件)
│   ├── integration/         # 集成测试 (20 个文件)
│   ├── e2e/                 # E2E 测试 (2 个文件)
│   └── performance/         # 性能测试 (4 个文件)
├── docker/                  # Docker 配置
├── scripts/                 # 脚本文件
└── docs/                    # 文档
```

---

## 二、数据库字段命名规范

### 2.1 通用命名规则

| 规则 | 说明 | 示例 |
|------|------|------|
| 使用 snake_case | 所有字段名使用小写字母和下划线 | `user_id`, `created_ts` |
| 避免缩写 | 除非是广泛认知的缩写 | `access_token` 而非 `acc_tok` |
| 布尔字段使用 is_/has_ 前缀 | 明确表示布尔类型 | `is_revoked`, `is_admin`, `is_enabled` |
| 时间戳字段使用 _ts 后缀 | 毫秒级时间戳，NOT NULL | `created_ts`, `last_used_ts` |
| 可选时间戳使用 _at 后缀 | 可为空的时间戳 | `expires_at`, `revoked_at` |

### 2.2 时间字段规范

| 字段类型 | 后缀 | 数据类型 | 可空性 | 说明 |
|----------|------|----------|--------|------|
| 创建时间 | `created_ts` | BIGINT | NOT NULL | 毫秒级时间戳 |
| 更新时间 | `updated_ts` | BIGINT | NULLABLE | 毫秒级时间戳 |
| 过期时间 | `expires_at` | BIGINT | NULLABLE | 毫秒级时间戳 |
| 撤销时间 | `revoked_at` | BIGINT | NULLABLE | 毫秒级时间戳 |
| 最后使用时间 | `last_used_ts` | BIGINT | NULLABLE | 毫秒级时间戳 |
| 最后活跃时间 | `last_seen_ts` | BIGINT | NULLABLE | 毫秒级时间戳 |
| 添加时间 | `added_ts` | BIGINT | NOT NULL | 毫秒级时间戳 |
| 验证时间 | `validated_at` | BIGINT | NULLABLE | 毫秒级时间戳 |

### 2.3 禁止使用的字段

| 禁止字段 | 替代字段 | 原因 |
|----------|----------|------|
| `invalidated` | `is_revoked` | 语义重复 |
| `invalidated_ts` | `revoked_at` | 命名不一致 |
| `created_at` | `created_ts` | 统一使用 _ts 后缀 |
| `updated_at` | `updated_ts` | 统一使用 _ts 后缀 |
| `expires_ts` | `expires_at` | 可选时间戳使用 _at |
| `revoked_ts` | `revoked_at` | 可选时间戳使用 _at |
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

### 4.3 必需索引

以下表必须创建复合索引以提高查询性能：

| 表名 | 索引字段 | 说明 |
|------|----------|------|
| presence_subscriptions | (user_id, observed_user_id) | 在线状态订阅查询 |
| events | (room_id, origin_server_ts DESC) | 时间范围查询 |
| room_memberships | (user_id, membership) | 成员状态查询 |
| access_tokens | (user_id, is_revoked) | Token 验证查询 |

### 4.4 外键约束

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

## 七、代码架构规范

### 7.1 存储层职责边界

| Storage | 职责 | 禁止操作 |
|---------|------|----------|
| RoomStorage | 房间元数据管理 | 禁止直接访问事件数据 |
| EventStorage | 事件数据管理 | 禁止访问成员关系数据 |
| MemberStorage | 成员关系管理 | 禁止访问房间元数据 |

### 7.2 服务层设计原则

1. **单一职责**：每个服务只负责一个业务领域
2. **避免重复**：功能相似的服务应合并或明确职责边界
3. **依赖注入**：通过构造函数注入依赖

```rust
pub struct SyncService {
    room_storage: Arc<RoomStorage>,
    event_storage: Arc<EventStorage>,
    member_storage: Arc<MemberStorage>,
}
```

### 7.3 服务合并建议

| 现状 | 建议 |
|------|------|
| sync_service + sliding_sync_service | 合并为统一的同步服务 |
| room_service + space_service | 明确职责边界或合并 |
| thread_service + message_service | 合并线程相关功能 |

---

## 八、API 设计规范

### 8.1 路由组织

1. **模块化拆分**：将路由按功能模块拆分到独立文件
2. **路由注册**：使用 `create_xxx_router` 函数组织路由
3. **文档生成**：添加 API 文档生成工具

```rust
pub fn create_room_router() -> Router<AppState> {
    Router::new()
        .route("/create", post(create_room))
        .route("/:room_id", get(get_room))
        .route("/:room_id/join", post(join_room))
}

pub fn create_router() -> Router<AppState> {
    Router::new()
        .nest("/_matrix/client/v3/rooms", create_room_router())
        .nest("/_matrix/client/v3/sync", create_sync_router())
}
```

### 8.2 认证中间件

所有需要认证的路由必须添加 `AuthenticatedUser` 中间件：

```rust
.route("/protected", get(protected_handler))
.route_layer(middleware::from_fn_with_state(state, auth_middleware))
```

### 8.3 错误响应格式

```json
{
  "errcode": "M_FORBIDDEN",
  "error": "You are not allowed to access this resource"
}
```

---

## 九、测试规范

### 9.1 测试类型

| 类型 | 文件位置 | 覆盖目标 |
|------|----------|----------|
| 单元测试 | `tests/unit/` | 核心逻辑、工具函数 |
| 集成测试 | `tests/integration/` | API 端点、数据库操作 |
| E2E 测试 | `tests/e2e/` | 完整用户流程 |
| 性能测试 | `tests/performance/` | 性能基准 |

### 9.2 测试覆盖要求

| 模块 | 单元测试 | 集成测试 | E2E 测试 |
|------|----------|----------|----------|
| 核心业务逻辑 | ✅ 必须 | ✅ 必须 | ⚠️ 可选 |
| API 端点 | ⚠️ 可选 | ✅ 必须 | ✅ 必须 |
| 数据库操作 | ✅ 必须 | ✅ 必须 | ❌ 不需要 |
| E2EE 加密 | ✅ 必须 | ✅ 必须 | ✅ 必须 |
| 联邦协议 | ✅ 必须 | ✅ 必须 | ⚠️ 可选 |

### 9.3 测试数据管理

1. **测试数据库隔离**：使用独立的测试数据库
2. **数据清理**：每个测试后清理测试数据
3. **数据工厂**：使用测试数据工厂生成测试数据

```rust
#[tokio::test]
async fn test_create_room() {
    let db = TestDatabase::new().await;
    let user = db.create_test_user("testuser").await;
    
    let room = create_room(&db, user.user_id).await.unwrap();
    assert_eq!(room.creator, user.user_id);
    
    db.cleanup().await;
}
```

---

## 十、MSC 功能实现规范

### 10.1 已实现 MSC

| MSC | 描述 | 状态 | 备注 |
|------|------|------|------|
| MSC3030 | Timestamp to event | ✅ 已实现 | 完整实现 |
| MSC2776 | Presence list | ✅ 已实现 | 完整实现 |
| MSC2654 | Read markers | ✅ 已实现 | 完整实现 |
| MSC3079 | VoIP call events | ✅ 已实现 | 完整实现 |
| MSC3882 | QR code login | ✅ 已实现 | 完整实现 |
| MSC3886 | Sliding sync | ✅ 已实现 | 完整实现 |
| MSC3983 | Thread | ✅ 已实现 | 完整实现 |
| MSC3245 | Room summary | ✅ 已实现 | 完整实现 |
| MSC4380 | 邀请屏蔽 | ✅ 已实现 | 完整实现 |
| MSC4354 | Sticky Event | ✅ 已实现 | 完整实现 |
| MSC4388 | 二维码登录完善 | ✅ 已实现 | 完整实现 |
| MSC4261 | Widget API | ✅ 已实现 | 完整实现 |

### 10.2 待实现 MSC (低优先级)

| MSC | 描述 | 优先级 | 说明 |
|------|------|--------|------|
| MSC4222 | 扩展同步 | 🟡 中 | 扩展同步功能 |
| MSC3903 | Extensible Events | 🟢 低 | 可扩展事件格式 |

### 10.3 MSC 实现流程

1. **研究规范**：详细阅读 MSC 规范文档
2. **设计数据模型**：设计数据库表结构
3. **实现存储层**：实现数据访问接口
4. **实现服务层**：实现业务逻辑
5. **实现 API**：实现 HTTP API 端点
6. **编写测试**：编写单元测试和集成测试
7. **更新文档**：更新 API 文档和项目规则

---

## 十一、安全与合规规范

### 11.1 已实现安全功能

| 功能 | 状态 | 说明 |
|------|------|------|
| 私聊加密 | ✅ | E2EE 加密基础功能 |
| Token 管理 | ✅ | Access/Refresh Token 机制 |
| 密码策略 | ✅ | Argon2 哈希，密码策略配置 |
| 管理员注册 | ✅ | HMAC-SHA256 签名验证 |
| Rate Limiting | ✅ | 请求频率限制 |
| IP Blocking | ✅ | IP 黑名单功能 |

### 11.2 待完善安全功能

| 功能 | 状态 | 问题 |
|------|------|------|
| CAPTCHA | ⚠️ 基础实现 | 缺少多种验证码支持 |
| SAML | ⚠️ 基础实现 | SSO 流程不完整 |
| OIDC | ⚠️ 基础实现 | OAuth 2.0 流程不完整 |
| E2EE | ⚠️ 部分实现 | 跨设备密钥验证不完整 |

### 11.3 安全最佳实践

1. **密码安全**
   - 使用 Argon2id 进行密码哈希
   - 支持从旧版哈希迁移
   - 登录失败锁定机制

2. **Token 安全**
   - JWT 签名验证
   - Token 黑名单机制
   - Refresh Token 轮换

3. **SQL 注入防护**
   ```rust
   sqlx::query_as::<_, User>("SELECT * FROM users WHERE user_id = $1")
       .bind(user_id)
       .fetch_one(&pool)
       .await?;
   ```

---

## 十二、性能优化规范

### 12.1 数据库索引策略

```sql
CREATE INDEX idx_events_room_time ON events(room_id, origin_server_ts DESC);
CREATE INDEX idx_users_lower_email ON users(LOWER(email));
CREATE INDEX idx_events_content ON events USING GIN(content);
```

### 12.2 查询优化

```sql
EXPLAIN ANALYZE SELECT * FROM events 
WHERE room_id = $1 ORDER BY origin_server_ts DESC LIMIT 100;

CREATE INDEX idx_events_covering ON events(room_id, origin_server_ts DESC) 
INCLUDE (event_id, type, sender);
```

### 12.3 缓存策略

| 缓存类型 | TTL | 说明 |
|----------|-----|------|
| Token 缓存 | 3600秒 | JWT 令牌验证结果 |
| 用户活跃状态 | 60秒 | 在线状态 |
| 房间摘要 | 300秒 | 房间信息 |
| 用户管理员状态 | 3600秒 | is_admin 状态 |

---

## 十三、核心表结构参考

### 13.1 用户表 (users)

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

### 13.2 设备表 (devices)

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

### 13.3 访问令牌表 (access_tokens)

```sql
CREATE TABLE access_tokens (
    id BIGSERIAL PRIMARY KEY,
    token TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL,
    device_id TEXT,
    created_ts BIGINT NOT NULL,
    expires_at BIGINT,
    is_valid BOOLEAN DEFAULT TRUE,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX idx_access_tokens_user_valid ON access_tokens(user_id, is_valid);
```

### 13.4 刷新令牌表 (refresh_tokens)

```sql
CREATE TABLE refresh_tokens (
    id BIGSERIAL PRIMARY KEY,
    token_hash TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL,
    device_id TEXT,
    created_ts BIGINT NOT NULL,
    expires_at BIGINT,
    is_revoked BOOLEAN DEFAULT FALSE,
    revoked_at BIGINT,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX idx_refresh_tokens_user_revoked ON refresh_tokens(user_id, is_revoked);
```

---

## 十四、测试账户

| 角色 | 用户名 | 密码 | 用途 |
|------|--------|------|------|
| 管理员 | admin | Admin@123 | 管理 API 测试 |
| 用户1 | testuser1 | Test@123 | 基础功能测试 |
| 用户2 | testuser2 | Test@123 | 交互测试 |
| 用户3 | testuser3 | Test@123 | 群组测试 |

---

## 十五、常见错误修复

### 15.1 字段名称不一致

| 错误 | 正确 | 说明 |
|------|------|------|
| `invalidated` | `is_revoked` | 布尔字段应使用 is_ 前缀 |
| `created_at` | `created_ts` | 统一使用 _ts 后缀 |
| `updated_at` | `updated_ts` | 统一使用 _ts 后缀 |
| `expires_ts` | `expires_at` | 可选时间戳使用 _at |
| `revoked_ts` | `revoked_at` | 可选时间戳使用 _at |
| `enabled` | `is_enabled` | 布尔字段应使用 is_ 前缀 |

### 15.2 数据类型不匹配

| 错误 | 正确 | 说明 |
|------|------|------|
| `expires_at: i64` | `expires_at: Option<i64>` | 可为空的字段应使用 Option |
| `id: i32` | `id: i64` | BIGSERIAL 对应 i64 |

---

## 十六、相关文档

- 项目分析报告: `docs/synapse-rust/PROJECT_ANALYSIS_REPORT.md`
- 数据模型文档: `docs/synapse-rust/data-models.md`
- 字段标准文档: `migrations/DATABASE_FIELD_STANDARDS.md`
- PostgreSQL 指南: `.trae/pg-aiguide/SKILL.md`
- API 测试文档: `/home/tzd/api-test/api-test.md`

---

## 十七、版本历史

| 版本 | 日期 | 变更说明 |
|------|------|----------|
| 1.0.0 | 2026-03-01 | 初始版本，综合项目规范 |
| 1.1.0 | 2026-03-05 | 更新字段标准化规范 |
| 2.0.0 | 2026-03-12 | 基于项目分析报告全面优化，添加 MSC 规范、架构规范、测试规范、安全规范 |
| 2.1.0 | 2026-03-13 | 迁移文件优化合并，删除 12 个冗余文件，创建统一迁移文件 |
