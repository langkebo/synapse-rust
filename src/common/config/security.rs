use serde::Deserialize;

// ============================================================================
// SECTION: Security Configuration
// ============================================================================

/// 安全配置。
///
/// 配置认证、加密和密码哈希参数。
#[derive(Debug, Clone, Deserialize, Default)]
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
    /// 是否允许旧版 SHA-256 密码哈希验证（出于安全考虑，建议设置为 false）
    #[serde(default = "default_allow_legacy_hashes")]
    pub allow_legacy_hashes: bool,
    /// 登录失败锁定阈值（连续失败多少次后锁定账户）
    #[serde(default = "default_login_failure_lockout_threshold")]
    pub login_failure_lockout_threshold: u32,
    /// 锁定持续时间（秒）
    #[serde(default = "default_login_lockout_duration_seconds")]
    pub login_lockout_duration_seconds: u64,
    /// 是否强制管理员登录必须通过 MFA
    #[serde(default)]
    pub admin_mfa_required: bool,
    /// 管理员 TOTP 共享密钥，支持 Base32；解析失败时回退为原始字节
    #[serde(default)]
    pub admin_mfa_shared_secret: String,
    /// 允许的时间漂移窗口（30 秒步长）
    #[serde(default = "default_admin_mfa_allowed_drift_steps")]
    pub admin_mfa_allowed_drift_steps: u32,
    /// 是否启用基于 user_type 的管理员 RBAC
    #[serde(default = "default_admin_rbac_enabled")]
    pub admin_rbac_enabled: bool,
    /// UIA 会话超时时间（秒），默认 900 秒（15 分钟）
    #[serde(default = "default_ui_auth_session_timeout")]
    pub ui_auth_session_timeout: i64,
}

fn default_login_failure_lockout_threshold() -> u32 {
    5
}

fn default_login_lockout_duration_seconds() -> u64 {
    900
}

pub fn default_admin_mfa_allowed_drift_steps() -> u32 {
    1
}

pub fn default_admin_rbac_enabled() -> bool {
    true
}

pub fn default_ui_auth_session_timeout() -> i64 {
    900
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
    Vec::new()
}

fn default_allow_credentials() -> bool {
    false
}

pub fn default_allowed_methods() -> Vec<String> {
    vec!["GET".to_string(), "POST".to_string(), "PUT".to_string(), "DELETE".to_string(), "OPTIONS".to_string()]
}

pub fn default_allowed_headers() -> Vec<String> {
    vec!["Authorization".to_string(), "Content-Type".to_string(), "Accept".to_string(), "X-Requested-With".to_string()]
}

pub fn default_cors_max_age() -> u64 {
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
    #[serde(default = "default_admin_registration_allow_external_access")]
    pub allow_external_access: bool,
    #[serde(default = "default_admin_registration_production_only")]
    pub production_only: bool,
    #[serde(default)]
    pub ip_whitelist: Vec<String>,
    #[serde(default)]
    pub require_captcha: bool,
    #[serde(default)]
    pub require_manual_approval: bool,
    #[serde(default)]
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