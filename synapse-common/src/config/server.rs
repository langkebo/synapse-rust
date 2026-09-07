use educe::Educe;
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
#[derive(Clone, Deserialize, Default, Educe)]
#[educe(Debug)]
/// Represents ServerConfig.
pub struct ServerConfig {
    /// 服务器名称（域名）
    /// Matrix 规范要求的唯一标识符，格式如 "example.com"
    pub name: String,

    /// 监听主机地址
    /// 默认 "0.0.0.0" 表示监听所有接口
    #[serde(default = "default_server_host")]
    /// `host` field.
    pub host: String,

    /// 监听端口
    /// 默认 8008 (HTTP) 或 8448 (HTTPS)
    #[serde(default = "default_server_port")]
    /// `port` field.
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
    #[educe(Debug(ignore))]
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
    #[educe(Debug(ignore))]
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
    /// `suppress_key_server_warning` field.
    pub suppress_key_server_warning: bool,

    /// 是否提供 .well-known 服务
    ///
    /// 启用后，服务器将在 https://<server_name>/.well-known/matrix/server
    /// 提供服务，告诉其他服务器将联邦流量发送到端口 443 而非 8448。
    #[serde(default)]
    /// `serve_server_wellknown` field.
    pub serve_server_wellknown: bool,

    /// 文件描述符软限制
    ///
    /// 设置 synapse 可以使用的文件描述符数量的软限制。
    /// 设置为 0 表示使用硬限制。
    #[serde(default)]
    /// `soft_file_limit` field.
    pub soft_file_limit: u32,

    /// 用户代理后缀
    ///
    /// 附加到 Synapse 用户代理字符串后的后缀。
    #[serde(default)]
    /// `user_agent_suffix` field.
    pub user_agent_suffix: Option<String>,

    /// Web 客户端位置
    ///
    /// 当用户访问根路径时重定向到的 Web 客户端 URL。
    #[serde(default)]
    /// `web_client_location` field.
    pub web_client_location: Option<String>,

    /// 地图瓦片样式 URL
    ///
    /// 可选。配置后，`GET /.well-known/matrix/client` 响应体将追加
    /// `"m.tile_server": { "map_style_url": <url> }`，供客户端发现地图
    /// 瓦片样式。默认 `None`。
    ///
    /// 可通过标准环境变量覆盖机制 `SYNAPSE__SERVER__MAP_STYLE_URL` 覆盖。
    #[serde(default)]
    /// `map_style_url` field.
    pub map_style_url: Option<String>,

    // ===== 原有字段 =====
    /// 注册共享密钥（用于管理员注册）
    #[educe(Debug(ignore))]
    /// `registration_shared_secret` field.
    pub registration_shared_secret: Option<String>,

    /// 管理员联系邮箱
    pub admin_contact: Option<String>,

    /// 最大上传大小（字节）
    ///
    /// G-1: 这是上传大小上限的唯一权威来源。media 路由的 DefaultBodyLimit、
    /// 全局 RequestBodyLimitLayer 及 `m.upload.size` 响应均从本字段派生。
    #[serde(default = "default_max_upload_size_value")]
    /// `max_upload_size` field.
    pub max_upload_size: u64,

    /// 最大图片分辨率
    pub max_image_resolution: u32,

    /// 远程媒体缓存保留时间（秒），默认 30 天
    #[serde(default = "default_remote_media_lifetime")]
    /// `remote_media_lifetime` field.
    pub remote_media_lifetime: u64,

    /// 本地媒体保留时间（秒），0 表示永不过期
    #[serde(default)]
    /// `local_media_lifetime` field.
    pub local_media_lifetime: u64,

    /// 是否允许用户注册
    pub enable_registration: bool,

    /// 是否启用注册验证码
    pub enable_registration_captcha: bool,

    /// 后台任务执行间隔（秒）
    pub background_tasks_interval: u64,

    /// 脱水设备过期清理任务执行间隔（秒）
    #[serde(default = "default_dehydrated_device_cleanup_interval_secs")]
    /// `dehydrated_device_cleanup_interval_secs` field.
    pub dehydrated_device_cleanup_interval_secs: u64,

    /// 是否使访问令牌过期
    pub expire_access_token: bool,

    /// 访问令牌过期时间
    pub expire_access_token_lifetime: i64,

    /// 刷新令牌滑动窗口大小
    pub refresh_token_sliding_window_size: i64,

    #[serde(default = "default_warmup_pool")]
    /// `warmup_pool` field.
    pub warmup_pool: bool,

    /// 是否允许未认证用户访问公共房间目录
    #[serde(default)]
    /// `allow_public_rooms_without_auth` field.
    pub allow_public_rooms_without_auth: bool,

    /// 是否允许通过联邦访问公共房间目录
    #[serde(default = "default_true")]
    /// `allow_public_rooms_over_federation` field.
    pub allow_public_rooms_over_federation: bool,

    /// 新用户自动加入的房间列表
    #[serde(default)]
    /// `auto_join_rooms` field.
    pub auto_join_rooms: Vec<String>,

    /// 是否自动创建 auto_join_rooms 中不存在的房间
    #[serde(default = "default_true")]
    /// `autocreate_auto_join_rooms` field.
    pub autocreate_auto_join_rooms: bool,

    /// 默认启用加密的房间类型（空表示不默认启用）
    #[serde(default)]
    /// `encryption_enabled_by_default_for_room_type` field.
    pub encryption_enabled_by_default_for_room_type: Option<String>,

    /// 应用服务配置文件路径列表
    #[serde(default)]
    /// `app_service_config_files` field.
    pub app_service_config_files: Vec<String>,

    /// 是否启用 Presence 功能
    #[serde(default = "default_true")]
    /// `presence_enabled` field.
    pub presence_enabled: bool,

    /// 不参与 presence 计算的房间 ID 列表（Synapse parity:
    /// `exclude_rooms_from_presence`）。在这些房间中的成员关系/事件
    /// 不会触发 presence 更新。默认为空列表。
    #[serde(default)]
    /// `exclude_rooms_from_presence` field.
    pub exclude_rooms_from_presence: Vec<String>,

    /// 最后活跃时间的粒度（毫秒），默认 120000 (2 分钟)。
    /// 控制 presence last_active_ts 的更新频率，避免每次心跳都写库。
    #[serde(default = "default_last_active_granularity")]
    /// `last_active_granularity` field.
    pub last_active_granularity: u64,

    /// 同步在线超时（毫秒），默认 30000 (30 秒)。
    /// 客户端长轮询同步 online presence 的最大等待时间。
    #[serde(default = "default_sync_online_timeout")]
    /// `sync_online_timeout` field.
    pub sync_online_timeout: u64,

    /// 空闲超时（毫秒），默认 300000 (5 分钟)。
    /// 用户超过该时间无活动后，presence 由 online 切换为 unavailable。
    #[serde(default = "default_idle_timeout")]
    /// `idle_timeout` field.
    pub idle_timeout: u64,

    /// 媒体文件存储路径。
    ///
    /// 控制媒体服务把上传的文件写到哪个目录。可通过标准环境变量覆盖
    /// 机制 `SYNAPSE__SERVER__MEDIA_PATH` 覆盖。默认 `./data/media`。
    #[serde(default = "default_media_path")]
    /// `media_path` field.
    pub media_path: String,

    /// Megolm 加密密钥文件路径。
    ///
    /// 用于持久化 E2EE megolm 会话的加密密钥。可通过标准环境变量覆盖
    /// 机制 `SYNAPSE__SERVER__MEGOLM_ENCRYPTION_KEY_PATH` 覆盖。
    /// 未设置时服务器会生成临时密钥并在重启后丢失已加密的会话。
    #[serde(default)]
    /// `megolm_encryption_key_path` field.
    pub megolm_encryption_key_path: Option<String>,

    /// 是否抑制 r0 路由弃用告警。
    ///
    /// r0 路由为兼容旧版 Matrix 客户端而保留。默认情况下，启动时会
    /// 打印一条 WARN 日志列出已注册的 r0 路由数量。当部署明确需要
    /// 支持 r0 客户端时，可设为 `true` 抑制该告警。
    ///
    /// 也可通过环境变量 `SYNAPSE__SERVER__SUPPRESS_R0_DEPRECATION_WARNING` 覆盖。
    #[serde(default)]
    /// `suppress_r0_deprecation_warning` field.
    pub suppress_r0_deprecation_warning: bool,

    /// 是否抑制 vendor 私有端点旧别名弃用告警。
    ///
    /// ISSUE-13: 私有端点 (/my_rooms, /search_rooms, /search_recipients)
    /// 已迁移至 `/_matrix/vendor/v1/`。旧的 `/_matrix/client/v3/` 别名
    /// 保留用于向后兼容但已弃用。默认启动时打印一条 WARN 日志。
    /// 当部署明确需要保留旧别名时，可设为 `true` 抑制该告警。
    ///
    /// 也可通过环境变量 `SYNAPSE__SERVER__SUPPRESS_VENDOR_ENDPOINT_WARNING` 覆盖。
    #[serde(default)]
    /// `suppress_vendor_endpoint_warning` field.
    pub suppress_vendor_endpoint_warning: bool,

    /// 是否启动 burn-after-read 处理器。
    ///
    /// 默认 `true`。可通过标准环境变量覆盖机制
    /// `SYNAPSE__SERVER__ENABLE_BURN_AFTER_READ_PROCESSOR` 覆盖。
    #[serde(default = "default_true")]
    /// `enable_burn_after_read_processor` field.
    pub enable_burn_after_read_processor: bool,

    /// 联邦重试最大次数。
    ///
    /// 后台任务对每个待处理事务最多重试多少次后放弃。达到上限后重试计数器
    /// 重置，下次 tick 重新开始尝试。增大可提高 federation 可靠性，
    /// 但会增加服务器负载。
    #[serde(default = "default_federation_retry_max_count")]
    /// `federation_retry_max_count` field.
    pub federation_retry_max_count: u64,

    /// 优雅关闭时等待 in-flight 请求排空的超时时间（秒）。
    ///
    /// SIGTERM 收到后，服务器停止接收新请求并等待已有请求完成。
    /// 如果超过此时间还有请求未完成则强制退出，防止长轮询（如 90s+ /sync）
    /// 永久阻塞滚动更新。增大可给请求更多时间完成，但会延迟重启。
    #[serde(default = "default_drain_timeout_secs")]
    /// `drain_timeout_secs` field.
    pub drain_timeout_secs: u64,

    /// Megolm 会话密钥清理间隔（秒）。
    ///
    /// 定期删除已过期的 Megolm 加密会话密钥，防止 megolm_sessions 表无限增长。
    /// 每次清理还会重新加密设备列表中的会话密钥。调大可减少清理频率，
    /// 但会增加存储占用；调小则更积极释放空间。
    #[serde(default = "default_megolm_cleanup_interval_secs")]
    /// `megolm_cleanup_interval_secs` field.
    pub megolm_cleanup_interval_secs: u64,

    /// 数据裁剪间隔（秒）。
    ///
    /// 定期裁剪以下过期数据：
    /// - device list changes（默认 90 天）
    /// - device lists stream
    /// - 用户 session（默认 90 天）
    /// - presence 记录（默认 7 天）
    /// - 一次性密钥（默认 7 天）
    ///
    /// 增大间隔会延迟数据清理，占用更多存储但减少清理开销。
    #[serde(default = "default_pruning_interval_secs")]
    /// `pruning_interval_secs` field.
    pub pruning_interval_secs: u64,

    /// 数据库健康检查间隔（秒）。
    ///
    /// 定期检查数据库连接池使用率、超时率等指标。
    #[serde(default = "default_health_check_interval_secs")]
    /// `health_check_interval_secs` field.
    pub health_check_interval_secs: u64,

    /// 性能指标采集间隔（秒）。
    ///
    /// 定期采集慢查询数、平均查询时间、TPS、缓存命中率等。
    #[serde(default = "default_performance_check_interval_secs")]
    /// `performance_check_interval_secs` field.
    pub performance_check_interval_secs: u64,

    /// 数据完整性检查间隔（秒）。
    ///
    /// 定期检查外键完整性、孤立记录、重复条目等。
    #[serde(default = "default_integrity_check_interval_secs")]
    /// `integrity_check_interval_secs` field.
    pub integrity_check_interval_secs: u64,

    /// 数据库维护任务间隔（秒）。
    ///
    /// 定期执行 VACUUM ANALYZE、重建索引等维护操作。
    /// 维护任务启动后有 5 分钟预热期避免与冷启动流量冲突。
    #[serde(default = "default_maintenance_interval_secs")]
    /// `maintenance_interval_secs` field.
    pub maintenance_interval_secs: u64,

    /// 单次 `PUT /_matrix/client/v3/sendToDevice` 允许的最大收件人数（用户×设备组合）。
    ///
    /// 跨所有用户的 device 数量合计。超过会返回 400。
    /// 灵感来自 Synapse v1.155 (#19617) 对 to-device EDU 大小的限制。
    /// 默认 5000。
    #[serde(default = "default_to_device_max_recipients")]
    /// `to_device_max_recipients` field.
    pub to_device_max_recipients: usize,

    /// 单个 to-device 消息体的最大字节数（序列化后）。
    ///
    /// 超过会返回 400。保护下游存储和联邦队列不被巨型消息塞满。
    /// 默认 64 KiB（65536 bytes）。
    #[serde(default = "default_to_device_max_payload_bytes")]
    /// `to_device_max_payload_bytes` field.
    pub to_device_max_payload_bytes: usize,

    /// 管理员踢出用户（`evict_user_from_joined_rooms`）时的最大并发房间数。
    ///
    /// 单用户加入的房间数可能高达数百至数千；并发移除受此上限节流，
    /// 防止瞬时连接池耗尽。参考 Synapse 的 `evict_max_concurrency = 8` 默认。
    /// 设为 1 退化为串行；设为 0 等同 1（不并发）。
    #[serde(default = "default_admin_evict_max_concurrency")]
    /// `admin_evict_max_concurrency` field.
    pub admin_evict_max_concurrency: usize,

    /// 管理员踢出用户时分页拉取已加入房间的页大小。
    ///
    /// 单用户加入的海量房间不会一次返回，而是按此大小分页游标遍历。
    /// 默认 1000。Synapse 内部使用 100（`/_matrix/client/v3/joined_rooms` 默认），
    /// 但管理员内部接口可以稍大以减少往返。
    #[serde(default = "default_admin_evict_page_size")]
    /// `admin_evict_page_size` field.
    pub admin_evict_page_size: i64,

    /// `EventNotifier` 推荐给 sync 路由调用的默认 idle 等待时长（秒）。
    ///
    /// sync 长轮询调用方通常会对 `EventNotifier::slots_for(...).notified()`
    /// 包一个 `tokio::time::timeout(this_value, ...)`。运维调小此值能让
    /// sync 更快返回（更频繁的"无新事件"空响应），调大能减少 CPU 唤醒。
    ///
    /// 默认 5 秒，与 synapse 的 `notifier.notify_sleep_time` 量级一致。
    /// 设为 0 表示"不推荐 timeout"（调用方应自己决定）。
    #[serde(default = "default_event_notifier_idle_timeout_secs")]
    /// `event_notifier_idle_timeout_secs` field.
    pub event_notifier_idle_timeout_secs: u64,
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

/// Defaults the dehydrated.
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

fn default_last_active_granularity() -> u64 {
    120_000
}

fn default_sync_online_timeout() -> u64 {
    30_000
}

fn default_idle_timeout() -> u64 {
    300_000
}

fn default_federation_retry_max_count() -> u64 {
    5
}

fn default_drain_timeout_secs() -> u64 {
    30
}

fn default_megolm_cleanup_interval_secs() -> u64 {
    21600 // 6 hours
}

fn default_pruning_interval_secs() -> u64 {
    86400 // 24 hours
}

fn default_health_check_interval_secs() -> u64 {
    10
}

fn default_performance_check_interval_secs() -> u64 {
    300 // 5 minutes
}

fn default_integrity_check_interval_secs() -> u64 {
    3600 // 1 hour
}

fn default_maintenance_interval_secs() -> u64 {
    86400 // 24 hours
}

fn default_to_device_max_recipients() -> usize {
    5000
}

fn default_to_device_max_payload_bytes() -> usize {
    64 * 1024
}

fn default_admin_evict_max_concurrency() -> usize {
    8
}

fn default_admin_evict_page_size() -> i64 {
    1000
}

fn default_event_notifier_idle_timeout_secs() -> u64 {
    5
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
        #[cfg(not(debug_assertions))]
        {
            let fallback = format!("http://{}:{}", host, self.port);
            tracing::warn!(
                host = %host,
                port = %self.port,
                "public_baseurl is not configured — falling back to HTTP ({fallback}). \
                 Set public_baseurl to an HTTPS URL in production."
            );
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config() -> ServerConfig {
        ServerConfig::default()
    }

    #[test]
    fn get_server_name_prefers_server_name_field() {
        let mut config = make_config();
        config.name = "fallback.example.com".into();
        config.server_name = Some("primary.example.com".into());
        assert_eq!(config.get_server_name(), "primary.example.com");
    }

    #[test]
    fn get_server_name_falls_back_to_name() {
        let mut config = make_config();
        config.name = "example.com".into();
        config.server_name = None;
        assert_eq!(config.get_server_name(), "example.com");
    }

    #[test]
    fn get_event_server_name_delegates_to_get_server_name() {
        let mut config = make_config();
        config.name = "events.example.com".into();
        config.server_name = None;
        assert_eq!(config.get_event_server_name(), "events.example.com");
    }

    #[test]
    fn get_public_baseurl_uses_configured_value() {
        let mut config = make_config();
        config.public_baseurl = Some("https://matrix.example.com".into());
        assert_eq!(config.get_public_baseurl(), "https://matrix.example.com");
    }

    #[test]
    fn get_public_baseurl_empty_string_falls_back() {
        let mut config = make_config();
        config.public_baseurl = Some("".into());
        config.host = "192.168.1.1".into();
        config.port = 8448;
        assert_eq!(config.get_public_baseurl(), "http://192.168.1.1:8448");
    }

    #[test]
    fn get_public_baseurl_zero_zero_zero_zero_replaced_with_localhost() {
        let mut config = make_config();
        config.public_baseurl = None;
        config.host = "0.0.0.0".into();
        config.port = 8008;
        assert_eq!(config.get_public_baseurl(), "http://localhost:8008");
    }

    #[test]
    fn get_public_baseurl_double_colon_replaced_with_localhost() {
        let mut config = make_config();
        config.public_baseurl = None;
        config.host = "::".into();
        config.port = 443;
        assert_eq!(config.get_public_baseurl(), "http://localhost:443");
    }

    #[test]
    fn get_public_baseurl_uses_host_as_is_when_not_wildcard() {
        let mut config = make_config();
        config.public_baseurl = None;
        config.host = "matrix.example.com".into();
        config.port = 8080;
        assert_eq!(config.get_public_baseurl(), "http://matrix.example.com:8080");
    }

    // ── exclude_rooms_from_presence (MSC-inspired, Synapse parity) ──

    #[test]
    fn exclude_rooms_from_presence_defaults_to_empty() {
        let config = make_config();
        assert!(config.exclude_rooms_from_presence.is_empty(), "default should be an empty list");
    }

    #[test]
    fn exclude_rooms_from_presence_parses_from_yaml() {
        let config: ServerConfig = serde_yaml::from_str(minimal_server_yaml()).expect("parse should succeed");
        assert!(config.exclude_rooms_from_presence.is_empty());
    }

    #[test]
    fn exclude_rooms_from_presence_parses_populated_list() {
        let yaml = format!(
            r#"
{}
exclude_rooms_from_presence:
  - "!internal:example.com"
  - "!lobby:example.com"
"#,
            minimal_server_yaml_body()
        );
        let config: ServerConfig = serde_yaml::from_str(&yaml).expect("parse should succeed");
        assert_eq!(config.exclude_rooms_from_presence, vec!["!internal:example.com", "!lobby:example.com"]);
    }

    // ── Presence tuning parameters (P2.4) ───────────────────────────

    #[test]
    fn last_active_granularity_defaults_to_two_minutes() {
        let config: ServerConfig = serde_yaml::from_str(minimal_server_yaml()).expect("parse should succeed");
        assert_eq!(config.last_active_granularity, 120_000, "default should be 120000 ms (2 min)");
    }

    #[test]
    fn sync_online_timeout_defaults_to_thirty_seconds() {
        let config: ServerConfig = serde_yaml::from_str(minimal_server_yaml()).expect("parse should succeed");
        assert_eq!(config.sync_online_timeout, 30_000, "default should be 30000 ms (30 s)");
    }

    #[test]
    fn idle_timeout_defaults_to_five_minutes() {
        let config: ServerConfig = serde_yaml::from_str(minimal_server_yaml()).expect("parse should succeed");
        assert_eq!(config.idle_timeout, 300_000, "default should be 300000 ms (5 min)");
    }

    #[test]
    fn presence_tuning_params_parse_from_yaml() {
        let yaml = format!(
            r#"
{}
last_active_granularity: 60000
sync_online_timeout: 15000
idle_timeout: 180000
"#,
            minimal_server_yaml_body()
        );
        let config: ServerConfig = serde_yaml::from_str(&yaml).expect("parse should succeed");
        assert_eq!(config.last_active_granularity, 60_000);
        assert_eq!(config.sync_online_timeout, 15_000);
        assert_eq!(config.idle_timeout, 180_000);
    }

    // ── map_style_url (tile server map style) ──────────────────────

    #[test]
    fn map_style_url_defaults_to_none() {
        let config = make_config();
        assert!(config.map_style_url.is_none(), "default should be None");
    }

    #[test]
    fn map_style_url_parses_from_yaml() {
        let yaml = format!(
            r#"
{}
map_style_url: "https://tiles.example.com/style.json"
"#,
            minimal_server_yaml_body()
        );
        let config: ServerConfig = serde_yaml::from_str(&yaml).expect("parse should succeed");
        assert_eq!(config.map_style_url.as_deref(), Some("https://tiles.example.com/style.json"));
    }

    /// Minimal YAML body satisfying all required (non-defaulted) ServerConfig fields.
    fn minimal_server_yaml_body() -> &'static str {
        r#"
name: example.com
registration_shared_secret: null
admin_contact: null
max_image_resolution: 100
enable_registration: false
enable_registration_captcha: false
background_tasks_interval: 60
expire_access_token: false
expire_access_token_lifetime: 0
refresh_token_sliding_window_size: 0
"#
    }

    fn minimal_server_yaml() -> &'static str {
        minimal_server_yaml_body()
    }
}
