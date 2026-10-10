use base64::Engine;
use educe::Educe;
use rand::RngCore;
use serde::Deserialize;

// ============================================================================
// SECTION: Security Configuration
// ============================================================================

/// 安全配置。
///
/// 配置认证、加密和密码哈希参数。
#[derive(Clone, Deserialize, Educe)]
#[educe(Debug)]
/// Represents SecurityConfig.
pub struct SecurityConfig {
    /// 密钥字符串
    #[educe(Debug(ignore))]
    /// `secret` field.
    pub secret: String,
    /// 令牌过期时间
    pub expiry_time: i64,
    /// 刷新令牌过期时间
    pub refresh_token_expiry: i64,
    /// Argon2 内存成本
    #[serde(default = "default_argon2_m_cost")]
    /// `argon2_m_cost` field.
    pub argon2_m_cost: u32,
    /// Argon2 时间成本
    #[serde(default = "default_argon2_t_cost")]
    /// `argon2_t_cost` field.
    pub argon2_t_cost: u32,
    /// Argon2 并行度
    #[serde(default = "default_argon2_p_cost")]
    /// `argon2_p_cost` field.
    pub argon2_p_cost: u32,
    /// 是否允许旧版 SHA-256 密码哈希验证（出于安全考虑，建议设置为 false）
    #[serde(default = "default_allow_legacy_hashes")]
    /// `allow_legacy_hashes` field.
    pub allow_legacy_hashes: bool,
    /// 登录失败锁定阈值（连续失败多少次后锁定账户）
    #[serde(default = "default_login_failure_lockout_threshold")]
    /// `login_failure_lockout_threshold` field.
    pub login_failure_lockout_threshold: u32,
    /// 锁定持续时间（秒）
    #[serde(default = "default_login_lockout_duration_seconds")]
    /// `login_lockout_duration_seconds` field.
    pub login_lockout_duration_seconds: u64,
    /// 登录锁定机制在 Redis 不可用时是否放行（fail-open）。
    ///
    /// - false（默认）：Redis 挂掉时直接返回 503，拒绝所有登录请求（避免 fail-open 暴力破解窗口）
    /// - true：Redis 挂掉时跳过锁定检查，登录照常进行
    ///
    /// 默认 fail-closed：Redis 故障时宁可拒绝登录，也不暴露暴力破解窗口。
    /// 仅当 Redis 与 synapse 同进程内嵌（无外部依赖）时，才可显式设为 true。
    #[serde(default = "default_login_lockout_fail_open")]
    /// `login_lockout_fail_open_on_redis_error` field.
    pub login_lockout_fail_open_on_redis_error: bool,
    /// 单个出口 IP 的登录失败上限（跨用户名聚合，SEC-05）。
    ///
    /// 与 `login_failure_lockout_threshold`（按 (ip, username) 对）是两个独立桶：
    /// 后者挡单账号爆破，本项挡「同一地址轮换用户名」的喷洒式爆破。
    /// 必须 ≥ 前者，否则更具体的桶永远先触发、本桶形同虚设。
    #[serde(default = "default_login_lockout_max_ip_attempts")]
    /// `login_lockout_max_ip_attempts` field.
    pub login_lockout_max_ip_attempts: u32,
    /// 登录锁定豁免名单（IP 或 CIDR，例如 `127.0.0.1` / `10.0.0.0/8`）。
    ///
    /// 用途：受信出口（CI runner、内网 NAT、本机联调）不应被登录锁定波及 ——
    /// 按 IP 聚合的锁定会让整个出口的**所有**用户名一起被拒 900 秒，
    /// 直接把自动化测试打成「认证全失败」（2026-10-10 实测：API 全量测试因
    /// 命中外层 IP 桶而静默降级为匿名探测，却仍报 100/100）。
    ///
    /// 安全边界：豁免只作用于**锁定**（不拒绝、不计数），限流（rate limit）与
    /// 密码校验仍照常生效；且默认**空** —— 必须显式配置才放宽。
    ///
    /// ⚠️ 只能写在 `homeserver.yaml` 里：**无法**用
    /// `SYNAPSE__SECURITY__LOGIN_LOCKOUT_IP_ALLOWLIST` 覆盖 —— 实测
    /// 逗号分隔 / JSON 数组 / 带下标（`…_ALLOWLIST__0`）三种格式都不被接受
    /// （`config` crate 0.14 报 `expected an array`）。该事实由
    /// `config::loader::tests::list_valued_security_field_cannot_be_overridden_by_env` 钉死。
    #[serde(default)]
    /// `login_lockout_ip_allowlist` field.
    pub login_lockout_ip_allowlist: Vec<String>,
    /// 是否强制管理员登录必须通过 MFA
    #[serde(default)]
    /// `admin_mfa_required` field.
    pub admin_mfa_required: bool,
    /// 管理员 TOTP 共享密钥，支持 Base32；解析失败时回退为原始字节
    #[serde(default)]
    #[educe(Debug(ignore))]
    /// `admin_mfa_shared_secret` field.
    pub admin_mfa_shared_secret: String,
    /// 允许的时间漂移窗口（30 秒步长）
    #[serde(default = "default_admin_mfa_allowed_drift_steps")]
    /// `admin_mfa_allowed_drift_steps` field.
    pub admin_mfa_allowed_drift_steps: u32,
    /// 是否启用基于 user_type 的管理员 RBAC
    #[serde(default = "default_admin_rbac_enabled")]
    /// `admin_rbac_enabled` field.
    pub admin_rbac_enabled: bool,
    /// UIA 会话超时时间（秒），默认 900 秒（15 分钟）
    #[serde(default = "default_ui_auth_session_timeout")]
    /// `ui_auth_session_timeout` field.
    pub ui_auth_session_timeout: i64,
    /// HMAC secret for CSRF token signing. If not explicitly configured,
    /// a cryptographically random 32-byte secret is generated at startup.
    /// This secret is ephemeral (not persisted), so CSRF tokens from
    /// previous server runs will be invalid after restart.
    #[serde(default = "default_csrf_secret")]
    #[educe(Debug(ignore))]
    /// `csrf_secret` field.
    pub csrf_secret: String,
    /// 是否启用审计事件的异步写入（B-5）。
    ///
    /// 客户端 API 的写请求（POST/PUT/DELETE，含最高频的 `PUT /send/{txnId}`）
    /// 原本在 extractor 里同步落一条审计记录，等于给每次消息发送多加一个 DB
    /// 往返，直接放大写路径 p99。开启后审计事件改为投递到有界通道，由后台
    /// 任务批量落库，请求关键路径只剩一次非阻塞 `try_send`。
    ///
    /// 审计本身是合规需求但属于旁路：通道满或后端任务停止时**丢弃并记录告警**，
    /// 绝不阻塞业务请求（fail-soft）。真正需要同步确认落库的管理面操作
    /// （`/_synapse/admin/...`）不走这条路径，仍由 `create_event` 同步写并校验结果。
    #[serde(default = "default_audit_async_enabled")]
    /// `audit_async_enabled` field.
    pub audit_async_enabled: bool,
    /// 审计异步写入通道容量（条）。满了就丢弃并告警，防止审计压力反压业务请求。
    #[serde(default = "default_audit_channel_capacity")]
    /// `audit_channel_capacity` field.
    pub audit_channel_capacity: usize,
    /// 后台任务单次批量落库的最大条数。
    #[serde(default = "default_audit_batch_size")]
    /// `audit_batch_size` field.
    pub audit_batch_size: usize,
    /// 后台任务的最大攒批时间（毫秒）；超过则无论积攒多少都立即落库，
    /// 避免低流量时审计事件长时间停留在内存里。
    #[serde(default = "default_audit_flush_interval_ms")]
    /// `audit_flush_interval_ms` field.
    pub audit_flush_interval_ms: u64,
}

fn default_login_failure_lockout_threshold() -> u32 {
    5
}

fn default_login_lockout_duration_seconds() -> u64 {
    900
}

fn default_login_lockout_fail_open() -> bool {
    // 默认 false：fail-closed。Redis 故障时拒绝登录，避免 fail-open 暴力破解窗口。
    false
}

/// 单出口 IP 的登录失败上限默认值。
///
/// 与 `default_login_failure_lockout_threshold()`（5）保持 6 倍关系：足以让
/// 「同地址轮换用户名」的喷洒式爆破撞墙，又不会把正常 NAT 出口的日常误输密码
/// 直接锁死。原先该值硬编码在 `synapse-web/src/routes/auth_compat.rs`，
/// 与这里的配置项并存在**两个真相源**，已收敛到配置。
fn default_login_lockout_max_ip_attempts() -> u32 {
    30
}

/// Defaults the admin.
pub fn default_admin_mfa_allowed_drift_steps() -> u32 {
    1
}

/// Defaults the admin.
pub fn default_admin_rbac_enabled() -> bool {
    true
}

/// Defaults the ui.
pub fn default_ui_auth_session_timeout() -> i64 {
    900
}

/// Defaults the audit.
pub fn default_audit_async_enabled() -> bool {
    true
}

/// Defaults the audit.
pub fn default_audit_channel_capacity() -> usize {
    // 8192 条缓冲：足以吸收短时的写入尖峰（按单机 5k msg/s 估算约 1.6 秒缓冲），
    // 又不会让未落库的审计事件在内存里堆积过多。
    8192
}

/// Defaults the audit.
pub fn default_audit_batch_size() -> usize {
    64
}

/// Defaults the audit.
pub fn default_audit_flush_interval_ms() -> u64 {
    200
}

fn default_argon2_m_cost() -> u32 {
    65536
}

fn default_argon2_t_cost() -> u32 {
    3
}

fn default_argon2_p_cost() -> u32 {
    1
}

fn default_allow_legacy_hashes() -> bool {
    false
}

/// Generate a cryptographically random 32-byte secret for CSRF token signing.
/// This is used as a fallback when no explicit csrf_secret is configured.
fn default_csrf_secret() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            secret: String::new(),
            expiry_time: 0,
            refresh_token_expiry: 0,
            argon2_m_cost: default_argon2_m_cost(),
            argon2_t_cost: default_argon2_t_cost(),
            argon2_p_cost: default_argon2_p_cost(),
            allow_legacy_hashes: default_allow_legacy_hashes(),
            login_failure_lockout_threshold: default_login_failure_lockout_threshold(),
            login_lockout_duration_seconds: default_login_lockout_duration_seconds(),
            login_lockout_fail_open_on_redis_error: default_login_lockout_fail_open(),
            login_lockout_max_ip_attempts: default_login_lockout_max_ip_attempts(),
            login_lockout_ip_allowlist: Vec::new(),
            admin_mfa_required: false,
            admin_mfa_shared_secret: String::new(),
            admin_mfa_allowed_drift_steps: default_admin_mfa_allowed_drift_steps(),
            admin_rbac_enabled: default_admin_rbac_enabled(),
            ui_auth_session_timeout: default_ui_auth_session_timeout(),
            csrf_secret: default_csrf_secret(),
            audit_async_enabled: default_audit_async_enabled(),
            audit_channel_capacity: default_audit_channel_capacity(),
            audit_batch_size: default_audit_batch_size(),
            audit_flush_interval_ms: default_audit_flush_interval_ms(),
        }
    }
}

/// CORS 配置。
///
/// 配置跨域资源共享策略。
#[derive(Debug, Clone, Deserialize)]
/// Represents CorsConfig.
pub struct CorsConfig {
    /// 允许的来源列表
    #[serde(default = "default_allowed_origins")]
    /// `allowed_origins` field.
    pub allowed_origins: Vec<String>,
    /// 是否允许凭证
    #[serde(default = "default_allow_credentials")]
    /// `allow_credentials` field.
    pub allow_credentials: bool,
    /// 允许的 HTTP 方法
    #[serde(default = "default_allowed_methods")]
    /// `allowed_methods` field.
    pub allowed_methods: Vec<String>,
    /// 允许的请求头
    #[serde(default = "default_allowed_headers")]
    /// `allowed_headers` field.
    pub allowed_headers: Vec<String>,
    /// 预检请求最大缓存时间（秒）
    #[serde(default = "default_cors_max_age")]
    /// `max_age_seconds` field.
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
    Vec::new()
}

fn default_allow_credentials() -> bool {
    false
}

/// Defaults the allowed.
pub fn default_allowed_methods() -> Vec<String> {
    vec!["GET".to_string(), "POST".to_string(), "PUT".to_string(), "DELETE".to_string(), "OPTIONS".to_string()]
}

/// Defaults the allowed.
pub fn default_allowed_headers() -> Vec<String> {
    vec!["Authorization".to_string(), "Content-Type".to_string(), "Accept".to_string(), "X-Requested-With".to_string()]
}

/// Defaults the cors.
pub fn default_cors_max_age() -> u64 {
    86400
}

#[derive(Clone, Deserialize, Educe)]
#[educe(Debug)]
/// Represents AdminRegistrationConfig.
pub struct AdminRegistrationConfig {
    #[serde(default = "default_admin_registration_enabled")]
    /// `enabled` field.
    pub enabled: bool,
    #[serde(default = "default_admin_registration_shared_secret")]
    #[educe(Debug(ignore))]
    /// `shared_secret` field.
    pub shared_secret: String,
    #[serde(default = "default_admin_registration_nonce_timeout")]
    /// `nonce_timeout_seconds` field.
    pub nonce_timeout_seconds: u64,
    #[serde(default = "default_admin_registration_allow_external_access")]
    /// `allow_external_access` field.
    pub allow_external_access: bool,
    #[serde(default = "default_admin_registration_production_only")]
    /// `production_only` field.
    pub production_only: bool,
    #[serde(default)]
    /// `ip_whitelist` field.
    pub ip_whitelist: Vec<String>,
    #[serde(default)]
    /// `require_captcha` field.
    pub require_captcha: bool,
    #[serde(default)]
    /// `require_manual_approval` field.
    pub require_manual_approval: bool,
    #[serde(default)]
    /// `approval_tokens` field.
    pub approval_tokens: Vec<String>,
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

fn default_admin_registration_allow_external_access() -> bool {
    false
}

fn default_admin_registration_production_only() -> bool {
    true
}

impl Default for AdminRegistrationConfig {
    fn default() -> Self {
        Self {
            enabled: default_admin_registration_enabled(),
            shared_secret: default_admin_registration_shared_secret(),
            nonce_timeout_seconds: default_admin_registration_nonce_timeout(),
            allow_external_access: default_admin_registration_allow_external_access(),
            production_only: default_admin_registration_production_only(),
            ip_whitelist: Vec::new(),
            require_captcha: false,
            require_manual_approval: false,
            approval_tokens: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `secret` / `expiry_time` / `refresh_token_expiry` 没有 serde 默认值，
    /// 必须显式给出；其余字段一律有 default。
    fn minimal_security_yaml() -> &'static str {
        "secret: test_secret\nexpiry_time: 3600\nrefresh_token_expiry: 604800\n"
    }

    #[test]
    fn login_lockout_defaults_are_safe() {
        let cfg: SecurityConfig =
            serde_yaml::from_str(minimal_security_yaml()).expect("minimal security yaml must deserialize");

        assert_eq!(cfg.login_failure_lockout_threshold, 5);
        assert_eq!(cfg.login_lockout_duration_seconds, 900);
        assert_eq!(cfg.login_lockout_max_ip_attempts, 30, "IP 聚合上限默认 30，且必须可配置");
        assert!(cfg.login_lockout_ip_allowlist.is_empty(), "豁免名单默认必须为空 —— 放宽必须是显式动作");
        assert!(!cfg.login_lockout_fail_open_on_redis_error, "默认 fail-closed");
    }

    /// 键名 ↔ 字段 直连断言。
    ///
    /// 存在的理由：2026-10-10 实测发现 `synapse-web/src/routes/auth_compat.rs`
    /// 把锁定时长与 IP 上限**硬编码**（900s / 30 次），`homeserver.yaml` 里写
    /// `login_lockout_duration_seconds: 300` **完全不生效** —— 运行态返回
    /// `retry_after_ms: 900000` 而配置写着 300。这类「配置写了但没接线」的缺陷，
    /// 只有"用真实键名反序列化后断言字段值"能锁住（改字段名/挪层级都会红）。
    #[test]
    fn login_lockout_yaml_keys_are_wired_to_fields() {
        let yaml = format!(
            "{minimal}\n{extra}",
            minimal = minimal_security_yaml(),
            extra = "login_lockout_duration_seconds: 300\n\
                     login_failure_lockout_threshold: 7\n\
                     login_lockout_max_ip_attempts: 42\n\
                     login_lockout_ip_allowlist:\n  - 127.0.0.1\n  - 10.0.0.0/8\n  - ::1\n"
        );
        let cfg: SecurityConfig = serde_yaml::from_str(&yaml).expect("explicit security yaml must deserialize");

        assert_eq!(cfg.login_lockout_duration_seconds, 300, "homeserver.yaml 的值必须真的被读进来");
        assert_eq!(cfg.login_failure_lockout_threshold, 7);
        assert_eq!(cfg.login_lockout_max_ip_attempts, 42);
        assert_eq!(
            cfg.login_lockout_ip_allowlist,
            vec!["127.0.0.1".to_string(), "10.0.0.0/8".to_string(), "::1".to_string()]
        );
    }
}
