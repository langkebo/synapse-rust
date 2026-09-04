use serde::Deserialize;
use educe::Educe;

// ============================================================================
// SECTION: Database Configuration
// ============================================================================

/// 默认连接最长生命周期（秒）：30 分钟。
///
/// 与旧版 Synapse 默认一致，避免长连接被 PG `idle_session_timeout` 提前关闭。
fn default_database_max_lifetime_secs() -> u64 {
    1800
}

/// 默认连接空闲超时（秒）：10 分钟。
///
/// 用于释放长时间未使用的连接。
fn default_database_idle_timeout_secs() -> u64 {
    600
}

/// 当 `min_idle` 未显式设置时使用的最小空闲连接数。
fn default_database_min_idle_floor() -> u32 {
    5
}

/// PG `statement_timeout`（秒）：单个 SQL 语句的最长执行时间。
///
/// 生产建议 30s；批量/报表场景需要拉到 5-10 分钟时应通过 Config 调高，
/// 不应继续走硬编码。
fn default_database_statement_timeout_secs() -> u64 {
    30
}

/// PG `lock_timeout`（秒）：等待锁的最长时间。
fn default_database_lock_timeout_secs() -> u64 {
    10
}

/// PG `idle_in_transaction_session_timeout`（秒）：
/// 事务开启但长时间空闲时的最长时间，防止连接被独占。
fn default_database_idle_in_transaction_timeout_secs() -> u64 {
    60
}

/// 数据库连接配置。
#[derive(Clone, Deserialize, Educe)]
#[educe(Debug)]
pub struct DatabaseConfig {
    /// 数据库主机地址
    pub host: String,
    /// 数据库端口
    pub port: u16,
    /// 数据库用户名
    pub username: String,
    /// 数据库密码
    #[educe(Debug(ignore))]
    pub password: String,
    /// 数据库名称
    pub name: String,
    /// 连接池大小。
    ///
    /// ⚠️ 已废弃：实际连接池上限由 [`max_size`](Self::max_size) 控制（`server/database.rs`
    /// 的 `PgPoolOptions::max_connections` 只读 `max_size`），本字段零引用、仅保留以
    /// 兼容旧配置，勿再依赖。
    pub pool_size: u32,
    /// 最大连接数（实际生效的连接池上限，对齐 Synapse ≥50）
    pub max_size: u32,
    /// 最小空闲连接数
    pub min_idle: Option<u32>,
    /// 连接超时时间（秒）
    pub connection_timeout: u64,
    /// 连接最长生命周期（秒）。默认 1800s（30 分钟）。
    #[serde(default = "default_database_max_lifetime_secs")]
    pub max_lifetime_secs: u64,
    /// 连接空闲超时（秒）。默认 600s（10 分钟）。
    #[serde(default = "default_database_idle_timeout_secs")]
    pub idle_timeout_secs: u64,
    /// 当 [`min_idle`](Self::min_idle) 未设置时使用的下限。
    ///
    /// 仅在旧配置缺省 `min_idle` 时生效，避免连接池频繁扩张/收缩。
    #[serde(default = "default_database_min_idle_floor")]
    pub min_idle_floor: u32,
    /// PG `statement_timeout`（秒）。每个新连接初始化时通过 `SET statement_timeout` 生效。
    #[serde(default = "default_database_statement_timeout_secs")]
    pub statement_timeout_secs: u64,
    /// PG `lock_timeout`（秒）。
    #[serde(default = "default_database_lock_timeout_secs")]
    pub lock_timeout_secs: u64,
    /// PG `idle_in_transaction_session_timeout`（秒）。
    #[serde(default = "default_database_idle_in_transaction_timeout_secs")]
    pub idle_in_transaction_timeout_secs: u64,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            host: String::new(),
            port: 5432,
            username: String::new(),
            password: String::new(),
            name: String::new(),
            pool_size: 0,
            max_size: 50,
            min_idle: None,
            connection_timeout: 60,
            max_lifetime_secs: default_database_max_lifetime_secs(),
            idle_timeout_secs: default_database_idle_timeout_secs(),
            min_idle_floor: default_database_min_idle_floor(),
            statement_timeout_secs: default_database_statement_timeout_secs(),
            lock_timeout_secs: default_database_lock_timeout_secs(),
            idle_in_transaction_timeout_secs: default_database_idle_in_transaction_timeout_secs(),
        }
    }
}

/// Redis 缓存配置。
#[derive(Clone, Deserialize, Default, Educe)]
#[educe(Debug)]
pub struct RedisConfig {
    /// Redis 主机地址
    pub host: String,
    /// Redis 端口
    pub port: u16,
    /// Redis 密码（可选）
    #[educe(Debug(ignore))]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redis_connection_url_without_password() {
        let config = RedisConfig { host: "localhost".into(), port: 6379, password: None, ..Default::default() };
        assert_eq!(config.connection_url(), "redis://localhost:6379/");
    }

    #[test]
    fn redis_connection_url_with_password() {
        let config = RedisConfig {
            host: "redis.example.com".into(),
            port: 6380,
            password: Some("secret".into()),
            ..Default::default()
        };
        assert_eq!(config.connection_url(), "redis://:secret@redis.example.com:6380/");
    }

    #[test]
    fn redis_connection_url_empty_password_skipped() {
        let config =
            RedisConfig { host: "localhost".into(), port: 6379, password: Some("".into()), ..Default::default() };
        assert_eq!(config.connection_url(), "redis://localhost:6379/");
    }

    #[test]
    fn debug_output_redacts_passwords() {
        // 审查 #25：密钥字段 Debug 派生时必须脱敏（#[educe(Debug(ignore))]）。
        let db = DatabaseConfig {
            host: "localhost".into(),
            port: 5432,
            username: "synapse".into(),
            password: "db-s3cr3t".into(),
            name: "synapse".into(),
            pool_size: 20,
            max_size: 20,
            min_idle: None,
            connection_timeout: 30,
            ..Default::default()
        };
        let dbg = format!("{db:?}");
        assert!(!dbg.contains("db-s3cr3t"), "DatabaseConfig Debug 泄露密码: {dbg}");

        let redis = RedisConfig {
            host: "localhost".into(),
            port: 6379,
            password: Some("redis-s3cr3t".into()),
            ..Default::default()
        };
        let dbg = format!("{redis:?}");
        assert!(!dbg.contains("redis-s3cr3t"), "RedisConfig Debug 泄露密码: {dbg}");
    }
}
