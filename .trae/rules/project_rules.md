# Synapse Rust 项目规则

> **版本**：2.0.0  
> **创建日期**：2026-01-28  
> **更新日期**：2026-01-29  
> **项目状态**：开发中  
> **参考文档**：[Synapse 官方文档](https://element-hq.github.io/synapse/latest/)、[Matrix 规范](https://spec.matrix.org/)

---

## 一、项目概述

### 1.1 项目目标

Synapse Rust 项目旨在使用 Rust 语言重新实现 Matrix 协议的开源 Homeserver——Synapse，以获得更好的性能、更低的内存占用以及更强的安全性。项目包含以下核心目标：

**功能完整性目标**：确保所有 Matrix 协议功能在 Rust 实现中得到完整实现，包括用户管理、房间管理、事件处理、联邦通信等核心功能。同时实现增强功能模块：好友系统、私聊管理、语音消息、安全控制。

**性能提升目标**：利用 Rust 语言的内存安全特性和零成本抽象，实现比 Python 原版更优的性能表现。具体指标包括：API 响应时间降低 50% 以上，内存占用降低 40% 以上，并发处理能力提升 3 倍以上。

**架构一致性目标**：增强功能的 Rust 实现应与 synapse_rust 项目的主体架构保持高度一致，遵循相同的分层架构、错误处理规范、异步编程模式和代码风格规范。

### 1.2 技术栈

| 类别 | 技术 | 版本 | 用途 | 兼容性要求 |
|------|------|------|------|-----------|
| 编程语言 | Rust | 1.93.0 | 核心开发 | 必须使用此版本，启用 edition2024 |
| 异步运行时 | Tokio | 1.35+ | 异步处理 | 需与 Rust 1.93.0 兼容 |
| Web 框架 | Axum | 0.7 | HTTP 服务 | 需支持 async-trait 0.1 |
| Web 中间件 | Tower-HTTP | 0.5 | CORS、追踪等 | 需与 Axum 0.7 匹配 |
| 数据库 | PostgreSQL | 15+ | 数据持久化 | 支持 SSL 连接 |
| ORM | SQLx | 0.7 | 数据库操作 | 需启用 postgres、rustls |
| 连接池 | deadpool | 0.10 | 连接池管理 | 需与 SQLx 配合 |
| 缓存 | Redis | 7.0+ | 分布式缓存 | 支持 Redis Cluster |
| 本地缓存 | Moka | 0.12 | LRU 缓存 | 需支持 async-trait |
| 序列化 | serde | 1.0 | JSON 序列化 | 需启用 derive、json |
| 配置管理 | config | 0.14 | 配置解析 | 支持 YAML 格式 |
| JWT 认证 | jsonwebtoken | 9.0 | Token 生成 | 需与 Rust 1.93.0 兼容 |
| 密码学 | argon2 | 0.5 | 密码哈希 | 算法参数设为安全等级 3 |
| 日志追踪 | tracing | 0.1 | 结构化日志 | 需支持 tracing-subscriber |

---

## 二、Rust 版本与编译环境规范

### 2.1 强制版本要求

**必须使用 Rust 1.93.0 或更高版本进行开发**。此版本要求基于以下原因：

1. **edition2024 支持**：部分依赖（如 base64ct）需要 edition2024 特性，Rust 1.93.0 是首个稳定支持该特性的版本
2. **依赖兼容性**：项目依赖链中的密码学库、序列化库等需要较新的 Rust 版本
3. **未来扩展性**：确保项目能够使用最新的语言特性和库功能

### 2.2 编译器配置

在 `rust-toolchain.toml` 文件中明确指定 Rust 版本：

```toml
[toolchain]
channel = "1.93.0"
components = ["rustfmt", "clippy", "rust-src"]
targets = ["x86_64-unknown-linux-gnu"]
profile = "default"
```

### 2.3 编译验证流程

**强制要求**：在测试、编译或构建之前，必须先检查并修复项目中的所有错误代码。只有在确保没有错误的情况下，才能进行测试、编译和构建操作。

**错误检查优先级**：

| 优先级 | 检查项 | 命令 | 说明 |
|-------|--------|------|------|
| P0 | 代码诊断 | `cargo check` 或 IDE 诊断 | 必须通过，无任何错误 |
| P0 | 编译检查 | `cargo build` | 必须成功，无编译错误 |
| P1 | 静态分析 | `cargo clippy` | 警告可接受，错误必须修复 |
| P1 | 格式检查 | `cargo fmt --check` | 必须通过 |
| P2 | 单元测试 | `cargo test` | 测试失败视为错误 |
| P2 | 集成测试 | `cargo test --test integration` | 测试失败视为错误 |

**标准验证流程**：

```bash
# 1. 清理并检查（第一步：必须无错误）
echo "=== Step 1: 代码诊断 ==="
rm -rf .sqlx
cargo check

if [ $? -ne 0 ]; then
    echo "❌ 错误：代码诊断失败，必须修复所有错误后继续"
    exit 1
fi

# 2. 执行完整编译
echo "=== Step 2: 完整编译 ==="
cargo build

if [ $? -ne 0 ]; then
    echo "❌ 错误：编译失败，必须修复所有错误后继续"
    exit 1
fi

# 3. 运行 Clippy 检查
echo "=== Step 3: 静态分析 ==="
cargo clippy --all-features -- -D warnings

if [ $? -ne 0 ]; then
    echo "❌ 错误：Clippy 检查失败，必须修复所有警告和错误后继续"
    exit 1
fi

# 4. 运行格式化检查
echo "=== Step 4: 格式检查 ==="
cargo fmt --check

if [ $? -ne 0 ]; then
    echo "❌ 错误：格式检查失败，请运行 'cargo fmt' 修复格式问题"
    exit 1
fi

# 5. 执行测试（只有在以上步骤全部通过后才能执行）
echo "=== Step 5: 执行测试 ==="
cargo test

if [ $? -ne 0 ]; then
    echo "❌ 错误：测试失败，必须修复所有失败的测试后继续"
    exit 1
fi

echo "✅ 所有检查通过，项目状态良好"
```

**IDE 开发流程**：

1. 编写代码后立即运行 `cargo check` 或依赖 IDE 实时诊断
2. 发现错误时立即修复，不要带着错误继续开发
3. 提交前必须执行完整的验证流程
4. CI/CD 流水线必须包含完整的验证步骤

**禁止行为**：

```bash
# 禁止：带着错误运行测试
cargo test  # 错误：可能掩盖真实问题

# 禁止：带着错误进行构建镜像
docker build  # 错误：可能在构建阶段暴露问题

# 禁止：跳过错误直接提交
git commit -m "..."  # 错误：可能引入 regressions
```

**错误处理原则**：

1. **零容忍**：对编译错误和测试失败零容忍
2. **立即修复**：发现问题立即处理，不积累技术债务
3. **完整验证**：每次修复后重新运行完整验证流程
4. **记录追踪**：将发现和修复的错误记录在 CHANGELOG 中

### 2.4 工具链管理

```bash
# 安装指定版本
rustup install 1.93.0

# 设置默认工具链
rustup default 1.93.0

# 添加必要组件
rustup component add rustfmt clippy rust-src

# 验证安装版本
rustc --version
```

---

## 三、依赖管理策略

### 3.1 依赖选择原则

**兼容性优先，最新为辅**：

1. **首要原则**：所有依赖必须与 Rust 1.93.0 完全兼容
2. **版本锁定**：在 `Cargo.toml` 中使用精确版本号或兼容版本范围
3. **最小依赖**：只引入必要的依赖，避免过度依赖
4. **定期审查**：每月审查依赖版本，评估安全和性能影响

### 3.2 依赖版本规范

```toml
# Cargo.toml 示例配置

[dependencies]
# 核心依赖 - 锁定主版本号
tokio = { version = "1.35", features = ["full"] }
axum = "0.7"
sqlx = { version = "0.7", features = ["postgres", "runtime-tokio", "rustls"] }

# 密码学依赖 - 需要特别注意版本兼容性
# ed25519-dalek: v2.0+ 使用 SigningKey/VerifyingKey 替代 Keypair
ed25519-dalek = { version = "2.1", features = ["rand_core", "pkcs8", "std"] }
x25519-dalek = { version = "2.0", features = ["static_secrets", "serde"] }
argon2 = "0.5"

# 缓存依赖
moka = { version = "0.12", features = ["future"] }
redis = { version = "0.25", features = ["tls", "tokio-native-tls-comp"] }
```

### 3.3 密码学库特殊要求

**ed25519-dalek v2.0 API 变更**：

```rust
// 错误用法（v1.x API）
use ed25519_dalek::Keypair;

// 正确用法（v2.x API）
use ed25519_dalek::{SigningKey, VerifyingKey};

// 生成密钥对
let signing_key = SigningKey::generate(&mut rng);
let verifying_key = signing_key.verifying_key();

// 签名
let signature = signing_key.sign(message);

// 验证签名
verifying_key.verify(message, &signature)?;
```

**x25519-dalek 特性启用**：

```toml
[dependencies.x25519-dalek]
version = "2.0"
features = ["static_secrets", "serde"]  # 必须启用 static_secrets 特性
```

### 3.4 依赖更新机制

**更新策略**：

| 场景 | 更新方式 | 审批要求 |
|------|---------|---------|
| 安全补丁 | 立即更新 | 需代码审查 |
| 小版本更新（patch） | 每月集中更新 | 需测试通过 |
| 次版本更新（minor） | 每季度评估 | 需全面测试 |
| 主版本更新（major） | 每半年评估 | 需团队审批 |

**更新流程**：

```bash
# 1. 检查可更新依赖
cargo outdated

# 2. 创建更新分支
git checkout -b dependency-update-YYYY-MM

# 3. 执行更新
cargo update

# 4. 运行完整测试
cargo test --all-features

# 5. 更新 Cargo.lock 并提交
git add Cargo.lock
git commit -m "chore: update dependencies to latest compatible versions"
```

### 3.5 依赖安全审计

```bash
# 使用 cargo-audit 检查安全漏洞
cargo install cargo-audit
cargo audit

# 使用 cargo-deny 检查依赖许可证和合规性
cargo install cargo-deny
cargo deny check
```

---

## 四、代码质量标准

### 4.0 与 Synapse 对齐原则

**目标约束**：

1. **安全不低于 Synapse**：认证、密码存储、CORS、限流与审计能力至少达到 Synapse 当前实现水平  
2. **质量不低于 Synapse**：核心路径测试覆盖、错误处理与可观测性与 Synapse 相当  
3. **性能优于 Synapse**：在保证安全与正确性的前提下实现更低延迟与更高吞吐

### 4.1 编码规范

**命名约定**：

| 类型 | 约定 | 示例 |
|------|------|------|
| 模块 | 蛇形小写 | `user_storage` |
| 结构体 | 帕斯卡命名 | `UserStorage` |
| 函数 | 蛇形小写 | `create_user` |
| 常量 | 蛇形大写 | `MAX_CONNECTIONS` |
| 类型参数 | 简短驼峰 | `T: Into<String>` |
| 特征 | 形容词或名词 | `Storage` |

**代码格式**：

```bash
# 格式化代码
cargo fmt

# 检查格式
cargo fmt --check

# 修复可自动修复的问题
cargo fmt --
```

### 4.2 Clippy 规则

项目启用以下 Clippy 规则作为强制要求：

```bash
# 运行严格模式的 Clippy
cargo clippy --all-features -- -D warnings -A clippy::result_large_err -A clippy::arc_with_non_send_sync
```

**禁止使用的模式**：

```rust
// 禁止：过度使用 unwrap
fn process_data(data: &[u8]) -> Result<(), Error> {
    let value = data.parse::<i32>().unwrap(); // 错误
    Ok(())
}

// 正确：使用 ? 操作符
fn process_data(data: &[u8]) -> Result<(), Error> {
    let value = data.parse::<i32>()?;
    Ok(())
}

// 禁止：使用 expect 代替适当的错误处理
fn get_config() -> Config {
    let config = std::fs::read_to_string("config.yaml").expect("Failed to read config"); // 错误
}

// 正确：返回有意义的错误
fn get_config() -> Result<Config, ConfigError> {
    let content = std::fs::read_to_string("config.yaml")
        .map_err(|e| ConfigError::io(e))?;
    Ok(parse_config(&content)?)
}
```

### 4.3 文档要求

**公共 API 必须包含文档注释**：

```rust
/// 获取用户的访问令牌列表
///
/// # Arguments
///
/// * `user_id` - 用户的完整 Matrix ID，如 "@alice:server.com"
/// * `valid_only` - 是否仅返回有效的令牌
///
/// # Returns
///
/// 返回包含访问令牌的向量，如果用户不存在则返回空向量
///
/// # Errors
///
/// 如果数据库查询失败，返回 [`sqlx::Error`]
///
/// # Example
///
/// ```ignore
/// let tokens = storage.get_access_tokens("@alice:server.com", true).await?;
/// ```
pub async fn get_access_tokens(
    &self,
    user_id: &str,
    valid_only: bool,
) -> Result<Vec<AccessToken>, sqlx::Error> {
    // 实现
}
```

### 4.4 错误处理规范

**统一错误类型**：

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("Authentication failed: {reason}")]
    Unauthorized { reason: String },

    #[error("Resource not found: {resource}")]
    NotFound { resource: String },

    #[error("Invalid request: {message}")]
    BadRequest { message: String },

    #[error("Internal server error: {details}")]
    Internal { details: String },

    #[error("Database error: {source}")]
    Database { source: sqlx::Error },
}

impl ApiError {
    pub fn unauthorized() -> Self {
        Self::Unauthorized {
            reason: "Invalid or missing access token".to_string(),
        }
    }

    pub fn internal<S: Into<String>>(details: S) -> Self {
        Self::Internal {
            details: details.into(),
        }
    }
}
```

**错误转换**：

```rust
// 从 sqlx::Error 转换为 ApiError
impl From<sqlx::Error> for ApiError {
    fn from(source: sqlx::Error) -> Self {
        Self::Database { source }
    }
}

// 从第三方错误转换为 ApiError
impl From<jsonwebtoken::errors::Error> for ApiError {
    fn from(source: jsonwebtoken::errors::Error) -> Self {
        Self::Unauthorized {
            reason: format!("Token validation failed: {}", source),
        }
    }
}
```

### 4.5 禁止阻塞与异步规范

**规则**：

1. 任何 async 路径内禁止使用 `block_on`、`std::thread::sleep` 等阻塞调用  
2. 阻塞型 IO 必须通过 `spawn_blocking` 或专用线程池执行  
3. 缓存/数据库不可用时的降级策略必须明确记录并可配置  

---

### 4.6 测试基线与覆盖要求

**最低要求**：

1. 认证、注册、房间、同步、联邦、媒体为 P0 用例  
2. 新增功能必须补齐单元与集成用例  
3. 回归缺陷必须新增针对性测试

---

### 4.7 Worker 与复制一致性要求

**规则**：

1. 需要支持多进程/多实例部署时，必须提供 `worker_app`、`instance_map`、`stream_writers` 等职责配置模型  
2. 复制通道必须进行实例身份认证，禁止匿名或未授权实例写入  
3. 每个 stream 的写入实例必须有明确配置约束，禁止未配置实例写入  
4. 复制通道必须提供队列长度、延迟与失败率指标，并提供可配置降级策略

---

## 五、数据库开发规范
---

## 五、数据库开发规范

### 5.1 数据库Schema管理

**迁移文件命名规范**：

```
migrations/
├── 20260128000001_create_users_table.sql
├── 20260128000002_create_rooms_table.sql
├── 20260128000003_create_access_tokens_table.sql
└── 20260128000004_create_device_keys_table.sql
```

**迁移文件模板**：

```sql
-- 创建访问令牌表
-- 版本: 1.0.0
-- 描述: 存储用户访问令牌信息
-- 依赖: 20260128000001_create_users_table

CREATE TABLE IF NOT EXISTS access_tokens (
    id BIGSERIAL PRIMARY KEY,
    token VARCHAR(255) NOT NULL UNIQUE,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255),
    created_ts BIGINT NOT NULL,
    expires_ts BIGINT,
    invalidated_ts BIGINT
);

CREATE INDEX IF NOT EXISTS idx_access_tokens_user_id ON access_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_access_tokens_token ON access_tokens(token);
CREATE INDEX IF NOT EXISTS idx_access_tokens_expires_ts ON access_tokens(expires_ts);
```

### 5.2 SQL命名规范

**列名命名规则**：

| 类型 | 命名规则 | 示例 |
|------|---------|------|
| 时间戳 | 使用 `_ts` 后缀 | `created_ts`, `expires_ts`, `invalidated_ts` |
| 标识符 | 使用 `_id` 后缀 | `user_id`, `room_id`, `device_id` |
| 布尔值 | 使用 `is_` 或 `has_` 前缀 | `is_admin`, `has_device` |
| 计数 | 使用 `_count` 后缀 | `message_count` |

**禁止使用的模式**：

```sql
-- 错误：使用 NULLABLE 关键字
CREATE TABLE example (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255) NULLABLE  -- 错误：SQL 标准不支持 NULLABLE
);

-- 正确：直接使用 NULL 或 NOT NULL
CREATE TABLE example (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255),          -- 默认为 NULL
    status VARCHAR(50) NOT NULL -- 明确指定 NOT NULL
);
```

### 5.3 SQLx 查询规范

**查询函数签名**：

```rust
pub struct AccessToken {
    pub id: i64,
    pub token: String,
    pub user_id: String,
    pub device_id: Option<String>,  // 可空的字段必须使用 Option<T>
    pub created_ts: i64,
    pub expires_ts: Option<i64>,
    pub invalidated_ts: Option<i64>,
}

pub async fn create_token(
    &self,
    token: &str,
    user_id: &str,
    device_id: Option<&str>,
    expires_ts: Option<i64>,
) -> Result<AccessToken, sqlx::Error> {
    let now = chrono::Utc::now().timestamp();
    let row = sqlx::query!(
        r#"
        INSERT INTO access_tokens (token, user_id, device_id, created_ts, expires_ts)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, token, user_id, device_id, created_ts, expires_ts, invalidated_ts
        "#,
        token,
        user_id,
        device_id,
        now,
        expires_ts
    )
    .fetch_one(&*self.pool)
    .await?;

    Ok(AccessToken {
        id: row.id,
        token: row.token,
        user_id: row.user_id,
        device_id: row.device_id,
        created_ts: row.created_ts,
        expires_ts: row.expires_ts,
        invalidated_ts: row.invalidated_ts,
    })
}
```

### 5.4 数据库连接管理

```rust
// 数据库连接池配置
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub connection_timeout_secs: u64,
    pub idle_timeout_secs: u64,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:5432/synapse".to_string()),
            max_connections: num_cpus::get() as u32 * 4,
            min_connections: num_cpus::get() as u32,
            connection_timeout_secs: 30,
            idle_timeout_secs: 600,
        }
    }
}

// 创建连接池
pub async fn create_pool(config: &DatabaseConfig) -> Result<sqlx::PgPool, sqlx::Error> {
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(config.max_connections)
        .min_connections(config.min_connections)
        .connect_timeout(std::time::Duration::from_secs(config.connection_timeout_secs))
        .idle_timeout(std::time::Duration::from_secs(config.idle_timeout_secs))
        .connect(&config.url)
        .await
}
```

---

## 六、Web 开发规范

### 6.1 Axum 路由开发

**路由处理器签名规范**：

```rust
// 错误：处理器签名不匹配
async fn create_room(
    Path(room_id): Path<String>,
    Json(body): Json<CreateRoomRequest>,
) -> Result<Json<RoomIdResponse>, ApiError> {
    // 错误：缺少必要的提取器
}

// 正确：包含所有必要的状态和认证
async fn create_room(
    State(state): State<Arc<AppState>>,
    ExtractAuth(auth): ExtractAuth,
    Json(body): Json<CreateRoomRequest>,
) -> Result<Json<RoomIdResponse>, ApiError> {
    let service = &state.services.room_service;
    let room_id = service.create_room(&auth.user_id, &body).await?;
    Ok(Json(RoomIdResponse { room_id }))
}

// 推荐：添加 debug_handler 宏以获得更好的错误信息
#[axum::debug_handler]
async fn create_room(
    State(state): State<Arc<AppState>>,
    ExtractAuth(auth): ExtractAuth,
    Json(body): Json<CreateRoomRequest>,
) -> Result<Json<RoomIdResponse>, ApiError> {
    // 实现
}
```

### 6.2 状态管理

**AppState 定义**：

```rust
#[derive(Clone)]
pub struct AppState {
    pub services: ServiceContainer,
    pub cache: Arc<CacheManager>,
}

impl AppState {
    pub fn new(services: ServiceContainer, cache: Arc<CacheManager>) -> Self {
        Self { services, cache }
    }
}

// 路由器创建工厂
pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/_matrix/client/versions", get(get_client_versions))
        .route("/_matrix/client/r0/register", post(register))
        .route("/_matrix/client/r0/login", post(login))
        // ... 其他路由
        .with_state(state)  // 一次性注入状态
}

// 服务器组装
pub async fn new(
    database_url: &str,
    server_name: &str,
    jwt_secret: &str,
    address: SocketAddr,
) -> Result<Self, Box<dyn std::error::Error>> {
    let pool = sqlx::PgPool::connect(database_url).await?;
    initialize_database(&pool).await?;
    let pool = Arc::new(pool);

    let cache = Arc::new(CacheManager::new(CacheConfig::default()));
    let services = ServiceContainer::new(&pool, cache.clone(), jwt_secret, server_name);
    let app_state = Arc::new(AppState::new(services, cache.clone()));

    let client_routes = create_router(app_state.clone());
    let admin_routes = create_admin_router(app_state.clone());
    let media_routes = create_media_router(app_state.clone(), media_path.clone());
    let federation_routes = create_federation_router(app_state.clone());

    let router = Router::new()
        .merge(client_routes)
        .merge(admin_routes)
        .merge(media_routes)
        .merge(federation_routes)
        // 全局中间件
        .layer(CorsLayer::new())
        .layer(TraceLayer::new_for_http());

    Ok(Self {
        app_state,
        router,
        address,
    })
}
```

### 6.3 CORS 配置

```rust
use tower_http::cors::{Any, CorsLayer};

pub fn create_cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(Any)  // 非生产环境可使用 Any
        .allow_methods(Any)  // 或显式指定: vec![Method::GET, Method::POST]
        .allow_headers(Any)  // 或显式指定: vec![HeaderName::ACCEPT, HeaderName::AUTHORIZATION]
        .allow_credentials(false)  // 启用 credentials 时不能使用 Any
}
```

### 6.4 认证提取器

```rust
#[derive(Debug)]
pub struct AuthContext {
    pub user_id: String,
    pub device_id: Option<String>,
    pub is_admin: bool,
}

pub struct ExtractAuth(pub AuthContext);

#[axum::async_trait]
impl<S> FromRequestParts<S> for ExtractAuth
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        let state = parts
            .extensions
            .get::<Arc<AppState>>()
            .ok_or_else(|| ApiError::internal("Missing app state"))?;

        let auth_header = parts
            .headers
            .get(http::header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            .and_then(|h| h.strip_prefix("Bearer "))
            .ok_or_else(|| ApiError::unauthorized())?;

        let claims = state
            .services
            .auth_service
            .validate_token(auth_header)
            .await
            .map_err(|_| ApiError::unauthorized())?;

        Ok(ExtractAuth(AuthContext {
            user_id: claims.user_id,
            device_id: claims.device_id,
            is_admin: claims.admin,
        }))
    }
}
```

---

## 八、统一配置规范

### 8.1 路由认证中间件配置

**统一认证逻辑**：所有需要身份验证的路由必须使用统一的认证中间件 `AuthenticatedUser` 提取器。

```rust
// 统一认证用户结构
#[derive(Clone)]
pub struct AuthenticatedUser {
    pub user_id: String,
    pub device_id: Option<String>,
    pub is_admin: bool,
    pub access_token: String,
}

// 统一认证提取器实现
impl<S> FromRequestParts<S> for AuthenticatedUser
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let token = extract_token_from_headers(&parts.headers)?;

        let app_state = parts
            .extensions
            .get::<AppState>()
            .ok_or_else(|| ApiError::internal("Missing app state".to_string()))?;

        let (user_id, device_id, is_admin) = app_state
            .services
            .auth_service
            .validate_token(&token)
            .await?;

        Ok(AuthenticatedUser {
            user_id,
            device_id,
            is_admin,
            access_token: token,
        })
    }
}

// 路由处理器使用统一认证
async fn create_room(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let room_service = RoomService::new(&state.services);
    Ok(Json(
        room_service
            .create_room(&auth_user.user_id, &body)
            .await?,
    ))
}
```

**认证配置规范**：

| 配置项 | 用途 | 参数范围 | 默认值 |
|--------|------|----------|--------|
| token_expiry | 访问令牌过期时间（秒） | 3600-86400 | 86400 |
| refresh_token_expiry | 刷新令牌过期时间（秒） | 604800-2592000 | 604800 |
| max_devices_per_user | 每用户最大设备数 | 1-10 | 5 |
| password_min_length | 密码最小长度 | 8-32 | 8 |
| require_strong_password | 是否要求强密码 | true/false | true |

**认证与密码安全要求**：

1. 密码哈希必须使用 Argon2id 或 bcrypt，并允许配置成本参数  
2. 禁止自定义弱哈希算法作为存储方案  
3. 密码与 Token 相关逻辑必须有针对性测试覆盖  
4. 管理员注册通道必须保证参数语义与实际行为一致  
5. 用户名校验规则需与 Matrix 规范对齐

### 8.2 请求处理配置

**统一请求验证**：

```rust
// 请求验证中间件
pub struct RequestValidator {
    max_body_size: usize,
    allowed_content_types: Vec<String>,
}

impl RequestValidator {
    pub fn new() -> Self {
        Self {
            max_body_size: 10 * 1024 * 1024, // 10MB
            allowed_content_types: vec![
                "application/json".to_string(),
                "multipart/form-data".to_string(),
            ],
        }
    }

    pub fn validate(&self, content_type: Option<&str>, body_size: usize) -> Result<(), ApiError> {
        if let Some(ct) = content_type {
            if !self.allowed_content_types.iter().any(|allowed| ct.contains(allowed)) {
                return Err(ApiError::bad_request("Unsupported content type".to_string()));
            }
        }

        if body_size > self.max_body_size {
            return Err(ApiError::bad_request("Request body too large".to_string()));
        }

        Ok(())
    }
}
```

**请求处理配置**：

| 配置项 | 用途 | 参数范围 | 默认值 |
|--------|------|----------|--------|
| max_body_size | 最大请求体大小（字节） | 1MB-50MB | 10MB |
| request_timeout | 请求超时时间（秒） | 30-300 | 60 |
| rate_limit_requests | 速率限制请求数 | 10-1000 | 100 |
| rate_limit_window | 速率限制时间窗口（秒） | 60-3600 | 60 |

**限流降级策略**：

1. 缓存或 Redis 不可用时必须遵循可配置策略  
2. 默认策略为 fail‑closed，并记录告警日志  
3. 必须提供全局与端点级限流指标

### 8.3 错误响应配置

**统一错误响应格式**：

```rust
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, error_code, message) = match &self {
            ApiError::Unauthorized { reason } => {
                (StatusCode::UNAUTHORIZED, "M_UNAUTHORIZED", reason)
            }
            ApiError::NotFound { resource } => {
                (StatusCode::NOT_FOUND, "M_NOT_FOUND", resource)
            }
            ApiError::BadRequest { message } => {
                (StatusCode::BAD_REQUEST, "M_BAD_JSON", message)
            }
            ApiError::Internal { details } => {
                (StatusCode::INTERNAL_SERVER_ERROR, "M_UNKNOWN", details)
            }
            ApiError::Forbidden => {
                (StatusCode::FORBIDDEN, "M_FORBIDDEN", "Access denied")
            }
        };

        let body = json!({
            "errcode": error_code,
            "error": message
        });

        (status, Json(body)).into_response()
    }
}
```

**错误响应配置**：

| 错误类型 | HTTP状态码 | Matrix错误码 | 描述 |
|----------|------------|-------------|------|
| Unauthorized | 401 | M_UNAUTHORIZED | 认证失败 |
| Forbidden | 403 | M_FORBIDDEN | 权限不足 |
| NotFound | 404 | M_NOT_FOUND | 资源未找到 |
| BadRequest | 400 | M_BAD_JSON | 请求格式错误 |
| Internal | 500 | M_UNKNOWN | 服务器内部错误 |

### 8.4 日志记录配置

**统一日志格式**：

```rust
use tracing::{info, warn, error, debug};

// 日志配置
pub fn init_tracing() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .init();
}

// 日志使用示例
async fn create_room(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    info!(
        user_id = %auth_user.user_id,
        device_id = ?auth_user.device_id,
        "Creating room"
    );

    match RoomService::new(&state.services).create_room(&auth_user.user_id, &body).await {
        Ok(result) => {
            info!(
                room_id = %result["room_id"],
                "Room created successfully"
            );
            Ok(Json(result))
        }
        Err(e) => {
            error!(
                user_id = %auth_user.user_id,
                error = %e,
                "Failed to create room"
            );
            Err(e)
        }
    }
}
```

**日志配置规范**：

| 配置项 | 用途 | 参数范围 | 默认值 |
|--------|------|----------|--------|
| log_level | 日志级别 | trace/debug/info/warn/error | info |
| log_format | 日志格式 | json/pretty | json |
| log_file | 日志文件路径 | - | stdout |
| log_rotation | 日志轮转策略 | daily/weekly/never | daily |
| log_retention | 日志保留天数 | 1-90 | 30 |

**可观测性要求**：

1. 注册、登录、同步、联邦为必备指标  
2. 关键路径必须记录耗时与错误码  
3. 生产环境必须可启用结构化日志

### 8.5 跨域设置（CORS）配置

**统一CORS配置**：

```rust
use tower_http::cors::{Any, CorsLayer};
use tower_http::cors::CorsLayer;

pub fn create_cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(Any)  // 生产环境应指定具体域名
        .allow_methods(Any)
        .allow_headers(Any)
        .allow_credentials(false)
        .max_age(std::time::Duration::from_secs(86400))
}

// 生产环境CORS配置
pub fn create_production_cors_layer(allowed_origins: Vec<String>) -> CorsLayer {
    let origins: Vec<HeaderValue> = allowed_origins
        .iter()
        .filter_map(|origin| HeaderValue::from_str(origin).ok())
        .collect();

    CorsLayer::new()
        .allow_origin(origins)
        .allow_methods(vec![Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers(vec![
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            header::ACCEPT,
        ])
        .allow_credentials(true)
        .max_age(std::time::Duration::from_secs(3600))
}
```

**CORS配置规范**：

| 配置项 | 用途 | 参数范围 | 默认值 |
|--------|------|----------|--------|
| allowed_origins | 允许的源域名 | - | * (所有) |
| allowed_methods | 允许的HTTP方法 | GET/POST/PUT/DELETE/... | * (所有) |
| allowed_headers | 允许的请求头 | - | * (所有) |
| allow_credentials | 是否允许携带凭证 | true/false | false |
| max_age | 预检请求缓存时间（秒） | 0-86400 | 86400 |

**生产环境强制要求**：

1. `allowed_origins` 不能为 `*`  
2. 若 `allow_credentials=true`，必须显式列出 `allowed_origins`  
3. 仅允许必要的 HTTP 方法与请求头

### 8.6 配置验证与测试

**配置验证函数**：

```rust
pub struct ConfigValidator;

impl ConfigValidator {
    pub fn validate_auth_config(config: &AuthConfig) -> Result<(), ConfigError> {
        if config.token_expiry < 3600 || config.token_expiry > 86400 {
            return Err(ConfigError::InvalidRange(
                "token_expiry".to_string(),
                3600,
                86400,
            ));
        }

        if config.password_min_length < 8 || config.password_min_length > 32 {
            return Err(ConfigError::InvalidRange(
                "password_min_length".to_string(),
                8,
                32,
            ));
        }

        Ok(())
    }

    pub fn validate_request_config(config: &RequestConfig) -> Result<(), ConfigError> {
        if config.max_body_size < 1024 * 1024 || config.max_body_size > 50 * 1024 * 1024 {
            return Err(ConfigError::InvalidRange(
                "max_body_size".to_string(),
                1024 * 1024,
                50 * 1024 * 1024,
            ));
        }

        Ok(())
    }
}
```

**配置测试用例**：

```rust
#[cfg(test)]
mod config_tests {
    use super::*;

    #[test]
    fn test_valid_auth_config() {
        let config = AuthConfig {
            token_expiry: 86400,
            password_min_length: 8,
            ..Default::default()
        };

        assert!(ConfigValidator::validate_auth_config(&config).is_ok());
    }

    #[test]
    fn test_invalid_token_expiry() {
        let config = AuthConfig {
            token_expiry: 100,  // 小于最小值
            ..Default::default()
        };

        let result = ConfigValidator::validate_auth_config(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_production_cors_config() {
        let cors_layer = create_production_cors_layer(vec![
            "https://example.com".to_string(),
            "https://app.example.com".to_string(),
        ]);

        // 验证CORS配置正确创建
        assert!(true);
    }
}
```

### 8.7 环境一致性保证

**开发环境配置**：

```yaml
# config.dev.yaml
database:
  url: "postgres://synapse:synapse@localhost:5432/synapse_dev"
  max_connections: 10

cache:
  redis_url: "redis://localhost:6379"
  local_max_capacity: 1000

cors:
  allowed_origins: ["*"]
  allow_credentials: false

logging:
  level: "debug"
  format: "pretty"
```

**生产环境配置**：

```yaml
# config.prod.yaml
database:
  url: "${DATABASE_URL}"
  max_connections: 100

cache:
  redis_url: "${REDIS_URL}"
  local_max_capacity: 10000

cors:
  allowed_origins: ["${ALLOWED_ORIGINS}"]
  allow_credentials: true

logging:
  level: "info"
  format: "json"
  file: "/var/log/synapse-rust/app.log"
```

**配置加载与验证**：

```rust
use config::{Config, Environment, File};

pub fn load_config() -> Result<AppConfig, ConfigError> {
    let env = std::env::var("ENV").unwrap_or_else(|_| "dev".to_string());

    let config = Config::builder()
        .add_source(File::with_name(&format!("config.{}", env)))
        .add_source(Environment::with_prefix("SYNAPSE").separator("__"))
        .build()?;

    let config: AppConfig = config.try_deserialize()?;

    ConfigValidator::validate_auth_config(&config.auth)?;
    ConfigValidator::validate_request_config(&config.request)?;

    Ok(config)
}
```

---

## 九、错误案例分析与预防策略

### 7.1 Rust 版本不兼容错误

**错误类型**：`feature 'edition2024' is required`

**原因分析**：依赖库（如 base64ct）使用了较新的 Rust 特性，而项目使用的 Rust 1.75.0 不支持 edition2024

**预防策略**：
1. 在 `rust-toolchain.toml` 中明确指定 Rust 版本
2. 使用 `cargo update` 后检查 `Cargo.lock` 的变更
3. 定期运行 `cargo update` 并审查依赖变更
4. 在 CI/CD 中添加版本验证步骤

**修复验证**：

```bash
# 检查当前工具链
rustc --version

# 升级到兼容版本
rustup install 1.93.0
rustup default 1.93.0

# 重新编译
cargo clean
cargo build --release
```

### 7.2 SQL 语法错误

**错误类型**：`column "created_at" of relation "access_tokens" does not exist`

**原因分析**：代码中使用的列名与数据库实际列名不一致

**预防策略**：
1. 建立统一的列名命名规范（使用 `_ts` 后缀表示时间戳）
2. 迁移文件必须经过双重审查
3. 使用 SQLx 的编译时检查功能
4. 在开发环境运行 `rm -rf .sqlx` 清除缓存

**正确示例**：

```sql
-- 迁移文件
CREATE TABLE access_tokens (
    id BIGSERIAL PRIMARY KEY,
    token VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255),
    created_ts BIGINT NOT NULL,      -- 使用 _ts 后缀
    expires_ts BIGINT,               -- 使用 _ts 后缀
    invalidated_ts BIGINT            -- 使用 _ts 后缀
);

-- Rust 结构体对应
pub struct AccessToken {
    pub id: i64,
    pub token: String,
    pub user_id: String,
    pub device_id: Option<String>,
    pub created_ts: i64,
    pub expires_ts: Option<i64>,
    pub invalidated_ts: Option<i64>,
}
```

### 7.3 类型不匹配错误

**错误类型**：`expected 'String', found 'Option<String>'`

**原因分析**：数据库字段允许 NULL，但 Rust 代码中定义为非 Option 类型

**预防策略**：
1. 所有数据库可空字段必须在 Rust 中使用 `Option<T>`
2. 在创建结构体时标注所有可选字段
3. 使用 SQLx 的 `query_as!` 宏时仔细检查类型映射
4. 编写数据库操作函数时，显式处理 NULL 情况

**正确示例**：

```rust
pub struct AccessToken {
    pub id: i64,
    pub token: String,
    pub user_id: String,
    pub device_id: Option<String>,  // 明确标注为 Option
    pub created_ts: i64,
    pub expires_ts: Option<i64>,
    pub invalidated_ts: Option<i64>,
}

pub async fn create_token(
    &self,
    token: &str,
    user_id: &str,
    device_id: Option<&str>,  // 参数也使用 Option
    expires_ts: Option<i64>,
) -> Result<AccessToken, sqlx::Error> {
    let row = sqlx::query!(
        r#"
        INSERT INTO access_tokens (token, user_id, device_id, created_ts, expires_ts)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, token, user_id, device_id, created_ts, expires_ts, invalidated_ts
        "#,
        token,
        user_id,
        device_id,  // SQLx 会自动处理 None -> NULL
        now,
        expires_ts
    )
    .fetch_one(&*self.pool)
    .await?;
    
    Ok(AccessToken {
        id: row.id,
        token: row.token,
        user_id: row.user_id,
        device_id: row.device_id,  // 返回 Option<String>
        created_ts: row.created_ts,
        expires_ts: row.expires_ts,
        invalidated_ts: row.invalidated_ts,
    })
}
```

### 7.4 Axum 路由处理器错误

**错误类型**：`the trait bound '{closure}: Handler<_, _>' is not satisfied`

**原因分析**：路由处理器函数签名与 Axum 期望不匹配

**预防策略**：
1. 为所有路由处理器添加 `#[axum::debug_handler]` 属性
2. 统一使用标准提取器：State、Json、Path、Query、Header
3. 避免在处理器内部定义复杂的闭包
4. 保持处理器函数签名简洁，参数不超过 5 个

**正确示例**：

```rust
#[axum::debug_handler]
async fn register(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RegisterRequest>,
) -> Result<Json<RegisterResponse>, ApiError> {
    let result = state.services.auth_service.register(&req).await?;
    Ok(Json(result))
}

async fn login(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, ApiError> {
    let result = state.services.auth_service.login(&req).await?;
    Ok(Json(result))
}
```

### 7.5 密码学库 API 变更

**错误类型**：`could not find 'StaticSecret' in 'x25519_dalek'`、`unresolved import 'ed25519_dalek::Keypair'`

**原因分析**：密码学库在主版本升级时发生了 API 变更

**预防策略**：
1. 锁定密码学库的具体版本，避免自动更新
2. 在依赖更新前仔细阅读 CHANGELOG
3. 为密码学库编写专门的兼容性测试
4. 考虑使用抽象层隔离第三方库变更

**正确配置**：

```toml
[dependencies]
# ed25519-dalek v2.x API
ed25519-dalek = { version = "2.1", features = ["rand_core", "pkcs8", "std"] }

# x25519-dalek 需要启用 static_secrets 特性
x25519-dalek = { version = "2.0", features = ["static_secrets", "serde"] }

# argon2 保持稳定版本
argon2 = "0.5"
```

### 7.6 数据库缓存不一致

**错误类型**：SQLx 编译时检查与运行时数据库结构不匹配

**原因分析**：修改数据库结构后未清除 SQLx 缓存

**预防策略**：
1. 数据库结构变更后立即执行 `rm -rf .sqlx`
2. 使用 `DATABASE_URL` 环境变量确保正确的数据库连接
3. 在 CI/CD 中添加缓存清理步骤
4. 迁移脚本与代码同步提交

**标准流程**：

```bash
# 1. 备份当前数据库
pg_dump -U synapse synapse_db > backup_$(date +%Y%m%d).sql

# 2. 清理 SQLx 缓存
rm -rf .sqlx

# 3. 执行迁移
sqlx migrate run

# 4. 重新编译
cargo build --release

# 5. 运行测试验证
cargo test
```

---

## 八、架构设计原则

### 8.1 分层架构

系统采用清晰的分层架构，从下到上依次为：

1. **数据持久层**：负责所有数据库操作，使用 SQLx 进行类型安全的 SQL 查询
2. **缓存层**：提供两级缓存（本地 Moka + 分布式 Redis），提升访问性能
3. **业务逻辑层**：封装业务逻辑，处理认证、授权、数据验证等
4. **Web 表现层**：使用 Axum 框架处理 HTTP 请求，定义路由和中间件

各层之间通过明确定义的接口进行通信，层与层之间的依赖关系严格遵循自上而下的方向，避免循环依赖。

### 8.2 依赖注入

使用构造函数注入依赖，避免全局状态：

```rust
pub struct AuthService {
    user_storage: UserStorage<'static>,
    device_storage: DeviceStorage<'static>,
    token_storage: TokenStorage<'static>,
    cache: Arc<CacheManager>,
    jwt_secret: Vec<u8>,
}

impl AuthService {
    pub fn new(
        user_storage: UserStorage<'static>,
        device_storage: DeviceStorage<'static>,
        token_storage: TokenStorage<'static>,
        cache: Arc<CacheManager>,
        jwt_secret: Vec<u8>,
    ) -> Self {
        Self {
            user_storage,
            device_storage,
            token_storage,
            cache,
            jwt_secret,
        }
    }
}
```

### 8.3 模块化

- 每个模块职责单一
- 模块间通过 trait 定义接口
- 使用 `pub use` 导出公共接口
- 私有实现细节隐藏在模块内部

---

## 九、安全规范

### 9.1 认证安全

- 用户密码使用 argon2 算法哈希存储，算法参数设置为安全等级 3
- JWT 使用 HS256 算法签名，密钥长度不少于 256 位
- 访问令牌有效期为 24 小时，刷新令牌有效期为 7 天
- 令牌验证结果缓存 5 分钟，平衡安全性和性能

### 9.2 传输安全

- 所有 API 强制使用 HTTPS 连接，禁止 HTTP 传输
- 敏感数据（如密码、令牌）在客户端使用 TLS 1.3 加密传输
- 服务器配置支持 HSTS 响应头，强制浏览器使用 HTTPS

### 9.3 数据安全

- 数据库连接使用 SSL，连接凭证从环境变量读取
- 敏感数据（如密码哈希）不记录日志
- 用户密码永不以明文形式存储或传输
- 实现防重放攻击机制，请求包含时间戳和随机数

### 9.4 输入验证

- 所有用户输入必须进行验证，包括类型检查、长度限制、格式验证
- 使用正则表达式验证特定格式的输入，如邮箱、URL 等
- 对特殊字符进行转义或过滤，防止注入攻击
- SQLx 查询使用参数化查询，防止 SQL 注入

### 9.5 审计日志

- 记录所有安全相关操作的审计日志
- 日志内容包含时间戳、用户 ID、操作类型、IP 地址、操作结果等
- 审计日志独立存储，防止被篡改

---

## 十、性能优化指南

### 10.1 缓存策略

采用两级缓存架构，平衡访问延迟和内存占用：

- **本地缓存**：使用 Moka，提供最快的访问速度
- **分布式缓存**：使用 Redis，支持多实例共享
- **缓存键设计**：`{prefix}:{entity}:{id}`
- **缓存过期**：用户配置缓存 5 分钟，房间配置缓存 10 分钟，事件列表缓存 1 分钟
- **缓存失效**：使用 Redis Pub/Sub 实现缓存失效的广播机制

### 10.2 数据库优化

- 使用连接池管理数据库连接，默认大小为 CPU 核心数乘以 4
- 连接池支持预热功能，启动时预先建立 min_size 个连接
- 使用索引优化查询性能
- 批量操作减少数据库往返次数
- 使用事务保证数据一致性

### 10.3 异步处理

- 所有 I/O 操作使用异步方式，不阻塞 Tokio 线程
- 使用合适的并发度控制，避免过多并发请求导致数据库过载
- 对于重量级操作（如媒体处理），使用后台任务队列异步执行
- 使用 `tokio::spawn` 并行执行独立任务

### 10.4 内存管理

- 使用 `Box` 处理大对象，避免栈溢出
- 使用 `Arc` 共享不可变数据，减少内存复制
- 及时释放不再使用的资源
- 使用 `Vec::with_capacity` 预分配容量，减少重新分配

---

## 十一、测试规范

### 11.1 测试分类

| 级别 | 覆盖率目标 | 说明 |
|------|-----------|------|
| 单元测试 | 80% | 测试单个函数或模块 |
| 集成测试 | 60% | 测试模块间交互 |
| API 测试 | 100% | 测试所有 API 端点 |

### 11.2 测试配置

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    async fn setup_test_db() -> PgPool {
        let config = DatabaseConfig::default();
        create_pool(&config).await.expect("Failed to create test pool")
    }

    #[tokio::test]
    async fn test_user_registration() {
        let pool = setup_test_db().await;
        let storage = UserStorage::new(&pool);
        
        let result = storage.create_user("test_user", "password123").await;
        assert!(result.is_ok());
    }
}
```

### 11.3 测试覆盖率

- 核心业务逻辑的单元测试覆盖率不低于 80%
- 数据访问层的单元测试覆盖率不低于 90%
- 工具函数和辅助功能的单元测试覆盖率不低于 70%
- 每个 API 端点必须有对应的集成测试

---

## 十二、部署规范

### 12.1 环境配置

```yaml
# config.yaml
server:
  name: "localhost"
  host: "0.0.0.0"
  port: 8008

database:
  url: "postgres://synapse_user:synapse_password@localhost:5432/synapse_db"
  pool_size: 10

cache:
  redis_url: "redis://localhost:6379"
  local_max_capacity: 10000

jwt:
  secret: "${JWT_SECRET}"
  expiry: 86400
```

### 12.2 Docker 部署

```dockerfile
FROM rust:1.93 AS builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
COPY --from=builder /app/target/release/synapse-rust /usr/local/bin/
COPY config.yaml /etc/synapse/config.yaml
EXPOSE 8008
CMD ["synapse-rust"]
```

---

## 十三、版本控制与兼容性测试

### 13.1 分支策略

```
main          - 生产环境代码
develop       - 开发环境代码
feature/*     - 功能分支
hotfix/*      - 紧急修复分支
release/*     - 发布分支
```

### 13.2 提交规范

```
<type>(<scope>): <subject>

<body>

<footer>
```

**类型**：
- `feat`: 新功能
- `fix`: Bug 修复
- `docs`: 文档更新
- `style`: 代码格式调整
- `refactor`: 重构
- `perf`: 性能优化
- `test`: 测试相关
- `chore`: 构建或辅助工具变动

### 13.3 兼容性测试

```bash
# 运行兼容性测试套件
cargo test --all-features

# 运行 API 集成测试
cargo test --test api_integration

# 运行数据库迁移测试
cargo test --test db_migrations
```

### 13.4 变更日志

使用 Conventional Commits 自动生成变更日志：

```bash
# 安装 conventional-changelog-cli
npm install -g conventional-changelog-cli

# 生成变更日志
conventional-changelog -p angular -i CHANGELOG.md -s
```

---

## 十四、参考资料

### 14.1 官方文档

- [Synapse 官方文档](https://element-hq.github.io/synapse/latest/)
- [Matrix 规范](https://spec.matrix.org/)
- [Axum 框架文档](https://docs.rs/axum/latest/axum/)
- [SQLx 文档](https://docs.rs/sqlx/latest/sqlx/)
- [Rust 1.93.0 发布说明](https://blog.rust-lang.org/2024/11/28/Rust-1.93.0.html)

### 14.2 Rust 最佳实践

- [Rust 官方文档](https://doc.rust-lang.org/)
- [Rust 异步编程](https://rust-lang.github.io/async-book/)
- [Rust 内存安全](https://doc.rust-lang.org/book/ch10-00-lifetimes.html)
- [Rust 编码规范](https://rust-lang.github.io/api-guidelines/)

### 14.3 安全相关

- [OWASP Web 应用安全指南](https://owasp.org/www-project-web-security-testing-guide/)
- [Rust 安全公告](https://groups.google.com/forum/#!forum/rust-security-announcements)

---

## 十五、变更日志

| 版本 | 日期 | 变更说明 |
|------|------|----------|
| 1.0.0 | 2026-01-28 | 初始版本，定义项目规则和开发规范 |
| 2.0.0 | 2026-01-29 | 重大更新，添加 Rust 版本规范、依赖管理策略、错误案例分析与预防策略 |
| 3.0.0 | 2026-01-29 | 新增统一配置规范，包括路由认证中间件、请求处理、错误响应、日志记录、CORS配置等标准化配置项 |
