use config::Config as ConfigBuilder;
use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
// The issue is that parking_lot::RwLock doesn't poison on panic unlike std::sync::RwLock
// parking_lot's lock() methods return &T directly, not Result<_, PoisonError<T>>

use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Config load error: {0}")]
    LoadError(String),
    #[error("Config parse error: {0}")]
    ParseError(String),
    #[error("Validation error: {0}")]
    ValidationError(String),
}

/// 服务器配置结构。
///
/// Matrix Homeserver 的主配置类，包含所有配置子项。
/// 通过环境变量或配置文件加载。
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// 服务器配置
    pub server: ServerConfig,
    /// 数据库配置
    pub database: DatabaseConfig,
    /// Redis 配置
    pub redis: RedisConfig,
    /// 日志配置
    pub logging: LoggingConfig,
    /// 联邦配置
    pub federation: FederationConfig,
    /// 安全配置
    pub security: SecurityConfig,
    /// 搜索配置
    pub search: SearchConfig,
    /// 限流配置
    #[serde(default)]
    pub rate_limit: RateLimitConfig,
    /// 管理员注册配置
    #[serde(default)]
    pub admin_registration: AdminRegistrationConfig,
    /// 工作节点配置
    #[serde(default)]
    pub worker: WorkerConfig,
    /// CORS 配置
    #[serde(default)]
    pub cors: CorsConfig,
    /// SMTP邮件配置
    #[serde(default)]
    pub smtp: SmtpConfig,
}

/// 搜索服务配置。
#[derive(Debug, Clone, Deserialize)]
pub struct SearchConfig {
    /// Elasticsearch 服务器 URL
    pub elasticsearch_url: String,
    /// 是否启用搜索功能
    pub enabled: bool,
}

/// 限流配置。
///
/// 配置 API 请求限流规则，包括全局限流和端点级限流。
#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitConfig {
    /// 是否启用限流
    #[serde(default = "default_rate_limit_enabled")]
    pub enabled: bool,
    /// 默认限流规则
    #[serde(default)]
    pub default: RateLimitRule,
    /// 端点级限流规则
    #[serde(default)]
    pub endpoints: Vec<RateLimitEndpointRule>,
    /// IP 头优先级列表
    #[serde(default)]
    pub ip_header_priority: Vec<String>,
    /// 是否包含请求头进行限流判断
    #[serde(default)]
    pub include_headers: bool,
    /// 豁免路径列表
    #[serde(default)]
    pub exempt_paths: Vec<String>,
    /// 豁免路径前缀列表
    #[serde(default)]
    pub exempt_path_prefixes: Vec<String>,
    /// 端点别名映射
    #[serde(default)]
    pub endpoint_aliases: HashMap<String, String>,
    /// 错误时是否开放访问
    #[serde(default = "default_rate_limit_fail_open")]
    pub fail_open_on_error: bool,
}

fn default_rate_limit_enabled() -> bool {
    true
}

fn default_rate_limit_fail_open() -> bool {
    false
}

/// 单个限流规则。
///
/// 定义令牌桶算法的参数：每秒补充令牌数和桶容量。
#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitRule {
    /// 每秒允许的请求数
    #[serde(default = "default_rate_limit_per_second")]
    pub per_second: u32,
    /// 令牌桶容量（突发请求数）
    #[serde(default = "default_rate_limit_burst_size")]
    pub burst_size: u32,
}

fn default_rate_limit_per_second() -> u32 {
    10
}

fn default_rate_limit_burst_size() -> u32 {
    20
}

impl Default for RateLimitRule {
    fn default() -> Self {
        Self {
            per_second: default_rate_limit_per_second(),
            burst_size: default_rate_limit_burst_size(),
        }
    }
}

/// 端点级限流规则。
///
/// 为特定 API 路径配置独立的限流参数。
#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitEndpointRule {
    /// 匹配的路径
    pub path: String,
    /// 路径匹配类型
    #[serde(default)]
    pub match_type: RateLimitMatchType,
    /// 该路径的限流规则
    pub rule: RateLimitRule,
}

/// 路径匹配类型。
#[derive(Debug, Clone, Copy, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum RateLimitMatchType {
    /// 精确匹配
    #[default]
    Exact,
    /// 前缀匹配
    Prefix,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: default_rate_limit_enabled(),
            default: RateLimitRule::default(),
            endpoints: Vec::new(),
            ip_header_priority: vec![
                "x-forwarded-for".to_string(),
                "x-real-ip".to_string(),
                "forwarded".to_string(),
            ],
            include_headers: true,
            exempt_paths: vec!["/".to_string(), "/_matrix/client/versions".to_string()],
            exempt_path_prefixes: Vec::new(),
            endpoint_aliases: HashMap::new(),
            fail_open_on_error: default_rate_limit_fail_open(),
        }
    }
}

/// 配置管理器。
///
/// 提供线程安全的配置访问和更新方法。
pub struct ConfigManager {
    /// 内部配置存储
    config: Arc<RwLock<Config>>,
}

impl ConfigManager {
    /// 创建新的配置管理器。
    ///
    /// # 参数
    ///
    /// * `config` - 初始配置
    pub fn new(config: Config) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
        }
    }

    /// 安全读取配置（只读）。
    ///
    /// # 返回值
    ///
    /// 成功时返回配置只读引用
    fn read_config(&self, _location: &str) -> RwLockReadGuard<'_, Config> {
        self.config.read()
    }

    /// 安全写入配置（可写）。
    ///
    /// # 返回值
    ///
    /// 成功时返回配置可变引用
    fn write_config(&self, _location: &str) -> RwLockWriteGuard<'_, Config> {
        self.config.write()
    }

    /// 获取服务器名称。
    ///
    /// # 返回值
    ///
    /// 返回服务器名称字符串
    pub fn get_server_name(&self) -> String {
        let config = self.read_config("get_server_name");
        config.server.name.clone()
    }

    /// 获取服务器主机地址。
    ///
    /// # 返回值
    ///
    /// 返回服务器主机字符串
    pub fn get_server_host(&self) -> String {
        let config = self.read_config("get_server_host");
        config.server.host.clone()
    }

    /// 获取服务器端口。
    ///
    /// # 返回值
    ///
    /// 返回服务器端口号
    pub fn get_server_port(&self) -> u16 {
        let config = self.read_config("get_server_port");
        config.server.port
    }

    /// 获取数据库连接 URL。
    ///
    /// # 返回值
    ///
    /// 返回 PostgreSQL 连接字符串
    pub fn get_database_url(&self) -> String {
        let config = self.read_config("get_database_url");
        format!(
            "postgres://{}:{}@{}:{}/{}",
            config.database.username,
            config.database.password,
            config.database.host,
            config.database.port,
            config.database.name
        )
    }

    /// 获取 Redis 连接 URL。
    ///
    /// # 返回值
    ///
    /// 返回 Redis 连接字符串
    pub fn get_redis_url(&self) -> String {
        let config = self.read_config("get_redis_url");
        format!("redis://{}:{}", config.redis.host, config.redis.port)
    }

    /// 获取完整配置副本。
    ///
    /// # 返回值
    ///
    /// 返回配置克隆
    pub fn get_config(&self) -> Config {
        let config = self.read_config("get_config");
        config.clone()
    }

    /// 更新配置。
    ///
    /// # 参数
    ///
    /// * `f` - 配置更新闭包
    ///
    /// # 返回值
    ///
    /// 成功时返回 `Ok(())`，失败时返回错误
    pub fn update_config<F>(&self, f: F)
    where
        F: FnOnce(&mut Config),
    {
        let mut config = self.write_config("update_config");
        f(&mut config);
    }
}

impl Clone for ConfigManager {
    fn clone(&self) -> Self {
        Self {
            config: Arc::clone(&self.config),
        }
    }
}

/// 服务器配置。
///
/// 配置 Matrix Homeserver 的网络和会话参数。
///
/// 官方 Synapse 对应配置: `server_name`, `public_baseurl`, `signing_key_path` 等
/// 文档: https://matrix-org.github.io/synapse/latest/usage/configuration/config_documentation.html#server
#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    /// 服务器名称（域名）
    /// Matrix 规范要求的唯一标识符，格式如 "example.com"
    pub name: String,

    /// 监听主机地址
    /// 默认 "0.0.0.0" 表示监听所有接口
    pub host: String,

    /// 监听端口
    /// 默认 8008 (HTTP) 或 8448 (HTTPS)
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

    // ===== 原有字段 =====

    /// 注册共享密钥（用于管理员注册）
    pub registration_shared_secret: Option<String>,

    /// 管理员联系邮箱
    pub admin_contact: Option<String>,

    /// 最大上传大小（字节）
    pub max_upload_size: u64,

    /// 最大图片分辨率
    pub max_image_resolution: u32,

    /// 是否允许用户注册
    pub enable_registration: bool,

    /// 是否启用注册验证码
    pub enable_registration_captcha: bool,

    /// 后台任务执行间隔（秒）
    pub background_tasks_interval: u64,

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
}

fn default_suppress_key_server_warning() -> bool {
    false
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
    pub fn get_public_baseurl(&self) -> String {
        if let Some(baseurl) = &self.public_baseurl {
            baseurl.clone()
        } else {
            format!("http://{}:{}", self.host, self.port)
        }
    }

    /// 获取事件 ID 生成用的服务器名称。
    ///
    /// 这是 generate_event_id 函数使用的服务器名称。
    /// 优先使用配置中的 server_name，回退到 name 字段。
    pub fn get_event_server_name(&self) -> &str {
        self.get_server_name()
    }
}

fn default_warmup_pool() -> bool {
    true
}

/// 数据库连接配置。
#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    /// 数据库主机地址
    pub host: String,
    /// 数据库端口
    pub port: u16,
    /// 数据库用户名
    pub username: String,
    /// 数据库密码
    pub password: String,
    /// 数据库名称
    pub name: String,
    /// 连接池大小
    pub pool_size: u32,
    /// 最大连接数
    pub max_size: u32,
    /// 最小空闲连接数
    pub min_idle: Option<u32>,
    /// 连接超时时间（秒）
    pub connection_timeout: u64,
}

/// Redis 缓存配置。
#[derive(Debug, Clone, Deserialize)]
pub struct RedisConfig {
    /// Redis 主机地址
    pub host: String,
    /// Redis 端口
    pub port: u16,
    /// 缓存键前缀
    pub key_prefix: String,
    /// 连接池大小
    pub pool_size: u32,
    /// 是否启用 Redis 缓存
    pub enabled: bool,
}

/// 日志配置。
#[derive(Debug, Clone, Deserialize)]
pub struct LoggingConfig {
    /// 日志级别
    pub level: String,
    /// 日志格式
    pub format: String,
    /// 日志文件路径
    pub log_file: Option<String>,
    /// 日志目录
    pub log_dir: Option<String>,
}

/// 联邦配置。
///
/// 配置与其他 Matrix 服务器的联邦通信参数。
#[derive(Debug, Clone, Deserialize)]
pub struct FederationConfig {
    /// 是否启用联邦功能
    pub enabled: bool,
    /// 是否允许入口
    pub allow_ingress: bool,
    /// 联邦服务器名称
    pub server_name: String,
    /// 联邦通信端口
    pub federation_port: u16,
    /// 连接池大小
    pub connection_pool_size: u32,
    /// 最大事务负载大小
    pub max_transaction_payload: u64,
    /// CA 证书文件
    pub ca_file: Option<PathBuf>,
    /// 客户端 CA 证书文件
    pub client_ca_file: Option<PathBuf>,
    /// 签名密钥
    pub signing_key: Option<String>,
    /// 密钥 ID
    pub key_id: Option<String>,
}

/// 安全配置。
///
/// 配置认证、加密和密码哈希参数。
#[derive(Debug, Clone, Deserialize)]
pub struct SecurityConfig {
    /// 密钥字符串
    pub secret: String,
    /// 令牌过期时间
    pub expiry_time: i64,
    /// 刷新令牌过期时间
    pub refresh_token_expiry: i64,
    /// Argon2 内存成本
    #[serde(default = "default_argon2_m_cost")]
    pub argon2_m_cost: u32,
    /// Argon2 时间成本
    #[serde(default = "default_argon2_t_cost")]
    pub argon2_t_cost: u32,
    /// Argon2 并行度
    #[serde(default = "default_argon2_p_cost")]
    pub argon2_p_cost: u32,
}

fn default_argon2_m_cost() -> u32 {
    4096
}

fn default_argon2_t_cost() -> u32 {
    3
}

fn default_argon2_p_cost() -> u32 {
    1
}

/// CORS 配置。
///
/// 配置跨域资源共享策略。
#[derive(Debug, Clone, Deserialize)]
pub struct CorsConfig {
    /// 允许的来源列表
    #[serde(default = "default_allowed_origins")]
    pub allowed_origins: Vec<String>,
    /// 是否允许凭证
    #[serde(default = "default_allow_credentials")]
    pub allow_credentials: bool,
    /// 允许的 HTTP 方法
    #[serde(default = "default_allowed_methods")]
    pub allowed_methods: Vec<String>,
    /// 允许的请求头
    #[serde(default = "default_allowed_headers")]
    pub allowed_headers: Vec<String>,
    /// 预检请求最大缓存时间（秒）
    #[serde(default = "default_cors_max_age")]
    pub max_age_seconds: u64,
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self {
            allowed_origins: default_allowed_origins(),
            allow_credentials: default_allow_credentials(),
            allowed_methods: default_allowed_methods(),
            allowed_headers: default_allowed_headers(),
            max_age_seconds: default_cors_max_age(),
        }
    }
}

fn default_allowed_origins() -> Vec<String> {
    vec!["*".to_string()]
}

fn default_allow_credentials() -> bool {
    false
}

fn default_allowed_methods() -> Vec<String> {
    vec![
        "GET".to_string(),
        "POST".to_string(),
        "PUT".to_string(),
        "DELETE".to_string(),
        "OPTIONS".to_string(),
    ]
}

fn default_allowed_headers() -> Vec<String> {
    vec![
        "Authorization".to_string(),
        "Content-Type".to_string(),
        "Accept".to_string(),
        "X-Requested-With".to_string(),
    ]
}

fn default_cors_max_age() -> u64 {
    86400
}

#[derive(Debug, Clone, Deserialize)]
pub struct AdminRegistrationConfig {
    #[serde(default = "default_admin_registration_enabled")]
    pub enabled: bool,
    #[serde(default = "default_admin_registration_shared_secret")]
    pub shared_secret: String,
    #[serde(default = "default_admin_registration_nonce_timeout")]
    pub nonce_timeout_seconds: u64,
}

fn default_admin_registration_enabled() -> bool {
    false
}

fn default_admin_registration_shared_secret() -> String {
    "".to_string()
}

fn default_admin_registration_nonce_timeout() -> u64 {
    60
}

impl Default for AdminRegistrationConfig {
    fn default() -> Self {
        Self {
            enabled: default_admin_registration_enabled(),
            shared_secret: default_admin_registration_shared_secret(),
            nonce_timeout_seconds: default_admin_registration_nonce_timeout(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorkerConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_worker_instance_name")]
    pub instance_name: String,
    #[serde(default)]
    pub worker_app: Option<String>,
    #[serde(default)]
    pub instance_map: HashMap<String, InstanceLocationConfig>,
    #[serde(default)]
    pub stream_writers: StreamWriters,
    #[serde(default)]
    pub replication: ReplicationConfig,
}

fn default_worker_instance_name() -> String {
    "master".to_string()
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            instance_name: default_worker_instance_name(),
            worker_app: None,
            instance_map: HashMap::new(),
            stream_writers: StreamWriters::default(),
            replication: ReplicationConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct InstanceLocationConfig {
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub tls: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StreamWriters {
    #[serde(default = "default_stream_writers")]
    pub events: Vec<String>,
    #[serde(default = "default_stream_writers")]
    pub typing: Vec<String>,
    #[serde(default = "default_stream_writers")]
    pub to_device: Vec<String>,
    #[serde(default = "default_stream_writers")]
    pub account_data: Vec<String>,
    #[serde(default = "default_stream_writers")]
    pub receipts: Vec<String>,
    #[serde(default = "default_stream_writers")]
    pub presence: Vec<String>,
    #[serde(default = "default_stream_writers")]
    pub push_rules: Vec<String>,
    #[serde(default = "default_stream_writers")]
    pub device_lists: Vec<String>,
}

fn default_stream_writers() -> Vec<String> {
    vec![default_worker_instance_name()]
}

impl Default for StreamWriters {
    fn default() -> Self {
        let default = default_stream_writers();
        Self {
            events: default.clone(),
            typing: default.clone(),
            to_device: default.clone(),
            account_data: default.clone(),
            receipts: default.clone(),
            presence: default.clone(),
            push_rules: default.clone(),
            device_lists: default,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ReplicationConfig {
    pub enabled: bool,
    pub server_name: String,
    pub http: ReplicationHttpConfig,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ReplicationHttpConfig {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub secret: Option<String>,
    pub secret_path: Option<String>,
}

impl Config {
    pub async fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let config_path = std::env::var("SYNAPSE_CONFIG_PATH")
            .unwrap_or_else(|_| "/app/config/homeserver.yaml".to_string());

        let config = ConfigBuilder::builder()
            .add_source(config::File::with_name(&config_path))
            .add_source(config::Environment::with_prefix("SYNAPSE"))
            .build()?;

        let config_values: Config = config.try_deserialize()?;
        Ok(config_values)
    }

    pub fn database_url(&self) -> String {
        format!(
            "postgres://{}:{}@{}:{}/{}",
            self.database.username,
            self.database.password,
            self.database.host,
            self.database.port,
            self.database.name
        )
    }

    pub fn redis_url(&self) -> String {
        format!("redis://{}:{}", self.redis.host, self.redis.port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_database_url() {
        let config = Config {
            server: ServerConfig {
                name: "test".to_string(),
                host: "127.0.0.1".to_string(),
                port: 8000,
                public_baseurl: None,
                signing_key_path: None,
                macaroon_secret_key: None,
                form_secret: None,
                server_name: None,
                suppress_key_server_warning: false,
                registration_shared_secret: None,
                admin_contact: None,
                max_upload_size: 1000000,
                max_image_resolution: 1000000,
                enable_registration: true,
                enable_registration_captcha: false,
                background_tasks_interval: 60,
                expire_access_token: true,
                expire_access_token_lifetime: 3600,
                refresh_token_lifetime: 604800,
                refresh_token_sliding_window_size: 1000,
                session_duration: 86400,
                warmup_pool: true,
            },
            database: DatabaseConfig {
                host: "localhost".to_string(),
                port: 5432,
                username: "testuser".to_string(),
                password: "testpass".to_string(),
                name: "testdb".to_string(),
                pool_size: 10,
                max_size: 20,
                min_idle: Some(5),
                connection_timeout: 30,
            },
            redis: RedisConfig {
                host: "localhost".to_string(),
                port: 6379,
                key_prefix: "test:".to_string(),
                pool_size: 10,
                enabled: true,
            },
            logging: LoggingConfig {
                level: "info".to_string(),
                format: "json".to_string(),
                log_file: None,
                log_dir: None,
            },
            federation: FederationConfig {
                enabled: true,
                allow_ingress: false,
                server_name: "test.example.com".to_string(),
                federation_port: 8448,
                connection_pool_size: 10,
                max_transaction_payload: 50000,
                ca_file: None,
                client_ca_file: None,
                signing_key: Some("test_signing_key".to_string()),
                key_id: Some("ed25519:test_key".to_string()),
            },
            security: SecurityConfig {
                secret: "test_secret".to_string(),
                expiry_time: 3600,
                refresh_token_expiry: 604800,
                argon2_m_cost: 4096,
                argon2_t_cost: 3,
                argon2_p_cost: 1,
            },
            search: SearchConfig {
                elasticsearch_url: "http://localhost:9200".to_string(),
                enabled: false,
            },
            rate_limit: RateLimitConfig::default(),
            admin_registration: AdminRegistrationConfig::default(),
            worker: WorkerConfig::default(),
            cors: CorsConfig {
                allowed_origins: vec!["*".to_string()],
                allow_credentials: false,
                allowed_methods: default_allowed_methods(),
                allowed_headers: default_allowed_headers(),
                max_age_seconds: default_cors_max_age(),
            },
            smtp: SmtpConfig::default(),
        };

        let url = config.database_url();
        assert_eq!(url, "postgres://testuser:testpass@localhost:5432/testdb");
    }

    #[test]
    fn test_config_redis_url() {
        let config = Config {
            server: ServerConfig {
                name: "test".to_string(),
                host: "127.0.0.1".to_string(),
                port: 8000,
                public_baseurl: None,
                signing_key_path: None,
                macaroon_secret_key: None,
                form_secret: None,
                server_name: None,
                suppress_key_server_warning: false,
                registration_shared_secret: None,
                admin_contact: None,
                max_upload_size: 1000000,
                max_image_resolution: 1000000,
                enable_registration: true,
                enable_registration_captcha: false,
                background_tasks_interval: 60,
                expire_access_token: true,
                expire_access_token_lifetime: 3600,
                refresh_token_lifetime: 604800,
                refresh_token_sliding_window_size: 1000,
                session_duration: 86400,
                warmup_pool: true,
            },
            database: DatabaseConfig {
                host: "localhost".to_string(),
                port: 5432,
                username: "testuser".to_string(),
                password: "testpass".to_string(),
                name: "testdb".to_string(),
                pool_size: 10,
                max_size: 20,
                min_idle: Some(5),
                connection_timeout: 30,
            },
            redis: RedisConfig {
                host: "redis.example.com".to_string(),
                port: 6380,
                key_prefix: "prod:".to_string(),
                pool_size: 20,
                enabled: true,
            },
            logging: LoggingConfig {
                level: "info".to_string(),
                format: "json".to_string(),
                log_file: None,
                log_dir: None,
            },
            federation: FederationConfig {
                enabled: true,
                allow_ingress: false,
                server_name: "test.example.com".to_string(),
                federation_port: 8448,
                connection_pool_size: 10,
                max_transaction_payload: 50000,
                ca_file: None,
                client_ca_file: None,
                signing_key: Some("test_signing_key".to_string()),
                key_id: Some("ed25519:test_key".to_string()),
            },
            security: SecurityConfig {
                secret: "test_secret".to_string(),
                expiry_time: 3600,
                refresh_token_expiry: 604800,
                argon2_m_cost: 4096,
                argon2_t_cost: 3,
                argon2_p_cost: 1,
            },
            search: SearchConfig {
                elasticsearch_url: "http://localhost:9200".to_string(),
                enabled: false,
            },
            rate_limit: RateLimitConfig::default(),
            admin_registration: AdminRegistrationConfig::default(),
            worker: WorkerConfig::default(),
            cors: CorsConfig {
                allowed_origins: vec!["*".to_string()],
                allow_credentials: false,
                allowed_methods: default_allowed_methods(),
                allowed_headers: default_allowed_headers(),
                max_age_seconds: default_cors_max_age(),
            },
            smtp: SmtpConfig::default(),
        };

        let url = config.redis_url();
        assert_eq!(url, "redis://redis.example.com:6380");
    }

    #[test]
    fn test_server_config_defaults() {
        let config = ServerConfig {
            name: "test".to_string(),
            host: "0.0.0.0".to_string(),
            port: 8080,
            public_baseurl: None,
            signing_key_path: None,
            macaroon_secret_key: None,
            form_secret: None,
            server_name: None,
            suppress_key_server_warning: false,
            registration_shared_secret: Some("secret".to_string()),
            admin_contact: Some("admin@example.com".to_string()),
            max_upload_size: 50000000,
            max_image_resolution: 8000000,
            enable_registration: true,
            enable_registration_captcha: true,
            background_tasks_interval: 30,
            expire_access_token: true,
            expire_access_token_lifetime: 86400,
            refresh_token_lifetime: 2592000,
            refresh_token_sliding_window_size: 5000,
            session_duration: 3600,
            warmup_pool: true,
        };

        assert_eq!(config.name, "test");
        assert_eq!(config.port, 8080);
        assert!(config.enable_registration);
        assert!(config.registration_shared_secret.is_some());
    }

    #[test]
    fn test_database_config_defaults() {
        let config = DatabaseConfig {
            host: "db.example.com".to_string(),
            port: 5432,
            username: "synapse".to_string(),
            password: "secure_password".to_string(),
            name: "synapse".to_string(),
            pool_size: 10,
            max_size: 20,
            min_idle: None,
            connection_timeout: 60,
        };

        assert_eq!(config.host, "db.example.com");
        assert_eq!(config.port, 5432);
        assert!(config.min_idle.is_none());
    }

    #[test]
    fn test_redis_config_defaults() {
        let config = RedisConfig {
            host: "127.0.0.1".to_string(),
            port: 6379,
            key_prefix: "synapse:".to_string(),
            pool_size: 16,
            enabled: true,
        };

        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 6379);
        assert!(config.enabled);
    }

    #[test]
    fn test_logging_config_with_file() {
        let config = LoggingConfig {
            level: "debug".to_string(),
            format: "text".to_string(),
            log_file: Some("/var/log/synapse.log".to_string()),
            log_dir: Some("/var/log".to_string()),
        };

        assert_eq!(config.level, "debug");
        assert!(config.log_file.is_some());
        assert!(config.log_dir.is_some());
    }

    #[test]
    fn test_federation_config_defaults() {
        let config = FederationConfig {
            enabled: true,
            allow_ingress: true,
            server_name: "federation.example.com".to_string(),
            federation_port: 8448,
            connection_pool_size: 50,
            max_transaction_payload: 100000,
            ca_file: Some(PathBuf::from("/etc/synapse/ca.crt")),
            client_ca_file: None,
            signing_key: None,
            key_id: None,
        };

        assert!(config.enabled);
        assert!(config.allow_ingress);
        assert!(config.ca_file.is_some());
    }

    #[test]
    fn test_security_config_defaults() {
        let config = SecurityConfig {
            secret: "very_secure_secret_key".to_string(),
            expiry_time: 3600,
            refresh_token_expiry: 604800,
            argon2_m_cost: 4096,
            argon2_t_cost: 3,
            argon2_p_cost: 1,
        };

        assert!(config.secret.len() > 16);
        assert_eq!(config.argon2_m_cost, 4096);
    }
}

/// SMTP邮件服务配置。
///
/// 配置用于发送验证邮件的SMTP服务器参数。
#[derive(Debug, Clone, Deserialize, Default)]
pub struct SmtpConfig {
    /// 是否启用SMTP功能
    #[serde(default = "default_smtp_enabled")]
    pub enabled: bool,
    /// SMTP服务器地址
    #[serde(default)]
    pub host: String,
    /// SMTP服务器端口
    #[serde(default = "default_smtp_port")]
    pub port: u16,
    /// SMTP用户名
    #[serde(default)]
    pub username: String,
    /// SMTP密码
    #[serde(default)]
    pub password: String,
    /// 发件人地址
    #[serde(default)]
    pub from: String,
    /// 是否使用TLS
    #[serde(default = "default_true")]
    pub tls: bool,
    /// 验证码有效期（秒）
    #[serde(default = "default_verification_expire")]
    pub verification_token_expire: i64,
    /// 速率限制配置
    #[serde(default)]
    pub rate_limit: SmtpRateLimitConfig,
}

fn default_smtp_enabled() -> bool {
    false
}

fn default_smtp_port() -> u16 {
    587
}

fn default_true() -> bool {
    true
}

fn default_verification_expire() -> i64 {
    900 // 15分钟
}

/// SMTP发送速率限制配置。
#[derive(Debug, Clone, Deserialize)]
pub struct SmtpRateLimitConfig {
    /// 每分钟最大发送数
    #[serde(default = "default_smtp_per_minute")]
    pub per_minute: u32,
    /// 每小时最大发送数
    #[serde(default = "default_smtp_per_hour")]
    pub per_hour: u32,
}

impl Default for SmtpRateLimitConfig {
    fn default() -> Self {
        Self {
            per_minute: default_smtp_per_minute(),
            per_hour: default_smtp_per_hour(),
        }
    }
}

fn default_smtp_per_minute() -> u32 {
    3
}

fn default_smtp_per_hour() -> u32 {
    10
}

// ============================================================================
// 官方 Synapse 配置模块（未实现）
// 以下配置模块参考官方 Synapse 配置文档，使用注释标记暂未实现
// 文档: https://matrix-org.github.io/synapse/latest/usage/configuration/config_documentation.html
// ============================================================================

/*
/// 媒体存储配置。
///
/// 官方 Synapse 配置文档: https://matrix-org.github.io/synapse/latest/usage/configuration/config_documentation.html#media_store
///
/// 配置媒体文件（图片、视频等）的存储位置和访问方式。
///
/// # 待实现功能
/// - 媒体文件上传 API: `POST /_matrix/media/v3/upload`
/// - 媒体文件下载 API: `GET /_matrix/media/v3/download/{serverName}/{mediaId}`
/// - 缩略图生成: `GET /_matrix/media/v3/thumbnail/{serverName}/{mediaId}`
/// - URL 预览: `GET /_matrix/media/v3/preview_url`
/// - 二级存储提供者（S3, Azure Blob 等）
///
/// # 配置示例
/// ```yaml
/// media_store:
///   enabled: true
///   storage_path: "/var/lib/synapse/media"
///   upload_size: "100M"
///   url_preview_enabled: true
///   max_thumbnail_size: "10M"
///   min_thumbnail_size: "10K"
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct MediaStoreConfig {
    /// 是否启用媒体存储功能
    #[serde(default)]
    pub enabled: bool,

    /// 媒体文件存储路径
    pub storage_path: String,

    /// 最大上传大小（如 "100M", "1G"）
    #[serde(default = "default_max_upload_size")]
    pub upload_size: String,

    /// 是否启用 URL 预览功能
    #[serde(default)]
    pub url_preview_enabled: bool,

    /// 缩略图配置
    #[serde(default)]
    pub thumbnails: ThumbnailConfig,

    /// 二级存储提供者（S3, Azure 等）
    #[serde(default)]
    pub storage_providers: Vec<StorageProviderConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ThumbnailConfig {
    /// 最大缩略图大小
    #[serde(default = "default_max_thumbnail_size")]
    pub max_size: String,

    /// 最小缩略图大小
    #[serde(default = "default_min_thumbnail_size")]
    pub min_size: String,

    /// 支持的缩略图尺寸列表
    #[serde(default)]
    pub sizes: Vec<ThumbnailSize>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ThumbnailSize {
    pub width: u32,
    pub height: u32,
    pub method: String, // "crop", "scale", "fit"
}

#[derive(Debug, Clone, Deserialize)]
pub struct StorageProviderConfig {
    pub provider: String, // "s3", "azure", "gcs"
    pub bucket: String,
    pub region: Option<String>,
    pub endpoint_url: Option<String>,
    pub access_key_id: Option<String>,
    pub secret_access_key: Option<String>,
}
*/

/*
/// 监听器配置。
///
/// 官方 Synapse 配置文档: https://matrix-org.github.io/synapse/latest/usage/configuration/config_documentation.html#listeners
///
/// 配置多个监听器，每个监听器可以监听不同的端口并提供不同的资源。
///
/// # 待实现功能
/// - 多端口监听支持（当前只有单一 host:port）
/// - 按资源类型分离监听器（client, federation, metrics）
/// - TLS/HTTPS 支持
/// - X-Forwarded-For 处理
/// - 资源访问控制
///
/// # 配置示例
/// ```yaml
/// listeners:
///   - type: http
///     port: 8008
///     tls: false
///     x_forwarded: true
///     resources:
///       - names: [client, federation]
///         compress: true
///   - type: metrics
///     port: 9148
///     resources:
///       - names: [metrics]
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct ListenersConfig {
    #[serde(default)]
    pub listeners: Vec<ListenerConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListenerConfig {
    /// 监听器类型: http, https, metrics, manhole
    #[serde(default)]
    pub r#type: String,

    /// 监听端口
    pub port: u16,

    /// 监听地址
    #[serde(default = "default_listen_host")]
    pub host: String,

    /// 是否启用 TLS
    #[serde(default)]
    pub tls: bool,

    /// TLS 证书路径
    pub tls_certificate_path: Option<String>,

    /// TLS 私钥路径
    pub tls_private_key_path: Option<String>,

    /// 是否处理 X-Forwarded-For 头
    #[serde(default = "default_x_forwarded")]
    pub x_forwarded: bool,

    /// 资源配置
    #[serde(default)]
    pub resources: Vec<ListenerResource>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListenerResource {
    /// 资源名称列表: client, federation, metrics, static
    pub names: Vec<String>,

    /// 是否压缩响应
    #[serde(default = "default_compress")]
    pub compress: bool,
}

fn default_listen_host() -> String {
    "::".to_string()
}

fn default_x_forwarded() -> bool {
    false
}

fn default_compress() -> bool {
    false
}
*/

/*
/// URL 预览配置。
///
/// 官方 Synapse 配置文档: https://matrix-org.github.io/synapse/latest/usage/configuration/config_documentation.html#url_preview
///
/// 配置 URL 预览功能，用于在客户端显示链接的预览信息。
///
/// # 待实现功能
/// - URL 内容抓取
/// - Open Graph 解析
/// - oEmbed 支持
/// - URL 黑名单/白名单
/// - 缓存配置
/// - 图片下载代理
///
/// # 配置示例
/// ```yaml
/// url_preview:
///   enabled: true
///   url_blacklist:
///     - domain: "example.com"
///     - regex: "^https://.*\\.internal\\.com"
///   spider_enabled: true
///   oembed_enabled: true
///   max_spider_size: "10M"
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct UrlPreviewConfig {
    /// 是否启用 URL 预览
    #[serde(default)]
    pub enabled: bool,

    /// URL 黑名单
    #[serde(default)]
    pub url_blacklist: Vec<UrlBlacklistRule>,

    /// 是否启用网页爬虫
    #[serde(default = "default_spider_enabled")]
    pub spider_enabled: bool,

    /// 是否启用 oEmbed
    #[serde(default)]
    pub oembed_enabled: bool,

    /// 最大抓取大小
    #[serde(default = "default_max_spider_size")]
    pub max_spider_size: String,

    /// 预览缓存时间（秒）
    #[serde(default = "default_preview_cache_duration")]
    pub cache_duration: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UrlBlacklistRule {
    /// 域名匹配
    pub domain: Option<String>,

    /// 正则表达式匹配
    pub regex: Option<String>,
}

fn default_spider_enabled() -> bool {
    true
}

fn default_max_spider_size() -> String {
    "10M".to_string()
}

fn default_preview_cache_duration() -> u64 {
    86400 // 24小时
}
*/

/*
/// 限制配置。
///
/// 官方 Synapse 配置文档: https://matrix-org.github.io/synapse/latest/usage/configuration/config_documentation.html#limits
///
/// 配置各种资源限制，防止资源滥用。
///
/// # 待实现功能
/// - 上传大小限制（已在 ServerConfig 中有基础实现）
/// - 房间加入限制
/// - 事件内容大小限制
/// - 联邦限制
/// - 速率限制（已实现 RateLimitConfig）
///
/// # 配置示例
/// ```yaml
/// limits:
///   upload_size: "100M"
///   room_join_complexity_limit: 10000
///   event_fields_size_limit: "65536"
/// federation:
///   event_size_limit: "10M"
///   batch_size_limit: 50
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct LimitsConfig {
    /// 最大上传大小
    #[serde(default = "default_max_upload_size")]
    pub upload_size: String,

    /// 房间加入复杂度限制
    #[serde(default = "default_room_join_complexity")]
    pub room_join_complexity_limit: u64,

    /// 事件字段大小限制
    #[serde(default = "default_event_fields_size")]
    pub event_fields_size_limit: u64,

    /// 联邦限制配置
    #[serde(default)]
    pub federation: FederationLimitsConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FederationLimitsConfig {
    /// 单个事件大小限制
    #[serde(default = "default_federation_event_size")]
    pub event_size_limit: String,

    /// 批量事件数量限制
    #[serde(default = "default_batch_size")]
    pub batch_size_limit: u64,
}

fn default_max_upload_size() -> String {
    "100M".to_string()
}

fn default_room_join_complexity() -> u64 {
    10000
}

fn default_event_fields_size() -> u64 {
    65536
}

fn default_federation_event_size() -> String {
    "10M".to_string()
}

fn default_batch_size() -> u64 {
    50
}
*/

/*
/// 密码配置。
///
/// 官方 Synapse 配置文档: https://matrix-org.github.io/synapse/latest/usage/configuration/config_documentation.html#password_config
///
/// 配置密码策略和认证模块。
///
/// # 待实现功能
/// - 密码 pepper（全局盐值）
/// - 多认证模块支持（bcrypt, argon2, custom）
/// - 密码复杂度要求
/// - 密码重用检查
/// - 密码过期策略
///
/// # 配置示例
/// ```yaml
/// password_config:
///   enabled: true
///   pepper: "YOUR_PEPPER_SECRET"
///   minimum_length: 8
///   require_digit: true
///   require_symbol: true
///   modules:
///     - module: "argon2"
///     - module: "bcrypt"
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct PasswordConfig {
    /// 是否启用密码认证
    #[serde(default = "default_password_enabled")]
    pub enabled: bool,

    /// 密码 pepper（全局盐值，对所有密码哈希添加额外安全性）
    pub pepper: Option<String>>

    /// 最小密码长度
    #[serde(default = "default_min_password_length")]
    pub minimum_length: u32,

    /// 是否要求数字
    #[serde(default)]
    pub require_digit: bool,

    /// 是否要求符号
    #[serde(default)]
    pub require_symbol: bool,

    /// 是否要求大写字母
    #[serde(default)]
    pub require_uppercase: bool,

    /// 是否要求小写字母
    #[serde(default)]
    pub require_lowercase: bool,

    /// 认证模块列表
    #[serde(default)]
    pub modules: Vec<PasswordAuthModule>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PasswordAuthModule {
    pub module: String, // "argon2", "bcrypt", "custom"
    pub config: Option<serde_json::Value>,
}

fn default_password_enabled() -> bool {
    true
}

fn default_min_password_length() -> u32 {
    8
}
*/

/*
/// OpenID Connect 配置。
///
/// 官方 Synapse 配置文档: https://matrix-org.github.io/synapse/latest/usage/configuration/config_documentation.html#oidc_config
///
/// 配置 OpenID Connect 单点登录（SSO）。
///
/// # 待实现功能
/// - OIDC 认证流程
/// - Token 验证
/// - 用户属性映射
/// - 多提供者支持
///
/// # 配置示例
/// ```yaml
/// oidc:
///   enabled: true
///   issuer: "https://accounts.example.com"
///   client_id: "your-client-id"
///   client_secret: "your-client-secret"
///   scopes: ["openid", "profile", "email"]
///   attribute_mapping:
///     localpart: "preferred_username"
///     displayname: "name"
///     email: "email"
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct OidcConfig {
    /// 是否启用 OIDC
    #[serde(default)]
    pub enabled: bool,

    /// OIDC 提供者 URL
    pub issuer: String,

    /// 客户端 ID
    pub client_id: String,

    /// 客户端密钥
    pub client_secret: Option<String>,

    /// 请求的 scopes
    #[serde(default = "default_oidc_scopes")]
    pub scopes: Vec<String>,

    /// 用户属性映射
    #[serde(default)]
    pub attribute_mapping: OidcAttributeMapping,

    /// 回调 URL
    pub callback_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OidcAttributeMapping {
    pub localpart: Option<String>,
    pub displayname: Option<String>,
    pub email: Option<String>,
}

fn default_oidc_scopes() -> Vec<String> {
    vec!["openid".to_string(), "profile".to_string()]
}
*/

/*
/// VoIP 配置。
///
/// 官方 Synapse 配置文档: https://matrix-org.github.io/synapse/latest/usage/configuration/config_documentation.html#voip
///
/// 配置语音/视频通话的 TURN/STUN 服务器。
///
/// # 待实现功能
/// - TURN 服务器配置
/// - STUN 服务器配置
/// - TURN 凭证生成
/// - TURN 共享密钥支持
///
/// # 配置示例
/// ```yaml
/// voip:
///   turn:
///     turn_uris:
///       - "turn:turn.example.com:3478?transport=udp"
///       - "turn:turn.example.com:3478?transport=tcp"
///     turn_shared_secret: "YOUR_TURN_SECRET"
///     turn_user_lifetime: "1h"
///     turn_username: "turn_username"
///     turn_password: "turn_password"
///   stun:
///     stun_uris:
///       - "stun:stun.example.com:3478"
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct VoipConfig {
    /// TURN 服务器配置
    #[serde(default)]
    pub turn: Option<TurnConfig>,

    /// STUN 服务器配置
    #[serde(default)]
    pub stun: Option<StunConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TurnConfig {
    /// TURN 服务器 URL 列表
    pub turn_uris: Vec<String>,

    /// TURN 共享密钥
    pub turn_shared_secret: Option<String>,

    /// TURN 用户名
    pub turn_username: Option<String>,

    /// TURN 密码
    pub turn_password: Option<String>,

    /// TURN 凭证有效期
    #[serde(default = "default_turn_user_lifetime")]
    pub turn_user_lifetime: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StunConfig {
    /// STUN 服务器 URL 列表
    pub stun_uris: Vec<String>,
}

fn default_turn_user_lifetime() -> String {
    "1h".to_string()
}
*/

/*
/// 推送配置。
///
/// 官方 Synapse 配置文档: https://matrix-org.github.io/synapse/latest/usage/configuration/config_documentation.html#push
///
/// 配置推送通知服务（用于移动端）。
///
/// # 待实现功能
/// - 推送网关配置
/// - 推送规则处理
/// - Pushkey 管理
/// - Apple Push Notification Service (APNs)
/// - Firebase Cloud Messaging (FCM)
///
/// # 配置示例
/// ```yaml
/// push:
///   enabled: true
///   group_unread_count_by_room: true
///   include_content: false
///   app_id: "io.element.matrix"
///   apns:
///     cert_file: "/path/to/cert.pem"
///     key_file: "/path/to/key.pem"
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct PushConfig {
    /// 是否启用推送
    #[serde(default)]
    pub enabled: bool,

    /// 按房间分组未读计数
    #[serde(default = "default_group_unread")]
    pub group_unread_count_by_room: bool,

    /// 是否包含消息内容
    #[serde(default)]
    pub include_content: bool,

    /// 应用 ID
    pub app_id: Option<String>,

    /// APNs 配置
    #[serde(default)]
    pub apns: Option<ApnsConfig>,

    /// FCM 配置
    #[serde(default)]
    pub fcm: Option<FcmConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApnsConfig {
    pub cert_file: String,
    pub key_file: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FcmConfig {
    pub api_key: String,
}

fn default_group_unread() -> bool {
    true
}
*/

/*
/// 账户有效性配置。
///
/// 官方 Synapse 配置文档: https://matrix-org.github.io/synapse/latest/usage/configuration/config_documentation.html#account_validity
///
/// 配置临时账户功能。
///
/// # 待实现功能
/// - 账户有效期设置
/// - 账户续期 API
/// - 过期账户自动停用
/// - 续期邮件发送
///
/// # 配置示例
/// ```yaml
/// account_validity:
///   enabled: true
///   period: "30d"
///   renew_at: "7d"
///   renewal_email_subject: "Renew your account"
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct AccountValidityConfig {
    /// 是否启用账户有效性
    #[serde(default)]
    pub enabled: bool,

    /// 账户有效期
    #[serde(default = "default_validity_period")]
    pub period: String,

    /// 续期提醒时间
    #[serde(default = "default_renew_at")]
    pub renew_at: String,

    /// 续期邮件主题
    pub renewal_email_subject: Option<String>,

    /// 续期邮件模板
    pub renewal_email_template: Option<String>,
}

fn default_validity_period() -> String {
    "30d".to_string()
}

fn default_renew_at() -> String {
    "7d".to_string()
}
*/

/*
/// CAS 认证配置。
///
/// 官方 Synapse 配置文档: https://matrix-org.github.io/synapse/latest/usage/configuration/config_documentation.html#cas_config
///
/// 配置 CAS (Central Authentication Service) 单点登录。
///
/// # 待实现功能
/// - CAS 认证流程
/// - 属性获取
/// - 用户属性映射
///
/// # 配置示例
/// ```yaml
/// cas:
///   enabled: true
///   server_url: "https://cas.example.com"
///   service_url: "https://matrix.example.com"
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct CasConfig {
    /// 是否启用 CAS
    #[serde(default)]
    pub enabled: bool,

    /// CAS 服务器 URL
    pub server_url: String,

    /// 服务 URL
    pub service_url: String,
}
*/

/*
/// SAML2 认证配置。
///
/// 官方 Synapse 配置文档: https://matrix-org.github.io/synapse/latest/usage/configuration/config_documentation.html#saml2_config
///
/// 配置 SAML2 单点登录（企业级 SSO）。
///
/// # 待实现功能
/// - SAML2 认证流程
/// - 元数据配置
/// - 属性映射
/// - 多 IdP 支持
///
/// # 配置示例
/// ```yaml
/// saml2:
///   enabled: true
///   sp_config:
///     endpoint:
///       - "https://matrix.example.com/_matrix/saml2/authn_response"
///     cert_file: "/path/to/cert.pem"
///     key_file: "/path/to/key.pem"
///   idp_metadata:
///     - url: "https://idp.example.com/metadata"
///   attribute_mapping:
///     uid: "name-id"
///     displayname: "displayName"
///     email: "emailAddress"
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct Saml2Config {
    /// 是否启用 SAML2
    #[serde(default)]
    pub enabled: bool,

    /// 服务提供者配置
    #[serde(default)]
    pub sp_config: Option<SamlSpConfig>,

    /// 身份提供者元数据
    #[serde(default)]
    pub idp_metadata: Vec<SamlIdpMetadata>,

    /// 属性映射
    #[serde(default)]
    pub attribute_mapping: SamlAttributeMapping,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SamlSpConfig {
    pub endpoint: Vec<String>,
    pub cert_file: String,
    pub key_file: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SamlIdpMetadata {
    pub url: Option<String>,
    pub file: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SamlAttributeMapping {
    pub uid: String,
    pub displayname: Option<String>,
    pub email: Option<String>,
}

impl Default for SamlAttributeMapping {
    fn default() -> Self {
        Self {
            uid: "name-id".to_string(),
            displayname: None,
            email: None,
        }
    }
}
*/

/*
/// UI 认证配置。
///
/// 官方 Synapse 配置文档: https://matrix-org.github.io/synapse/latest/usage/configuration/config_documentation.html#ui_auth
///
/// 配置用户交互认证（UIAA）会话参数。
///
/// # 待实现功能
/// - 会话超时配置
/// - 认证流程配置
/// - 重试策略
///
/// # 配置示例
/// ```yaml
/// ui_auth:
///   session_timeout: "15m"
///   maximum_sessions: 100
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct UiAuthConfig {
    /// 会话超时时间
    #[serde(default = "default_ui_auth_session_timeout")]
    pub session_timeout: String,

    /// 最大会话数
    #[serde(default = "default_max_ui_auth_sessions")]
    pub maximum_sessions: u32,
}

fn default_ui_auth_session_timeout() -> String {
    "15m".to_string()
}

fn default_max_ui_auth_sessions() -> u32 {
    100
}
*/

/*
/// 房间配置。
///
/// 官方 Synapse 配置文档: https://matrix-org.github.io/synapse/latest/usage/configuration/config_documentation.html#rooms
///
/// 配置房间默认参数和行为。
///
/// # 待实现功能
/// - 默认房间版本
/// - 房间导出配置
/// - 房间加入规则
/// - 房间状态事件限制
///
/// # 配置示例
/// ```yaml
/// rooms:
///   default_room_version: "10"
///   filter_room_lists: true
///   export_metrics: false
///   state_event_limit: 1000
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct RoomsConfig {
    /// 默认房间版本
    #[serde(default = "default_room_version")]
    pub default_room_version: String,

    /// 是否过滤房间列表
    #[serde(default)]
    pub filter_room_lists: bool,

    /// 是否导出指标
    #[serde(default)]
    pub export_metrics: bool,

    /// 状态事件数量限制
    #[serde(default = "default_state_event_limit")]
    pub state_event_limit: u64,
}

fn default_room_version() -> String {
    "10".to_string()
}

fn default_state_event_limit() -> u64 {
    1000
}
*/

/*
/// 消息保留配置。
///
/// 官方 Synapse 配置文档: https://matrix-org.github.io/synapse/latest/usage/configuration/config_documentation.html#retention
///
/// 配置自动删除旧消息的策略。
///
/// # 待实现功能
/// - 保留策略配置
/// - 状态事件保留
/// - 默认保留策略
/// - 最大保留期
///
/// # 配置示例
/// ```yaml
/// retention:
///   enabled: true
///   default_policy:
///     min_lifetime: "1d"
///     max_lifetime: "365d"
///   allowed_lifetime_min: "1d"
///   allowed_lifetime_max: "365d"
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct RetentionConfig {
    /// 是否启用消息保留
    #[serde(default)]
    pub enabled: bool,

    /// 默认保留策略
    #[serde(default)]
    pub default_policy: Option<RetentionPolicy>,

    /// 最小允许保留期
    pub allowed_lifetime_min: Option<String>,

    /// 最大允许保留期
    pub allowed_lifetime_max: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RetentionPolicy {
    pub min_lifetime: Option<String>,
    pub max_lifetime: Option<String>,
}
*/

/*
/// 用户目录配置。
///
/// 官方 Synapse 配置文档: https://matrix-org.github.io/synapse/latest/usage/configuration/config_documentation.html#user_directory
///
/// 配置用户搜索目录行为。
///
/// # 待实现功能
/// - 搜索所有用户开关
/// - 用户索引更新频率
/// - 优先用户列表
/// - 搜索结果显示数量限制
///
/// # 配置示例
/// ```yaml
/// user_directory:
///   enabled: true
///   search_all_users: false
///   prefer_local_users: true
///   indexing_interval: "1h"
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct UserDirectoryConfig {
    /// 是否启用用户目录
    #[serde(default = "default_user_directory_enabled")]
    pub enabled: bool,

    /// 是否搜索所有用户（包括非共享房间的用户）
    #[serde(default)]
    pub search_all_users: bool,

    /// 是否优先显示本地用户
    #[serde(default = "default_prefer_local_users")]
    pub prefer_local_users: bool,

    /// 索引更新间隔
    #[serde(default = "default_indexing_interval")]
    pub indexing_interval: String,
}

fn default_user_directory_enabled() -> bool {
    true
}

fn default_prefer_local_users() -> bool {
    true
}

fn default_indexing_interval() -> String {
    "1h".to_string()
}
*/

/*
/// 性能指标配置。
///
/// 官方 Synapse 配置文档: https://matrix-org.github.io/synapse/latest/usage/configuration/config_documentation.html#metrics
///
/// 配置 Prometheus 性能指标导出。
///
/// # 待实现功能
/// - Prometheus 端点
/// - 指标标签配置
/// - 自定义指标
/// - OpenTelemetry 支持
///
/// # 配置示例
/// ```yaml
/// metrics:
///   enabled: true
///   port: 9148
///   labels:
///     - "instance:production"
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct MetricsConfig {
    /// 是否启用指标
    #[serde(default)]
    pub enabled: bool,

    /// 指标端口
    #[serde(default = "default_metrics_port")]
    pub port: u16,

    /// 额外的标签
    #[serde(default)]
    pub labels: Vec<String>,
}

fn default_metrics_port() -> u16 {
    9148
}
*/

/*
/// 客户端配置。
///
/// 官方 Synapse 配置文档: https://matrix-org.github.io/synapse/latest/usage/configuration/config_documentation.html#client
///
/// 配置客户端行为参数。
///
/// # 待实现功能
/// - 最大请求大小
/// - 同步响应配置
/// - 事件获取限制
/// - Well-known 配置
///
/// # 配置示例
/// ```yaml
/// client:
///   max_request_size: "10M"
///   max_sync_events: 100
///   well_known:
///     client_name: "Synapse (Rust)"
///     client_url: "https://github.com/element-hq/synapse"
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct ClientConfig {
    /// 最大请求大小
    #[serde(default = "default_client_max_request_size")]
    pub max_request_size: String,

    /// 最大同步事件数量
    #[serde(default = "default_max_sync_events")]
    pub max_sync_events: u64,

    /// Well-known 配置
    #[serde(default)]
    pub well_known: Option<WellKnownConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WellKnownConfig {
    pub client_name: Option<String>,
    pub client_url: Option<String>,
}

fn default_client_max_request_size() -> String {
    "10M".to_string()
}

fn default_max_sync_events() -> u64 {
    100
}
*/

/*
/*
/// 服务器通知配置。
///
/// 官方 Synapse 配置文档: https://matrix-org.github.io/synapse/latest/usage/configuration/config_documentation.html#server_notices
///
/// 配置服务器通知系统（用于向用户发送系统消息）。
///
/// # 待实现功能
/// - 系统通知房间配置
/// - 通知发送 API
/// - 通知模板
///
/// # 配置示例
/// ```yaml
/// server_notices:
///   system_mxid_localpart: "notices"
///   system_display_name: "Server Notices"
///   server_notices_room: "!notices:example.com"
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct ServerNoticesConfig {
    /// 系统通知用户的 MXID 本地部分
    pub system_mxid_localpart: String,

    /// 系统通知用户的显示名称
    pub system_display_name: Option<String>,

    /// 系统通知房间 ID
    pub server_notices_room: Option<String>,
}
*/

/*
/// 第三方协议规则配置。
///
/// 官方 Synapse 配置文档: https://matrix-org.github.io/synapse/latest/usage/configuration/config_documentation.html#third_party_rules
///
/// 配置第三方协议桥接规则。
///
/// # 待实现功能
/// - 协议列表
/// - 网络字段
/// - 匹配规则
///
/// # 配置示例
/// ```yaml
/// third_party_rules:
///   - protocol: "irc"
///     fields:
///       - network: "freenode"
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct ThirdPartyRulesConfig {
    pub rules: Vec<ThirdPartyRule>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ThirdPartyRule {
    pub protocol: String,
    pub fields: Vec<ThirdPartyField>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ThirdPartyField {
    pub network: String,
}
*/

/*
/// 实验性功能配置。
///
/// 官方 Synapse 配置文档: https://matrix-org.github.io/synapse/latest/usage/configuration/config_documentation.html#experimental
///
/// 配置 MSC (Matrix Spec Change) 实验性功能。
///
/// # 待实现功能
/// - MSC 编号列表
/// - 功能开关
///
/// # 配置示例
/// ```yaml
/// experimental:
///   mscs:
///     - "msc2815"  # Broadcast to device
///     - "msc3785"  # Read receipts
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct ExperimentalConfig {
    /// 启用的 MSC 列表
    #[serde(default)]
    pub mscs: Vec<String>,
}
*/

/// Sentry 错误追踪配置。
///
/// 官方 Synapse 配置文档: https://matrix-org.github.io/synapse/latest/usage/configuration/config_documentation.html#sentry
///
/// 配置 Sentry 错误追踪。
///
/// # 待实现功能
/// - Sentry DSN 配置
/// - 环境信息
/// - 错误采样率
///
/// # 配置示例
/// ```yaml
/// sentry:
///   enabled: true
///   dsn: "https://your-sentry-dsn"
///   environment: "production"
///   sample_rate: 0.1
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct SentryConfig {
    /// 是否启用 Sentry
    #[serde(default)]
    pub enabled: bool,

    /// Sentry DSN
    pub dsn: Option<String>,

    /// 环境名称
    pub environment: Option<String>,

    /// 采样率 (0.0 - 1.0)
    #[serde(default = "default_sentry_sample_rate")]
    pub sample_rate: f32,
}

fn default_sentry_sample_rate() -> f32 {
    0.1
}
*/

// ============================================================================
// 配置增强说明
//
// 要启用上述配置模块，请按以下步骤操作：
//
// 1. 取消相应配置结构体的注释
// 2. 将该配置添加到主 Config 结构体中：
//    pub struct Config {
//        // ...
//        #[serde(default)]
//        pub listeners: ListenersConfig,
//        #[serde(default)]
//        pub media_store: MediaStoreConfig,
//        // ... 等等
//    }
// 3. 在配置文件（homeserver.yaml）中添加相应配置
// 4. 实现对应的功能代码（Service, Storage, Routes 等）
// 5. 添加测试用例
// 6. 更新文档
//
// 注意：启用新配置后，需要更新 Default 实现以提供合理的默认值。
// ============================================================================
