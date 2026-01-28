use sqlx::{Pool, Postgres};
use crate::common::*;
use crate::storage::*;
use crate::cache::*;
use crate::auth::*;
use std::sync::Arc;

#[derive(Clone)]
pub struct ServiceContainer {
    pub user_storage: UserStorage<'static>,
    pub device_storage: DeviceStorage<'static>,
    pub token_storage: AccessTokenStorage<'static>,
    pub room_storage: RoomStorage<'static>,
    pub member_storage: RoomMemberStorage<'static>,
    pub event_storage: EventStorage<'static>,
    pub presence_storage: PresenceStorage,
    pub auth_service: AuthService,
    pub cache: Arc<CacheManager>,
    pub server_name: String,
}

impl ServiceContainer {
    pub fn new(
        pool: &'static sqlx::PgPool,
        cache: Arc<CacheManager>,
        jwt_secret: &str,
        server_name: &str,
    ) -> Self {
        let presence_storage = PresenceStorage::new(pool.clone(), cache.clone());
        let auth_service = AuthService::new(pool, cache.clone(), jwt_secret, server_name);

        Self {
            user_storage: UserStorage::new(pool),
            device_storage: DeviceStorage::new(pool),
            token_storage: AccessTokenStorage::new(pool),
            room_storage: RoomStorage::new(pool),
            member_storage: RoomMemberStorage::new(pool),
            event_storage: EventStorage::new(pool),
            presence_storage,
            auth_service,
            cache,
            server_name: server_name.to_string(),
        }
    }
}

pub struct PresenceStorage {
    pool: sqlx::Pool<Postgres>,
    cache: Arc<CacheManager>,
}

impl PresenceStorage {
    pub fn new(pool: sqlx::Pool<Postgres>, cache: Arc<CacheManager>) -> Self {
        Self { pool, cache }
    }

    pub async fn set_presence(&self, user_id: &str, presence: &str, status_msg: Option<&str>) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp();
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
        ).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn get_presence(&self, user_id: &str) -> Result<Option<(String, Option<String>)>, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            SELECT presence, status_msg FROM presence WHERE user_id = $1
            "#,
            user_id
        ).fetch_optional(&self.pool).await?;
        Ok(result.map(|r| (r.presence, r.status_msg)))
    }
}

pub mod registration_service;
pub mod room_service;
pub mod sync_service;
pub mod media_service;

pub use registration_service::*;
pub use room_service::*;
pub use sync_service::*;
pub use media_service::*;
