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
use crate::federation::{DeviceSyncManager, EventAuthChain, KeyRotationManager};
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
    /// 私聊服务
    pub private_chat_service: Arc<PrivateChatService>,
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
        
        let private_chat_storage = PrivateChatStorage::new(pool.clone());
        let private_chat_service = Arc::new(PrivateChatService::new(private_chat_storage, user_storage.clone()));

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
            private_chat_service,
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
pub mod database_initializer;
pub mod friend_service;
pub mod media_service;
pub mod moderation_service;
pub mod private_chat_service;
pub mod registration_service;
pub mod room_service;
pub mod search_service;
pub mod sync_service;
pub mod voice_service;

pub use admin_registration_service::*;
pub use database_initializer::*;
pub use friend_service::*;
pub use media_service::*;
pub use moderation_service::*;
pub use private_chat_service::*;
pub use registration_service::*;
pub use room_service::*;
pub use sync_service::*;
pub use voice_service::*;
