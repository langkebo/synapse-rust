use serde::Deserialize;

// ============================================================================
// SECTION: Server Configuration
// ============================================================================

/// 服务器配置。
///
/// 配置 Matrix Homeserver 的网络和会话参数。
///
/// 官方 Synapse 对应配置: `server_name`, `public_baseurl`, `signing_key_path` 等
/// 文档: https://matrix-org.github.io/synapse/latest/usage/configuration/config_documentation.html#server
#[derive(Debug, Clone, Deserialize, Default)]
pub struct ServerConfig {
    /// 服务器名称（域名）
    /// Matrix 规范要求的唯一标识符，格式如 "example.com"
    pub name: String,

    /// 监听主机地址
    /// 默认 "0.0.0.0" 表示监听所有接口
    #[serde(default = "default_server_host")]
    pub host: String,

    /// 监听端口
    /// 默认 8008 (HTTP) 或 8448 (HTTPS)
    #[serde(default = "default_server_port")]
    pub port: u16,

    // ===== 新增关键字段 =====
    /// 公开基础 URL
    ///
    /// 客户端用于访问服务器的公开 URL。
    /// 当服务器位于反向代理后时必须设置。
    ///
    /// 示例: "https://matrix.example.com"
    ///
    /// 用途:
    /// - 生成 .well-known 响应
    /// - 构建客户端访问 URL
    /// - 生成事件 ID 的服务器名称部分
    #[serde(default)]
    pub public_baseurl: Option<String>,

    /// 签名密钥文件路径
    ///
    /// 用于联邦通信的 Ed25519 签名密钥文件路径。
    /// 如果不存在，服务器会在启动时自动生成。
    ///
    /// 示例: "/etc/synapse/signing_key.pem"
    ///
    /// 用途:
    /// - 签名服务器事件
    /// - 联邦通信身份验证
    /// - 生成事件 ID
    #[serde(default)]
    pub signing_key_path: Option<String>,

    /// Macaroon 密钥
    ///
    /// 用于生成和验证访问令牌（Macaroon）的 HMAC 密钥。
    /// 这个密钥必须保密，泄露会破坏访问令牌安全性。
    ///
    /// 生成方法: `openssl rand -hex 32`
    ///
    /// 用途:
    /// - 签名访问令牌
    /// - 验证令牌完整性
    #[serde(default)]
    pub macaroon_secret_key: Option<String>,

    /// 表单密钥
    ///
    /// 用于用户交互认证（UIAA）表单的 HMAC 密钥。
    ///
    /// 生成方法: `openssl rand -hex 32`
    ///
    /// 用途:
    /// - UIAA 会话签名
    /// - 防止表单伪造
    #[serde(default)]
    pub form_secret: Option<String>,

    /// 服务器名称（与 name 字段相同）
    ///
    /// 保留此字段是为了与官方 Synapse 配置命名保持一致。
    /// 在代码中应该统一使用此字段而非 `name`。
    #[serde(default)]
    pub server_name: Option<String>,

    /// 是否抑制密钥服务器警告
    ///
    /// 当没有配置密钥服务器时是否显示警告。
    /// 密钥服务器用于端到端加密设备密钥的备份和恢复。
    #[serde(default = "default_suppress_key_server_warning")]
    pub suppress_key_server_warning: bool,

    /// 是否提供 .well-known 服务
    ///
    /// 启用后，服务器将在 https://<server_name>/.well-known/matrix/server
    /// 提供服务，告诉其他服务器将联邦流量发送到端口 443 而非 8448。
    #[serde(default)]
    pub serve_server_wellknown: bool,

    /// 文件描述符软限制
    ///
    /// 设置 synapse 可以使用的文件描述符数量的软限制。
    /// 设置为 0 表示使用硬限制。
    #[serde(default)]
    pub soft_file_limit: u32,

    /// 用户代理后缀
    ///
    /// 附加到 Synapse 用户代理字符串后的后缀。
    #[serde(default)]
    pub user_agent_suffix: Option<String>,

    /// Web 客户端位置
    ///
    /// 当用户访问根路径时重定向到的 Web 客户端 URL。
    #[serde(default)]
    pub web_client_location: Option<String>,

    // ===== 原有字段 =====
    /// 注册共享密钥（用于管理员注册）
    pub registration_shared_secret: Option<String>,

    /// 管理员联系邮箱
    pub admin_contact: Option<String>,

    /// 最大上传大小（字节）
    #[serde(default = "default_max_upload_size_value")]
    pub max_upload_size: u64,

    /// 最大图片分辨率
    pub max_image_resolution: u32,

    /// 远程媒体缓存保留时间（秒），默认 30 天
    #[serde(default = "default_remote_media_lifetime")]
    pub remote_media_lifetime: u64,

    /// 本地媒体保留时间（秒），0 表示永不过期
    #[serde(default)]
    pub local_media_lifetime: u64,

    /// 是否允许用户注册
    pub enable_registration: bool,

    /// 是否启用注册验证码
    pub enable_registration_captcha: bool,

    /// 后台任务执行间隔（秒）
    pub background_tasks_interval: u64,

    /// 脱水设备过期清理任务执行间隔（秒）
    #[serde(default = "default_dehydrated_device_cleanup_interval_secs")]
    pub dehydrated_device_cleanup_interval_secs: u64,

    /// 是否使访问令牌过期
    pub expire_access_token: bool,

    /// 访问令牌过期时间
    pub expire_access_token_lifetime: i64,

    /// 刷新令牌生命周期
    pub refresh_token_lifetime: i64,

    /// 刷新令牌滑动窗口大小
    pub refresh_token_sliding_window_size: i64,

    /// 会话持续时间
    pub session_duration: i64,

    #[serde(default = "default_warmup_pool")]
    pub warmup_pool: bool,

    /// 是否允许未认证用户访问公共房间目录
    #[serde(default)]
    pub allow_public_rooms_without_auth: bool,

    /// 是否允许通过联邦访问公共房间目录
    #[serde(default = "default_true")]
    pub allow_public_rooms_over_federation: bool,

    /// 新用户自动加入的房间列表
    #[serde(default)]
    pub auto_join_rooms: Vec<String>,

    /// 是否自动创建 auto_join_rooms 中不存在的房间
    #[serde(default = "default_true")]
    pub autocreate_auto_join_rooms: bool,

    /// 默认启用加密的房间类型（空表示不默认启用）
    #[serde(default)]
    pub encryption_enabled_by_default_for_room_type: Option<String>,

    /// 应用服务配置文件路径列表
    #[serde(default)]
    pub app_service_config_files: Vec<String>,

    /// 是否启用 Presence 功能
    #[serde(default = "default_true")]
    pub presence_enabled: bool,

    /// 媒体文件存储路径。
    ///
    /// 控制媒体服务把上传的文件写到哪个目录。可通过标准环境变量覆盖
    /// 机制 `SYNAPSE__SERVER__MEDIA_PATH` 覆盖。默认 `./data/media`。
    #[serde(default = "default_media_path")]
    pub media_path: String,

    /// Megolm 加密密钥文件路径。
    ///
    /// 用于持久化 E2EE megolm 会话的加密密钥。可通过标准环境变量覆盖
    /// 机制 `SYNAPSE__SERVER__MEGOLM_ENCRYPTION_KEY_PATH` 覆盖。
    /// 未设置时服务器会生成临时密钥并在重启后丢失已加密的会话。
    #[serde(default)]
    pub megolm_encryption_key_path: Option<String>,

    /// 是否启动 burn-after-read 处理器。
    ///
    /// 默认 `true`。可通过标准环境变量覆盖机制
    /// `SYNAPSE__SERVER__ENABLE_BURN_AFTER_READ_PROCESSOR` 覆盖。
    #[serde(default = "default_true")]
    pub enable_burn_after_read_processor: bool,

    /// 刷新令牌 TTL（秒），默认 30 天。
    ///
    /// 仅在 `ServiceContainer` 装配 `RefreshTokenService` 时使用，
    /// 与 `refresh_token_lifetime` 字段独立。
    #[serde(default = "default_refresh_token_ttl_secs")]
    pub refresh_token_ttl_secs: i64,
}

fn default_suppress_key_server_warning() -> bool {
    false
}

fn default_server_host() -> String {
    "0.0.0.0".to_string()
}

fn default_server_port() -> u16 {
    8008
}

fn default_max_upload_size_value() -> u64 {
    50000000
}

fn default_remote_media_lifetime() -> u64 {
    2592000
}

pub fn default_dehydrated_device_cleanup_interval_secs() -> u64 {
    3600
}

fn default_warmup_pool() -> bool {
    true
}

fn default_true() -> bool {
    true
}

fn default_media_path() -> String {
    "./data/media".to_string()
}

fn default_refresh_token_ttl_secs() -> i64 {
    2_592_000
}

impl ServerConfig {
    /// 获取服务器名称。
    ///
    /// 优先使用 `server_name` 字段，如果不存在则使用 `name` 字段。
    /// 这样可以平滑迁移配置格式。
    pub fn get_server_name(&self) -> &str {
        self.server_name.as_ref().unwrap_or(&self.name)
    }

    /// 获取公开基础 URL。
    ///
    /// 如果未配置 public_baseurl，则根据 host 和 port 构造默认值。
    /// `0.0.0.0` 是绑定地址而非可达地址，会被回退到 `localhost` 以避免
    /// 客户端拿到一个无法访问的 URL。
    pub fn get_public_baseurl(&self) -> String {
        if let Some(baseurl) = &self.public_baseurl {
            if !baseurl.is_empty() {
                return baseurl.clone();
            }
        }
        let host = if self.host == "0.0.0.0" || self.host == "::" { "localhost" } else { self.host.as_str() };
        format!("http://{}:{}", host, self.port)
    }

    /// 获取事件 ID 生成用的服务器名称。
    ///
    /// 这是 generate_event_id 函数使用的服务器名称。
    /// 优先使用配置中的 server_name，回退到 name 字段。
    pub fn get_event_server_name(&self) -> &str {
        self.get_server_name()
    }
}
