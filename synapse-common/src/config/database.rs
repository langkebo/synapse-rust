use educe::Educe;
use serde::Deserialize;

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

/// 默认 PG 端口。
fn default_database_port() -> u16 {
    5432
}

/// 默认最大连接数（对齐 Synapse ≥50，pool 实际生效上限）。
fn default_database_max_size() -> u32 {
    50
}

/// 默认连接超时（秒）。与旧版 Synapse 一致。
fn default_database_connection_timeout_secs() -> u64 {
    60
}

/// 数据库连接配置。
#[derive(Clone, Deserialize, Educe)]
#[educe(Debug)]
/// Represents DatabaseConfig.
pub struct DatabaseConfig {
    /// 数据库主机地址
    pub host: String,
    /// 数据库端口
    #[serde(default = "default_database_port")]
    pub port: u16,
    /// 数据库用户名
    pub username: String,
    /// 数据库密码
    #[educe(Debug(ignore))]
    /// `password` field.
    pub password: String,
    /// 数据库名称
    pub name: String,
    /// 连接池大小。
    ///
    /// ⚠️ 已废弃：实际连接池上限由 [`max_size`](Self::max_size) 控制（`server/database.rs`
    /// 的 `PgPoolOptions::max_connections` 只读 `max_size`），本字段零引用、仅保留以
    /// 兼容旧配置，勿再依赖。
    #[serde(default)]
    pub pool_size: u32,
    /// 最大连接数（实际生效的连接池上限，对齐 Synapse ≥50）
    #[serde(default = "default_database_max_size")]
    pub max_size: u32,
    /// 最小空闲连接数
    #[serde(default)]
    pub min_idle: Option<u32>,
    /// 连接超时时间（秒）
    #[serde(default = "default_database_connection_timeout_secs")]
    pub connection_timeout: u64,
    /// 连接最长生命周期（秒）。默认 1800s（30 分钟）。
    #[serde(default = "default_database_max_lifetime_secs")]
    /// `max_lifetime_secs` field.
    pub max_lifetime_secs: u64,
    /// 连接空闲超时（秒）。默认 600s（10 分钟）。
    #[serde(default = "default_database_idle_timeout_secs")]
    /// `idle_timeout_secs` field.
    pub idle_timeout_secs: u64,
    /// 当 [`min_idle`](Self::min_idle) 未设置时使用的下限。
    ///
    /// 仅在旧配置缺省 `min_idle` 时生效，避免连接池频繁扩张/收缩。
    #[serde(default = "default_database_min_idle_floor")]
    /// `min_idle_floor` field.
    pub min_idle_floor: u32,
    /// PG `statement_timeout`（秒）。每个新连接初始化时通过 `SET statement_timeout` 生效。
    #[serde(default = "default_database_statement_timeout_secs")]
    /// `statement_timeout_secs` field.
    pub statement_timeout_secs: u64,
    /// PG `lock_timeout`（秒）。
    #[serde(default = "default_database_lock_timeout_secs")]
    /// `lock_timeout_secs` field.
    pub lock_timeout_secs: u64,
    /// PG `idle_in_transaction_session_timeout`（秒）。
    #[serde(default = "default_database_idle_in_transaction_timeout_secs")]
    /// `idle_in_transaction_timeout_secs` field.
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
/// Represents RedisConfig.
pub struct RedisConfig {
    /// Redis 主机地址
    pub host: String,
    /// Redis 端口
    pub port: u16,
    /// Redis 密码（可选）
    #[educe(Debug(ignore))]
    /// `password` field.
    pub password: Option<String>,
    /// 缓存键前缀
    pub key_prefix: String,
    /// 连接池大小
    pub pool_size: u32,
    /// 是否启用 Redis 缓存
    pub enabled: bool,
    /// 连接超时时间（毫秒）
    #[serde(default = "default_redis_connection_timeout")]
    /// `connection_timeout_ms` field.
    pub connection_timeout_ms: u64,
    /// 命令超时时间（毫秒）
    #[serde(default = "default_redis_command_timeout")]
    /// `command_timeout_ms` field.
    pub command_timeout_ms: u64,
    /// 熔断器配置
    #[serde(default)]
    /// `circuit_breaker` field.
    pub circuit_breaker: CircuitBreakerConfig,
}

impl RedisConfig {
    /// Connections the url.
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
/// Represents CircuitBreakerConfig.
pub struct CircuitBreakerConfig {
    /// 是否启用熔断器
    #[serde(default = "default_circuit_breaker_enabled")]
    /// `enabled` field.
    pub enabled: bool,
    /// 熔断器打开的失败阈值
    #[serde(default = "default_failure_threshold")]
    /// `failure_threshold` field.
    pub failure_threshold: u32,
    /// 熔断器半开状态下的成功阈值
    #[serde(default = "default_success_threshold")]
    /// `success_threshold` field.
    pub success_threshold: u32,
    /// 熔断器打开后的超时时间（毫秒）
    #[serde(default = "default_timeout_ms")]
    /// `timeout_ms` field.
    pub timeout_ms: u64,
    /// 滑动窗口大小（秒）
    #[serde(default = "default_window_size_seconds")]
    /// `window_size_seconds` field.
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

    /// 09-ticket: 程序化构造的 `DatabaseConfig`（或反序列化缺字段时）必须落在生产安全的连接池值。
    ///
    /// 修复前：`max_size: 0` / `connection_timeout: 0` —— `PgPoolOptions::max_connections(0)`
    /// 实际意味着"无限"（pg driver 内部回退 32）但 `connection_timeout: 0` 立即超时。
    /// 修复后：缺字段反序列化使用 default fn（50 / 60s），与 `Default` impl 一致。
    #[test]
    fn database_config_serde_defaults_apply_when_fields_missing() {
        // 仅给必填的 host/user/pass/name 四个字段；其他都依赖 serde default。
        let yaml_str = "---
host: localhost
username: synapse
password: secret
name: synapse
";
        let cfg: DatabaseConfig = serde_yaml::from_str(yaml_str).expect("serde should accept missing optional fields");
        assert_eq!(cfg.port, 5432, "port 应默认 5432");
        assert_eq!(cfg.max_size, 50, "max_size 应默认 50（对齐 Synapse）");
        assert_eq!(cfg.connection_timeout, 60, "connection_timeout 应默认 60s");
        assert_eq!(cfg.max_lifetime_secs, 1800, "max_lifetime_secs 应默认 1800s");
        assert_eq!(cfg.idle_timeout_secs, 600, "idle_timeout_secs 应默认 600s");
        assert_eq!(cfg.statement_timeout_secs, 30, "statement_timeout_secs 应默认 30s");
        assert_eq!(cfg.lock_timeout_secs, 10, "lock_timeout_secs 应默认 10s");
        assert_eq!(cfg.idle_in_transaction_timeout_secs, 60, "idle_in_transaction 应默认 60s");
        assert_eq!(cfg.min_idle_floor, 5, "min_idle_floor 应默认 5");
    }

    /// 09-ticket: `Default::default()` 给出的值与 serde 反序列化缺字段的值一致。
    ///
    /// 防止 default fn 与 `Default` impl 漂移（一旦不一致就会出现"程序构造 vs 配置反序列化
    /// 行为不同"的诡异 bug）。
    #[test]
    fn database_config_default_and_serde_defaults_match() {
        let from_default = DatabaseConfig::default();
        let yaml_str = "---
host: any
username: any
password: any
name: any
";
        let from_serde: DatabaseConfig =
            serde_yaml::from_str(yaml_str).expect("serde should accept missing optional fields");
        assert_eq!(from_default.max_size, from_serde.max_size);
        assert_eq!(from_default.connection_timeout, from_serde.connection_timeout);
        assert_eq!(from_default.max_lifetime_secs, from_serde.max_lifetime_secs);
        assert_eq!(from_default.idle_timeout_secs, from_serde.idle_timeout_secs);
        assert_eq!(from_default.statement_timeout_secs, from_serde.statement_timeout_secs);
        assert_eq!(from_default.lock_timeout_secs, from_serde.lock_timeout_secs);
        assert_eq!(from_default.idle_in_transaction_timeout_secs, from_serde.idle_in_transaction_timeout_secs);
        assert_eq!(from_default.min_idle_floor, from_serde.min_idle_floor);
    }
}
