# Synapse Rust 项目重构开发实施方案

> **版本**：1.0.0  
> **创建日期**：2026-01-28  
> **项目状态**：开发中  
> **参考文档**：[Synapse 官方文档](https://element-hq.github.io/synapse/latest/)、[Matrix 规范](https://spec.matrix.org/)、[Rust 高级编程指南](https://www.hackerrank.com/skills-directory/rust_advanced)

---

## 一、实施方案概述

### 1.1 实施目标

本实施方案旨在指导 Synapse Rust 项目的完整重构开发，确保：
- 完整实现 Matrix 协议核心功能
- 完整实现 Enhanced API 功能（好友、私聊、语音、安全）
- 代码质量达到预定标准
- 测试覆盖率达到 80% 以上
- API 兼容性 100%

### 1.2 实施原则

1. **分阶段实施**：将开发分为多个阶段，每个阶段有明确的目标和交付物
2. **质量优先**：每个阶段完成后进行严格的代码质量检查
3. **文档同步**：及时更新相关文档，标注完成状态
4. **测试驱动**：每个功能完成后立即编写测试用例
5. **持续集成**：确保代码始终可编译、可测试

### 1.3 参考文档

本实施方案参考以下技术文档：
- [api-reference.md](./api-reference.md) - API 参考文档
- [api-complete.md](./api-complete.md) - 完整 API 文档
- [architecture-design.md](./architecture-design.md) - 架构设计文档
- [module-structure.md](./module-structure.md) - 模块结构文档
- [data-models.md](./data-models.md) - 数据模型文档
- [error-handling.md](./error-handling.md) - 错误处理文档
- [implementation-guide.md](./implementation-guide.md) - 实现指南文档
- [migration-guide.md](./migration-guide.md) - 数据迁移指南
- [project-assessment-skillset.md](./project-assessment-skillset.md) - 项目评估技能集

---

## 二、开发实施路线图

### 2.1 总体路线图

```
┌─────────────────────────────────────────────────────────────────────────────────────────┐
│                         Synapse Rust 项目重构开发路线图                          │
├─────────────────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  阶段 1：项目初始化（第 1-2 周）                                        │
│  ├─ 创建项目目录结构                                                        │
│  ├─ 配置 Cargo.toml                                                         │
│  ├─ 初始化 Git 仓库                                                        │
│  ├─ 设置开发环境                                                             │
│  └─ 创建基础模块框架                                                         │
│                                                                             │
│  阶段 2：通用模块开发（第 3-4 周）                                        │
│  ├─ 错误处理模块                                                             │
│  ├─ 配置管理模块                                                             │
│  ├─ 加密工具模块                                                             │
│  └─ 单元测试                                                                 │
│                                                                             │
│  阶段 3：存储层开发（第 5-7 周）                                          │
│  ├─ 用户存储模块                                                             │
│  ├─ 设备存储模块                                                             │
│  ├─ 令牌存储模块                                                             │
│  ├─ 房间存储模块                                                             │
│  ├─ 事件存储模块                                                             │
│  ├─ 成员存储模块                                                             │
│  ├─ 在线存储模块                                                             │
│  └─ 单元测试                                                                 │
│                                                                             │
│  阶段 4：缓存层开发（第 8 周）                                            │
│  ├─ 缓存管理器                                                               │
│  ├─ 本地缓存（Moka）                                                        │
│  ├─ 分布式缓存（Redis）                                                      │
│  └─ 单元测试                                                                 │
│                                                                             │
│  阶段 5：认证模块开发（第 9 周）                                          │
│  ├─ 认证服务                                                               │
│  ├─ 用户注册                                                                 │
│  ├─ 用户登录                                                                 │
│  ├─ 令牌验证                                                                 │
│  └─ 单元测试                                                                 │
│                                                                             │
│  阶段 6：服务层开发（第 10-14 周）                                       │
│  ├─ 注册服务                                                               │
│  ├─ 房间服务                                                               │
│  ├─ 同步服务                                                               │
│  ├─ 媒体服务                                                               │
│  └─ 单元测试                                                                 │
│                                                                             │
│  阶段 7：E2EE 开发（第 15-18 周）                                          │
│  ├─ 设备密钥服务                                                           │
│  ├─ 跨签名密钥服务                                                         │
│  ├─ Megolm 加密服务                                                        │
│  ├─ 密钥备份服务                                                           │
│  └─ 单元测试                                                                 │
│                                                                             │
│  阶段 8：Enhanced API 开发（第 19-22 周）                                 │
│  ├─ 好友服务                                                               │
│  ├─ 私聊服务                                                               │
│  ├─ 语音服务                                                               │
│  └─ 单元测试                                                                 │
│                                                                             │
│  阶段 9：Web 层开发（第 23-25 周）                                        │
│  ├─ 路由定义                                                               │
│  ├─ 中间件实现                                                             │
│  ├─ 请求处理器                                                             │
│  └─ 单元测试                                                                 │
│                                                                             │
│  阶段 10：数据库迁移（第 26 周）                                           │
│  ├─ 迁移脚本开发                                                             │
│  ├─ 迁移工具实现                                                             │
│  └─ 集成测试                                                                 │
│                                                                             │
│  阶段 11：集成测试与优化（第 27-28 周）                                   │
│  ├─ 集成测试                                                               │
│  ├─ 性能测试                                                               │
│  ├─ 代码优化                                                               │
│  └─ 文档完善                                                                 │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────────────────┘
```

### 2.2 里程碑

| 里程碑 | 完成时间 | 交付物 | 状态 |
|--------|---------|--------|------|
| M1：项目初始化完成 | 第 2 周 | 项目目录结构、基础模块框架 | 📝 待完成 |
| M2：通用模块完成 | 第 4 周 | 错误处理、配置管理、加密工具 | 📝 待完成 |
| M3：存储层完成 | 第 7 周 | 所有存储模块、单元测试 | 📝 待完成 |
| M4：缓存层完成 | 第 8 周 | 缓存管理器、两级缓存 | 📝 待完成 |
| M5：认证模块完成 | 第 9 周 | 认证服务、注册登录 | 📝 待完成 |
| M6：核心服务完成 | 第 14 周 | 注册、房间、同步、媒体服务 | 📝 待完成 |
| M7：E2EE 完成 | 第 18 周 | 设备密钥、跨签名、Megolm、备份服务 | 📝 待完成 |
| M8：Enhanced API 完成 | 第 22 周 | 好友、私聊、语音服务 | 📝 待完成 |
| M9：Web 层完成 | 第 25 周 | 所有路由、中间件、处理器 | 📝 待完成 |
| M10：数据库迁移完成 | 第 26 周 | 迁移脚本、迁移工具 | 📝 待完成 |
| M11：项目交付 | 第 28 周 | 完整项目、测试报告、文档 | 📝 待完成 |

---

## 三、阶段 1：项目初始化（第 1-2 周）

### 3.1 阶段目标

创建项目基础架构，包括目录结构、配置文件、依赖管理等。

### 3.2 参考文档

- [architecture-design.md](./architecture-design.md) - 架构设计文档
- [module-structure.md](./module-structure.md) - 模块结构文档

### 3.3 任务清单

#### 任务 1.1：创建项目目录结构

**目标**：创建完整的项目目录结构

**步骤**：
1. 创建 `src/` 目录
2. 创建 `src/common/` 目录
3. 创建 `src/storage/` 目录
4. 创建 `src/cache/` 目录
5. 创建 `src/auth/` 目录
6. 创建 `src/services/` 目录
7. 创建 `src/web/` 目录
8. 创建 `tests/` 目录
9. 创建 `migrations/` 目录
10. 创建 `docs/` 目录

**命令**：
```bash
cd /home/hula/synapse_rust
mkdir -p src/{common,storage,cache,auth,services,web/{routes,middleware,handlers}}
mkdir -p tests
mkdir -p migrations
mkdir -p docs/synapse-rust
```

**验收标准**：
- ✅ 所有目录创建成功
- ✅ 目录结构符合 [module-structure.md](./module-structure.md) 规范

**状态**：✅ 已完成  
**完成时间**：2026-01-28

**完成内容**：
- ✅ 项目目录结构创建
- ✅ Cargo.toml 配置（添加 license, repository, readme, keywords 字段）
- ✅ Git 仓库初始化（commit: e8c7659）
- ✅ 开发环境设置（PostgreSQL 和 Redis 容器已运行）
- ✅ 基础模块框架创建
- ✅ Rust 工具链升级至 1.93.0

**代码质量**：
- ⚠️ cargo check - 存在编译错误（278个），主要由于 Handler trait 和 Clone trait 未实现
- ⚠️ cargo clippy - 待运行
- ✅ cargo fmt - 代码格式正确

**测试覆盖率**：
- ⏳ 测试覆盖率：待测试

**已知问题**：
- 存储结构体缺少 Clone trait 实现
- ServiceContainer 生命周期配置需要调整
- 路由处理器需要实现 Handler trait

**后续修复计划**：
- 阶段 2：实现错误处理、配置管理、加密工具模块

---

### 附录 A：依赖版本记录

#### A.1 更新日志

| 日期 | 更新类型 | 描述 |
|------|----------|------|
| 2026-01-28 | 初始配置 | 项目初始化依赖配置 |
| 2026-01-28 | 工具链升级 | Rust 1.75.0 → 1.93.0 |
| 2026-01-28 | 依赖更新 | 核心依赖更新至最新稳定版本 |
| 2026-01-28 | 安全修复 | pyo3 0.22.6 → 0.24.2 (修复 RUSTSEC-2025-0020) |

#### A.2 当前依赖版本

| 依赖包 | 当前版本 | 最新版本 | 更新状态 |
|--------|----------|----------|----------|
| tokio | 1.49 | 1.49 | ✅ 最新 |
| axum | 0.8 | 0.8.8 | ✅ 兼容 |
| tower-http | 0.6 | 0.6.8 | ✅ 兼容 |
| hyper | 1 | 1.8 | ✅ 兼容 |
| sqlx | 0.8 | 0.8.6 | ✅ 最新 |
| deadpool | 0.12 | 0.12.x | ✅ 最新 |
| deadpool-postgres | 0.12 | 0.12.x | ✅ 最新 |
| redis | 0.26 | 0.26.1 | ✅ 最新 |
| moka | 0.12 | 0.12.13 | ✅ 最新 |
| serde | 1.0 | 1.0.x | ✅ 最新 |
| serde_json | 1.0 | 1.0.x | ✅ 最新 |
| serde_with | 3 | 3.x | ✅ 最新 |
| rand | 0.8 | 0.8.x | ✅ 最新 |
| sha2 | 0.10 | 0.10.x | ✅ 最新 |
| hmac | 0.12 | 0.12.x | ✅ 最新 |
| base64 | 0.22 | 0.22.x | ✅ 最新 |
| zeroize | 1 | 1.x | ✅ 最新 |
| config | 0.14 | 0.14.x | ✅ 最新 |
| jsonwebtoken | 9 | 9.x | ✅ 最新 |
| tracing | 0.1 | 0.1.x | ✅ 最新 |
| tracing-subscriber | 0.3 | 0.3.x | ✅ 最新 |
| chrono | 0.4 | 0.4.43 | ✅ 兼容 |
| reqwest | 0.12 | 0.12.x | ✅ 最新 |
| pyo3 | 0.24 | 0.27.x | ⚠️ 待更新 |

#### A.3 安全漏洞状态

| CVE ID | 严重程度 | 依赖包 | 状态 | 解决方案 |
|--------|----------|--------|------|----------|
| RUSTSEC-2025-0020 | 高 | pyo3 (< 0.24.1) | ✅ 已修复 | 升级到 0.24.2 |
| RUSTSEC-2023-0071 | 中 | rsa (传递依赖) | ⚠️ 无法修复 | sqlx 尚未修复 |

**注意**：rsa 漏洞来自 sqlx-mysql 的传递依赖，目前 sqlx 尚未发布包含修复版本的发布。

#### A.4 可用更新（未应用）

| 依赖包 | 当前版本 | 最新版本 | 未更新原因 |
|--------|----------|----------|------------|
| cargo-audit | 0.21.2 | 0.22.0 | 稳定版本已足够 |
| deadpool-postgres | 0.12.1 | 0.14.1 | 需测试兼容性 |
| jsonwebtoken | 9.3.1 | 10.3.0 | 有breaking changes |
| redis | 0.26.1 | 1.0.2 | 需测试兼容性 |
| reqwest | 0.12.28 | 0.13.1 | 需测试兼容性 |

#### A.5 依赖管理建议

1. **定期更新**：建议每月运行 `cargo update` 和 `cargo audit`
2. **安全检查**：在 CI/CD 中集成 `cargo audit`
3. **版本锁定**：生产环境使用 `Cargo.lock` 锁定版本
4. **兼容性测试**：更新依赖后运行完整测试套件
- 阶段 3：完善存储层实现，添加必要的 trait 实现

---

#### 任务 1.2：配置 Cargo.toml

**目标**：配置项目依赖和元数据

**步骤**：
1. 更新 `Cargo.toml` 依赖列表
2. 配置项目元数据
3. 配置特性标志
4. 配置开发依赖

**配置内容**：
```toml
[package]
name = "synapse-rust"
version = "0.1.0"
edition = "2021"
authors = ["Synapse Rust Team"]
description = "Matrix Homeserver implemented in Rust"
license = "Apache-2.0"
repository = "https://github.com/langkebo/synapse"
readme = "README.md"
keywords = ["matrix", "homeserver", "synapse", "rust", "federation"]

[features]
default = ["server"]
server = ["dep:axum", "dep:tower-http"]
python = ["dep:pyo3"]

[dependencies]
tokio = { version = "1.35", features = ["full"] }
async-trait = "0.1"
axum = { version = "0.7", optional = true }
tower-http = { version = "0.5", optional = true, features = ["fs", "cors", "trace"] }
hyper = "1.0"
sqlx = { version = "0.7", features = ["postgres", "runtime-tokio", "chrono", "json"] }
deadpool = "0.10"
deadpool-postgres = "0.10"
redis = { version = "0.26", features = ["aio", "tokio-comp"] }
moka = { version = "0.12", features = ["future", "sync"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
serde_with = "3.4"
rand = "0.8"
sha2 = "0.10"
hmac = "0.12"
base64 = "0.22"
zeroize = "1.7"
argon2 = "0.5"
thiserror = "1.0"
anyhow = "1.0"
config = "0.14"
dotenvy = "0.15"
jsonwebtoken = "9.0"
tracing = "0.1"
tracing-subscriber = "0.3"
tracing-appender = "0.2"
chrono = { version = "0.4", features = ["serde"] }
time = { version = "0.3", features = ["serde"] }
uuid = { version = "1.6", features = ["v4", "serde"] }
once_cell = "1.19"
futures = "0.3"
dashmap = { version = "6.0", features = ["serde"] }
parking_lot = "0.12"
lazy_static = "1.4"
reqwest = { version = "0.11", features = ["json"] }
pyo3 = { version = "0.20", optional = true, features = ["extension-module"] }

[dev-dependencies]
tokio-test = "0.4"
tarpaulin = "0.27"
cargo-llvm-cov = "0.5"

[profile.release]
opt-level = 3
lto = true
codegen-units = 1

[profile.dev]
opt-level = 0

[workspace]
members = []
```

**验收标准**：
- ✅ Cargo.toml 配置正确
- ✅ 所有依赖版本兼容
- ✅ 特性标志配置正确

**状态**：📝 待完成

---

#### 任务 1.3：初始化 Git 仓库

**目标**：初始化 Git 仓库并配置

**步骤**：
1. 初始化 Git 仓库
2. 创建 `.gitignore` 文件
3. 创建初始提交
4. 配置远程仓库

**.gitignore 内容**：
```
target/
Cargo.lock
.env
*.db
*.log
.DS_Store
.vscode/
.idea/
*.swp
*~
```

**命令**：
```bash
cd /home/hula/synapse_rust
git init
git add .
git commit -m "Initial commit: Project structure and Cargo.toml"
```

**验收标准**：
- ✅ Git 仓库初始化成功
- ✅ .gitignore 配置正确
- ✅ 初始提交成功

**状态**：📝 待完成

---

#### 任务 1.4：设置开发环境

**目标**：配置开发环境和工具

**步骤**：
1. 安装 Rust 工具链
2. 配置 PostgreSQL 数据库
3. 配置 Redis 缓存
4. 配置环境变量

**命令**：
```bash
# 安装 Rust 工具
rustup update stable
rustup component add clippy rustfmt
cargo install cargo-watch
cargo install cargo-tarpaulin
cargo install cargo-llvm-cov

# 配置 PostgreSQL
sudo apt install postgresql postgresql-contrib
sudo -u postgres psql -c "CREATE DATABASE synapse_db;"
sudo -u postgres psql -d synapse_db -f schema.sql

# 配置 Redis
sudo apt install redis-server
sudo systemctl start redis-server

# 配置环境变量
echo "DATABASE_URL=postgres://synapse_user:synapse_password@localhost/synapse_db" >> .env
echo "REDIS_URL=redis://localhost:6379" >> .env
echo "JWT_SECRET=$(openssl rand -hex 32)" >> .env
echo "SERVER_NAME=localhost" >> .env
```

**验收标准**：
- ✅ Rust 工具链安装完成
- ✅ PostgreSQL 数据库配置完成
- ✅ Redis 缓存配置完成
- ✅ 环境变量配置完成

**状态**：📝 待完成

---

#### 任务 1.5：创建基础模块框架

**目标**：创建所有模块的基础框架

**步骤**：
1. 创建 `src/lib.rs` 文件
2. 创建 `src/main.rs` 文件
3. 创建各模块的 `mod.rs` 文件
4. 创建各模块的基础结构体和 trait 定义

**src/lib.rs 内容**：
```rust
pub mod common;
pub mod storage;
pub mod cache;
pub mod auth;
pub mod services;
pub mod web;

pub use common::error::ApiError;
pub use common::config::Config;
```

**src/main.rs 内容**：
```rust
use synapse_rust::web::create_app;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();
    
    let app = create_app().await;
    
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8008").await.unwrap();
    tracing::info!("listening on {}", listener.local_addr().unwrap());
    
    axum::serve(listener, app).await.unwrap();
}
```

**验收标准**：
- ✅ 所有模块框架创建成功
- ✅ lib.rs 导出正确
- ✅ main.rs 基础结构正确

**状态**：📝 待完成

---

### 3.4 代码质量检查

**检查项**：
- ✅ `cargo check` - 编译检查
- ✅ `cargo clippy` - 代码检查
- ✅ `cargo fmt --check` - 格式检查

**命令**：
```bash
cd /home/hula/synapse_rust
cargo check
cargo clippy -- -D warnings
cargo fmt --check
```

**修复标准**：
- ✅ 所有编译错误修复
- ✅ 所有 clippy 警告修复
- ✅ 代码格式正确

**状态**：📝 待完成

---

### 3.5 测试用例

**测试项**：
- ✅ 项目目录结构测试
- ✅ Cargo.toml 配置测试
- ✅ 模块框架测试

**命令**：
```bash
cd /home/hula/synapse_rust
cargo test
```

**测试标准**：
- ✅ 所有测试通过
- ✅ 测试覆盖率达到 60%

**状态**：📝 待完成

---

### 3.6 文档更新

**更新文档**：
- ✅ [architecture-design.md](./architecture-design.md) - 标注阶段 1 完成
- ✅ [module-structure.md](./module-structure.md) - 标注阶段 1 完成
- ✅ [project-assessment-skillset.md](./project-assessment-skillset.md) - 更新项目重构进度

**更新内容**：
```markdown
## 阶段 1：项目初始化

**状态**：✅ 已完成  
**完成时间**：2026-01-28

**完成内容**：
- ✅ 项目目录结构创建
- ✅ Cargo.toml 配置
- ✅ Git 仓库初始化
- ✅ 开发环境设置
- ✅ 基础模块框架创建

**代码质量**：
- ✅ cargo check 通过
- ✅ cargo clippy 通过
- ✅ cargo fmt 通过

**测试覆盖率**：
- ✅ 测试覆盖率：60%
```

**状态**：📝 待完成

---

## 四、阶段 2：通用模块开发（第 3-4 周）

### 4.1 阶段目标

实现通用模块，包括错误处理、配置管理、加密工具。

### 4.2 参考文档

- [error-handling.md](./error-handling.md) - 错误处理文档
- [implementation-guide.md](./implementation-guide.md) - 实现指南文档

### 4.3 任务清单

#### 任务 2.1：错误处理模块

**目标**：实现统一的错误处理机制

**步骤**：
1. 创建 `src/common/error.rs` 文件
2. 定义 `ApiError` 枚举
3. 实现 `From` trait
4. 实现 `IntoResponse` trait

**src/common/error.rs 内容**：
```rust
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("Bad request: {0}")]
    BadRequest(String),
    
    #[error("Unauthorized")]
    Unauthorized,
    
    #[error("Forbidden")]
    Forbidden,
    
    #[error("Not found: {0}")]
    NotFound(String),
    
    #[error("Conflict: {0}")]
    Conflict(String),
    
    #[error("Rate limited")]
    RateLimited,
    
    #[error("Internal error: {0}")]
    Internal(String),
    
    #[error("Database error: {0}")]
    Database(String),
    
    #[error("Cache error: {0}")]
    Cache(String),
    
    #[error("Authentication error: {0}")]
    Authentication(String),
    
    #[error("Validation error: {0}")]
    Validation(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, errcode, error) = match self {
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "M_BAD_JSON", msg),
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, "M_UNAUTHORIZED", "Unauthorized"),
            ApiError::Forbidden => (StatusCode::FORBIDDEN, "M_FORBIDDEN", "Forbidden"),
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, "M_NOT_FOUND", msg),
            ApiError::Conflict(msg) => (StatusCode::CONFLICT, "M_USER_IN_USE", msg),
            ApiError::RateLimited => (StatusCode::TOO_MANY_REQUESTS, "M_LIMIT_EXCEEDED", "Rate limited"),
            ApiError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, "M_UNKNOWN", msg),
            ApiError::Database(msg) => (StatusCode::INTERNAL_SERVER_ERROR, "M_UNKNOWN", msg),
            ApiError::Cache(msg) => (StatusCode::INTERNAL_SERVER_ERROR, "M_UNKNOWN", msg),
            ApiError::Authentication(msg) => (StatusCode::UNAUTHORIZED, "M_UNKNOWN_TOKEN", msg),
            ApiError::Validation(msg) => (StatusCode::BAD_REQUEST, "M_INVALID_PARAM", msg),
        };
        
        let body = json!({
            "errcode": errcode,
            "error": error
        });
        
        (status, Json(body)).into_response()
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        ApiError::Database(err.to_string())
    }
}

impl From<redis::RedisError> for ApiError {
    fn from(err: redis::RedisError) -> Self {
        ApiError::Cache(err.to_string())
    }
}

impl From<jsonwebtoken::errors::Error> for ApiError {
    fn from(err: jsonwebtoken::errors::Error) -> Self {
        ApiError::Authentication(err.to_string())
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(err: serde_json::Error) -> Self {
        ApiError::BadRequest(err.to_string())
    }
}
```

**验收标准**：
- ✅ ApiError 枚举定义完整
- ✅ From trait 实现正确
- ✅ IntoResponse trait 实现正确
- ✅ 所有错误变体对应正确的 HTTP 状态码

**状态**：📝 待完成

---

#### 任务 2.2：配置管理模块

**目标**：实现配置管理功能

**步骤**：
1. 创建 `src/common/config.rs` 文件
2. 定义 `Config` 结构体
3. 实现 `Config::load()` 函数
4. 实现配置验证

**src/common/config.rs 内容**：
```rust
use config::{Config as ConfigLoader, Environment, File};
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub cache: CacheConfig,
    pub jwt: JwtConfig,
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    pub name: String,
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
    pub pool_size: u32,
}

#[derive(Debug, Deserialize)]
pub struct CacheConfig {
    pub redis_url: String,
    pub local_max_capacity: u64,
}

#[derive(Debug, Deserialize)]
pub struct JwtConfig {
    pub secret: String,
    pub access_token_expiry: i64,
    pub refresh_token_expiry: i64,
}

impl Config {
    pub async fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let config = ConfigLoader::builder()
            .add_source(Environment::default().separator("__"))
            .add_source(File::from(Path::new("config.toml")).required(false))
            .build()?;
        
        Ok(config)
    }
    
    pub fn validate(&self) -> Result<(), String> {
        if self.server.name.is_empty() {
            return Err("Server name cannot be empty".to_string());
        }
        
        if self.server.port == 0 {
            return Err("Server port cannot be 0".to_string());
        }
        
        if self.jwt.secret.len() < 32 {
            return Err("JWT secret must be at least 32 characters".to_string());
        }
        
        Ok(())
    }
}
```

**验收标准**：
- ✅ Config 结构体定义完整
- ✅ Config::load() 函数实现正确
- ✅ 配置验证逻辑正确
- ✅ 支持环境变量覆盖

**状态**：📝 待完成

---

#### 任务 2.3：加密工具模块

**目标**：实现加密和哈希工具

**步骤**：
1. 创建 `src/common/crypto.rs` 文件
2. 实现 `hash_password()` 函数
3. 实现 `verify_password()` 函数
4. 实现 `generate_token()` 函数
5. 实现 `generate_room_id()` 函数
6. 实现 `generate_event_id()` 函数

**src/common/crypto.rs 内容**：
```rust
use argon2::{
    password_hash::{rand_core::OsRng, Argon2, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Algorithm,
};
use rand::Rng;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn hash_password(password: &str) -> Result<String, String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::new(
        Algorithm::Argon2id,
        argon2::Params::default(),
    )
    .map_err(|e| format!("Failed to create Argon2: {}", e))?;
    
    let password_hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| format!("Failed to hash password: {}", e))?;
    
    Ok(password_hash.to_string())
}

pub fn verify_password(password: &str, hash: &str) -> Result<bool, String> {
    let parsed_hash = PasswordHash::new(hash)
        .map_err(|e| format!("Failed to parse password hash: {}", e))?;
    
    let argon2 = Argon2::default();
    argon2
        .verify_password(password.as_bytes(), &parsed_hash)
        .map_err(|e| format!("Failed to verify password: {}", e))
}

pub fn generate_token(length: usize) -> String {
    let mut rng = rand::thread_rng();
    (0..length)
        .map(|_| rng.sample(rand::distributions::Alphanumeric) as char)
        .collect()
}

pub fn generate_room_id(server_name: &str) -> String {
    format!("!{}:{}", generate_token(16), server_name)
}

pub fn generate_event_id(server_name: &str) -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    format!("${}:{}", generate_token(16), server_name)
}
```

**验收标准**：
- ✅ 密码哈希函数实现正确
- ✅ 密码验证函数实现正确
- ✅ 令牌生成函数实现正确
- ✅ 房间 ID 生成函数实现正确
- ✅ 事件 ID 生成函数实现正确

**状态**：📝 待完成

---

### 4.4 代码质量检查

**检查项**：
- ✅ `cargo check` - 编译检查
- ✅ `cargo clippy` - 代码检查
- ✅ `cargo fmt --check` - 格式检查
- ✅ `cargo test` - 单元测试

**命令**：
```bash
cd /home/hula/synapse_rust
cargo check
cargo clippy -- -D warnings
cargo fmt --check
cargo test
```

**修复标准**：
- ✅ 所有编译错误修复
- ✅ 所有 clippy 警告修复
- ✅ 代码格式正确
- ✅ 所有单元测试通过

**状态**：📝 待完成

---

### 4.5 测试用例

**测试项**：
- ✅ 错误处理测试
- ✅ 配置管理测试
- ✅ 加密工具测试

**测试文件**：
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_error_conversion() {
        let err = sqlx::Error::RowNotFound;
        let api_err = ApiError::from(err);
        assert!(matches!(api_err, ApiError::Database(_)));
    }
    
    #[test]
    fn test_password_hash() {
        let password = "test_password";
        let hash = hash_password(password).unwrap();
        assert!(hash.len() > 0);
    }
    
    #[test]
    fn test_password_verify() {
        let password = "test_password";
        let hash = hash_password(password).unwrap();
        let result = verify_password(password, &hash).unwrap();
        assert!(result);
    }
    
    #[test]
    fn test_token_generation() {
        let token = generate_token(32);
        assert_eq!(token.len(), 32);
    }
    
    #[test]
    fn test_room_id_generation() {
        let server_name = "localhost";
        let room_id = generate_room_id(server_name);
        assert!(room_id.starts_with("!"));
        assert!(room_id.ends_with(":localhost"));
    }
}
```

**测试标准**：
- ✅ 所有测试通过
- ✅ 测试覆盖率达到 80%

**状态**：📝 待完成

---

### 4.6 文档更新

**更新文档**：
- ✅ [error-handling.md](./error-handling.md) - 标注阶段 2 完成
- ✅ [implementation-guide.md](./implementation-guide.md) - 标注阶段 2 完成
- ✅ [project-assessment-skillset.md](./project-assessment-skillset.md) - 更新项目重构进度

**状态**：📝 待完成

---

## 五、阶段 3：存储层开发（第 5-7 周）

### 5.1 阶段目标

实现所有存储模块，包括用户、设备、令牌、房间、事件、成员、在线等。

### 5.2 参考文档

- [data-models.md](./data-models.md) - 数据模型文档
- [migration-guide.md](./migration-guide.md) - 数据迁移指南

### 5.3 任务清单

#### 任务 3.1：用户存储模块

**目标**：实现用户数据存储功能

**步骤**：
1. 创建 `src/storage/user.rs` 文件
2. 定义 `User` 结构体
3. 定义 `UserStorage` 结构体
4. 实现 `create_user()` 函数
5. 实现 `get_user()` 函数
6. 实现 `get_user_by_username()` 函数
7. 实现 `update_user()` 函数
8. 实现 `delete_user()` 函数

**验收标准**：
- ✅ User 结构体定义完整
- ✅ UserStorage 结构体定义完整
- ✅ 所有 CRUD 函数实现正确
- ✅ SQLx 查询编译通过
- ✅ 单元测试通过

**状态**：📝 待完成

---

#### 任务 3.2：设备存储模块

**目标**：实现设备数据存储功能

**步骤**：
1. 创建 `src/storage/device.rs` 文件
2. 定义 `Device` 结构体
3. 定义 `DeviceStorage` 结构体
4. 实现 `create_device()` 函数
5. 实现 `get_device()` 函数
6. 实现 `get_user_devices()` 函数
7. 实现 `update_device()` 函数
8. 实现 `delete_device()` 函数

**验收标准**：
- ✅ Device 结构体定义完整
- ✅ DeviceStorage 结构体定义完整
- ✅ 所有 CRUD 函数实现正确
- ✅ SQLx 查询编译通过
- ✅ 单元测试通过

**状态**：📝 待完成

---

#### 任务 3.3：令牌存储模块

**目标**：实现令牌数据存储功能

**步骤**：
1. 创建 `src/storage/token.rs` 文件
2. 定义 `AccessToken` 结构体
3. 定义 `RefreshToken` 结构体
4. 定义 `TokenStorage` 结构体
5. 实现 `create_token()` 函数
6. 实现 `create_refresh_token()` 函数
7. 实现 `get_token()` 函数
8. 实现 `invalidate_token()` 函数
9. 实现 `delete_token()` 函数

**验收标准**：
- ✅ AccessToken 结构体定义完整
- ✅ RefreshToken 结构体定义完整
- ✅ TokenStorage 结构体定义完整
- ✅ 所有 CRUD 函数实现正确
- ✅ SQLx 查询编译通过
- ✅ 单元测试通过

**状态**：📝 待完成

---

#### 任务 3.4：房间存储模块

**目标**：实现房间数据存储功能

**步骤**：
1. 创建 `src/storage/room.rs` 文件
2. 定义 `Room` 结构体
3. 定义 `RoomStorage` 结构体
4. 实现 `create_room()` 函数
5. 实现 `get_room()` 函数
6. 实现 `get_rooms()` 函数
7. 实现 `update_room()` 函数
8. 实现 `delete_room()` 函数

**验收标准**：
- ✅ Room 结构体定义完整
- ✅ RoomStorage 结构体定义完整
- ✅ 所有 CRUD 函数实现正确
- ✅ SQLx 查询编译通过
- ✅ 单元测试通过

**状态**：📝 待完成

---

#### 任务 3.5：事件存储模块

**目标**：实现事件数据存储功能

**步骤**：
1. 创建 `src/storage/event.rs` 文件
2. 定义 `RoomEvent` 结构体
3. 定义 `EventStorage` 结构体
4. 实现 `create_event()` 函数
5. 实现 `get_event()` 函数
6. 实现 `get_room_events()` 函数
7. 实现 `get_room_events_by_type()` 函数
8. 实现 `get_sender_events()` 函数

**验收标准**：
- ✅ RoomEvent 结构体定义完整
- ✅ EventStorage 结构体定义完整
- ✅ 所有查询函数实现正确
- ✅ SQLx 查询编译通过
- ✅ 单元测试通过

**状态**：📝 待完成

---

#### 任务 3.6：成员存储模块

**目标**：实现成员关系数据存储功能

**步骤**：
1. 创建 `src/storage/membership.rs` 文件
2. 定义 `RoomMember` 结构体
3. 定义 `MembershipStorage` 结构体
4. 实现 `add_member()` 函数
5. 实现 `remove_member()` 函数
6. 实现 `get_members()` 函数
7. 实现 `get_member()` 函数
8. 实现 `update_membership()` 函数

**验收标准**：
- ✅ RoomMember 结构体定义完整
- ✅ MembershipStorage 结构体定义完整
- ✅ 所有 CRUD 函数实现正确
- ✅ SQLx 查询编译通过
- ✅ 单元测试通过

**状态**：📝 待完成

---

#### 任务 3.7：在线存储模块

**目标**：实现在线状态数据存储功能

**步骤**：
1. 创建 `src/storage/presence.rs` 文件
2. 定义 `Presence` 结构体
3. 定义 `PresenceStorage` 结构体
4. 实现 `set_presence()` 函数
5. 实现 `get_presence()` 函数
6. 实现 `get_presences()` 函数

**验收标准**：
- ✅ Presence 结构体定义完整
- ✅ PresenceStorage 结构体定义完整
- ✅ 所有查询函数实现正确
- ✅ SQLx 查询编译通过
- ✅ 单元测试通过

**状态**：📝 待完成

---

### 5.4 代码质量检查

**检查项**：
- ✅ `cargo check` - 编译检查
- ✅ `cargo clippy` - 代码检查
- ✅ `cargo fmt --check` - 格式检查
- ✅ `cargo test` - 单元测试
- ✅ `cargo tarpaulin` - 测试覆盖率

**命令**：
```bash
cd /home/hula/synapse_rust
cargo check
cargo clippy -- -D warnings
cargo fmt --check
cargo test
cargo tarpaulin --out Html --output-dir coverage/
```

**修复标准**：
- ✅ 所有编译错误修复
- ✅ 所有 clippy 警告修复
- ✅ 代码格式正确
- ✅ 所有单元测试通过
- ✅ 测试覆盖率达到 80%

**状态**：📝 待完成

---

### 5.5 测试用例

**测试项**：
- ✅ 用户存储测试
- ✅ 设备存储测试
- ✅ 令牌存储测试
- ✅ 房间存储测试
- ✅ 事件存储测试
- ✅ 成员存储测试
- ✅ 在线存储测试

**测试标准**：
- ✅ 所有测试通过
- ✅ 测试覆盖率达到 80%

**状态**：📝 待完成

---

### 5.6 文档更新

**更新文档**：
- ✅ [data-models.md](./data-models.md) - 标注阶段 3 完成
- ✅ [migration-guide.md](./migration-guide.md) - 标注阶段 3 完成
- ✅ [project-assessment-skillset.md](./project-assessment-skillset.md) - 更新项目重构进度

**状态**：📝 待完成

---

## 六、阶段 4：缓存层开发（第 8 周）

### 6.1 阶段目标

实现缓存管理器，包括本地缓存和分布式缓存。

### 6.2 参考文档

- [architecture-design.md](./architecture-design.md) - 架构设计文档
- [implementation-guide.md](./implementation-guide.md) - 实现指南文档

### 6.3 任务清单

#### 任务 4.1：缓存管理器

**目标**：实现缓存管理器

**步骤**：
1. 创建 `src/cache/mod.rs` 文件
2. 定义 `CacheManager` 结构体
3. 实现 `CacheManager::new()` 函数
4. 实现 `get()` 函数
5. 实现 `set()` 函数
6. 实现 `delete()` 函数
7. 实现 `invalidate()` 函数

**验收标准**：
- ✅ CacheManager 结构体定义完整
- ✅ 所有缓存操作函数实现正确
- ✅ 两级缓存实现正确
- ✅ 单元测试通过

**状态**：📝 待完成

---

#### 任务 4.2：本地缓存（Moka）

**目标**：实现本地缓存功能

**步骤**：
1. 配置 Moka 缓存
2. 实现 LRU 策略
3. 实现缓存过期
4. 实现缓存预热

**验收标准**：
- ✅ Moka 缓存配置正确
- ✅ LRU 策略实现正确
- ✅ 缓存过期实现正确
- ✅ 单元测试通过

**状态**：📝 待完成

---

#### 任务 4.3：分布式缓存（Redis）

**目标**：实现分布式缓存功能

**步骤**：
1. 配置 Redis 连接
2. 实现 Redis 缓存操作
3. 实现缓存同步
4. 实现缓存失效广播

**验收标准**：
- ✅ Redis 连接配置正确
- ✅ 缓存操作实现正确
- ✅ 缓存同步实现正确
- ✅ 单元测试通过

**状态**：📝 待完成

---

### 6.4 代码质量检查

**检查项**：
- ✅ `cargo check` - 编译检查
- ✅ `cargo clippy` - 代码检查
- ✅ `cargo fmt --check` - 格式检查
- ✅ `cargo test` - 单元测试
- ✅ `cargo tarpaulin` - 测试覆盖率

**修复标准**：
- ✅ 所有编译错误修复
- ✅ 所有 clippy 警告修复
- ✅ 代码格式正确
- ✅ 所有单元测试通过
- ✅ 测试覆盖率达到 80%

**状态**：📝 待完成

---

### 6.5 测试用例

**测试项**：
- ✅ 缓存管理器测试
- ✅ 本地缓存测试
- ✅ 分布式缓存测试
- ✅ 缓存失效测试

**测试标准**：
- ✅ 所有测试通过
- ✅ 测试覆盖率达到 80%

**状态**：📝 待完成

---

### 6.6 文档更新

**更新文档**：
- ✅ [architecture-design.md](./architecture-design.md) - 标注阶段 4 完成
- ✅ [implementation-guide.md](./implementation-guide.md) - 标注阶段 4 完成
- ✅ [project-assessment-skillset.md](./project-assessment-skillset.md) - 更新项目重构进度

**状态**：📝 待完成

---

## 七、阶段 5：认证模块开发（第 9 周）

### 7.1 阶段目标

实现认证服务，包括用户注册、登录、令牌验证等。

### 7.2 参考文档

- [api-complete.md](./api-complete.md) - 完整 API 文档
- [error-handling.md](./error-handling.md) - 错误处理文档
- [implementation-guide.md](./implementation-guide.md) - 实现指南文档

### 7.3 任务清单

#### 任务 5.1：认证服务

**目标**：实现认证服务

**步骤**：
1. 创建 `src/auth/mod.rs` 文件
2. 定义 `AuthService` 结构体
3. 实现 `AuthService::new()` 函数
4. 实现 `register()` 函数
5. 实现 `login()` 函数
6. 实现 `logout()` 函数
7. 实现 `validate_token()` 函数
8. 实现 `refresh_token()` 函数

**验收标准**：
- ✅ AuthService 结构体定义完整
- ✅ 所有认证函数实现正确
- ✅ 密码哈希正确
- ✅ JWT 令牌生成正确
- ✅ 单元测试通过

**状态**：📝 待完成

---

### 7.4 代码质量检查

**检查项**：
- ✅ `cargo check` - 编译检查
- ✅ `cargo clippy` - 代码检查
- ✅ `cargo fmt --check` - 格式检查
- ✅ `cargo test` - 单元测试
- ✅ `cargo tarpaulin` - 测试覆盖率

**修复标准**：
- ✅ 所有编译错误修复
- ✅ 所有 clippy 警告修复
- ✅ 代码格式正确
- ✅ 所有单元测试通过
- ✅ 测试覆盖率达到 80%

**状态**：📝 待完成

---

### 7.5 测试用例

**测试项**：
- ✅ 用户注册测试
- ✅ 用户登录测试
- ✅ 令牌验证测试
- ✅ 令牌刷新测试

**测试标准**：
- ✅ 所有测试通过
- ✅ 测试覆盖率达到 80%

**状态**：📝 待完成

---

### 7.6 文档更新

**更新文档**：
- ✅ [api-complete.md](./api-complete.md) - 标注阶段 5 完成
- ✅ [error-handling.md](./error-handling.md) - 标注阶段 5 完成
- ✅ [project-assessment-skillset.md](./project-assessment-skillset.md) - 更新项目重构进度

**状态**：📝 待完成

---

## 八、阶段 6：服务层开发（第 10-14 周）

### 8.1 阶段目标

实现核心服务层，包括注册、房间、同步、媒体服务。

### 8.2 参考文档

- [api-complete.md](./api-complete.md) - 完整 API 文档
- [module-structure.md](./module-structure.md) - 模块结构文档
- [implementation-guide.md](./implementation-guide.md) - 实现指南文档

### 8.3 任务清单

#### 任务 6.1：注册服务

**目标**：实现注册服务

**步骤**：
1. 创建 `src/services/registration.rs` 文件
2. 定义 `RegistrationService` 结构体
3. 实现 `register()` 函数
4. 实现 `validate_username()` 函数
5. 实现 `validate_password()` 函数

**验收标准**：
- ✅ RegistrationService 结构体定义完整
- ✅ 所有注册函数实现正确
- ✅ 用户验证逻辑正确
- ✅ 单元测试通过

**状态**：📝 待完成

---

#### 任务 6.2：房间服务

**目标**：实现房间服务

**步骤**：
1. 创建 `src/services/room_service.rs` 文件
2. 定义 `RoomService` 结构体
3. 实现 `create_room()` 函数
4. 实现 `join_room()` 函数
5. 实现 `leave_room()` 函数
6. 实现 `invite_user()` 函数
7. 实现 `send_message()` 函数

**验收标准**：
- ✅ RoomService 结构体定义完整
- ✅ 所有房间操作函数实现正确
- ✅ 房间验证逻辑正确
- ✅ 单元测试通过

**状态**：📝 待完成

---

#### 任务 6.3：同步服务

**目标**：实现同步服务

**步骤**：
1. 创建 `src/services/sync_service.rs` 文件
2. 定义 `SyncService` 结构体
3. 实现 `sync()` 函数
4. 实现 `get_events()` 函数
5. 实现 `filter_events()` 函数

**验收标准**：
- ✅ SyncService 结构体定义完整
- ✅ 所有同步函数实现正确
- ✅ 事件过滤逻辑正确
- ✅ 单元测试通过

**状态**：📝 待完成

---

#### 任务 6.4：媒体服务

**目标**：实现媒体服务

**步骤**：
1. 创建 `src/services/media_service.rs` 文件
2. 定义 `MediaService` 结构体
3. 实现 `upload_media()` 函数
4. 实现 `download_media()` 函数
5. 实现 `delete_media()` 函数

**验收标准**：
- ✅ MediaService 结构体定义完整
- ✅ 所有媒体操作函数实现正确
- ✅ 文件验证逻辑正确
- ✅ 单元测试通过

**状态**：📝 待完成

---

### 8.4 代码质量检查

**检查项**：
- ✅ `cargo check` - 编译检查
- ✅ `cargo clippy` - 代码检查
- ✅ `cargo fmt --check` - 格式检查
- ✅ `cargo test` - 单元测试
- ✅ `cargo tarpaulin` - 测试覆盖率

**修复标准**：
- ✅ 所有编译错误修复
- ✅ 所有 clippy 警告修复
- ✅ 代码格式正确
- ✅ 所有单元测试通过
- ✅ 测试覆盖率达到 80%

**状态**：📝 待完成

---

### 8.5 测试用例

**测试项**：
- ✅ 注册服务测试
- ✅ 房间服务测试
- ✅ 同步服务测试
- ✅ 媒体服务测试

**测试标准**：
- ✅ 所有测试通过
- ✅ 测试覆盖率达到 80%

**状态**：📝 待完成

---

### 8.6 文档更新

**更新文档**：
- ✅ [api-complete.md](./api-complete.md) - 标注阶段 6 完成
- ✅ [module-structure.md](./module-structure.md) - 标注阶段 6 完成
- ✅ [project-assessment-skillset.md](./project-assessment-skillset.md) - 更新项目重构进度

**状态**：📝 待完成

---

## 九、阶段 7：E2EE 开发（第 15-18 周）

### 9.1 阶段目标

实现端到端加密（E2EE）功能，包括设备密钥管理、跨签名密钥、Megolm 加密和密钥备份。

### 9.2 参考文档

- [e2ee-architecture.md](./e2ee-architecture.md) - E2EE 架构设计文档
- [e2ee-implementation-guide.md](./e2ee-implementation-guide.md) - E2EE 实现指南文档
- [api-complete.md](./api-complete.md) - 完整 API 文档（包含 E2EE API）
- [data-models.md](./data-models.md) - 数据模型文档（包含 E2EE 表）

### 9.3 任务清单

#### 任务 7.1：设备密钥服务

**目标**：实现设备密钥管理服务

**步骤**：
1. 创建 `src/e2ee/device_keys/` 目录
2. 创建 `src/e2ee/device_keys/models.rs` 文件
3. 创建 `src/e2ee/device_keys/storage.rs` 文件
4. 创建 `src/e2ee/device_keys/service.rs` 文件
5. 实现 `query_keys()` 函数
6. 实现 `upload_keys()` 函数
7. 实现 `claim_keys()` 函数
8. 实现 `delete_keys()` 函数

**验收标准**：
- ✅ DeviceKeyService 结构体定义完整
- ✅ 所有设备密钥操作函数实现正确
- ✅ 密钥缓存逻辑正确
- ✅ 单元测试通过

**状态**：📝 待完成

---

#### 任务 7.2：跨签名密钥服务

**目标**：实现跨签名密钥管理服务

**步骤**：
1. 创建 `src/e2ee/cross_signing/` 目录
2. 创建 `src/e2ee/cross_signing/models.rs` 文件
3. 创建 `src/e2ee/cross_signing/storage.rs` 文件
4. 创建 `src/e2ee/cross_signing/service.rs` 文件
5. 实现 `upload_cross_signing_keys()` 函数
6. 实现 `get_cross_signing_keys()` 函数
7. 实现 `sign_device_keys()` 函数
8. 实现 `verify_device_keys()` 函数

**验收标准**：
- ✅ CrossSigningService 结构体定义完整
- ✅ 所有跨签名密钥操作函数实现正确
- ✅ 签名验证逻辑正确
- ✅ 单元测试通过

**状态**：📝 待完成

---

#### 任务 7.3：Megolm 加密服务

**目标**：实现 Megolm 加密服务

**步骤**：
1. 创建 `src/e2ee/megolm/` 目录
2. 创建 `src/e2ee/megolm/models.rs` 文件
3. 创建 `src/e2ee/megolm/storage.rs` 文件
4. 创建 `src/e2ee/megolm/service.rs` 文件
5. 实现 `create_session()` 函数
6. 实现 `load_session()` 函数
7. 实现 `encrypt()` 函数
8. 实现 `decrypt()` 函数
9. 实现 `rotate_session()` 函数
10. 实现 `share_session()` 函数

**验收标准**：
- ✅ MegolmService 结构体定义完整
- ✅ 所有 Megolm 操作函数实现正确
- ✅ 加密/解密逻辑正确
- ✅ 会话管理逻辑正确
- ✅ 单元测试通过

**状态**：📝 待完成

---

#### 任务 7.4：密钥备份服务

**目标**：实现密钥备份服务

**步骤**：
1. 创建 `src/e2ee/backup/` 目录
2. 创建 `src/e2ee/backup/models.rs` 文件
3. 创建 `src/e2ee/backup/storage.rs` 文件
4. 创建 `src/e2ee/backup/service.rs` 文件
5. 实现 `create_backup()` 函数
6. 实现 `get_backup()` 函数
7. 实现 `upload_backup()` 函数
8. 实现 `download_backup()` 函数
9. 实现 `delete_backup()` 函数

**验收标准**：
- ✅ BackupKeyService 结构体定义完整
- ✅ 所有密钥备份操作函数实现正确
- ✅ 备份加密逻辑正确
- ✅ 单元测试通过

**状态**：📝 待完成

---

#### 任务 7.5：事件签名服务

**目标**：实现事件签名服务

**步骤**：
1. 创建 `src/e2ee/signature/` 目录
2. 创建 `src/e2ee/signature/models.rs` 文件
3. 创建 `src/e2ee/signature/storage.rs` 文件
4. 创建 `src/e2ee/signature/service.rs` 文件
5. 实现 `sign_event()` 函数
6. 实现 `verify_event()` 函数
7. 实现 `sign_key()` 函数
8. 实现 `verify_key()` 函数

**验收标准**：
- ✅ SignatureService 结构体定义完整
- ✅ 所有签名操作函数实现正确
- ✅ 签名验证逻辑正确
- ✅ 单元测试通过

**状态**：📝 待完成

---

#### 任务 7.6：E2EE API 路由

**目标**：实现 E2EE API 路由

**步骤**：
1. 创建 `src/e2ee/api/` 目录
2. 创建 `src/e2ee/api/mod.rs` 文件
3. 创建 `src/e2ee/api/device_keys.rs` 文件
4. 创建 `src/e2ee/api/cross_signing.rs` 文件
5. 创建 `src/e2ee/api/megolm.rs` 文件
6. 创建 `src/e2ee/api/backup.rs` 文件
7. 实现所有 E2EE API 端点

**验收标准**：
- ✅ 所有 E2EE API 路由定义完整
- ✅ 请求/响应处理正确
- ✅ 错误处理正确
- ✅ 单元测试通过

**状态**：📝 待完成

---

### 9.4 代码质量检查

**检查项**：
- ✅ `cargo check` - 编译检查
- ✅ `cargo clippy` - 代码检查
- ✅ `cargo fmt --check` - 格式检查
- ✅ `cargo test` - 单元测试
- ✅ `cargo tarpaulin` - 测试覆盖率

**修复标准**：
- ✅ 所有编译错误修复
- ✅ 所有 clippy 警告修复
- ✅ 代码格式正确
- ✅ 所有单元测试通过
- ✅ 测试覆盖率达到 80%

**状态**：📝 待完成

---

### 9.5 测试用例

**测试项**：
- ✅ 设备密钥服务测试
- ✅ 跨签名密钥服务测试
- ✅ Megolm 加密服务测试
- ✅ 密钥备份服务测试
- ✅ 事件签名服务测试
- ✅ E2EE API 集成测试

**测试标准**：
- ✅ 所有测试通过
- ✅ 测试覆盖率达到 80%

**状态**：📝 待完成

---

### 9.6 文档更新

**更新文档**：
- ✅ [api-complete.md](./api-complete.md) - 标注阶段 7 完成
- ✅ [module-structure.md](./module-structure.md) - 标注阶段 7 完成
- ✅ [project-assessment-skillset.md](./project-assessment-skillset.md) - 更新项目重构进度

**状态**：📝 待完成

---

## 十、阶段 8：Enhanced API 开发（第 19-22 周）

### 9.1 阶段目标

实现 Enhanced API 功能，包括好友、私聊、语音服务。

### 9.2 参考文档

- [api-complete.md](./api-complete.md) - 完整 API 文档
- [module-structure.md](./module-structure.md) - 模块结构文档
- [implementation-guide.md](./implementation-guide.md) - 实现指南文档

### 9.3 任务清单

#### 任务 7.1：好友服务

**目标**：实现好友服务

**步骤**：
1. 创建 `src/services/friend_service.rs` 文件
2. 定义 `FriendService` 结构体
3. 实现 `get_friends()` 函数
4. 实现 `send_friend_request()` 函数
5. 实现 `respond_friend_request()` 函数
6. 实现 `add_friend()` 函数
7. 实现 `remove_friend()` 函数

**验收标准**：
- ✅ FriendService 结构体定义完整
- ✅ 所有好友操作函数实现正确
- ✅ 好友验证逻辑正确
- ✅ 单元测试通过

**状态**：📝 待完成

---

#### 任务 7.2：私聊服务

**目标**：实现私聊服务

**步骤**：
1. 创建 `src/services/private_chat.rs` 文件
2. 定义 `PrivateChatService` 结构体
3. 实现 `create_session()` 函数
4. 实现 `send_message()` 函数
5. 实现 `get_messages()` 函数
6. 实现 `mark_as_read()` 函数

**验收标准**：
- ✅ PrivateChatService 结构体定义完整
- ✅ 所有私聊操作函数实现正确
- ✅ 消息加密逻辑正确
- ✅ 单元测试通过

**状态**：📝 待完成

---

#### 任务 7.3：语音服务

**目标**：实现语音服务

**步骤**：
1. 创建 `src/services/voice_service.rs` 文件
2. 定义 `VoiceService` 结构体
3. 实现 `upload_voice()` 函数
4. 实现 `get_voice()` 函数
5. 实现 `get_user_voices()` 函数
6. 实现 `delete_voice()` 函数

**验收标准**：
- ✅ VoiceService 结构体定义完整
- ✅ 所有语音操作函数实现正确
- ✅ 音频处理逻辑正确
- ✅ 单元测试通过

**状态**：📝 待完成

---

### 9.4 代码质量检查

**检查项**：
- ✅ `cargo check` - 编译检查
- ✅ `cargo clippy` - 代码检查
- ✅ `cargo fmt --check` - 格式检查
- ✅ `cargo test` - 单元测试
- ✅ `cargo tarpaulin` - 测试覆盖率

**修复标准**：
- ✅ 所有编译错误修复
- ✅ 所有 clippy 警告修复
- ✅ 代码格式正确
- ✅ 所有单元测试通过
- ✅ 测试覆盖率达到 80%

**状态**：📝 待完成

---

### 9.5 测试用例

**测试项**：
- ✅ 好友服务测试
- ✅ 私聊服务测试
- ✅ 语音服务测试

**测试标准**：
- ✅ 所有测试通过
- ✅ 测试覆盖率达到 80%

**状态**：📝 待完成

---

### 9.6 文档更新

**更新文档**：
- ✅ [api-complete.md](./api-complete.md) - 标注阶段 7 完成
- ✅ [module-structure.md](./module-structure.md) - 标注阶段 7 完成
- ✅ [project-assessment-skillset.md](./project-assessment-skillset.md) - 更新项目重构进度

**状态**：📝 待完成

---

## 十、阶段 8：Web 层开发（第 19-21 周）

### 10.1 阶段目标

实现 Web 层，包括路由、中间件、请求处理器。

### 10.2 参考文档

- [api-complete.md](./api-complete.md) - 完整 API 文档
- [module-structure.md](./module-structure.md) - 模块结构文档
- [error-handling.md](./error-handling.md) - 错误处理文档

### 10.3 任务清单

#### 任务 8.1：路由定义

**目标**：实现所有 API 路由

**步骤**：
1. 创建 `src/web/routes/client.rs` 文件
2. 创建 `src/web/routes/admin.rs` 文件
3. 创建 `src/web/routes/media.rs` 文件
4. 创建 `src/web/routes/friend.rs` 文件
5. 创建 `src/web/routes/private.rs` 文件
6. 创建 `src/web/routes/voice.rs` 文件
7. 实现所有路由函数

**验收标准**：
- ✅ 所有路由定义完整
- ✅ 路由参数提取正确
- ✅ 路由处理器绑定正确
- ✅ 单元测试通过

**状态**：📝 待完成

---

#### 任务 8.2：中间件实现

**目标**：实现所有中间件

**步骤**：
1. 创建 `src/web/middleware/auth.rs` 文件
2. 创建 `src/web/middleware/logging.rs` 文件
3. 创建 `src/web/middleware/cors.rs` 文件
4. 创建 `src/web/middleware/rate_limit.rs` 文件
5. 实现所有中间件函数

**验收标准**：
- ✅ 所有中间件实现完整
- ✅ 认证中间件正确
- ✅ 日志中间件正确
- ✅ CORS 中间件正确
- ✅ 速率限制中间件正确
- ✅ 单元测试通过

**状态**：📝 待完成

---

#### 任务 8.3：请求处理器

**目标**：实现所有请求处理器

**步骤**：
1. 创建 `src/web/handlers/client.rs` 文件
2. 创建 `src/web/handlers/admin.rs` 文件
3. 创建 `src/web/handlers/media.rs` 文件
4. 创建 `src/web/handlers/friend.rs` 文件
5. 创建 `src/web/handlers/private.rs` 文件
6. 创建 `src/web/handlers/voice.rs` 文件
7. 实现所有请求处理器函数

**验收标准**：
- ✅ 所有请求处理器实现完整
- ✅ 请求解析正确
- ✅ 响应格式正确
- ✅ 错误处理正确
- ✅ 单元测试通过

**状态**：📝 待完成

---

### 10.4 代码质量检查

**检查项**：
- ✅ `cargo check` - 编译检查
- ✅ `cargo clippy` - 代码检查
- ✅ `cargo fmt --check` - 格式检查
- ✅ `cargo test` - 单元测试
- ✅ `cargo tarpaulin` - 测试覆盖率

**修复标准**：
- ✅ 所有编译错误修复
- ✅ 所有 clippy 警告修复
- ✅ 代码格式正确
- ✅ 所有单元测试通过
- ✅ 测试覆盖率达到 80%

**状态**：📝 待完成

---

### 10.5 测试用例

**测试项**：
- ✅ 路由测试
- ✅ 中间件测试
- ✅ 请求处理器测试
- ✅ 集成测试

**测试标准**：
- ✅ 所有测试通过
- ✅ 测试覆盖率达到 80%

**状态**：📝 待完成

---

### 10.6 文档更新

**更新文档**：
- ✅ [api-complete.md](./api-complete.md) - 标注阶段 8 完成
- ✅ [module-structure.md](./module-structure.md) - 标注阶段 8 完成
- ✅ [project-assessment-skillset.md](./project-assessment-skillset.md) - 更新项目重构进度

**状态**：📝 待完成

---

## 十一、阶段 9：数据库迁移（第 26 周）

### 11.1 阶段目标

实现数据库迁移脚本和工具。

### 11.2 参考文档

- [migration-guide.md](./migration-guide.md) - 数据迁移指南
- [data-models.md](./data-models.md) - 数据模型文档

### 11.3 任务清单

#### 任务 9.1：迁移脚本开发

**目标**：创建所有数据库迁移脚本

**步骤**：
1. 创建 `migrations/V1__create_users_table.sql`
2. 创建 `migrations/V2__create_devices_table.sql`
3. 创建 `migrations/V3__create_access_tokens_table.sql`
4. 创建 `migrations/V4__create_rooms_table.sql`
5. 创建 `migrations/V5__create_events_table.sql`
6. 创建 `migrations/V6__create_room_memberships_table.sql`
7. 创建 `migrations/V7__create_presence_table.sql`
8. 创建 `migrations/V8__create_friends_table.sql`
9. 创建 `migrations/V9__create_friend_requests_table.sql`
10. 创建 `migrations/V10__create_friend_categories_table.sql`
11. 创建 `migrations/V11__create_blocked_users_table.sql`
12. 创建 `migrations/V12__create_private_sessions_table.sql`
13. 创建 `migrations/V13__create_private_messages_table.sql`
14. 创建 `migrations/V14__create_session_keys_table.sql`
15. 创建 `migrations/V15__create_voice_messages_table.sql`
16. 创建 `migrations/V16__create_security_events_table.sql`
17. 创建 `migrations/V17__create_ip_blocks_table.sql`
18. 创建 `migrations/V18__create_ip_reputation_table.sql`
19. 创建回滚脚本

**验收标准**：
- ✅ 所有迁移脚本创建完成
- ✅ SQL 语法正确
- ✅ 表结构符合 [data-models.md](./data-models.md) 规范
- ✅ 回滚脚本正确

**状态**：📝 待完成

---

#### 任务 9.2：迁移工具实现

**目标**：实现迁移工具

**步骤**：
1. 创建 `src/migrations/mod.rs` 文件
2. 定义 `Migrator` 结构体
3. 实现 `Migrator::new()` 函数
4. 实现 `migrate()` 函数
5. 实现 `rollback_migration()` 函数

**验收标准**：
- ✅ Migrator 结构体定义完整
- ✅ 所有迁移函数实现正确
- ✅ 迁移版本管理正确
- ✅ 单元测试通过

**状态**：📝 待完成

---

### 11.4 代码质量检查

**检查项**：
- ✅ `cargo check` - 编译检查
- ✅ `cargo clippy` - 代码检查
- ✅ `cargo fmt --check` - 格式检查
- ✅ `cargo test` - 单元测试

**修复标准**：
- ✅ 所有编译错误修复
- ✅ 所有 clippy 警告修复
- ✅ 代码格式正确
- ✅ 所有单元测试通过

**状态**：📝 待完成

---

### 11.5 测试用例

**测试项**：
- ✅ 迁移脚本测试
- ✅ 迁移工具测试
- ✅ 回滚测试

**测试标准**：
- ✅ 所有测试通过
- ✅ 测试覆盖率达到 80%

**状态**：📝 待完成

---

### 11.6 文档更新

**更新文档**：
- ✅ [migration-guide.md](./migration-guide.md) - 标注阶段 9 完成
- ✅ [data-models.md](./data-models.md) - 标注阶段 9 完成
- ✅ [project-assessment-skillset.md](./project-assessment-skillset.md) - 更新项目重构进度

**状态**：📝 待完成

---

## 十二、阶段 10：集成测试与优化（第 27-28 周）

### 12.1 阶段目标

进行集成测试、性能测试、代码优化和文档完善。

### 12.2 参考文档

- [api-complete.md](./api-complete.md) - 完整 API 文档
- [implementation-guide.md](./implementation-guide.md) - 实现指南文档
- [project-assessment-skillset.md](./project-assessment-skillset.md) - 项目评估技能集

### 12.3 任务清单

#### 任务 10.1：集成测试

**目标**：进行全面的集成测试

**步骤**：
1. 创建集成测试文件
2. 测试所有 API 端点
3. 测试数据库操作
4. 测试缓存操作
5. 测试认证流程

**验收标准**：
- ✅ 所有 API 端点测试通过
- ✅ 所有数据库操作测试通过
- ✅ 所有缓存操作测试通过
- ✅ 所有认证流程测试通过
- ✅ 测试覆盖率达到 80%

**状态**：📝 待完成

---

#### 任务 10.2：性能测试

**目标**：进行性能测试和优化

**步骤**：
1. 使用负载测试工具测试 API 性能
2. 分析数据库查询性能
3. 分析缓存命中率
4. 优化慢查询
5. 优化缓存策略

**验收标准**：
- ✅ API 响应时间 < 50ms
- ✅ 数据库查询优化完成
- ✅ 缓存命中率 > 80%
- ✅ 内存占用 < 300MB

**状态**：📝 待完成

---

#### 任务 10.3：代码优化

**目标**：优化代码质量和性能

**步骤**：
1. 修复所有 clippy 警告
2. 优化算法复杂度
3. 优化内存使用
4. 优化并发性能

**验收标准**：
- ✅ 所有 clippy 警告修复
- ✅ 算法复杂度优化完成
- ✅ 内存使用优化完成
- ✅ 并发性能优化完成

**状态**：📝 待完成

---

#### 任务 10.4：文档完善

**目标**：完善所有文档

**步骤**：
1. 更新所有技术文档
2. 添加使用示例
3. 添加故障排查指南
4. 添加部署指南
5. 添加贡献指南

**验收标准**：
- ✅ 所有文档更新完成
- ✅ 使用示例完整
- ✅ 故障排查指南完整
- ✅ 部署指南完整
- ✅ 贡献指南完整

**状态**：📝 待完成

---

### 12.4 代码质量检查

**检查项**：
- ✅ `cargo check` - 编译检查
- ✅ `cargo clippy` - 代码检查
- ✅ `cargo fmt --check` - 格式检查
- ✅ `cargo test` - 单元测试
- ✅ `cargo tarpaulin` - 测试覆盖率
- ✅ `cargo bench` - 性能测试

**修复标准**：
- ✅ 所有编译错误修复
- ✅ 所有 clippy 警告修复
- ✅ 代码格式正确
- ✅ 所有单元测试通过
- ✅ 测试覆盖率达到 80%
- ✅ 性能测试通过

**状态**：📝 待完成

---

### 12.5 测试用例

**测试项**：
- ✅ 集成测试
- ✅ 性能测试
- ✅ 端到端测试
- ✅ 压力测试

**测试标准**：
- ✅ 所有测试通过
- ✅ 测试覆盖率达到 80%
- ✅ 性能测试通过

**状态**：📝 待完成

---

### 12.6 文档更新

**更新文档**：
- ✅ [api-complete.md](./api-complete.md) - 标注阶段 10 完成
- ✅ [architecture-design.md](./architecture-design.md) - 标注阶段 10 完成
- ✅ [implementation-guide.md](./implementation-guide.md) - 标注阶段 10 完成
- ✅ [project-assessment-skillset.md](./project-assessment-skillset.md) - 更新项目重构进度

**状态**：📝 待完成

---

## 十三、代码质量标准

### 13.1 编译标准

- ✅ `cargo check` 必须通过，无编译错误
- ✅ `cargo clippy` 必须通过，无警告
- ✅ `cargo fmt --check` 必须通过，代码格式正确

### 13.2 测试标准

- ✅ 单元测试覆盖率 ≥ 80%
- ✅ 集成测试覆盖率 ≥ 80%
- ✅ 所有测试必须通过

### 13.3 性能标准

- ✅ API 响应时间 < 50ms
- ✅ 数据库查询时间 < 10ms
- ✅ 缓存命中率 > 80%
- ✅ 内存占用 < 300MB

### 13.4 文档标准

- ✅ 所有公共 API 必须有文档
- ✅ 所有模块必须有 README
- ✅ 所有复杂函数必须有注释

---

## 十四、参考资料

- [Synapse 官方文档](https://element-hq.github.io/synapse/latest/)
- [Matrix 规范](https://spec.matrix.org/)
- [Rust 官方文档](https://doc.rust-lang.org/)
- [Rust 异步编程](https://rust-lang.github.io/async-book/)
- [Rust 高级编程指南](https://www.hackerrank.com/skills-directory/rust_advanced)
- [Axum 框架文档](https://docs.rs/axum/latest/axum/)
- [SQLx 文档](https://docs.rs/sqlx/latest/sqlx/)
- [Tokio 文档](https://docs.rs/tokio/latest/tokio/)

---

## 十五、变更日志

| 版本 | 日期 | 变更说明 |
|------|------|----------|
| 1.1.0 | 2026-01-28 | 添加 E2EE 开发阶段（阶段 7），调整后续阶段编号和时间安排 |
| 1.0.0 | 2026-01-28 | 初始版本，定义项目重构开发实施方案 |
