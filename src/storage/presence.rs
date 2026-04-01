use crate::cache::CacheManager;
use sqlx::{Pool, Postgres};
use std::sync::Arc;

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
        Ok(result.map(|row| (row.0.unwrap_or_default(), row.1)))
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
                INSERT INTO typing (user_id, room_id, typing, last_active_ts)
                VALUES ($1, $2, $3, $4)
                ON CONFLICT (user_id, room_id)
                DO UPDATE SET typing = EXCLUDED.typing, last_active_ts = EXCLUDED.last_active_ts
                "#,
            )
            .bind(user_id)
            .bind(room_id)
            .bind(typing)
            .bind(now)
            .execute(&*self.pool)
            .await?;
        } else {
            sqlx::query(
                r#"
                DELETE FROM typing WHERE user_id = $1 AND room_id = $2
                "#,
            )
            .bind(user_id)
            .bind(room_id)
            .execute(&*self.pool)
            .await?;
        }
        Ok(())
    }

    pub async fn add_subscription(
        &self,
        subscriber_id: &str,
        target_id: &str,
    ) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            r#"
            INSERT INTO presence_subscriptions (subscriber_id, target_id, created_ts)
            VALUES ($1, $2, $3)
            ON CONFLICT (subscriber_id, target_id) DO NOTHING
            "#,
        )
        .bind(subscriber_id)
        .bind(target_id)
        .bind(now)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn remove_subscription(
        &self,
        subscriber_id: &str,
        target_id: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            DELETE FROM presence_subscriptions
            WHERE subscriber_id = $1 AND target_id = $2
            "#,
        )
        .bind(subscriber_id)
        .bind(target_id)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_subscriptions(&self, subscriber_id: &str) -> Result<Vec<String>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (String,)>(
            r#"
            SELECT target_id FROM presence_subscriptions
            WHERE subscriber_id = $1
            "#,
        )
        .bind(subscriber_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().map(|row| row.0).collect())
    }

    pub async fn get_subscribers(&self, target_id: &str) -> Result<Vec<String>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (String,)>(
            r#"
            SELECT subscriber_id FROM presence_subscriptions
            WHERE target_id = $1
            "#,
        )
        .bind(target_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().map(|row| row.0).collect())
    }

    pub async fn get_presence_batch(
        &self,
        user_ids: &[String],
    ) -> Result<Vec<(String, String, Option<String>)>, sqlx::Error> {
        if user_ids.is_empty() {
            return Ok(Vec::new());
        }

        let rows = sqlx::query_as::<_, (String, String, Option<String>)>(
            r#"
            SELECT user_id, presence, status_msg
            FROM presence
            WHERE user_id = ANY($1)
            "#,
        )
        .bind(user_ids)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }
}
