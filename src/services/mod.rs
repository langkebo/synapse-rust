use crate::auth::*;
use crate::cache::*;
use crate::common::*;
use crate::e2ee::backup::KeyBackupService;
use crate::e2ee::cross_signing::CrossSigningService;
use crate::e2ee::device_keys::DeviceKeyService;
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
    pub cross_signing_service: CrossSigningService,
    pub backup_service: KeyBackupService,
    pub voice_service: VoiceService,
    pub cache: Arc<CacheManager>,
    pub server_name: String,
}

impl ServiceContainer {
    pub fn new(
        pool: &Arc<sqlx::PgPool>,
        cache: Arc<CacheManager>,
        jwt_secret: &str,
        server_name: &str,
    ) -> Self {
        let presence_pool = pool.clone();
        let auth_service = AuthService::new(pool, cache.clone(), jwt_secret, server_name);
        let device_key_storage = crate::e2ee::device_keys::DeviceKeyStorage::new(pool);
        let device_keys_service = DeviceKeyService::new(device_key_storage, cache.clone());
        let cross_signing_storage = crate::e2ee::cross_signing::CrossSigningStorage::new(pool);
        let cross_signing_service = CrossSigningService::new(cross_signing_storage);
        let key_backup_storage = crate::e2ee::backup::KeyBackupStorage::new(pool);
        let backup_service = KeyBackupService::new(key_backup_storage);
        let presence_service = PresenceStorage::new(presence_pool.clone(), cache.clone());
        let voice_service = VoiceService::new("/tmp/synapse_voice");

        Self {
            user_storage: UserStorage::new(pool),
            device_storage: DeviceStorage::new(pool),
            token_storage: AccessTokenStorage::new(pool),
            room_storage: RoomStorage::new(pool),
            member_storage: RoomMemberStorage::new(pool),
            event_storage: EventStorage::new(pool),
            presence_storage: PresenceStorage::new(presence_pool, cache.clone()),
            presence_service,
            auth_service,
            device_keys_service,
            cross_signing_service,
            backup_service,
            voice_service,
            cache,
            server_name: server_name.to_string(),
        }
    }

    #[cfg(test)]
    pub fn new_test() -> Self {
        let pool = Arc::new(
            sqlx::PgPool::connect_lazy("postgres://synapse:synapse@localhost:5432/synapse")
                .unwrap(),
        );
        let cache = Arc::new(CacheManager::new(CacheConfig::default()));
        Self::new(&pool, cache, "test_secret", "localhost")
    }
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
        Ok(result.map(|r| (r.presence, r.status_msg)))
    }

    pub async fn set_typing(
        &self,
        room_id: &str,
        user_id: &str,
        typing: bool,
    ) -> Result<(), sqlx::Error> {
        if typing {
            sqlx::query!(
                r#"
                INSERT INTO typing (room_id, user_id, typing, last_active_ts)
                VALUES ($1, $2, $3, NOW() AT TIME ZONE 'UTC')
                ON CONFLICT (room_id, user_id)
                DO UPDATE SET typing = EXCLUDED.typing, last_active_ts = EXCLUDED.last_active_ts
                "#,
                room_id,
                user_id,
                typing,
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

pub mod friend_service;
pub mod media_service;
pub mod private_chat_service;
pub mod registration_service;
pub mod room_service;
pub mod sync_service;
pub mod voice_service;

pub use friend_service::*;
pub use media_service::*;
pub use private_chat_service::*;
pub use registration_service::*;
pub use room_service::*;
pub use sync_service::*;
pub use voice_service::*;
