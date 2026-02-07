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
#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    /// 服务器名称（域名）
    pub name: String,
    /// 监听主机地址
    pub host: String,
    /// 监听端口
    pub port: u16,
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
