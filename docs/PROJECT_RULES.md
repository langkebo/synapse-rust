# Synapse Rust 项目规则

> **版本**: 2.1.0
> **最后更新**: 2026-01-28
> **项目状态**: 源代码重建中
> **参考文档**: [Synapse 官方文档](https://element-hq.github.io/synapse/latest/)

---

## 一、项目概述

### 1.1 项目背景

Synapse Rust 项目旨在使用 Rust 语言重新实现 Matrix 协议的开源 Homeserver——Synapse，以获得更好的性能、更低的内存占用以及更强的安全性。原 Synapse Python 实现虽然在功能上非常完善，但在高并发场景下存在性能瓶颈。通过使用 Rust，我们期望在保持功能兼容性的同时，显著提升系统的整体性能表现。

在基础 Matrix 协议实现之外，本项目还包含一套增强功能模块，这些功能在原有的 Python 版 enhanced 目录中实现，包括好友系统、私聊管理、语音消息、安全控制等企业级功能。这些增强功能是本项目的重要组成部分，需要在 Rust 重构过程中完整保留并优化实现。

### 1.2 当前状态

由于执行 `git clean -fd` 命令导致源代码目录被意外删除，项目目前处于重建阶段。数据库配置和 schema 已恢复，但核心代码需要重新实现。

| 组件 | 状态 | 说明 |
|------|------|------|
| 数据库 schema | ✅ 已恢复 | users、devices、rooms、events 等表已创建 |
| 数据库用户 | ✅ 已配置 | synapse_user 用户已创建并授权 |
| 项目配置 | ✅ 已存在 | Cargo.toml、基础目录结构存在 |
| 源代码 | 🔄 重建中 | 需要重新实现所有模块 |
| 文档 | ⚠️ 需要更新 | 需与当前状态同步 |

### 1.3 核心目标

#### 1.3.1 性能目标

| 指标 | 当前值 | 目标值 | 提升幅度 |
|------|--------|--------|----------|
| 同步延迟 | 待测量 | 5ms | 基准建立 |
| 内存占用 | 待测量 | 200MB | 基准建立 |
| 并发用户 | 待测量 | 500K | 基准建立 |
| API 响应时间 | 待测量 | <10ms | 基准建立 |

#### 1.3.2 功能目标

- **API 兼容性**: 保持与 Matrix 规范完全兼容
- **E2EE 支持**: 实现完整的端到端加密功能
- **联邦通信**: 完整的 Federation API 支持
- **管理功能**: 完善的 Admin API 支持
- **媒体处理**: 媒体上传、存储、检索功能
- **增强功能**: 好友系统、私聊管理、语音消息（内部管理）

---

## 二、技术栈规范

### 2.1 核心技术选型

| 类别 | 技术 | 版本 | 用途 |
|------|------|------|------|
| 编程语言 | Rust | 2021 Edition | 核心开发 |
| 异步运行时 | Tokio | 1.35+ | 异步处理 |
| Web 框架 | Axum | 0.7 | HTTP 服务 |
| Web 中间件 | Tower-HTTP | 0.5 | CORS、追踪等 |
| 数据库 | PostgreSQL | 15+ | 数据持久化 |
| ORM | SQLx | 0.7 | 数据库操作 |
| 连接池 | deadpool | 0.10 | 连接池管理 |
| 缓存 | Redis | 7.0+ | 分布式缓存 |
| 本地缓存 | Moka | 0.12 | LRU 缓存 |
| 序列化 | serde | 1.0 | JSON 序列化 |
| 配置管理 | config | 0.14 | 配置解析 |
| JWT 认证 | jsonwebtoken | 9.0 | Token 生成 |
| 日志追踪 | tracing | 0.1 | 结构化日志 |

### 2.2 项目结构

```
synapse_rust/
├── Cargo.toml                 # 项目配置
├── src/
│   ├── lib.rs                # 库入口
│   ├── main.rs               # 服务入口
│   ├── common/               # 公共模块
│   │   ├── mod.rs
│   │   ├── error.rs          # 错误类型
│   │   ├── types.rs          # 公共类型
│   │   ├── config.rs         # 配置解析
│   │   └── crypto.rs         # 加密工具
│   ├── storage/              # 存储层
│   │   ├── mod.rs
│   │   ├── user.rs           # 用户存储
│   │   ├── device.rs         # 设备存储
│   │   ├── token.rs          # 令牌存储
│   │   ├── room.rs           # 房间存储
│   │   ├── membership.rs     # 成员存储
│   │   ├── event.rs          # 事件存储
│   │   ├── friend.rs         # 好友关系存储
│   │   └── private.rs        # 私聊会话存储
│   ├── cache/                # 缓存层
│   │   ├── mod.rs
│   │   ├── local.rs          # 本地缓存
│   │   └── redis.rs          # Redis 缓存
│   ├── auth/                 # 认证模块
│   │   └── mod.rs            # 认证服务
│   ├── services/             # 业务服务层
│   │   ├── mod.rs
│   │   ├── registration.rs   # 注册服务
│   │   ├── room.rs           # 房间服务
│   │   ├── sync.rs           # 同步服务
│   │   ├── media.rs          # 媒体服务
│   │   ├── friend.rs         # 好友服务
│   │   ├── private_chat.rs   # 私聊服务
│   │   └── voice.rs          # 语音消息服务
│   ├── web/                  # Web 路由层
│   │   ├── mod.rs
│   │   ├── routes/
│   │   │   ├── mod.rs        # 客户端 API
│   │   │   ├── admin.rs      # 管理 API
│   │   │   ├── media.rs      # 媒体 API
│   │   │   ├── federation.rs # 联邦 API
│   │   │   ├── friend.rs     # 好友 API (增强)
│   │   │   ├── private.rs    # 私聊 API (增强)
│   │   │   └── voice.rs      # 语音消息 API (增强)
│   │   └── middleware/       # HTTP 中间件
│   │       ├── mod.rs
│   │       ├── logging.rs
│   │       ├── cors.rs
│   │       └── auth.rs
│   └── server.rs             # 服务器配置
├── schema.sql                # 数据库 schema
├── config.yaml               # 配置文件模板
└── docs/                     # 文档目录
```

---

## 三、代码规范

### 3.1 格式化规范

代码格式化使用 rustfmt 工具自动执行，所有代码提交前必须通过格式化检查。

- 缩进使用四个空格，不使用制表符
- 行宽限制为 120 个字符
- 函数参数列表中的参数各自占一行
- 链式调用中的点号位于行首
- 模块声明之间一个空行，函数定义之间两个空行

### 3.2 命名规范

| 类型 | 规范 | 示例 |
|------|------|------|
| 模块名 | 蛇形小写 | user_storage, room_service |
| 结构体 | 帕斯卡命名 | UserStorage, RoomEvent |
| 枚举 | 帕斯卡命名 | MembershipState, EventType |
| 函数 | 蛇形小写 | create_user, get_by_id |
| 常量 | 全大写蛇形 | MAX_POOL_SIZE |
| 变量 | 描述性 snake_case | is_active, user_count |

### 3.3 注释规范

注释应解释「为什么」而非「是什么」，代码本身应尽可能自文档化。

- 单行注释使用 //
- 块注释使用 /* */
- 文档注释使用 ///，支持 Markdown 格式
- 公共 API 必须编写文档注释

```rust
/// 创建新用户
///
/// # 参数
/// * `username` - 用户名，必须唯一
/// * `password` - 原始密码，将被哈希处理
///
/// # 返回
/// 返回创建的用户信息和令牌元组
///
/// # 错误
/// 如果用户名已被占用，返回 [`ApiError::conflict`]
pub async fn create_user(
    username: &str,
    password: &str,
) -> Result<(User, TokenInfo), ApiError> {
    // 使用查询锁防止并发创建冲突
    let exists = self.storage.exists_by_username(username).await?;
    if exists {
        return Err(ApiError::conflict("Username already taken"));
    }
    // ...
}
```

---

## 四、错误处理规范

### 4.1 错误类型定义

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    pub code: String,      // 错误码
    pub message: String,   // 错误消息
    pub status: u16,       // HTTP 状态码
}

impl ApiError {
    pub fn bad_request(message: impl Into<String>) -> Self
    pub fn unauthorized(message: impl Into<String>) -> Self
    pub fn forbidden(message: impl Into<String>) -> Self
    pub fn not_found(message: impl Into<String>) -> Self
    pub fn conflict(message: impl Into<String>) -> Self
    pub fn internal(message: impl Into<String>) -> Self
}

pub type ApiResult<T> = Result<T, ApiError>;
```

### 4.2 错误码映射

| HTTP 状态码 | 错误码 | 说明 |
|-------------|--------|------|
| 400 | BAD_REQUEST | 请求参数错误 |
| 401 | UNAUTHORIZED | 未认证或 Token 无效 |
| 403 | FORBIDDEN | 权限不足 |
| 404 | NOT_FOUND | 资源不存在 |
| 409 | CONFLICT | 资源冲突 |
| 429 | RATE_LIMITED | 请求频率超限 |
| 500 | INTERNAL_ERROR | 服务器内部错误 |
| 502 | BAD_GATEWAY | 网关错误 |
| 503 | SERVER_BUSY | 服务繁忙 |

---

## 五、认证规范

### 5.1 JWT Token 结构

```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,           // 用户 ID
    pub user_id: String,       // 用户 ID
    pub admin: bool,           // 是否管理员
    pub exp: i64,              // 过期时间
    pub iat: i64,              // 签发时间
    pub device_id: Option<String>, // 设备 ID
}
```

### 5.2 认证流程

1. **注册流程**: 用户名 → 密码哈希 → 创建设备 → 生成 Token
2. **登录流程**: 验证密码 → 更新设备 → 生成 Token
3. **Token 验证**: 解析 JWT → 验证签名 → 检查过期 → 缓存验证

---

## 六、API 实现规范

### 6.1 Client API 实现状态

| 端点 | 方法 | 状态 | 优先级 |
|------|------|------|--------|
| `/_matrix/client/versions` | GET | 待实现 | P0 |
| `/_matrix/client/r0/register` | POST | 待实现 | P0 |
| `/_matrix/client/r0/register/available` | GET | 待实现 | P0 |
| `/_matrix/client/r0/login` | POST | 待实现 | P0 |
| `/_matrix/client/r0/logout` | POST | 待实现 | P1 |
| `/_matrix/client/r0/sync` | GET | 待实现 | P1 |
| `/_matrix/client/r0/rooms/:room_id/messages` | GET | 待实现 | P1 |
| `/_matrix/client/r0/createRoom` | POST | 待实现 | P1 |

### 6.2 Admin API 实现状态

| 端点 | 方法 | 状态 | 优先级 |
|------|------|------|--------|
| `/_synapse/admin/v1/server_version` | GET | 待实现 | P1 |
| `/_synapse/admin/v1/register` | POST | 待实现 | P1 |
| `/_synapse/admin/v1/users/:user_id` | GET | 待实现 | P1 |
| `/_synapse/admin/v1/rooms/:room_id` | GET | 待实现 | P1 |

### 6.3 Federation API 实现状态

| 端点 | 方法 | 状态 | 优先级 |
|------|------|------|--------|
| `/_matrix/federation/v1/version` | GET | 待实现 | P1 |
| `/_matrix/federation/v1/send/:txn_id` | PUT | 待实现 | P1 |

### 6.4 Enhanced API 实现状态（增强功能）

#### 好友系统 API

| 端点 | 方法 | 状态 | 优先级 |
|------|------|------|--------|
| `/_synapse/enhanced/friends` | GET | 待实现 | P1 |
| `/_synapse/enhanced/friend/request` | POST | 待实现 | P1 |
| `/_synapse/enhanced/friend/request/:request_id/respond` | POST | 待实现 | P1 |

#### 私聊管理 API

| 端点 | 方法 | 状态 | 优先级 |
|------|------|------|--------|
| `/_synapse/enhanced/private/sessions` | GET/POST | 待实现 | P1 |
| `/_synapse/enhanced/private/sessions/:session_id` | DELETE | 待实现 | P1 |

#### 语音消息 API

| 端点 | 方法 | 状态 | 优先级 |
|------|------|------|--------|
| `/_synapse/enhanced/voice/upload` | POST | 待实现 | P1 |
| `/_synapse/enhanced/voice/messages/:message_id` | GET | 待实现 | P1 |

---

## 七、数据库设计规范

### 7.1 核心表结构

#### 用户表（users）

```sql
CREATE TABLE users (
    user_id TEXT NOT NULL PRIMARY KEY,
    username TEXT NOT NULL UNIQUE,
    password_hash TEXT,
    admin BOOLEAN DEFAULT FALSE,
    is_guest BOOLEAN DEFAULT FALSE,
    creation_ts BIGINT NOT NULL,
    avatar_url TEXT,
    displayname TEXT,
    deactivated BOOLEAN DEFAULT FALSE,
    shadow_banned BOOLEAN DEFAULT FALSE,
    generation BIGINT NOT NULL
);
```

#### 设备表（devices）

```sql
CREATE TABLE devices (
    device_id TEXT NOT NULL PRIMARY KEY,
    user_id TEXT NOT NULL,
    display_name TEXT,
    last_seen_ts BIGINT NOT NULL,
    last_seen_ip TEXT,
    created_ts BIGINT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);
```

#### 房间表（rooms）

```sql
CREATE TABLE rooms (
    room_id TEXT NOT NULL PRIMARY KEY,
    is_public BOOLEAN NOT NULL DEFAULT FALSE,
    creator TEXT NOT NULL,
    creation_ts BIGINT NOT NULL,
    federate BOOLEAN NOT NULL DEFAULT TRUE,
    version TEXT NOT NULL DEFAULT '1',
    name TEXT,
    topic TEXT,
    avatar TEXT,
    encryption TEXT
);
```

#### 事件表（events）

```sql
CREATE TABLE events (
    event_id TEXT NOT NULL PRIMARY KEY,
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    content TEXT NOT NULL,
    state_key TEXT,
    depth BIGINT NOT NULL DEFAULT 0,
    origin_server_ts BIGINT NOT NULL,
    origin TEXT NOT NULL,
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);
```

---

## 八、缓存策略

### 8.1 两级缓存架构

```
┌─────────────────────────────────────┐
│           Application Layer         │
│    (Service → Cache Manager)        │
└──────────────┬──────────────────────┘
               │
    ┌──────────┴──────────┐
    │                     │
┌───┴───┐           ┌─────┴─────┐
│ Local │           │   Redis   │
│ Cache │           │   Cache   │
│ (Moka)│           │ (Redis)   │
└───────┘           └───────────┘
```

- **本地缓存 (Moka)**: LRU 策略，适用于热点数据
- **Redis 缓存**: 分布式缓存，支持多实例共享

---

## 九、增强功能模块

### 9.1 模块公开发布策略

| 模块 | 发布策略 | 说明 |
|------|----------|------|
| 好友系统 | ✅ 对外发布 | 核心社交功能 |
| 私聊管理 | ✅ 对外发布 | 端到端加密通信 |
| 语音消息 | ✅ 对外发布 | 用户体验增强 |
| 安全控制 | ❌ 内部管理 | 仅 Admin API 开放 |

### 9.2 安全控制模块评估

**决策**: 不建议公开发布该模块

**评估理由**:
1. 功能复杂度高，包含威胁检测、IP 声誉系统等
2. 实现难度大，需要集成外部威胁情报库
3. 维护成本高，安全规则需持续更新
4. 与 Matrix 协议重叠，认证、授权已有完善实现
5. 安全风险，公开功能可能被恶意用户研究绕过方法

**建议处理方式**:
- 仅作为内部管理功能，通过 Admin API 使用
- 不提供公开 API 接口
- 部署时仅限内网访问或添加额外认证

---

## 十、重建优先级

| 优先级 | 模块 | 预计工时 | 依赖 |
|--------|------|----------|------|
| P0 | 基础模块（common） | 2小时 | 无 |
| P0 | 存储层（storage） | 4小时 | common |
| P0 | 认证模块（auth） | 3小时 | storage |
| P1 | 服务层（services） | 4小时 | auth、storage |
| P1 | Web 路由层（web/routes） | 4小时 | services |
| P1 | 中间件（web/middleware） | 2小时 | web/routes |
| P1 | 服务器入口（server.rs、main.rs） | 2小时 | web |
| P2 | 测试模块 | 3小时 | 所有模块 |
| P2 | 文档完善 | 2小时 | 所有模块 |

---

## 十一、开发规范

### 11.1 Git 工作流程

- 主分支保护，禁止直接推送
- 功能开发使用 feature 分支
- 提交信息遵循 Conventional Commits 规范
- 每次提交需通过 CI 检查

### 11.2 代码审查

- 所有合并请求需要至少一人审查
- 审查重点: 代码质量、性能影响、安全性
- 审查通过后由作者合并

### 11.3 测试要求

- 单元测试覆盖率不低于 80%
- 关键路径必须有集成测试
- 性能敏感代码需要基准测试

---

## 十二、文档维护

### 12.1 文档更新规则

- API 变更需要同步更新 API 文档
- 数据库 schema 变更需要更新 ER 图
- 架构调整需要更新架构文档

### 12.2 文档版本管理

- 使用语义化版本号
- 每次重大变更更新版本号
- 保留历史版本供参考

---

## 附录 A: 快速参考

### A.1 常用命令

```bash
# 开发运行
cargo run

# 测试
cargo test

# 代码检查
cargo clippy

# 格式化
cargo fmt

# 构建发布版本
cargo build --release

# 数据库迁移
sqlx database create
sqlx migrate run
```

### A.2 环境变量

| 变量 | 说明 | 默认值 |
|------|------|--------|
| DATABASE_URL | 数据库连接字符串 | postgres://synapse:synapse@localhost:5432/synapse |
| SERVER_NAME | 服务器名称 | localhost |
| JWT_SECRET | JWT 密钥 | 自动生成 |
| HOST | 监听地址 | 0.0.0.0 |
| PORT | 监听端口 | 8008 |
| MEDIA_PATH | 媒体文件存储路径 | ./media |
| REDIS_URL | Redis 连接字符串 | redis://localhost:6379 |

---

## 附录 B: 外部参考

- [Matrix 规范](https://spec.matrix.org/)
- [Synapse 官方文档](https://element-hq.github.io/synapse/latest/)
- [Rust 官方文档](https://doc.rust-lang.org/)
- [Axum 框架文档](https://docs.rs/axum/latest/axum/)
- [SQLx 文档](https://docs.rs/sqlx/latest/sqlx/)
