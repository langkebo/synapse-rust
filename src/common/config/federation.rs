use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

// ============================================================================
// SECTION: Federation Configuration
// ============================================================================

/// 联邦配置。
///
/// 配置与其他 Matrix 服务器的联邦通信参数。
#[derive(Debug, Clone, Deserialize, Default)]
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
    pub signing_key_master_key: Option<String>,
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

fn default_federation_join_max_concurrency() -> usize {
    16
}

fn default_federation_join_acquire_timeout_ms() -> u64 {
    750
}