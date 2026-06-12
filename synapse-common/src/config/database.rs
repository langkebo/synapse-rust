use serde::Deserialize;

// ============================================================================
// SECTION: Database Configuration
// ============================================================================

/// 数据库连接配置。
#[derive(Debug, Clone, Deserialize, Default)]
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
#[derive(Debug, Clone, Deserialize, Default)]
pub struct RedisConfig {
    /// Redis 主机地址
    pub host: String,
    /// Redis 端口
    pub port: u16,
    /// Redis 密码（可选）
    pub password: Option<String>,
    /// 缓存键前缀
    pub key_prefix: String,
    /// 连接池大小
    pub pool_size: u32,
    /// 是否启用 Redis 缓存
    pub enabled: bool,
    /// 连接超时时间（毫秒）
    #[serde(default = "default_redis_connection_timeout")]
    pub connection_timeout_ms: u64,
    /// 命令超时时间（毫秒）
    #[serde(default = "default_redis_command_timeout")]
    pub command_timeout_ms: u64,
    /// 熔断器配置
    #[serde(default)]
    pub circuit_breaker: CircuitBreakerConfig,
}

impl RedisConfig {
    pub fn connection_url(&self) -> String {
        if let Some(password) = &self.password {
            if !password.is_empty() {
                return format!("redis://:{}@{}:{}/", password, self.host, self.port);
            }
        }

        format!("redis://{}:{}/", self.host, self.port)
    }
}

/// 熔断器配置
#[derive(Debug, Clone, Deserialize)]
pub struct CircuitBreakerConfig {
    /// 是否启用熔断器
    #[serde(default = "default_circuit_breaker_enabled")]
    pub enabled: bool,
    /// 熔断器打开的失败阈值
    #[serde(default = "default_failure_threshold")]
    pub failure_threshold: u32,
    /// 熔断器半开状态下的成功阈值
    #[serde(default = "default_success_threshold")]
    pub success_threshold: u32,
    /// 熔断器打开后的超时时间（毫秒）
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    /// 滑动窗口大小（秒）
    #[serde(default = "default_window_size_seconds")]
    pub window_size_seconds: u64,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            enabled: default_circuit_breaker_enabled(),
            failure_threshold: default_failure_threshold(),
            success_threshold: default_success_threshold(),
            timeout_ms: default_timeout_ms(),
            window_size_seconds: default_window_size_seconds(),
        }
    }
}

fn default_redis_connection_timeout() -> u64 {
    500
}

fn default_redis_command_timeout() -> u64 {
    500
}

fn default_circuit_breaker_enabled() -> bool {
    true
}

fn default_failure_threshold() -> u32 {
    10 // 10 failures (was 5) - more tolerance for transient failures
}

fn default_success_threshold() -> u32 {
    3 // Keep as is
}

fn default_timeout_ms() -> u64 {
    60_000 // 60 seconds (was 30s) - give more time before half-open
}

fn default_window_size_seconds() -> u64 {
    120 // 2 minutes (was 1 min) - larger window for better accuracy
}
