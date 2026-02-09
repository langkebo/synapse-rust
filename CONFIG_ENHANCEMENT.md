# Synapse-Rust 配置文件增强说明

## 概述

本文档描述了对 `src/common/config.rs` 的增强，添加了官方 Synapse 中存在但在本 Rust 实现中缺失的配置选项。

---

## 一、已添加的配置模块（注释形式）

以下配置模块已添加到配置文件中，但使用注释形式暂时禁用。要启用某个模块，请：

1. 取消该结构体的 `/*` 和 `*/` 注释
2. 将该配置字段添加到主 `Config` 结构体中
3. 实现相应的功能代码

### 已添加的配置模块

| 模块 | 状态 | 优先级 | 功能说明 |
|------|------|--------|----------|
| `MediaStoreConfig` | 注释 | 🔴 高 | 媒体文件存储、上传、下载、缩略图 |
| `ListenersConfig` | 注释 | 🔴 高 | 多监听器配置、TLS、资源分离 |
| `UrlPreviewConfig` | 注释 | 🟡 中 | URL 预览、Open Graph 解析 |
| `LimitsConfig` | 注释 | 🟡 中 | 资源限制、事件大小限制 |
| `PasswordConfig` | 注释 | 🟡 中 | 密码策略、pepper、认证模块 |
| `OidcConfig` | 注释 | 🟢 低 | OpenID Connect SSO |
| `VoipConfig` | 注释 | 🟡 中 | TURN/STUN 服务器配置 |
| `PushConfig` | 注释 | 🟢 低 | 推送通知（APNs、FCM） |
| `AccountValidityConfig` | 注释 | 🟢 低 | 临时账户、账户有效期 |
| `CasConfig` | 注释 | 🟢 低 | CAS 单点登录 |
| `Saml2Config` | 注释 | 🟢 低 | SAML2 企业 SSO |
| `UiAuthConfig` | 注释 | 🟡 中 | 用户交互认证配置 |
| `RoomsConfig` | 注释 | 🟡 中 | 房间默认配置 |
| `RetentionConfig` | 注释 | 🟡 中 | 消息保留策略 |
| `UserDirectoryConfig` | 注释 | 🟡 中 | 用户搜索目录 |
| `MetricsConfig` | 注释 | 🟡 中 | Prometheus 指标 |
| `ClientConfig` | 注释 | 🟡 中 | 客户端行为参数 |
| `ServerNoticesConfig` | 注释 | 🟢 低 | 系统通知 |
| `ThirdPartyRulesConfig` | 注释 | 🟢 低 | 第三方协议桥接 |
| `ExperimentalConfig` | 注释 | 🟢 低 | MSC 实验性功能 |
| `SentryConfig` | 注释 | 🟢 低 | Sentry 错误追踪 |

---

## 二、ServerConfig 增强字段

`ServerConfig` 结构体已添加以下关键字段：

### 2.1 新增字段

| 字段 | 类型 | 说明 |
|------|------|------|
| `public_baseurl` | `Option<String>` | 客户端访问的公开 URL |
| `signing_key_path` | `Option<String>` | 签名密钥文件路径 |
| `macaroon_secret_key` | `Option<String>` | Macaroon 访问令牌密钥 |
| `form_secret` | `Option<String>` | UIAA 表单密钥 |
| `server_name` | `Option<String>` | 服务器名称（与 name 相同） |
| `suppress_key_server_warning` | `bool` | 抑制密钥服务器警告 |

### 2.2 新增方法

```rust
impl ServerConfig {
    /// 获取服务器名称（优先 server_name，回退到 name）
    pub fn get_server_name(&self) -> &str;

    /// 获取公开基础 URL
    pub fn get_public_baseurl(&self) -> String;

    /// 获取用于生成事件 ID 的服务器名称
    pub fn get_event_server_name(&self) -> &str;
}
```

---

## 三、generate_event_id 问题解决方案

### 问题描述

当前代码中 `generate_event_id("localhost")` 使用硬编码的服务器名称。

### 解决方案：修改 Storage 层初始化

#### 步骤 1：修改 Storage 结构体

```rust
// src/storage/membership.rs
#[derive(Clone)]
pub struct RoomMemberStorage {
    pub pool: Arc<Pool<Postgres>>,
    pub server_name: String,  // 新增
}

impl RoomMemberStorage {
    pub fn new(pool: &Arc<Pool<Postgres>>, server_name: &str) -> Self {
        Self {
            pool: pool.clone(),
            server_name: server_name.to_string(),
        }
    }

    pub async fn add_member(...) -> Result<RoomMember, sqlx::Error> {
        let event_id = format!("${}", generate_event_id(&self.server_name));
        // ...
    }
}
```

#### 步骤 2：更新初始化代码

```rust
// 在服务初始化时传递 server_name
let member_storage = RoomMemberStorage::new(&pool, &config.server.get_server_name());
```

#### 步骤 3：同样更新其他 Storage

需要更新的 Storage 结构体：
- `RoomMemberStorage`
- `RoomStorage`
- `UserStorage`
- `EventStorage`
- `PrivateChatStorage`
- `DeviceStorage`

---

## 四、配置文件示例

### 4.1 最小配置示例

```yaml
server:
  name: "example.com"
  host: "0.0.0.0"
  port: 8008
  public_baseurl: "https://matrix.example.com"
  signing_key_path: "/etc/synapse/signing_key.pem"
  macaroon_secret_key: "YOUR_MACAROON_SECRET"
  form_secret: "YOUR_FORM_SECRET"

database:
  host: "localhost"
  port: 5432
  username: "synapse"
  password: "your_password"
  name: "synapse"
```

### 4.2 完整配置示例（包含所有未实现模块）

```yaml
server:
  name: "example.com"
  host: "0.0.0.0"
  port: 8008
  public_baseurl: "https://matrix.example.com"
  signing_key_path: "/etc/synapse/signing_key.pem"
  macaroon_secret_key: "YOUR_MACAROON_SECRET"
  form_secret: "YOUR_FORM_SECRET"
  registration_shared_secret: "YOUR_REGISTRATION_SECRET"
  admin_contact: "admin@example.com"
  max_upload_size: 104857600
  max_image_resolution: 8000000
  enable_registration: true
  enable_registration_captcha: false
  background_tasks_interval: 60
  expire_access_token: true
  expire_access_token_lifetime: 3600
  refresh_token_lifetime: 604800
  refresh_token_sliding_window_size: 1000
  session_duration: 86400
  warmup_pool: true

database:
  host: "localhost"
  port: 5432
  username: "synapse"
  password: "your_password"
  name: "synapse"
  pool_size: 10
  max_size: 20
  min_idle: 5
  connection_timeout: 30

redis:
  host: "localhost"
  port: 6379
  key_prefix: "synapse:"
  pool_size: 10
  enabled: true

logging:
  level: "info"
  format: "json"
  log_file: "/var/log/synapse/synapse.log"
  log_dir: "/var/log/synapse"

federation:
  enabled: true
  allow_ingress: true
  server_name: "example.com"
  federation_port: 8448
  connection_pool_size: 10
  max_transaction_payload: 50000
  ca_file: "/etc/synapse/ca.crt"
  client_ca_file: null
  signing_key: null
  key_id: null

security:
  secret: "YOUR_JWT_SECRET"
  expiry_time: 3600
  refresh_token_expiry: 604800
  argon2_m_cost: 4096
  argon2_t_cost: 3
  argon2_p_cost: 1

search:
  elasticsearch_url: "http://localhost:9200"
  enabled: false

rate_limit:
  enabled: true
  per_second: 10
  burst_size: 20
  fail_open_on_error: false

admin_registration:
  enabled: false
  shared_secret: ""
  nonce_timeout_seconds: 60

worker:
  enabled: false
  instance_name: "master"
  worker_app: null
  instance_map: {}

cors:
  allowed_origins: ["*"]
  allow_credentials: false
  allowed_methods: ["GET", "POST", "PUT", "DELETE", "OPTIONS"]
  allowed_headers: ["Authorization", "Content-Type", "Accept", "X-Requested-With"]
  max_age_seconds: 86400

smtp:
  enabled: false
  host: "smtp.example.com"
  port: 587
  username: ""
  password: ""
  from: "noreply@example.com"
  tls: true
  verification_token_expire: 900
  rate_limit:
    per_minute: 3
    per_hour: 10

# ===== 以下配置模块已定义但未实现 =====

# 监听器配置（待实现）
# listeners:
#   - type: http
#     port: 8008
#     host: "::"
#     tls: false
#     x_forwarded: true
#     resources:
#       - names: [client, federation]
#         compress: true

# 媒体存储配置（待实现）
# media_store:
#   enabled: true
#   storage_path: "/var/lib/synapse/media"
#   upload_size: "100M"
#   url_preview_enabled: true

# URL 预览配置（待实现）
# url_preview:
#   enabled: true
#   spider_enabled: true
#   max_spider_size: "10M"

# 限制配置（待实现）
# limits:
#   upload_size: "100M"
#   room_join_complexity_limit: 10000

# 密码配置（待实现）
# password_config:
#   enabled: true
#   pepper: "YOUR_PASSWORD_PEPPER"
#   minimum_length: 8

# VoIP 配置（待实现）
# voip:
#   turn:
#     turn_uris: ["turn:turn.example.com:3478?transport=udp"]
#     turn_shared_secret: "YOUR_TURN_SECRET"
#   stun:
#     stun_uris: ["stun:stun.example.com:3478"]

# 推送配置（待实现）
# push:
#   enabled: true

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

## 五、启用配置模块的步骤

### 步骤 1：取消注释

在 `src/common/config.rs` 中找到对应的配置结构体，取消 `/*` 和 `*/` 注释。

### 步骤 2：添加到主 Config

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    // ... 现有字段 ...

    /// 新增配置
    #[serde(default)]
    pub listeners: ListenersConfig,

    #[serde(default)]
    pub media_store: MediaStoreConfig,
}
```

### 步骤 3：实现 Default

```rust
impl Default for YourConfig {
    fn default() -> Self {
        Self {
            // ... 默认值 ...
        }
    }
}
```

### 步骤 4：实现功能代码

- Service 层: `src/services/your_service.rs`
- Storage 层: `src/storage/your_storage.rs`
- Routes 层: `src/web/routes/your_routes.rs`

### 步骤 5：添加测试

- 单元测试: `tests/unit/your_tests.rs`
- 集成测试: `tests/integration/your_tests.rs`

---

## 六、参考文档

- 官方 Synapse 配置文档: https://matrix-org.github.io/synapse/latest/usage/configuration/config_documentation.html
- Matrix 规范: https://spec.matrix.org/
- Matrix 客户端服务器 API: https://spec.matrix.org/v1.11/client-server-api/
