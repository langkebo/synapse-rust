use crate::auth::*;
use crate::cache::*;
use crate::common::metrics::MetricsCollector;
use crate::common::task_queue::RedisTaskQueue;
use crate::common::*;
// Import config types explicitly to avoid conflicts with the external `config` crate
use crate::common::config::{
    AdminRegistrationConfig, Config, CorsConfig, DatabaseConfig, FederationConfig,
    RateLimitConfig, RedisConfig, SearchConfig, SecurityConfig, ServerConfig, SmtpConfig,
    WorkerConfig,
};
use crate::e2ee::backup::KeyBackupService;
use crate::e2ee::cross_signing::CrossSigningService;
use crate::e2ee::device_keys::DeviceKeyService;
use crate::e2ee::megolm::MegolmService;
use crate::e2ee::to_device::ToDeviceService;
use crate::federation::{DeviceSyncManager, EventAuthChain, KeyRotationManager, FriendFederation};
use crate::storage::email_verification::EmailVerificationStorage;
use crate::storage::*;
use sqlx::{Pool, Postgres};
use std::sync::Arc;

/// 服务容器结构体。
///
/// 包含 Matrix Homeserver 运行所需的所有服务和存储句柄。
/// 该结构体在应用程序启动时创建，作为依赖注入的中心容器。
/// 所有服务和存储句柄都可以在线程间安全共享（实现了 Clone 和 Send）。
#[derive(Clone)]
pub struct ServiceContainer {
    /// 用户存储句柄
    pub user_storage: UserStorage,
    /// 设备存储句柄
    pub device_storage: DeviceStorage,
    /// 访问令牌存储句柄
    pub token_storage: AccessTokenStorage,
    /// 房间存储句柄
    pub room_storage: RoomStorage,
    /// 房间成员存储句柄
    pub member_storage: RoomMemberStorage,
    /// 事件存储句柄
    pub event_storage: EventStorage,
    /// 在线状态存储句柄
    pub presence_storage: PresenceStorage,
    /// 在线状态服务
    pub presence_service: PresenceStorage,
    /// 认证服务
    pub auth_service: AuthService,
    /// 设备密钥服务
    pub device_keys_service: DeviceKeyService,
    /// Megolm 会话服务
    pub megolm_service: MegolmService,
    /// 交叉签名服务
    pub cross_signing_service: CrossSigningService,
    /// 密钥备份服务
    pub backup_service: KeyBackupService,
    /// 设备间消息服务
    pub to_device_service: ToDeviceService,
    /// 语音消息服务
    pub voice_service: VoiceService,
    /// 注册服务
    pub registration_service: Arc<RegistrationService>,
    /// 房间服务
    pub room_service: Arc<RoomService>,
    /// 同步服务
    pub sync_service: Arc<SyncService>,
    /// 搜索服务
    pub search_service: Arc<crate::services::search_service::SearchService>,
    /// 媒体服务
    pub media_service: MediaService,
    /// 缓存管理器
    pub cache: Arc<CacheManager>,
    /// 任务队列
    pub task_queue: Option<Arc<RedisTaskQueue>>,
    /// 指标收集器
    pub metrics: Arc<MetricsCollector>,
    /// 服务器名称
    pub server_name: String,
    /// 服务器配置
    pub config: Config,
    /// 管理员注册服务
    pub admin_registration_service: AdminRegistrationService,
    /// 邮箱验证存储
    pub email_verification_storage: EmailVerificationStorage,
    /// 事件授权链服务
    pub event_auth_chain: EventAuthChain,
    /// 密钥轮换管理服务
    pub key_rotation_manager: KeyRotationManager,
    /// 设备同步管理服务
    pub device_sync_manager: DeviceSyncManager,
    /// 好友存储
    pub friend_storage: FriendRoomStorage,
    /// 好友房间服务
    pub friend_room_service: Arc<FriendRoomService>,
    /// 好友联邦服务
    pub friend_federation: Arc<FriendFederation>,
    /// 空间存储
    pub space_storage: SpaceStorage,
    /// 空间服务
    pub space_service: Arc<SpaceService>,
    /// 应用服务存储
    pub app_service_storage: ApplicationServiceStorage,
    /// 应用服务管理器
    pub app_service_manager: Arc<ApplicationServiceManager>,
    /// Worker 存储
    pub worker_storage: crate::worker::WorkerStorage,
    /// Worker 管理器
    pub worker_manager: Arc<crate::worker::WorkerManager>,
    /// 房间摘要存储
    pub room_summary_storage: crate::storage::room_summary::RoomSummaryStorage,
    /// 房间摘要服务
    pub room_summary_service: Arc<crate::services::room_summary_service::RoomSummaryService>,
    /// 消息保留策略存储
    pub retention_storage: crate::storage::retention::RetentionStorage,
    /// 消息保留策略服务
    pub retention_service: Arc<crate::services::retention_service::RetentionService>,
    /// 刷新令牌存储
    pub refresh_token_storage: crate::storage::refresh_token::RefreshTokenStorage,
    /// 刷新令牌服务
    pub refresh_token_service: Arc<crate::services::refresh_token_service::RefreshTokenService>,
    /// 注册令牌存储
    pub registration_token_storage: crate::storage::registration_token::RegistrationTokenStorage,
    /// 注册令牌服务
    pub registration_token_service: Arc<crate::services::registration_token_service::RegistrationTokenService>,
    /// 事件报告存储
    pub event_report_storage: crate::storage::event_report::EventReportStorage,
    /// 事件报告服务
    pub event_report_service: Arc<crate::services::event_report_service::EventReportService>,
    /// 背景更新存储
    pub background_update_storage: crate::storage::background_update::BackgroundUpdateStorage,
    /// 背景更新服务
    pub background_update_service: Arc<crate::services::background_update_service::BackgroundUpdateService>,
    /// 模块存储
    pub module_storage: crate::storage::module::ModuleStorage,
    /// 模块服务
    pub module_service: Arc<crate::services::module_service::ModuleService>,
    /// 账户有效性服务
    pub account_validity_service: Arc<crate::services::module_service::AccountValidityService>,
    /// SAML 存储
    pub saml_storage: crate::storage::saml::SamlStorage,
    /// SAML 服务
    pub saml_service: Arc<crate::services::saml_service::SamlService>,
    /// 验证码存储
    pub captcha_storage: crate::storage::captcha::CaptchaStorage,
    /// 验证码服务
    pub captcha_service: Arc<crate::services::captcha_service::CaptchaService>,
    /// 联邦黑名单存储
    pub federation_blacklist_storage: crate::storage::federation_blacklist::FederationBlacklistStorage,
    /// 联邦黑名单服务
    pub federation_blacklist_service: Arc<crate::services::federation_blacklist_service::FederationBlacklistService>,
    /// 推送通知存储
    pub push_notification_storage: crate::storage::push_notification::PushNotificationStorage,
    /// 推送通知服务
    pub push_notification_service: Arc<crate::services::push_notification_service::PushNotificationService>,
    /// 线程存储
    pub thread_storage: crate::storage::thread::ThreadStorage,
    /// 线程服务
    pub thread_service: Arc<crate::services::thread_service::ThreadService>,
    /// CAS 存储
    pub cas_storage: crate::storage::cas::CasStorage,
    /// CAS 服务
    pub cas_service: Arc<crate::services::cas_service::CasService>,
    /// 媒体配额存储
    pub media_quota_storage: crate::storage::media_quota::MediaQuotaStorage,
    /// 媒体配额服务
    pub media_quota_service: Arc<crate::services::media_quota_service::MediaQuotaService>,
    /// 服务器通知存储
    pub server_notification_storage: crate::storage::server_notification::ServerNotificationStorage,
    /// 服务器通知服务
    pub server_notification_service: Arc<crate::services::server_notification_service::ServerNotificationService>,
}

impl ServiceContainer {
    /// 创建新的服务容器。
    ///
    /// 初始化所有服务和存储句柄，建立与数据库和缓存的连接。
    /// 这是应用程序启动的关键步骤。
    ///
    /// # 参数
    ///
    /// * `pool` - PostgreSQL 数据库连接池
    /// * `cache` - 缓存管理器实例
    /// * `config` - 服务器配置
    /// * `task_queue` - 可选的任务队列实例
    ///
    /// # 返回值
    ///
    /// 返回完全配置的服务容器实例
    pub fn new(
        pool: &Arc<sqlx::PgPool>, 
        cache: Arc<CacheManager>, 
        config: Config,
        task_queue: Option<Arc<RedisTaskQueue>>,
    ) -> Self {
        let presence_pool = pool.clone();
        let metrics = Arc::new(MetricsCollector::new());
        let auth_service = AuthService::new(
            pool,
            cache.clone(),
            metrics.clone(),
            &config.security,
            &config.server.name,
        );
        let device_key_storage = crate::e2ee::device_keys::DeviceKeyStorage::new(pool);
        let device_keys_service = DeviceKeyService::new(device_key_storage, cache.clone());
        let megolm_storage = crate::e2ee::megolm::MegolmSessionStorage::new(pool);
        let encryption_key = generate_encryption_key();
        let megolm_service = MegolmService::new(megolm_storage, cache.clone(), encryption_key);
        let cross_signing_storage = crate::e2ee::cross_signing::CrossSigningStorage::new(pool);
        let cross_signing_service = CrossSigningService::new(cross_signing_storage);
        let key_backup_storage = crate::e2ee::backup::KeyBackupStorage::new(pool);
        let backup_service = KeyBackupService::new(key_backup_storage);
        let to_device_storage = crate::e2ee::to_device::ToDeviceStorage::new(pool);
        let user_storage = UserStorage::new(pool, cache.clone());
        let to_device_service =
            ToDeviceService::new(to_device_storage).with_user_storage(user_storage.clone());
        let presence_service = PresenceStorage::new(presence_pool.clone(), cache.clone());
        let voice_service = VoiceService::new(pool, cache.clone(), "/app/data/media/voice");
        let search_service = Arc::new(crate::services::search_service::SearchService::new(
            &config.search.elasticsearch_url,
            config.search.enabled,
        ));

        let server_name_for_storage = config.server.get_server_name().to_string();
        let member_storage = RoomMemberStorage::new(pool, &server_name_for_storage);
        let room_storage = RoomStorage::new(pool);
        let event_storage = EventStorage::new(pool);
        let presence_storage = PresenceStorage::new(presence_pool.clone(), cache.clone());

        let registration_service = Arc::new(RegistrationService::new(
            user_storage.clone(),
            auth_service.clone(),
            metrics.clone(),
            config.server.name.clone(),
            config.server.enable_registration,
            task_queue.clone(),
        ));
        let room_service = Arc::new(RoomService::new(
            room_storage.clone(),
            member_storage.clone(),
            event_storage.clone(),
            user_storage.clone(),
            auth_service.validator.clone(),
            config.server.name.clone(),
            task_queue.clone(),
        ));
        let sync_service = Arc::new(SyncService::new(
            presence_storage.clone(),
            member_storage.clone(),
            event_storage.clone(),
            room_storage.clone(),
            DeviceStorage::new(pool),
        ));
        let media_service = MediaService::new("/app/data/media", task_queue.clone());
        let admin_registration_service = AdminRegistrationService::new(
            auth_service.clone(),
            config.admin_registration.clone(),
            cache.clone(),
            metrics.clone(),
        );

        let email_verification_storage = EmailVerificationStorage::new(pool);

        let event_auth_chain = EventAuthChain::new();
        let server_name = config.server.name.clone();
        let key_rotation_manager = KeyRotationManager::new(pool, &server_name);
        let device_sync_manager = DeviceSyncManager::new(pool, Some(cache.clone()), task_queue.clone());

        let friend_storage = FriendRoomStorage::new(pool.clone());
        let friend_room_service = Arc::new(FriendRoomService::new(
            friend_storage.clone(),
            room_service.clone(),
            event_storage.clone(),
            config.server.name.clone(),
        ));
        let friend_federation = Arc::new(FriendFederation::new(friend_room_service.clone()));

        let space_storage = SpaceStorage::new(pool);
        let space_service = Arc::new(SpaceService::new(
            Arc::new(space_storage.clone()),
            Arc::new(room_storage.clone()),
            Arc::new(event_storage.clone()),
            config.server.name.clone(),
        ));

        let app_service_storage = ApplicationServiceStorage::new(pool);
        let app_service_manager = Arc::new(ApplicationServiceManager::new(
            Arc::new(app_service_storage.clone()),
            config.server.name.clone(),
        ));

        let worker_storage = crate::worker::WorkerStorage::new(pool);
        let worker_manager = Arc::new(crate::worker::WorkerManager::new(
            Arc::new(worker_storage.clone()),
            config.server.name.clone(),
        ));

        let room_summary_storage = crate::storage::room_summary::RoomSummaryStorage::new(pool);
        let room_summary_service = Arc::new(crate::services::room_summary_service::RoomSummaryService::new(
            Arc::new(room_summary_storage.clone()),
            Arc::new(event_storage.clone()),
        ));

        let retention_storage = crate::storage::retention::RetentionStorage::new(pool);
        let retention_service = Arc::new(crate::services::retention_service::RetentionService::new(
            Arc::new(retention_storage.clone()),
            pool.clone(),
        ));

        let refresh_token_storage = crate::storage::refresh_token::RefreshTokenStorage::new(pool);
        let refresh_token_service = Arc::new(crate::services::refresh_token_service::RefreshTokenService::new(
            Arc::new(refresh_token_storage.clone()),
            604800000,
        ));

        let registration_token_storage = crate::storage::registration_token::RegistrationTokenStorage::new(pool);
        let registration_token_service = Arc::new(crate::services::registration_token_service::RegistrationTokenService::new(
            Arc::new(registration_token_storage.clone()),
        ));

        let event_report_storage = crate::storage::event_report::EventReportStorage::new(pool);
        let event_report_service = Arc::new(crate::services::event_report_service::EventReportService::new(
            Arc::new(event_report_storage.clone()),
        ));

        let background_update_storage = crate::storage::background_update::BackgroundUpdateStorage::new(pool);
        let background_update_service = Arc::new(crate::services::background_update_service::BackgroundUpdateService::new(
            Arc::new(background_update_storage.clone()),
        ));

        let module_storage = crate::storage::module::ModuleStorage::new(pool);
        let module_service = Arc::new(crate::services::module_service::ModuleService::new(
            Arc::new(module_storage.clone()),
        ));
        let account_validity_service = Arc::new(crate::services::module_service::AccountValidityService::new(
            Arc::new(module_storage.clone()),
        ));

        let saml_storage = crate::storage::saml::SamlStorage::new(pool);
        let saml_service = Arc::new(crate::services::saml_service::SamlService::new(
            Arc::new(config.saml.clone()),
            Arc::new(saml_storage.clone()),
            config.server.name.clone(),
        ));

        let captcha_storage = crate::storage::captcha::CaptchaStorage::new(pool);
        let captcha_service = Arc::new(crate::services::captcha_service::CaptchaService::new(
            Arc::new(captcha_storage.clone()),
        ));

        let federation_blacklist_storage = crate::storage::federation_blacklist::FederationBlacklistStorage::new(pool);
        let federation_blacklist_service = Arc::new(crate::services::federation_blacklist_service::FederationBlacklistService::new(
            Arc::new(federation_blacklist_storage.clone()),
        ));

        let push_notification_storage = crate::storage::push_notification::PushNotificationStorage::new(pool);
        let push_notification_service = Arc::new(crate::services::push_notification_service::PushNotificationService::new(
            Arc::new(push_notification_storage.clone()),
        ));

        let thread_storage = crate::storage::thread::ThreadStorage::new(pool);
        let thread_service = Arc::new(crate::services::thread_service::ThreadService::new(
            Arc::new(thread_storage.clone()),
        ));

        let cas_storage = crate::storage::cas::CasStorage::new(pool);
        let cas_service = Arc::new(crate::services::cas_service::CasService::new(
            Arc::new(cas_storage.clone()),
            config.server.name.clone(),
        ));

        let media_quota_storage = crate::storage::media_quota::MediaQuotaStorage::new(pool);
        let media_quota_service = Arc::new(crate::services::media_quota_service::MediaQuotaService::new(
            Arc::new(media_quota_storage.clone()),
        ));

        let server_notification_storage = crate::storage::server_notification::ServerNotificationStorage::new(pool);
        let server_notification_service = Arc::new(crate::services::server_notification_service::ServerNotificationService::new(
            Arc::new(server_notification_storage.clone()),
        ));

        Self {
            user_storage,
            device_storage: DeviceStorage::new(pool),
            token_storage: AccessTokenStorage::new(pool),
            room_storage,
            member_storage,
            event_storage,
            presence_storage,
            presence_service,
            auth_service,
            device_keys_service,
            megolm_service,
            cross_signing_service,
            backup_service,
            to_device_service,
            voice_service,
            registration_service,
            room_service,
            sync_service,
            search_service,
            media_service,
            cache,
            task_queue,
            metrics,
            server_name: config.server.name.clone(),
            config,
            admin_registration_service,
            email_verification_storage,
            event_auth_chain,
            key_rotation_manager,
            device_sync_manager,
            friend_storage,
            friend_room_service,
            friend_federation,
            space_storage,
            space_service,
            app_service_storage,
            app_service_manager,
            worker_storage,
            worker_manager,
            room_summary_storage,
            room_summary_service,
            retention_storage,
            retention_service,
            refresh_token_storage,
            refresh_token_service,
            registration_token_storage,
            registration_token_service,
            event_report_storage,
            event_report_service,
            background_update_storage,
            background_update_service,
            module_storage,
            module_service,
            account_validity_service,
            saml_storage,
            saml_service,
            captcha_storage,
            captcha_service,
            federation_blacklist_storage,
            federation_blacklist_service,
            push_notification_storage,
            push_notification_service,
            thread_storage,
            thread_service,
            cas_storage,
            cas_service,
            media_quota_storage,
            media_quota_service,
            server_notification_storage,
            server_notification_service,
        }
    }

    // #[cfg(test)] - Removed to make it available for integration tests
    pub fn new_test() -> Self {
        let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://synapse:synapse@localhost:5432/synapse_test".to_string()
        });
        let host = std::env::var("DATABASE_HOST").unwrap_or_else(|_| "localhost".to_string());
        let port: u16 = std::env::var("DATABASE_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(5432);
        let user = std::env::var("DATABASE_USER").unwrap_or_else(|_| "synapse".to_string());
        let pass = std::env::var("DATABASE_PASSWORD").unwrap_or_else(|_| "synapse".to_string());
        let name = std::env::var("DATABASE_NAME").unwrap_or_else(|_| "synapse".to_string());
        let pool = Arc::new(
            sqlx::postgres::PgPoolOptions::new()
                .max_connections(50)
                .min_connections(5)
                .acquire_timeout(std::time::Duration::from_secs(2))
                .connect_lazy(&db_url)
                .unwrap(),
        );
        let cache = Arc::new(CacheManager::new(CacheConfig::default()));
        let config = Config {
            server: ServerConfig {
                name: "localhost".to_string(),
                host: "0.0.0.0".to_string(),
                port: 8008,
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
                host,
                port,
                username: user,
                password: pass,
                name,
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
                enabled: false,
            },
            logging: crate::common::config::LoggingConfig {
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
                signing_key: None,
                key_id: None,
            },
            security: SecurityConfig {
                secret: "test_secret".to_string(),
                expiry_time: 3600,
                refresh_token_expiry: 604800,
                argon2_m_cost: 2048,
                argon2_t_cost: 1,
                argon2_p_cost: 1,
            },
            search: SearchConfig {
                elasticsearch_url: "http://localhost:9200".to_string(),
                enabled: false,
            },
            rate_limit: RateLimitConfig::default(),
            admin_registration: AdminRegistrationConfig {
                enabled: true,
                shared_secret: "test_shared_secret".to_string(),
                nonce_timeout_seconds: 60,
            },
            worker: WorkerConfig::default(),
            cors: CorsConfig::default(),
            smtp: SmtpConfig::default(),
            voip: crate::common::config::VoipConfig::default(),
            push: crate::common::config::PushConfig::default(),
            url_preview: crate::common::config::UrlPreviewConfig::default(),
            oidc: crate::common::config::OidcConfig::default(),
            saml: crate::common::config::SamlConfig::default(),
            telemetry: crate::common::telemetry_config::OpenTelemetryConfig::default(),
            jaeger: crate::common::telemetry_config::JaegerConfig::default(),
            prometheus: crate::common::telemetry_config::PrometheusConfig::default(),
        };
        Self::new(&pool, cache, config, None)
    }
}

fn generate_encryption_key() -> [u8; 32] {
    let mut key = [0u8; 32];
    for byte in key.iter_mut() {
        *byte = rand::random();
    }
    key
}

#[derive(Clone)]
pub struct PresenceStorage {
    pool: Arc<Pool<Postgres>>,
    _cache: Arc<CacheManager>,
}

impl PresenceStorage {
    pub fn new(pool: Arc<Pool<Postgres>>, cache: Arc<CacheManager>) -> Self {
        Self {
            pool,
            _cache: cache,
        }
    }

    pub async fn set_presence(
        &self,
        user_id: &str,
        presence: &str,
        status_msg: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            r#"
            INSERT INTO presence (user_id, presence, status_msg, last_active_ts, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $4, $4)
            ON CONFLICT (user_id) DO UPDATE SET
                presence = EXCLUDED.presence,
                status_msg = EXCLUDED.status_msg,
                last_active_ts = EXCLUDED.last_active_ts,
                updated_ts = EXCLUDED.updated_ts
            "#,
        )
        .bind(user_id)
        .bind(presence)
        .bind(status_msg)
        .bind(now)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_presence(
        &self,
        user_id: &str,
    ) -> Result<Option<(String, Option<String>)>, sqlx::Error> {
        let result = sqlx::query_as::<_, (Option<String>, Option<String>)>(
            r#"
            SELECT presence, status_msg FROM presence WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await?;
        Ok(result.map(|r| (r.0.unwrap_or_default(), r.1)))
    }

    pub async fn set_typing(
        &self,
        room_id: &str,
        user_id: &str,
        typing: bool,
    ) -> Result<(), sqlx::Error> {
        if typing {
            let now = chrono::Utc::now().timestamp_millis();
            sqlx::query(
                r#"
                INSERT INTO typing (room_id, user_id, typing, last_active_ts)
                VALUES ($1, $2, $3, $4)
                ON CONFLICT (room_id, user_id)
                DO UPDATE SET typing = EXCLUDED.typing, last_active_ts = EXCLUDED.last_active_ts
                "#,
            )
            .bind(room_id)
            .bind(user_id)
            .bind(typing)
            .bind(now)
            .execute(&*self.pool)
            .await?;
        } else {
            sqlx::query(
                r#"
                DELETE FROM typing WHERE room_id = $1 AND user_id = $2
                "#,
            )
            .bind(room_id)
            .bind(user_id)
            .execute(&*self.pool)
            .await?;
        }
        Ok(())
    }
}

pub mod admin_registration_service;
pub mod application_service;
pub mod background_update_service;
pub mod cas_service;
pub mod captcha_service;
pub mod database_initializer;
pub mod event_report_service;
pub mod federation_blacklist_service;
pub mod friend_room_service;
pub mod media_quota_service;
pub mod media_service;
pub mod moderation_service;
pub mod module_service;
pub mod oidc_service;
pub mod push_notification_service;
pub mod push_service;
pub mod read_receipt_service;
pub mod refresh_token_service;
pub mod registration_service;
pub mod registration_token_service;
pub mod retention_service;
pub mod room_service;
pub mod room_summary_service;
pub mod saml_service;
pub mod search_service;
pub mod server_notification_service;
pub mod space_service;
pub mod sync_service;
pub mod telemetry_service;
pub mod thread_service;
pub mod url_preview_service;
pub mod voice_service;
pub mod voip_service;

pub use admin_registration_service::*;
pub use application_service::*;
pub use database_initializer::*;
pub use friend_room_service::*;
pub use media_service::*;
pub use moderation_service::*;
pub use oidc_service::*;
pub use push_service::*;
pub use read_receipt_service::*;
pub use registration_service::*;
pub use room_service::*;
pub use search_service::*;
pub use space_service::*;
pub use sync_service::*;
pub use url_preview_service::*;
pub use voice_service::*;
pub use voip_service::*;
