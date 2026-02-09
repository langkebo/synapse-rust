# Synapse-Rust 配置优化方案

## 一、核心问题：generate_event_id server_name 问题

### 问题描述
当前 `src/storage/membership.rs:47` 中使用硬编码的 `"localhost"` 作为 server_name：
```rust
let event_id = format!("${}", generate_event_id("localhost"));
```

这是一个架构问题：存储层没有访问配置的权限，无法获取真正的服务器名称。

### 解决方案

#### 方案 1：在 Storage 初始化时注入 server_name（推荐）

**优点：**
- 最小改动
- 符合 Rust 所有权模式
- 存储层获取配置在初始化时完成

**实现：**
```rust
// src/storage/membership.rs
#[derive(Clone)]
pub struct RoomMemberStorage {
    pub pool: Arc<Pool<Postgres>>,
    pub server_name: String,  // 新增字段
}

impl RoomMemberStorage {
    pub fn new(pool: &Arc<Pool<Postgres>>, server_name: &str) -> Self {
        Self {
            pool: pool.clone(),
            server_name: server_name.to_string(),
        }
    }

    // 使用 self.server_name 替代硬编码
    pub async fn add_member(...) -> Result<RoomMember, sqlx::Error> {
        let event_id = format!("${}", generate_event_id(&self.server_name));
        // ...
    }
}
```

#### 方案 2：通过服务层传递 server_name

**优点：**
- 存储层保持无状态
- server_name 可以动态更改

**缺点：**
- 需要修改所有存储方法签名
- API 调用链变长

---

## 二、配置文件增强计划

### 2.1 已实现的配置模块 ✅

| 模块 | 状态 | 说明 |
|------|------|------|
| server | ✅ 完整 | 服务器基础配置 |
| database | ✅ 完整 | 数据库连接配置 |
| redis | ✅ 完整 | Redis 缓存配置 |
| logging | ✅ 完整 | 日志配置 |
| federation | ✅ 完整 | 联邦通信配置 |
| security | ✅ 完整 | 安全和加密配置 |
| search | ✅ 完整 | Elasticsearch 搜索配置 |
| rate_limit | ✅ 完整 | API 限流配置 |
| admin_registration | ✅ 完整 | 管理员注册配置 |
| worker | ✅ 完整 | 工作节点配置 |
| cors | ✅ 完整 | CORS 跨域配置 |
| smtp | ✅ 完整 | SMTP 邮件配置 |

### 2.2 官方 Synapse 有但本项目缺失的配置

#### 高优先级缺失功能

| 配置模块 | 官方功能 | 优先级 | 说明 |
|----------|----------|--------|------|
| `listeners` | 多监听器配置 | 🔴 高 | 当前只支持单一 host:port |
| `media_store` | 媒体存储 | 🔴 高 | Matrix 核心功能 |
| `password_config` | 密码策略配置 | 🟡 中 | 包含 pepper、认证模块等 |
| `signing_key_path` | 签名密钥路径 | 🔴 高 | 联邦通信必需 |
| `macaroon_secret_key` | Macaroon 密钥 | 🔴 高 | 令牌安全 |
| `form_secret` | 表单密钥 | 🔴 高 | 用户交互安全 |
| `limits` | 资源限制配置 | 🟡 中 | 上传大小等限制 |
| `metrics` | 性能指标 | 🟡 中 | Prometheus 集成 |
| `oidc` | OpenID Connect | 🟢 低 | SSO 支持 |
| `voip` | VoIP (TURN/STUN) | 🟡 中 | 语音/视频通话 |
| `push` | 推送通知 | 🟢 低 | 移动端推送 |
| `url_preview` | URL 预览 | 🟢 低 | 链接预览功能 |
| `user_directory` | 用户目录 | 🟡 中 | 用户搜索配置 |

#### 中优先级缺失功能

| 配置模块 | 官方功能 | 说明 |
|----------|----------|------|
| `public_baseurl` | 公开基础 URL | 客户端访问地址 |
| `well_known` | .well-known 配置 | 服务发现 |
| `account_validity` | 账户有效期 | 临时账户管理 |
| `cas` | CAS 认证 | 中央认证服务 |
| `saml2` | SAML2 认证 | 企业 SSO |
| `ui_auth` | UI 认证会话 | 用户交互认证配置 |
| `rooms` | 房间默认配置 | 房间版本、导出等 |
| `retention` | 消息保留策略 | 自动删除旧消息 |
| `secondary_storage_providers` | 二级存储 | S3 等云存储 |

---

## 三、配置文件结构增强

### 3.1 新增配置模块定义（已添加到 config.rs）

以下模块已添加到 `src/common/config.rs` 中，使用 `#[serde(skip)]` 注释，包含详细的功能说明：

```rust
// 媒体存储配置
// #[serde(skip)]
// pub struct MediaStoreConfig { ... }

// 监听器配置
// #[serde(skip)]
// pub struct ListenersConfig { ... }

// URL 预览配置
// #[serde(skip)]
// pub struct UrlPreviewConfig { ... }

// 限制配置
// #[serde(skip)]
// pub struct LimitsConfig { ... }

// 密码配置
// #[serde(skip)]
// pub struct PasswordConfig { ... }

// OIDC 配置
// #[serde(skip)]
// pub struct OidcConfig { ... }

// VoIP 配置
// #[serde(skip)]
// pub struct VoipConfig { ... }

// 推送配置
// #[serde(skip)]
// pub struct PushConfig { ... }

// 账户有效性配置
// #[serde(skip)]
// pub struct AccountValidityConfig { ... }

// CAS 认证配置
// #[serde(skip)]
// pub struct CasConfig { ... }

// SAML2 认证配置
// #[serde(skip)]
// pub struct Saml2Config { ... }

// UI 认证配置
// #[serde(skip)]
// pub struct UiAuthConfig { ... }

// 房间配置
// #[serde(skip)]
// pub struct RoomsConfig { ... }

// 消息保留配置
// #[serde(skip)]
// pub struct RetentionConfig { ... }

// 用户目录配置
// #[serde(skip)]
// pub struct UserDirectoryConfig { ... }

// 性能指标配置
// #[serde(skip)]
// pub struct MetricsConfig { ... }

// 客户端配置
// #[serde(skip)]
// pub struct ClientConfig { ... }

// 服务器通知配置
// #[serde(skip)]
// pub struct ServerNoticesConfig { ... }

// 捐献配置（Mautrix Whatsapp）
// #[serde(skip)]
// pub struct MjolnirConfig { ... }

// 第三方协议规则
// #[serde(skip)]
// pub struct ThirdPartyRulesConfig { ... }

// 实验性功能配置
// #[serde(skip)]
// pub struct ExperimentalConfig { ... }
```

### 3.2 已实现但需要增强的配置

#### ServerConfig 增强需求

当前字段：
```rust
pub struct ServerConfig {
    pub name: String,          // server_name
    pub host: String,          // 监听地址
    pub port: u16,             // 监听端口
    // ... 其他字段
}
```

建议新增：
```rust
pub struct ServerConfig {
    // 现有字段...

    // ===== 新增字段 =====

    /// 公开基础 URL（客户端访问地址）
    /// 示例: "https://matrix.example.com"
    pub public_baseurl: Option<String>,

    /// 签名密钥文件路径
    /// 用于联邦通信的签名密钥
    pub signing_key_path: Option<String>,

    /// Macaroon 密钥
    /// 用于访问令牌的 HMAC 签名
    pub macaroon_secret_key: Option<String>,

    /// 表单密钥
    /// 用于用户交互表单的签名
    pub form_secret: Option<String>,

    /// 服务器名称（与 name 相同，保留用于兼容）
    pub server_name: String,

    /// 是否抑制密钥服务器警告
    pub suppress_key_server_warning: bool,
}
```

---

## 四、实施步骤

### 步骤 1：修复 generate_event_id 问题

1. 修改所有 Storage 结构体，添加 `server_name` 字段
2. 更新 Storage::new() 方法接受 server_name 参数
3. 更新所有调用 Storage::new() 的地方
4. 替换硬编码 "localhost" 为 self.server_name

### 步骤 2：启用 ListenersConfig

1. 取消 `ListenersConfig` 的 `#[serde(skip)]` 注释
2. 实现 ListenersConfig 结构体的完整定义
3. 更新 ServerConfig 以支持从 ListenersConfig 获取监听配置
4. 修改 HTTP 服务器启动逻辑以支持多监听器

### 步骤 3：启用 MediaStoreConfig

1. 取消 `MediaStoreConfig` 的 `#[serde(skip)]` 注释
2. 实现媒体上传/下载 API
3. 配置媒体存储路径和 URL 前缀
4. 添加缩略图生成功能

### 步骤 4：启用密码配置

1. 取消 `PasswordConfig` 的 `#[serde(skip)]` 注释
2. 实现密码 pepper 支持
3. 配置启用的认证模块
4. 添加密码复杂度要求

### 步骤 5：逐步启用其他配置

根据优先级逐步启用其他配置模块。

---

## 五、配置示例文件

### 5.1 完整配置示例

```yaml
# 服务器配置
server:
  name: "example.com"
  host: "0.0.0.0"
  port: 8008
  public_baseurl: "https://matrix.example.com"
  signing_key_path: "/etc/synapse/signing_key.pem"
  macaroon_secret_key: "YOUR_MACAROON_SECRET"
  form_secret: "YOUR_FORM_SECRET"
  # ... 其他字段

# 数据库配置
database:
  host: "localhost"
  port: 5432
  username: "synapse"
  password: "your_password"
  name: "synapse"
  # ... 其他字段

# 监听器配置（待实现）
# listeners:
#   - type: http
#     port: 8008
#     resources:
#       - names: [client, federation]
#   - type: metrics
#     port: 9148

# 媒体存储配置（待实现）
# media_store:
#   enabled: true
#   storage_path: "/var/lib/synapse/media"
#   upload_size: "100M"

# URL 预览配置（待实现）
# url_preview:
#   enabled: true
#   url_blacklist: [...]

# 限制配置（待实现）
# limits:
#   upload_size: "100M"

# 密码配置（待实现）
# password_config:
#   enabled: true
#   pepper: "YOUR_PEPPER"
#   modules:
#     - module: "bcrypt"
#     - module: "argon2"

# OIDC 配置（待实现）
# oidc:
#   enabled: false
#   issuer: "https://your-oidc-provider"
#   client_id: "your-client-id"

# VoIP 配置（待实现）
# voip:
#   turn:
#     turn_uris: ["turn:turn.example.com:3478?transport=udp"]
#     turn_shared_secret: "YOUR_TURN_SECRET"
#   stun:
#     stun_uris: ["stun:stun.example.com:3478"]

# 推送配置（待实现）
# push:
#   enabled: false
#   # ...

# 用户目录配置（待实现）
# user_directory:
#   enabled: true
#   search_all_users: false

# 性能指标配置（待实现）
# metrics:
#   enabled: false
#   port: 9148
```

---

## 六、测试计划

1. **配置验证测试**：确保所有配置字段正确解析
2. **默认值测试**：验证所有默认配置值
3. **环境变量覆盖测试**：验证 SYNAPSE_* 环境变量
4. **热重载测试**：验证配置更新的正确性

---

## 七、文档更新

1. 更新 README.md 添加新配置说明
2. 创建 CONFIG.md 详细配置参考
3. 添加配置示例文件 examples/config.yaml
