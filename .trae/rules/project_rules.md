# synapse-rust 项目规则

> 版本: v2.5.1
> 更新日期: 2026-10-06
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
| 当前版本 | v6.2.0 |
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

> 本项目是 **Cargo workspace**：代码按职责拆分为 8 个成员 crate，根 crate `synapse-rust`
> 只承载二进制入口与组装（thin binary）。**不存在** `src/storage/`、`src/services/`、
> `src/web/routes/` 这类目录——它们分别是 `synapse-storage`、`synapse-services`、
> `synapse-web` 三个成员 crate。

```
synapse-rust/                          # workspace 根（thin binary crate）
├── Cargo.toml                         # [workspace] members = 8 个 crate
├── migrations/                        # 数据库迁移（1 个统一基线 SQL）
│   ├── 00000000_unified_schema_v12.sql # 唯一迁移文件（统一基线）
│   ├── INDEXES.md / README.md         # 索引清单与迁移说明
├── src/                               # 根 crate：二进制入口 + 装配层（薄）
│   ├── bin/                           # 可执行入口（main/worker/healthcheck/迁移工具等）
│   ├── server/                        # 应用组装（router、services、telemetry、database）
│   ├── worker/                        # 工作进程支持
│   ├── tasks/、common/、e2ee/、storage/ # 薄转发层，指向对应成员 crate
│   ├── main.rs / lib.rs
├── synapse-common/                    # 公共模块：config、error、事件/PDU 模型、时间与校验
├── synapse-cache/                     # 缓存层：本地/远程缓存、失效、熔断
├── synapse-storage/                   # 数据访问层（按域拆分：event/room/user/membership/media/…）
├── synapse-e2ee/                      # 端到端加密：olm/megolm、backup、cross_signing、to_device、ssss
├── synapse-federation/                # 联邦协议：client、signing、server_acl、device_sync、edu
├── synapse-services/                  # 业务逻辑层：auth/room/event/sync/media/push/rtc/admin + container
├── synapse-web/                       # HTTP 层：routes/、middleware/、federation/（Axum 路由）
├── synapse-test-utils/                # 测试工具（EnvGuard、env_lock 等）
├── tests/                             # 测试文件
│   ├── unit/                          # 单元测试（约 130 个文件）
│   ├── integration/                   # 集成测试（约 165 个文件）
│   ├── e2e/                           # E2E 测试（3 个文件）
│   ├── performance/                   # 性能测试（5 个文件）
│   └── common/                        # 共享测试夹具（http_mock、snapshots 等）
├── docker/                            # Docker 配置
├── scripts/                           # 脚本文件（ci/、quality/、api_test/、test/）
├── benches/                           # Criterion 基准测试
├── monitoring/                        # Prometheus 配置与告警规则
└── docs/                              # 文档
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
| 更新时间 | `updated_ts` | BIGINT | **两种并存：`NOT NULL` 或可空，新表必须显式选一个** | 毫秒级时间戳。基线 SQL 里两种写法各占约一半（`NOT NULL` 43 / 可空 42），谁也不是笔误 ⇒ 不强制统一，但**必须显式选**：计数由 `scripts/ci/check_schema_timestamp_convention.py` 的棘轮冻结（只许朝 `NOT NULL` 方向变，当前值见 `scripts/ci/schema_timestamp_baseline`）。选 `NOT NULL` 时务必带 `DEFAULT`，否则每条 `INSERT` 都要自己写 |
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
    updated_ts BIGINT NOT NULL,
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

### 7.0 ServiceContainer 服务容器架构

```
ServiceContainer
├── Layered Service Groups (4)
│   ├── e2ee: E2eeServices
│   │   └── device_keys, cross_signing, megolm, backup, verification, key_request, ssss, to_device, device_trust
│   ├── rooms: RoomSyncServices
│   │   └── room, space, summary, sync, sliding_sync, thread, relations, typing, beacon, room_tag, directory
│   ├── federation: FederationServices
│   │   └── federation_client, key_rotation, device_sync, event_auth_chain, friend_federation
│   └── admin: AdminServices
│       └── audit, federation, media, security, server, token, user, registration, retention
│
├── Compatible Groups (4)
│   ├── core: CoreServices
│   │   └── auth, registration, search, media, modules, uia, rtc, identity, translation, push…
│   ├── account: AccountServices
│   │   └── account_data, refresh_token, dehydrated_device, oidc, oidc_mapping
│   ├── sso: SsoServices
│   │   └── saml, cas, builtin_oidc_provider
│   └── extensions: ExtensionServices
│       └── friends, voice, burn_after_read, openclaw, feature_flag, matrix_ai_connection, mcp_proxy
│
└── Individual Services
    ├── user_storage, threepid_storage, device_storage, token_storage, presence_storage
    ├── qr_login_storage, invite_blocklist_storage, sticky_event_storage
    ├── auth_service, registration_service, search_service, media_service
    ├── cache, task_queue, metrics, server_metrics, config
    └── pool, event_notifier, server_name, background_update_service…
```

> 对应代码: [container.rs](../../synapse-services/src/container.rs)

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
| MSC3245 | 语音消息（voice messages） | ⚠️ **按 by-design 部分支持** | 此前写作 "Room summary / 完整实现"，两处都错（MSC3245 是**语音消息**，服务端三端点 `convert`/`optimize`/`transcription` 恒返回 `M_UNRECOGNIZED`）。MSC3245 规定语音的编码/转写**在客户端**完成，因此服务端不实现是符合规范的；三端点已在 `ROUTE_CONTRACT.md`「有意不实现」清单登记（审计 DOC-03 / RD-19）。"Room summary" 是 MSC3266/MSC4380 族的能力，见下一行 |
| MSC4380 | 邀请屏蔽 | ✅ 已实现 | 完整实现 |
| MSC4354 | Sticky Event | ✅ 已实现 | 完整实现 |
| MSC4388 | 二维码登录完善 | ✅ 已实现 | 完整实现 |
| MSC4261 | Widget API | ✅ 已实现 | 完整实现 |
| MSC3266 | Room summary batch | ✅ 已实现 | room_summary.rs Msc3266RoomSummaryBatchResponse |
| MSC4133 | Extended profile | ✅ 已实现 | uk.tcpip.msc4133 命名空间，api_doc.rs |

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

- 文档总索引: `docs/INDEX.md`
- 路由契约: `docs/synapse-rust/ROUTE_CONTRACT.md`
- API 覆盖度: `docs/synapse-rust/API_COVERAGE_REPORT.md`
- 数据迁移说明: `migrations/README.md`
- 索引清单: `migrations/INDEXES.md`
- 遗留问题审计: `docs/audit/COMPREHENSIVE_LEGACY_ISSUES_REPORT_20261006.md`（当前权威；历史报告见 `docs/audit/archive/`）

---

## 十七、依赖管理最佳实践

### 17.1 依赖添加原则

1. **最小化原则**：优先使用标准库，其次使用成熟、维护活跃的第三方库
2. **版本锁定**：使用 `"1.2"` 而非 `"^1.2"` 或 `"1"` 以锁定 MAJOR.MINOR
3. **特性最小化**：`default-features = false`，只启用需要的 features
4. **避免重复**：新增依赖前检查 `cargo tree -d --workspace` 是否引入新版本重复

### 17.2 定期检查命令

```bash
# 每周：检查兼容版本更新
cargo update --dry-run

# 每月：检查重复依赖
cargo tree -d --workspace | grep -E '^[a-zA-Z]' | sort -u

# 每季度：检查过时依赖
cargo install cargo-outdated && cargo outdated -R

# 每季度：审计安全漏洞
cargo audit
```

### 17.3 重复依赖治理

| 优先级 | 操作 |
|--------|------|
| P0 | 禁用不必要的 crate features（如 `config/ron`），消除传递依赖 |
| P1 | `cargo update` 统一兼容版本 |
| P2 | 评估直接依赖升级（如 `redis` 0.27→0.29）以消除传递重复 |
| P3 | 记录 SemVer 不兼容的深层重复到 `docs/synapse-rust/DEPENDENCY_UPGRADE_TRACKER.md` |

### 17.4 已消除的重复依赖

| 依赖 | 旧版本 | 方式 |
|------|--------|------|
| `base64` | v0.21.7 | 禁用 `config/ron` feature（⚠️ 该分裂已被新的 `base64` 0.22/0.23 分裂取代，见 17.5） |
| `toml_datetime` | v0.6.11 | 连带消除 |
| `toml_edit` | v0.22.27 | 连带消除 |
| `winnow` | v0.7.15 | 连带消除 |

### 17.5 当前 SemVer 不兼容的深层重复依赖

> **唯一权威口径**：完整的实测清单（分组、版本、直接依赖者、根因、消除路径、复现命令）见
> [`docs/synapse-rust/DEPENDENCY_UPGRADE_TRACKER.md`](../../docs/synapse-rust/DEPENDENCY_UPGRADE_TRACKER.md)。
> 本节仅保留摘要；**数字以 tracker 为准，勿在本文件重复维护明细**。

**2026-10-04 实测摘要**（`cargo tree -d --workspace`）：

| 指标 | 值 |
|------|-----|
| 版本分裂组（同名 ≥2 个不同版本） | 38 组 |
| 同版本多来源出现（非 SemVer 分裂） | 16 个 crate |
| 可本地 `[patch]` 解决 | 0 组 |
| 最大根因 | `vodozemac 0.11`（新一代码 RustCrypto）与项目直连旧一代并存 |

**主要根因族**（详表见 tracker §3）：

| 族 | 主题 | 优先级 |
|----|------|--------|
| A | RustCrypto 新老两代（24 组，`digest` 0.10↔0.11 / `cipher` 0.4↔0.5 / `aead` 0.5↔0.6 / `sha2` 0.10↔0.11 / `ed25519-dalek` 2↔3 …） | P1 |
| B | `rand` 三代（0.8 / 0.9 / 0.10）+ `getrandom` 三代 | P0 |
| C | `hashbrown` / `hashlink` / `foldhash`（`dashmap`↔`config`↔`sqlx`↔`indexmap`） | P2 |
| D | `socket2`（`redis 0.29` 锁 0.5） | P1 |
| E/F/G | `nom`（`config` 锁 7）/ `base64`（`lettre`/`vodozemac` 用 0.23）/ `syn` 2↔3 | P2 |
| H/I/J | `itertools`（dev-only）/ `core-foundation`（macOS）/ `libwebp-sys2` | P3 |

**治理原则：**
- **最高杠杆的本地动作**：升级本项目**直连**的旧一代 RustCrypto（`aes`/`chacha20poly1305`/`sha2`/`hkdf`/`hmac`/`ed25519-dalek`）到新一代，可一次性消除族 A 的多数旧一代分支
- `rand` 三代分裂为当前最高优先级（P0），0.8 分支须待 `sqlx`/`rsa`/`argon2` 迁移
- `socket2` 分裂待 `redis` 1.x 迁移后消除（需独立 API 迁移任务）
- `itertools` 分裂仅影响 dev-dependency（`criterion`），不影响生产二进制体积
- `core-foundation` 分裂仅影响 macOS 平台

## 十八、版本历史

| 版本 | 日期 | 变更说明 |
|------|------|----------|
| 1.0.0 | 2026-03-01 | 初始版本，综合项目规范 |
| 1.1.0 | 2026-03-05 | 更新字段标准化规范 |
| 2.0.0 | 2026-03-12 | 基于项目分析报告全面优化，添加 MSC 规范、架构规范、测试规范、安全规范 |
| 2.1.0 | 2026-03-13 | 迁移文件优化合并，删除 12 个冗余文件，创建统一迁移文件 |
| 2.2.0 | 2026-06-12 | 更新 ServiceContainer 分层架构描述；文档版本同步到 v10；修复 Synapse v1.153 行为差异 |
| 2.3.0 | 2026-06-13 | 新增第十七节"依赖管理最佳实践"：依赖添加原则、定期检查命令、重复依赖治理策略、已消除重复依赖记录 |
| 2.4.0 | 2026-06-20 | 新增第十七节 17.5"当前 SemVer 不兼容的深层重复依赖"：记录 11 组版本分裂的根因与消除路径；`cargo update` 统一 4 个兼容补丁 + 清理 12 个未使用传递依赖 |
| 2.5.0 | 2026-10-06 | DOC-01 修复：重写 §1.3 目录结构为真实 workspace crate 布局（8 个成员 crate，`migrations/` 仅 1 个统一基线 SQL），修正 §7.0 `container.rs` 链接为 `synapse-services/src/container.rs` |
| 2.5.1 | 2026-10-06 | DOC-09 连带修复：§16「遗留问题审计」指向更新为当前权威 `docs/audit/COMPREHENSIVE_LEGACY_ISSUES_REPORT_20261006.md`（原 `LEGACY_ISSUES_REPORT_20261004.md` 已归档至 `docs/audit/archive/`），消除本地守卫 `workspace_rules_reference_paths_exist` 的失效引用 |
