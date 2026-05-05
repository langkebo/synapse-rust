use crate::cache::CacheManager;
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PresenceSnapshot {
    pub user_id: String,
    pub presence: String,
    pub status_msg: Option<String>,
    pub last_active_ts: Option<i64>,
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
        Ok(result.map(|row| (row.0.unwrap_or_default(), row.1)))
    }

    /// 与 [`Self::get_presence`] 类似，但额外返回 `last_active_ts`，以便 handler
    /// 计算 Matrix 规范要求的 `last_active_ago`/`currently_active` 字段。
    pub async fn get_presence_with_meta(
        &self,
        user_id: &str,
    ) -> Result<Option<(String, Option<String>, Option<i64>)>, sqlx::Error> {
        let result = sqlx::query_as::<_, (Option<String>, Option<String>, Option<i64>)>(
            r#"
            SELECT presence, status_msg, last_active_ts FROM presence WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await?;
        Ok(result.map(|row| (row.0.unwrap_or_default(), row.1, row.2)))
    }

    pub async fn get_presences(
        &self,
        user_ids: &[String],
    ) -> Result<HashMap<String, (String, Option<String>)>, sqlx::Error> {
        if user_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let rows = sqlx::query_as::<_, (String, Option<String>, Option<String>)>(
            r#"
            SELECT user_id, presence, status_msg FROM presence WHERE user_id = ANY($1)
            "#,
        )
        .bind(user_ids)
        .fetch_all(&*self.pool)
        .await?;

        let mut map = HashMap::new();
        for row in rows {
            map.insert(row.0, (row.1.unwrap_or_default(), row.2));
        }
        Ok(map)
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
        let result = sqlx::query(
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
        .await;

        match result {
            Ok(_) => Ok(()),
            Err(e) => {
                let err_msg = e.to_string();
                if err_msg.contains("column \"subscriber_id\" does not exist") {
                    return sqlx::query(
                        r#"
                        INSERT INTO presence_subscriptions (user_id, friend_id, created_ts)
                        VALUES ($1, $2, $3)
                        ON CONFLICT (user_id, friend_id) DO NOTHING
                        "#,
                    )
                    .bind(subscriber_id)
                    .bind(target_id)
                    .bind(now)
                    .execute(&*self.pool)
                    .await
                    .map(|_| ());
                }
                Err(e)
            }
        }
    }

    pub async fn remove_subscription(
        &self,
        subscriber_id: &str,
        target_id: &str,
    ) -> Result<(), sqlx::Error> {
        let result = sqlx::query(
            r#"
            DELETE FROM presence_subscriptions
            WHERE subscriber_id = $1 AND target_id = $2
            "#,
        )
        .bind(subscriber_id)
        .bind(target_id)
        .execute(&*self.pool)
        .await;

        match result {
            Ok(_) => Ok(()),
            Err(e) => {
                let err_msg = e.to_string();
                if err_msg.contains("column \"subscriber_id\" does not exist") {
                    return sqlx::query(
                        r#"
                        DELETE FROM presence_subscriptions
                        WHERE user_id = $1 AND friend_id = $2
                        "#,
                    )
                    .bind(subscriber_id)
                    .bind(target_id)
                    .execute(&*self.pool)
                    .await
                    .map(|_| ());
                }
                Err(e)
            }
        }
    }

    pub async fn get_subscriptions(&self, subscriber_id: &str) -> Result<Vec<String>, sqlx::Error> {
        let result = sqlx::query_as::<_, (String,)>(
            r#"
            SELECT target_id FROM presence_subscriptions
            WHERE subscriber_id = $1
            "#,
        )
        .bind(subscriber_id)
        .fetch_all(&*self.pool)
        .await;

        match result {
            Ok(rows) => Ok(rows.into_iter().map(|row| row.0).collect()),
            Err(e) => {
                let err_msg = e.to_string();
                if err_msg.contains("column \"subscriber_id\" does not exist") {
                    let fallback_result = sqlx::query_as::<_, (String,)>(
                        r#"
                        SELECT friend_id FROM presence_subscriptions
                        WHERE user_id = $1
                        "#,
                    )
                    .bind(subscriber_id)
                    .fetch_all(&*self.pool)
                    .await;
                    return match fallback_result {
                        Ok(rows) => Ok(rows.into_iter().map(|row| row.0).collect()),
                        Err(e2) => Err(e2),
                    };
                }
                Err(e)
            }
        }
    }

    pub async fn get_subscribers(&self, target_id: &str) -> Result<Vec<String>, sqlx::Error> {
        let result = sqlx::query_as::<_, (String,)>(
            r#"
            SELECT subscriber_id FROM presence_subscriptions
            WHERE target_id = $1
            "#,
        )
        .bind(target_id)
        .fetch_all(&*self.pool)
        .await;

        match result {
            Ok(rows) => Ok(rows.into_iter().map(|row| row.0).collect()),
            Err(e) => {
                let err_msg = e.to_string();
                if err_msg.contains("column \"subscriber_id\" does not exist") {
                    let fallback_result = sqlx::query_as::<_, (String,)>(
                        r#"
                        SELECT user_id FROM presence_subscriptions
                        WHERE friend_id = $1
                        "#,
                    )
                    .bind(target_id)
                    .fetch_all(&*self.pool)
                    .await;
                    return match fallback_result {
                        Ok(rows) => Ok(rows.into_iter().map(|row| row.0).collect()),
                        Err(e2) => Err(e2),
                    };
                }
                Err(e)
            }
        }
    }

    pub async fn get_presence_batch(
        &self,
        user_ids: &[String],
    ) -> Result<Vec<(String, String, Option<String>)>, sqlx::Error> {
        if user_ids.is_empty() {
            return Ok(Vec::new());
        }

        let result = sqlx::query_as::<_, (String, String, Option<String>)>(
            r#"
            SELECT user_id, presence, status_msg
            FROM presence
            WHERE user_id = ANY($1)
            "#,
        )
        .bind(user_ids)
        .fetch_all(&*self.pool)
        .await;

        match result {
            Ok(rows) => Ok(rows),
            Err(e) => Err(e),
        }
    }

    /// 与 [`Self::get_presence_batch`] 类似，但携带 `last_active_ts`，
    /// 让调用方自行换算 `last_active_ago` / `currently_active`。
    pub async fn get_presence_batch_with_meta(
        &self,
        user_ids: &[String],
    ) -> Result<Vec<(String, String, Option<String>, Option<i64>)>, sqlx::Error> {
        if user_ids.is_empty() {
            return Ok(Vec::new());
        }

        sqlx::query_as::<_, (String, String, Option<String>, Option<i64>)>(
            r#"
            SELECT user_id, presence, status_msg, last_active_ts
            FROM presence
            WHERE user_id = ANY($1)
            "#,
        )
        .bind(user_ids)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn get_presence_snapshots(
        &self,
        user_ids: &[String],
    ) -> Result<HashMap<String, PresenceSnapshot>, sqlx::Error> {
        if user_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let rows = sqlx::query_as::<_, PresenceSnapshot>(
            r#"
            SELECT user_id,
                   COALESCE(presence, 'offline') as presence,
                   status_msg,
                   last_active_ts
            FROM presence
            WHERE user_id = ANY($1)
            "#,
        )
        .bind(user_ids)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|snapshot| (snapshot.user_id.clone(), snapshot))
            .collect())
    }
}
