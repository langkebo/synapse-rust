use crate::cache::{CacheKeyBuilder, CacheManager, CacheTtl};
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};
use std::collections::HashMap;
use std::sync::Arc;
use tracing;

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
    cache: Arc<CacheManager>,
}

impl PresenceStorage {
    pub fn new(pool: Arc<Pool<Postgres>>, cache: Arc<CacheManager>) -> Self {
        Self {
            pool,
            cache,
        }
    }

    pub async fn set_presence(
        &self,
        user_id: &str,
        presence: &str,
        status_msg: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        tracing::debug!(user_id = %user_id, presence = %presence, "Setting presence");
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            r"
            INSERT INTO presence (user_id, presence, status_msg, last_active_ts, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $4, $4)
            ON CONFLICT (user_id) DO UPDATE SET
                presence = EXCLUDED.presence,
                status_msg = EXCLUDED.status_msg,
                last_active_ts = EXCLUDED.last_active_ts,
                updated_ts = EXCLUDED.updated_ts
            ",
        )
        .bind(user_id)
        .bind(presence)
        .bind(status_msg)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        let snapshot = PresenceSnapshot {
            user_id: user_id.to_string(),
            presence: presence.to_string(),
            status_msg: status_msg.map(|s| s.to_string()),
            last_active_ts: Some(now),
        };
        let key = CacheKeyBuilder::user_presence(user_id);
        let ttl = CacheTtl::user_presence().as_secs();
        if let Err(e) = self.cache.set(&key, &snapshot, ttl).await {
            tracing::warn!(target: "cache", "Failed to cache presence for {}: {}", user_id, e);
        }

        Ok(())
    }

    pub async fn get_presence(
        &self,
        user_id: &str,
    ) -> Result<Option<(String, Option<String>)>, sqlx::Error> {
        tracing::debug!(user_id = %user_id, "Querying presence");
        let key = CacheKeyBuilder::user_presence(user_id);
        if let Ok(Some(snapshot)) = self.cache.get::<PresenceSnapshot>(&key).await {
            return Ok(Some((snapshot.presence, snapshot.status_msg)));
        }

        let result = sqlx::query_as::<_, (String, Option<String>, Option<i64>)>(
            r"
            SELECT presence, status_msg, last_active_ts FROM presence WHERE user_id = $1
            ",
        )
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await?;

        if let Some((presence, status_msg, last_active_ts)) = &result {
            let snapshot = PresenceSnapshot {
                user_id: user_id.to_string(),
                presence: presence.clone(),
                status_msg: status_msg.clone(),
                last_active_ts: *last_active_ts,
            };
            let ttl = CacheTtl::user_presence().as_secs();
            if let Err(e) = self.cache.set(&key, &snapshot, ttl).await {
                tracing::warn!(target: "cache", "Failed to cache presence for {}: {}", user_id, e);
            }
        }

        Ok(result.map(|(presence, status_msg, _)| (presence, status_msg)))
    }

    pub async fn get_presence_with_meta(
        &self,
        user_id: &str,
    ) -> Result<Option<(String, Option<String>, Option<i64>)>, sqlx::Error> {
        let key = CacheKeyBuilder::user_presence(user_id);
        if let Ok(Some(snapshot)) = self.cache.get::<PresenceSnapshot>(&key).await {
            return Ok(Some((snapshot.presence, snapshot.status_msg, snapshot.last_active_ts)));
        }

        let result = sqlx::query_as::<_, (String, Option<String>, Option<i64>)>(
            r"
            SELECT presence, status_msg, last_active_ts FROM presence WHERE user_id = $1
            ",
        )
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await?;

        if let Some((presence, status_msg, last_active_ts)) = &result {
            let snapshot = PresenceSnapshot {
                user_id: user_id.to_string(),
                presence: presence.clone(),
                status_msg: status_msg.clone(),
                last_active_ts: *last_active_ts,
            };
            let ttl = CacheTtl::user_presence().as_secs();
            if let Err(e) = self.cache.set(&key, &snapshot, ttl).await {
                tracing::warn!(target: "cache", "Failed to cache presence for {}: {}", user_id, e);
            }
        }

        Ok(result)
    }

    pub async fn get_presences(
        &self,
        user_ids: &[String],
    ) -> Result<HashMap<String, (String, Option<String>)>, sqlx::Error> {
        if user_ids.is_empty() {
            return Ok(HashMap::new());
        }

        tracing::info!(count = user_ids.len(), "Bulk syncing presences");

        let mut map = HashMap::new();
        let mut missing_ids = Vec::new();

        for uid in user_ids {
            let key = CacheKeyBuilder::user_presence(uid);
            if let Ok(Some(snapshot)) = self.cache.get::<PresenceSnapshot>(&key).await {
                map.insert(uid.clone(), (snapshot.presence, snapshot.status_msg));
            } else {
                missing_ids.push(uid.clone());
            }
        }

        if missing_ids.is_empty() {
            return Ok(map);
        }

        let rows = sqlx::query_as::<_, (String, String, Option<String>, Option<i64>)>(
            r"
            SELECT user_id, presence, status_msg, last_active_ts FROM presence WHERE user_id = ANY($1)
            ",
        )
        .bind(&missing_ids)
        .fetch_all(&*self.pool)
        .await?;

        let ttl = CacheTtl::user_presence().as_secs();
        for row in rows {
            let snapshot = PresenceSnapshot {
                user_id: row.0.clone(),
                presence: row.1.clone(),
                status_msg: row.2.clone(),
                last_active_ts: row.3,
            };
            let key = CacheKeyBuilder::user_presence(&row.0);
            if let Err(e) = self.cache.set(&key, &snapshot, ttl).await {
                tracing::warn!(target: "cache", "Failed to cache presence for {}: {}", row.0, e);
            }
            map.insert(row.0, (row.1, row.2));
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
                r"
                INSERT INTO typing (user_id, room_id, typing, last_active_ts)
                VALUES ($1, $2, $3, $4)
                ON CONFLICT (user_id, room_id)
                DO UPDATE SET typing = EXCLUDED.typing, last_active_ts = EXCLUDED.last_active_ts
                ",
            )
            .bind(user_id)
            .bind(room_id)
            .bind(typing)
            .bind(now)
            .execute(&*self.pool)
            .await?;
        } else {
            sqlx::query(
                r"
                DELETE FROM typing WHERE user_id = $1 AND room_id = $2
                ",
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
            r"
            INSERT INTO presence_subscriptions (subscriber_id, target_id, created_ts)
            VALUES ($1, $2, $3)
            ON CONFLICT (subscriber_id, target_id) DO NOTHING
            ",
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
                        r"
                        INSERT INTO presence_subscriptions (user_id, friend_id, created_ts)
                        VALUES ($1, $2, $3)
                        ON CONFLICT (user_id, friend_id) DO NOTHING
                        ",
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
            r"
            DELETE FROM presence_subscriptions
            WHERE subscriber_id = $1 AND target_id = $2
            ",
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
                        r"
                        DELETE FROM presence_subscriptions
                        WHERE user_id = $1 AND friend_id = $2
                        ",
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
            r"
            SELECT target_id FROM presence_subscriptions
            WHERE subscriber_id = $1
            LIMIT 5000
            ",
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
                        r"
                        SELECT friend_id FROM presence_subscriptions
                        WHERE user_id = $1
                        ",
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
            r"
            SELECT subscriber_id FROM presence_subscriptions
            WHERE target_id = $1
            ",
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
                        r"
                        SELECT user_id FROM presence_subscriptions
                        WHERE friend_id = $1
                        ",
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

        tracing::info!(count = user_ids.len(), "Batch querying presence");

        let mut results = Vec::new();
        let mut missing_ids = Vec::new();

        for uid in user_ids {
            let key = CacheKeyBuilder::user_presence(uid);
            if let Ok(Some(snapshot)) = self.cache.get::<PresenceSnapshot>(&key).await {
                results.push((snapshot.user_id, snapshot.presence, snapshot.status_msg));
            } else {
                missing_ids.push(uid.clone());
            }
        }

        if missing_ids.is_empty() {
            return Ok(results);
        }

        let rows = sqlx::query_as::<_, (String, String, Option<String>, Option<i64>)>(
            r"
            SELECT user_id, presence, status_msg, last_active_ts
            FROM presence
            WHERE user_id = ANY($1)
            ",
        )
        .bind(&missing_ids)
        .fetch_all(&*self.pool)
        .await?;

        let ttl = CacheTtl::user_presence().as_secs();
        for row in &rows {
            let snapshot = PresenceSnapshot {
                user_id: row.0.clone(),
                presence: row.1.clone(),
                status_msg: row.2.clone(),
                last_active_ts: row.3,
            };
            let key = CacheKeyBuilder::user_presence(&row.0);
            if let Err(e) = self.cache.set(&key, &snapshot, ttl).await {
                tracing::warn!(target: "cache", "Failed to cache presence for {}: {}", row.0, e);
            }
        }

        results.extend(rows.into_iter().map(|(uid, presence, status_msg, _)| (uid, presence, status_msg)));
        Ok(results)
    }

    pub async fn get_presence_batch_with_meta(
        &self,
        user_ids: &[String],
    ) -> Result<Vec<(String, String, Option<String>, Option<i64>)>, sqlx::Error> {
        if user_ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut results = Vec::new();
        let mut missing_ids = Vec::new();

        for uid in user_ids {
            let key = CacheKeyBuilder::user_presence(uid);
            if let Ok(Some(snapshot)) = self.cache.get::<PresenceSnapshot>(&key).await {
                results.push((snapshot.user_id, snapshot.presence, snapshot.status_msg, snapshot.last_active_ts));
            } else {
                missing_ids.push(uid.clone());
            }
        }

        if missing_ids.is_empty() {
            return Ok(results);
        }

        let rows = sqlx::query_as::<_, (String, String, Option<String>, Option<i64>)>(
            r"
            SELECT user_id, presence, status_msg, last_active_ts
            FROM presence
            WHERE user_id = ANY($1)
            ",
        )
        .bind(&missing_ids)
        .fetch_all(&*self.pool)
        .await?;

        let ttl = CacheTtl::user_presence().as_secs();
        for row in &rows {
            let snapshot = PresenceSnapshot {
                user_id: row.0.clone(),
                presence: row.1.clone(),
                status_msg: row.2.clone(),
                last_active_ts: row.3,
            };
            let key = CacheKeyBuilder::user_presence(&row.0);
            if let Err(e) = self.cache.set(&key, &snapshot, ttl).await {
                tracing::warn!(target: "cache", "Failed to cache presence for {}: {}", row.0, e);
            }
        }

        results.extend(rows);
        Ok(results)
    }

    pub async fn get_presence_snapshots(
        &self,
        user_ids: &[String],
    ) -> Result<HashMap<String, PresenceSnapshot>, sqlx::Error> {
        if user_ids.is_empty() {
            return Ok(HashMap::new());
        }

        tracing::info!(count = user_ids.len(), "Batch querying presence snapshots");

        let mut map = HashMap::new();
        let mut missing_ids = Vec::new();

        for uid in user_ids {
            let key = CacheKeyBuilder::user_presence(uid);
            if let Ok(Some(snapshot)) = self.cache.get::<PresenceSnapshot>(&key).await {
                map.insert(snapshot.user_id.clone(), snapshot);
            } else {
                missing_ids.push(uid.clone());
            }
        }

        if missing_ids.is_empty() {
            return Ok(map);
        }

        let rows = sqlx::query_as::<_, PresenceSnapshot>(
            r"
            SELECT user_id,
                   COALESCE(presence, 'offline') as presence,
                   status_msg,
                   last_active_ts
            FROM presence
            WHERE user_id = ANY($1)
            ",
        )
        .bind(&missing_ids)
        .fetch_all(&*self.pool)
        .await?;

        let ttl = CacheTtl::user_presence().as_secs();
        for snapshot in &rows {
            let key = CacheKeyBuilder::user_presence(&snapshot.user_id);
            if let Err(e) = self.cache.set(&key, snapshot, ttl).await {
                tracing::warn!(target: "cache", "Failed to cache presence for {}: {}", snapshot.user_id, e);
            }
        }

        for snapshot in rows {
            map.insert(snapshot.user_id.clone(), snapshot);
        }

        Ok(map)
    }
}
