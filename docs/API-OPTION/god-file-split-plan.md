# 上帝文件拆分方案

> 日期: 2026-03-27
> 目标: 将巨型模块拆分为职责清晰的子模块

---

## 一、拆分目标

| 文件 | 当前行数 | 目标行数 | 减少 |
|------|----------|----------|------|
| src/web/routes/mod.rs | 4756 | < 2000 | 58% |
| src/common/config.rs | 4017 | < 2000 | 50% |

---

## 二、routes/mod.rs 拆分方案

### 2.1 当前结构分析

```
src/web/routes/mod.rs (4756 行)
├── 模块声明 (1-48): pub mod xxx;
├── 导出声明 (49-80): pub use xxx;
├── 导入声明 (81-140): use xxx;
├── 自定义提取器 (141-280): MatrixJson, RoomId, UserId, Pagination
├── Handler 函数 (281-378): ~100 行
├── create_router (379-465): 路由装配
├── 中间件/层 (466-4756): ~4300 行
```

### 2.2 目标结构

```
src/web/routes/
├── mod.rs                          # 模块导出 (~200 行)
├── extractors/                      # 提取器
│   ├── mod.rs                     # MatrixJson, Pagination
│   ├── auth.rs                    # AuthenticatedUser
│   └── room.rs                    # RoomId, RoomAliasId
├── handlers/                        # 独立 Handler
│   ├── mod.rs
│   ├── health.rs                  # 健康检查
│   ├── versions.rs                # 版本端点
│   └── sync.rs                    # Sync 端点
├── middleware/                     # 路由中间件 (从 mod.rs 提取)
│   └── routing.rs                 # create_router 主体
└── assembly/                      # 路由装配 (可选)
    ├── mod.rs
    ├── api_v3.rs
    └── admin.rs
```

### 2.3 实施步骤

#### 步骤 1: 创建目录结构

```bash
mkdir -p src/web/routes/extractors src/web/routes/handlers
```

#### 步骤 2: 拆分提取器

```rust
// src/web/routes/extractors/mod.rs
pub mod auth;
pub mod room;
pub mod pagination;

// 移动 MatrixJson
pub use crate::routes::MatrixJson;
```

#### 步骤 3: 拆分独立 Handler

```rust
// src/web/routes/handlers/mod.rs
pub mod health;
pub mod versions;
pub mod sync;
```

#### 步骤 4: 简化 mod.rs

```rust
// src/web/routes/mod.rs (目标 ~200 行)

// 仅保留模块声明和导出
pub mod extractors;
pub mod handlers;

pub use extractors::*;
pub use handlers::*;

// 仅保留 create_router 核心逻辑
pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(root_handler))
        .route("/health", get(health_check))
        // ... 核心路由
        .merge(handlers::sync::create_sync_router(state.clone()))
        // ...
}
```

---

## 三、config.rs 拆分方案

### 3.1 当前结构分析

```
src/common/config.rs (4017 行)
├── ConfigError (14)
├── VoipConfig (27-115)
├── PushConfig (117-174)
├── ApnsConfig (176)
├── FcmConfig (188)
├── WebPushConfig (195)
├── UrlPreviewConfig (291)
├── OidcConfig (348-390)
├── SamlConfig (421-514)
├── RetentionConfig (625)
├── Config (751-809)
├── SearchConfig (812)
├── RateLimitConfig (862)
├── ConfigManager (978-1111)
├── ServerConfig (1113-1301)
├── DatabaseConfig (1303)
├── RedisConfig (1326)
└── ...
```

### 3.2 目标结构

```
src/common/config/
├── mod.rs                      # ConfigManager 导出 (~150 行)
├── error.rs                    # ConfigError
├── voip.rs                     # VoipConfig
├── push.rs                     # PushConfig, ApnsConfig, FcmConfig, WebPushConfig
├── url_preview.rs              # UrlPreviewConfig, UrlBlacklistRule
├── auth/
│   ├── mod.rs
│   ├── oidc.rs                # OidcConfig, OidcAttributeMapping
│   └── saml.rs                # SamlConfig, SamlAttributeMapping
├── retention.rs                # RetentionConfig
├── search.rs                   # SearchConfig, PostgresFtsConfig
├── rate_limit.rs              # RateLimitConfig, RateLimitRule
├── server.rs                   # ServerConfig
├── database.rs                # DatabaseConfig
├── redis.rs                   # RedisConfig
└── circuit_breaker.rs         # CircuitBreakerConfig
```

### 3.3 实施步骤

#### 步骤 1: 创建目录结构

```bash
mkdir -p src/common/config/auth
```

#### 步骤 2: 拆分配置文件

每个配置文件移动到独立文件:

```rust
// src/common/config/voip.rs
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct VoipConfig { ... }
```

#### 步骤 3: 创建 config/mod.rs

```rust
// src/common/config/mod.rs
pub mod error;
pub mod voip;
pub mod push;
pub mod url_preview;
pub mod auth;
pub mod retention;
pub mod search;
pub mod rate_limit;
pub mod server;
pub mod database;
pub mod redis;
pub mod circuit_breaker;

pub use error::ConfigError;
pub use voip::VoipConfig;
// ... 其他导出
```

#### 步骤 4: 更新 import 路径

```rust
// 修改前
use crate::common::config::{Config, ConfigManager};

// 修改后
use crate::common::config::{Config, ConfigManager};
// 或
use crate::common::config;
```

---

## 四、兼容性考虑

### 4.1 路径兼容

使用 `pub use` 确保向后兼容:

```rust
// src/common/config.rs (兼容层)
pub use crate::common::config::{
    Config as AppConfig,
    ConfigManager,
    VoipConfig,
    // ... 其他类型
};

// 标记为废弃
#[deprecated(since = "0.2.0", note = "Use crate::common::config::VoipConfig instead")]
pub type VoipConfig = crate::common::config::VoipConfig;
```

### 4.2 渐进式迁移

1. 创建新模块结构
2. 保持原文件作为兼容层
3. 逐步将实现迁移到新模块
4. 确认无破坏后删除兼容层

---

## 五、风险缓解

| 风险 | 缓解措施 |
|------|----------|
| 破坏现有 import | 保持兼容层，逐一迁移 |
| 循环依赖 | 先分析依赖图，按层级迁移 |
| 编译失败 | 每次小改动后验证编译 |

---

## 六、验收标准

- [x] `cargo check --all-features` 通过
- [x] `cargo fmt --all -- --check` 通过
- [x] `cargo clippy --all-features -- -D warnings` 通过
- [x] 所有模块 import 正常工作
- [x] 运行时功能无异常
