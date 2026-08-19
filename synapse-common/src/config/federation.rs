use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

// ============================================================================
// SECTION: Federation Configuration
// ============================================================================

/// 联邦配置。
///
/// 配置与其他 Matrix 服务器的联邦通信参数。
#[derive(Clone, Deserialize, Default, derivative::Derivative)]
#[derivative(Debug)]
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
    #[derivative(Debug = "ignore")]
    pub signing_key: Option<String>,
    /// 密钥 ID
    pub key_id: Option<String>,
    /// 信任的密钥服务器列表
    ///
    /// 用于获取其他服务器的签名密钥。默认包含 matrix.org。
    /// 格式: [{"server_name": "matrix.org", "verify_keys": {"ed25519:auto": "key"}}]
    #[serde(default = "default_trusted_key_servers")]
    pub trusted_key_servers: Vec<TrustedKeyServer>,
    /// 密钥刷新间隔（秒）
    #[serde(default = "default_key_refresh_interval")]
    pub key_refresh_interval: u64,
    /// 是否抑制密钥服务器警告
    #[serde(default)]
    pub suppress_key_server_warning: bool,
    /// 签名验证缓存 TTL（秒），默认 1 小时
    #[serde(default = "default_signature_cache_ttl")]
    pub signature_cache_ttl: u64,
    /// 密钥缓存 TTL（秒），默认 1 小时
    #[serde(default = "default_key_cache_ttl")]
    pub key_cache_ttl: u64,
    /// 密钥轮换宽限期（毫秒），默认 10 分钟
    #[serde(default = "default_key_rotation_grace_period_ms")]
    pub key_rotation_grace_period_ms: u64,

    /// 拉取远端 server keys 的最大并发（全局），默认 32
    #[serde(default = "default_federation_key_fetch_max_concurrency")]
    pub key_fetch_max_concurrency: usize,

    /// 拉取远端 server keys 的单次请求超时（毫秒），默认 5000
    #[serde(default = "default_federation_key_fetch_timeout_ms")]
    pub key_fetch_timeout_ms: u64,

    /// 是否允许使用 HTTP 拉取远端 server keys（默认 false）。
    ///
    /// 生产环境必须保持 false（Matrix 联邦要求 TLS）。仅在测试/开发环境中
    /// 设为 true，以便使用本地 HTTP mock 服务器测试联邦密钥拉取行为。
    #[serde(default)]
    pub allow_http_key_fetch: bool,

    /// E-2: 是否跳过联邦密钥拉取的 SSRF 防护（默认 false）。
    ///
    /// 此前 `allow_http_key_fetch` 同时控制 HTTP 协议和 SSRF 防护，
    /// 开发环境启用 HTTP 时会意外关闭 SSRF 防护。现在两者独立：
    /// - `allow_http_key_fetch` 仅控制 HTTP/HTTPS 协议
    /// - `skip_ssrf_check` 仅控制 SSRF IP 黑名单检查
    ///
    /// 仅在本地 mock 测试（全部流量到 localhost）时才设为 true。
    #[serde(default)]
    pub skip_ssrf_check: bool,

    /// 是否处理入站联邦 EDUs（默认 false）
    #[serde(default)]
    pub process_inbound_edus: bool,

    /// 单个联邦 txn 允许的最大 EDU 数量（默认 100）
    #[serde(default = "default_federation_inbound_edus_max_per_txn")]
    pub inbound_edus_max_per_txn: usize,

    #[serde(default = "default_federation_inbound_edu_max_concurrency")]
    pub inbound_edu_max_concurrency: usize,

    #[serde(default = "default_federation_inbound_edu_acquire_timeout_ms")]
    pub inbound_edu_acquire_timeout_ms: u64,

    #[serde(default = "default_federation_inbound_edu_per_origin_max_concurrency")]
    pub inbound_edu_per_origin_max_concurrency: usize,

    /// 是否处理入站联邦 presence EDU（默认 false）
    #[serde(default)]
    pub process_inbound_presence_edus: bool,

    /// 单个联邦 txn 内 presence 更新的最大条数（默认 50）
    #[serde(default = "default_federation_inbound_presence_updates_max_per_txn")]
    pub inbound_presence_updates_max_per_txn: usize,

    #[serde(default = "default_federation_inbound_presence_backoff_ms")]
    pub inbound_presence_backoff_ms: u64,

    #[serde(default = "default_federation_join_max_concurrency")]
    pub join_max_concurrency: usize,

    #[serde(default = "default_federation_join_acquire_timeout_ms")]
    pub join_acquire_timeout_ms: u64,

    #[serde(default)]
    pub admission_mode: bool,

    /// Master key for encrypting federation signing keys at rest.
    ///
    /// When configured, signing keys stored in the database will be encrypted
    /// using AES-256-GCM with this master key. Keys are stored with an `enc:`
    /// prefix to indicate encryption. If not configured, keys are stored in
    /// plaintext (with a warning logged at startup).
    ///
    /// Can also be set via the `SYNAPSE__FEDERATION__SIGNING_KEY_MASTER_KEY`
    /// environment variable.
    ///
    /// Generate with: `openssl rand -hex 32`
    #[serde(default)]
    #[derivative(Debug = "ignore")]
    pub signing_key_master_key: Option<String>,

    /// Explicitly allow storing federation signing keys in plaintext when no
    /// master key is configured. Defaults to `false` (fail-closed: key
    /// persistence is refused without a master key).
    ///
    /// Production deployments should set `signing_key_master_key` for
    /// encryption at rest. This flag is intended for development environments
    /// that accept the risk of plaintext key storage.
    ///
    /// Can also be set via the `SYNAPSE__FEDERATION__ALLOW_PLAINTEXT_SIGNING_KEYS`
    /// environment variable.
    #[serde(default)]
    pub allow_plaintext_signing_keys: bool,

    /// 出站联邦事件批处理的最大事件数，默认 100。
    ///
    /// `EventBroadcaster` 在 flush 之前最多累积这么多事件。历史实现
    /// 硬编码为 20，需要保留旧行为的部署可在配置中显式设置该值。
    #[serde(default = "default_event_broadcast_batch_size")]
    pub event_broadcast_batch_size: usize,

    /// S1 修复：联邦请求签名时间戳（SigningTs）容差（毫秒），默认 86400000（24h）。
    ///
    /// X-Matrix Authorization 头中的 `ts` 参数指示请求签名时间。服务端拒绝
    /// `|ts - now|` 超过此容差的请求，防止合法签名请求被无限重放。
    /// 设为 0 则跳过校验（不推荐，仅用于测试）。
    #[serde(default = "default_signing_ts_tolerance_ms")]
    pub signing_ts_tolerance_ms: i64,

    /// S1 修复：是否启用联邦重放保护（基于 ReplayProtectionCache）。
    ///
    /// 启用后，每个成功验签的请求的签名哈希被记入重放保护缓存，窗口内
    /// 重复提交同一签名即被拒绝。默认 true。
    #[serde(default = "default_replay_protection_enabled")]
    pub replay_protection_enabled: bool,

    /// Per-origin federation rate limiting. When enabled, each remote server
    /// is rate-limited independently based on its authenticated `origin`.
    #[serde(default)]
    pub rate_limit: FederationRateLimitConfig,
}

fn default_signing_ts_tolerance_ms() -> i64 {
    86_400_000 // 24h
}

fn default_replay_protection_enabled() -> bool {
    true
}

/// Per-origin federation rate limit configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct FederationRateLimitConfig {
    /// Master switch for per-origin federation rate limiting.
    #[serde(default = "default_federation_rate_limit_enabled")]
    pub enabled: bool,
    /// Requests per second per origin.
    #[serde(default = "default_federation_rate_limit_per_second")]
    pub per_second: u32,
    /// Burst size (maximum tokens in the bucket).
    #[serde(default = "default_federation_rate_limit_burst_size")]
    pub burst_size: u32,
    /// S17/SEC-02: 限流后端（Redis）故障时的行为。false（默认，与主限流器
    /// 对齐）= fail-closed 返回 5xx；true = fail-open 放行并告警。
    /// 注意：2026-08-10 之前联邦限流在 Redis 故障时无条件放行且无配置项。
    #[serde(default = "default_federation_rate_limit_fail_open")]
    pub fail_open_on_error: bool,
}

impl Default for FederationRateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: default_federation_rate_limit_enabled(),
            per_second: default_federation_rate_limit_per_second(),
            burst_size: default_federation_rate_limit_burst_size(),
            fail_open_on_error: default_federation_rate_limit_fail_open(),
        }
    }
}

fn default_federation_rate_limit_enabled() -> bool {
    // S7 修复（2026-08-11）：联邦按源站限流默认开启。
    // 此前默认 false，入站联邦（事件鉴权/状态决议/签名/写库）默认无节流，
    // 易遭 DoS。per_second=50 / burst=200 已有合理默认；显式 "enabled": false
    // 仍为合法配置（受信私有联邦场景）。
    true
}

fn default_federation_rate_limit_per_second() -> u32 {
    50
}

fn default_federation_rate_limit_burst_size() -> u32 {
    200
}

fn default_federation_rate_limit_fail_open() -> bool {
    false
}

/// 信任的密钥服务器配置
#[derive(Debug, Clone, Deserialize)]
pub struct TrustedKeyServer {
    /// 服务器名称
    pub server_name: String,
    /// 验证密钥（可选）
    #[serde(default)]
    pub verify_keys: Option<HashMap<String, String>>,
}

fn default_trusted_key_servers() -> Vec<TrustedKeyServer> {
    vec![TrustedKeyServer { server_name: "matrix.org".to_string(), verify_keys: None }]
}

fn default_key_refresh_interval() -> u64 {
    86400
}

fn default_signature_cache_ttl() -> u64 {
    3600
}

fn default_key_cache_ttl() -> u64 {
    3600
}

fn default_key_rotation_grace_period_ms() -> u64 {
    600 * 1000
}

fn default_federation_key_fetch_max_concurrency() -> usize {
    32
}

fn default_federation_key_fetch_timeout_ms() -> u64 {
    5000
}

fn default_federation_inbound_edus_max_per_txn() -> usize {
    100
}

fn default_federation_inbound_presence_updates_max_per_txn() -> usize {
    50
}

fn default_federation_inbound_edu_max_concurrency() -> usize {
    8
}

fn default_federation_inbound_edu_acquire_timeout_ms() -> u64 {
    250
}

fn default_federation_inbound_edu_per_origin_max_concurrency() -> usize {
    2
}

fn default_federation_inbound_presence_backoff_ms() -> u64 {
    3000
}

fn default_event_broadcast_batch_size() -> usize {
    100
}

fn default_federation_join_max_concurrency() -> usize {
    16
}

fn default_federation_join_acquire_timeout_ms() -> u64 {
    750
}

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // S17 / SEC-02: 联邦限流必须有 fail_open_on_error 配置项，
    // 且默认与主限流器对齐（fail-closed）
    // ------------------------------------------------------------------

    #[test]
    fn federation_rate_limit_fail_open_defaults_to_false() {
        let config = FederationRateLimitConfig::default();
        assert!(!config.fail_open_on_error, "默认必须与主限流器 fail-closed 语义对齐");
    }

    #[test]
    fn federation_rate_limit_fail_open_deser_defaults_to_false() {
        // 缺字段（旧配置文件）→ 默认 fail-closed
        let config: FederationRateLimitConfig = serde_json::from_str("{}").unwrap();
        assert!(!config.fail_open_on_error);
        // S7 修复：联邦按源站限流默认开启（此前默认关闭，入站联邦无节流，易遭 DoS）
        assert!(config.enabled, "S7: 联邦按源站限流必须默认开启");
        assert_eq!(config.per_second, 50);
        assert_eq!(config.burst_size, 200);
    }

    #[test]
    fn federation_rate_limit_enabled_defaults_to_true() {
        // S7 修复：默认开启，与客户端限流/背压体系对齐
        let config = FederationRateLimitConfig::default();
        assert!(config.enabled, "S7: 联邦按源站限流必须默认开启");
    }

    #[test]
    fn federation_rate_limit_explicit_opt_out_still_supported() {
        // 显式关闭仍是合法配置（如受信私有联邦），只是不再是默认值
        let config: FederationRateLimitConfig = serde_json::from_str(r#"{"enabled": false}"#).unwrap();
        assert!(!config.enabled);
    }

    #[test]
    fn federation_rate_limit_fail_open_explicit_opt_in() {
        let config: FederationRateLimitConfig = serde_json::from_str(r#"{"fail_open_on_error": true}"#).unwrap();
        assert!(config.fail_open_on_error, "显式配置 true 时必须生效（放行）");
    }
}
