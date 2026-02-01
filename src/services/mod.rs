use crate::auth::*;
use crate::cache::*;
use crate::common::*;
use crate::e2ee::backup::KeyBackupService;
use crate::e2ee::cross_signing::CrossSigningService;
use crate::e2ee::device_keys::DeviceKeyService;
use crate::e2ee::megolm::MegolmService;
use crate::e2ee::to_device::ToDeviceService;
use crate::storage::*;
use sqlx::{Pool, Postgres};
use std::sync::Arc;

#[derive(Clone)]
pub struct ServiceContainer {
    pub user_storage: UserStorage,
    pub device_storage: DeviceStorage,
    pub token_storage: AccessTokenStorage,
    pub room_storage: RoomStorage,
    pub member_storage: RoomMemberStorage,
    pub event_storage: EventStorage,
    pub presence_storage: PresenceStorage,
    pub presence_service: PresenceStorage,
    pub auth_service: AuthService,
    pub device_keys_service: DeviceKeyService,
    pub megolm_service: MegolmService,
    pub cross_signing_service: CrossSigningService,
    pub backup_service: KeyBackupService,
    pub to_device_service: ToDeviceService,
    pub voice_service: VoiceService,
    pub private_chat_service: Arc<PrivateChatService>,
    pub registration_service: Arc<RegistrationService>,
    pub room_service: Arc<RoomService>,
    pub sync_service: Arc<SyncService>,
    pub private_chat_storage: PrivateChatStorage,
    pub search_service: Arc<crate::services::search_service::SearchService>,
    pub media_service: MediaService,
    pub cache: Arc<CacheManager>,
    pub server_name: String,
    pub config: Config,
}

impl ServiceContainer {
    pub fn new(pool: &Arc<sqlx::PgPool>, cache: Arc<CacheManager>, config: Config) -> Self {
        let presence_pool = pool.clone();
        let auth_service = AuthService::new(
            pool,
            cache.clone(),
            &config.security.secret,
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
        let to_device_service = ToDeviceService::new(to_device_storage);
        let presence_service = PresenceStorage::new(presence_pool.clone(), cache.clone());
        let voice_service = VoiceService::new(pool, cache.clone(), "/tmp/synapse_voice");
        let search_service = Arc::new(crate::services::search_service::SearchService::new(
            &config.search.elasticsearch_url,
            config.search.enabled,
        ));

        let user_storage = UserStorage::new(pool);
        let member_storage = RoomMemberStorage::new(pool);
        let room_storage = RoomStorage::new(pool);
        let event_storage = EventStorage::new(pool);
        let presence_storage = PresenceStorage::new(presence_pool.clone(), cache.clone());
        let private_chat_storage = PrivateChatStorage::new(pool);

        let private_chat_service = Arc::new(PrivateChatService::new(
            pool,
            search_service.clone(),
            config.server.name.clone(),
        ));
        let registration_service = Arc::new(RegistrationService::new(
            user_storage.clone(),
            auth_service.clone(),
            config.server.name.clone(),
            config.server.enable_registration,
        ));
        let room_service = Arc::new(RoomService::new(
            room_storage.clone(),
            member_storage.clone(),
            event_storage.clone(),
            config.server.name.clone(),
        ));
        let sync_service = Arc::new(SyncService::new(
            presence_storage.clone(),
            member_storage.clone(),
            event_storage.clone(),
            room_storage.clone(),
        ));
        let media_service = MediaService::new("media");

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
            private_chat_service,
            registration_service,
            room_service,
            sync_service,
            private_chat_storage,
            search_service,
            media_service,
            cache,
            server_name: config.server.name.clone(),
            config,
        }
    }

    #[cfg(test)]
    pub fn new_test() -> Self {
        let pool = Arc::new(
            sqlx::PgPool::connect_lazy("postgres://synapse:synapse@localhost:5432/synapse")
                .unwrap(),
        );
        let cache = Arc::new(CacheManager::new(CacheConfig::default()));
        let config = Config {
            server: ServerConfig {
                name: "localhost".to_string(),
                host: "0.0.0.0".to_string(),
                port: 8008,
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
                host: "localhost".to_string(),
                port: 5432,
                username: "synapse".to_string(),
                password: "synapse".to_string(),
                name: "synapse".to_string(),
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
                enabled: true,
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
                bcrypt_rounds: 12,
            },
            search: SearchConfig {
                elasticsearch_url: "http://localhost:9200".to_string(),
                enabled: false,
            },
            rate_limit: RateLimitConfig::default(),
        };
        Self::new(&pool, cache, config)
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
        sqlx::query!(
            r#"
            INSERT INTO presence (user_id, presence, status_msg, last_active_ts, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $4, $4)
            ON CONFLICT (user_id) DO UPDATE SET
                presence = EXCLUDED.presence,
                status_msg = EXCLUDED.status_msg,
                last_active_ts = EXCLUDED.last_active_ts,
                updated_ts = EXCLUDED.updated_ts
            "#,
            user_id,
            presence,
            status_msg,
            now
        ).execute(&*self.pool).await?;
        Ok(())
    }

    pub async fn get_presence(
        &self,
        user_id: &str,
    ) -> Result<Option<(String, Option<String>)>, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            SELECT presence, status_msg FROM presence WHERE user_id = $1
            "#,
            user_id
        )
        .fetch_optional(&*self.pool)
        .await?;
        Ok(result.map(|r| (r.presence.unwrap_or_default(), r.status_msg)))
    }

    pub async fn set_typing(
        &self,
        room_id: &str,
        user_id: &str,
        typing: bool,
    ) -> Result<(), sqlx::Error> {
        if typing {
            let now = chrono::Utc::now().timestamp_millis();
            sqlx::query!(
                r#"
                INSERT INTO typing (room_id, user_id, typing, last_active_ts)
                VALUES ($1, $2, $3, $4)
                ON CONFLICT (room_id, user_id)
                DO UPDATE SET typing = EXCLUDED.typing, last_active_ts = EXCLUDED.last_active_ts
                "#,
                room_id,
                user_id,
                typing,
                now,
            )
            .execute(&*self.pool)
            .await?;
        } else {
            sqlx::query!(
                r#"
                DELETE FROM typing WHERE room_id = $1 AND user_id = $2
                "#,
                room_id,
                user_id
            )
            .execute(&*self.pool)
            .await?;
        }
        Ok(())
    }
}

pub mod database_initializer;
pub mod friend_service;
pub mod media_service;
pub mod private_chat_service;
pub mod registration_service;
pub mod room_service;
pub mod search_service;
pub mod sync_service;
pub mod voice_service;

pub use database_initializer::*;
pub use friend_service::*;
pub use media_service::*;
pub use private_chat_service::*;
pub use registration_service::*;
pub use room_service::*;
pub use sync_service::*;
pub use voice_service::*;
