use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};
use std::collections::HashMap;
use std::sync::Arc;
use synapse_cache::{CacheKeyBuilder, CacheManager, CacheTtl};
use tracing;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PresenceSnapshot {
    pub user_id: String,
    pub presence: String,
    pub status_msg: Option<String>,
    pub last_active_ts: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now_ms() -> i64 {
        chrono::Utc::now().timestamp_millis()
    }

    #[test]
    fn test_presence_snapshot_construction_online() {
        let snapshot = PresenceSnapshot {
            user_id: "@alice:example.com".to_string(),
            presence: "online".to_string(),
            status_msg: Some("Working".to_string()),
            last_active_ts: Some(now_ms()),
        };
        assert_eq!(snapshot.user_id, "@alice:example.com");
        assert_eq!(snapshot.presence, "online");
        assert_eq!(snapshot.status_msg.as_deref(), Some("Working"));
        assert!(snapshot.last_active_ts.is_some());
    }

    #[test]
    fn test_presence_snapshot_construction_offline_no_status() {
        let snapshot = PresenceSnapshot {
            user_id: "@bob:example.com".to_string(),
            presence: "offline".to_string(),
            status_msg: None,
            last_active_ts: None,
        };
        assert_eq!(snapshot.presence, "offline");
        assert!(snapshot.status_msg.is_none());
        assert!(snapshot.last_active_ts.is_none());
    }

    #[test]
    fn test_presence_snapshot_serde_roundtrip_full() {
        let snapshot = PresenceSnapshot {
            user_id: "@carol:example.com".to_string(),
            presence: "away".to_string(),
            status_msg: Some("Be right back".to_string()),
            last_active_ts: Some(1_700_000_000_000),
        };
        let json = serde_json::to_string(&snapshot).expect("serialize PresenceSnapshot");
        let restored: PresenceSnapshot = serde_json::from_str(&json).expect("deserialize PresenceSnapshot");
        assert_eq!(restored.user_id, snapshot.user_id);
        assert_eq!(restored.presence, snapshot.presence);
        assert_eq!(restored.status_msg, snapshot.status_msg);
        assert_eq!(restored.last_active_ts, snapshot.last_active_ts);
    }

    #[test]
    fn test_presence_snapshot_serde_roundtrip_null_fields() {
        let snapshot = PresenceSnapshot {
            user_id: "@dave:example.com".to_string(),
            presence: "unavailable".to_string(),
            status_msg: None,
            last_active_ts: None,
        };
        let json = serde_json::to_string(&snapshot).expect("serialize");
        // Verify JSON contains null for optional fields
        assert!(json.contains("\"status_msg\":null"));
        assert!(json.contains("\"last_active_ts\":null"));
        let restored: PresenceSnapshot = serde_json::from_str(&json).expect("deserialize");
        assert!(restored.status_msg.is_none());
        assert!(restored.last_active_ts.is_none());
    }

    #[test]
    fn test_presence_snapshot_empty_status_message() {
        let snapshot = PresenceSnapshot {
            user_id: "@eve:example.com".to_string(),
            presence: "online".to_string(),
            status_msg: Some(String::new()),
            last_active_ts: Some(0),
        };
        assert_eq!(snapshot.status_msg.as_deref(), Some(""));
        assert_eq!(snapshot.last_active_ts, Some(0));
    }

    #[test]
    fn test_presence_snapshot_clone_preserves_fields() {
        let snapshot = PresenceSnapshot {
            user_id: "@frank:example.com".to_string(),
            presence: "online".to_string(),
            status_msg: Some("Busy".to_string()),
            last_active_ts: Some(now_ms()),
        };
        let cloned = snapshot.clone();
        assert_eq!(cloned.user_id, snapshot.user_id);
        assert_eq!(cloned.presence, snapshot.presence);
        assert_eq!(cloned.status_msg, snapshot.status_msg);
        assert_eq!(cloned.last_active_ts, snapshot.last_active_ts);
    }

    #[test]
    fn test_presence_snapshot_json_field_names() {
        // Verify the JSON field names match the Matrix spec format (snake_case)
        let snapshot = PresenceSnapshot {
            user_id: "@x:example.com".to_string(),
            presence: "online".to_string(),
            status_msg: None,
            last_active_ts: None,
        };
        let json = serde_json::to_string(&snapshot).expect("serialize");
        assert!(json.contains("\"user_id\""));
        assert!(json.contains("\"presence\""));
        assert!(json.contains("\"status_msg\""));
        assert!(json.contains("\"last_active_ts\""));
    }
}

#[derive(Clone)]
pub struct PresenceStorage {
    pool: Arc<Pool<Postgres>>,
    cache: Arc<CacheManager>,
}

impl PresenceStorage {
    pub fn new(pool: Arc<Pool<Postgres>>, cache: Arc<CacheManager>) -> Self {
        Self { pool, cache }
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

    pub async fn get_presence(&self, user_id: &str) -> Result<Option<(String, Option<String>)>, sqlx::Error> {
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

        let keys: Vec<String> = user_ids.iter().map(|uid| CacheKeyBuilder::user_presence(uid)).collect();
        let snapshots = match self.cache.get_batch::<PresenceSnapshot>(&keys).await {
            Ok(s) => s,
            Err(_) => vec![None; keys.len()],
        };
        for (uid, snapshot) in user_ids.iter().zip(snapshots.into_iter()) {
            if let Some(snapshot) = snapshot {
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

    pub async fn set_typing(&self, room_id: &str, user_id: &str, typing: bool) -> Result<(), sqlx::Error> {
        if typing {
            let now = chrono::Utc::now().timestamp_millis();
            sqlx::query(
                r"
                INSERT INTO typing (user_id, room_id, is_typing, last_active_ts)
                VALUES ($1, $2, $3, $4)
                ON CONFLICT (user_id, room_id)
                DO UPDATE SET is_typing = EXCLUDED.is_typing, last_active_ts = EXCLUDED.last_active_ts
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

    pub async fn add_subscription(&self, subscriber_id: &str, target_id: &str) -> Result<(), sqlx::Error> {
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

    pub async fn remove_subscription(&self, subscriber_id: &str, target_id: &str) -> Result<(), sqlx::Error> {
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

        let keys: Vec<String> = user_ids.iter().map(|uid| CacheKeyBuilder::user_presence(uid)).collect();
        let snapshots = match self.cache.get_batch::<PresenceSnapshot>(&keys).await {
            Ok(s) => s,
            Err(_) => vec![None; keys.len()],
        };
        for (uid, snapshot) in user_ids.iter().zip(snapshots.into_iter()) {
            if let Some(snapshot) = snapshot {
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

        let keys: Vec<String> = user_ids.iter().map(|uid| CacheKeyBuilder::user_presence(uid)).collect();
        let snapshots = match self.cache.get_batch::<PresenceSnapshot>(&keys).await {
            Ok(s) => s,
            Err(_) => vec![None; keys.len()],
        };
        for (uid, snapshot) in user_ids.iter().zip(snapshots.into_iter()) {
            if let Some(snapshot) = snapshot {
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

        let keys: Vec<String> = user_ids.iter().map(|uid| CacheKeyBuilder::user_presence(uid)).collect();
        let snapshots = match self.cache.get_batch::<PresenceSnapshot>(&keys).await {
            Ok(s) => s,
            Err(_) => vec![None; keys.len()],
        };
        for (uid, snapshot) in user_ids.iter().zip(snapshots.into_iter()) {
            if let Some(snapshot) = snapshot {
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

mod db_tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;
    use sqlx::{Pool, Postgres};
    use std::env;
    use std::sync::Arc;
    use synapse_cache::{CacheConfig, CacheManager};

    async fn test_pool() -> Arc<Pool<Postgres>> {
        let db_url = env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
        let pool =
            PgPoolOptions::new().max_connections(2).connect(&db_url).await.expect("Failed to connect to test database");
        Arc::new(pool)
    }

    fn test_cache() -> Arc<CacheManager> {
        Arc::new(CacheManager::new(&CacheConfig::default()))
    }

    async fn ensure_test_user(pool: &sqlx::PgPool, user_id: &str) {
        let username = user_id.strip_prefix('@').and_then(|u| u.split(':').next()).unwrap_or("testuser");
        sqlx::query(
            "INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, EXTRACT(EPOCH FROM NOW()) * 1000) ON CONFLICT (user_id) DO NOTHING",
        )
        .bind(user_id)
        .bind(username)
        .execute(pool)
        .await
        .expect("failed to create test user");
    }

    async fn cleanup_presence_data(pool: &sqlx::PgPool, suffix: &str) {
        let _ =
            sqlx::query("DELETE FROM presence WHERE user_id LIKE $1").bind(format!("%{suffix}%")).execute(pool).await;
        let _ = sqlx::query("DELETE FROM presence_subscriptions WHERE subscriber_id LIKE $1 OR target_id LIKE $1")
            .bind(format!("%{suffix}%"))
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM typing WHERE user_id LIKE $1").bind(format!("%{suffix}%")).execute(pool).await;
    }

    // ================================================================
    // set_presence
    // ================================================================

    #[tokio::test]
    async fn test_set_presence_online() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@pres_test_online_{suffix}:localhost");
        cleanup_presence_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let storage = PresenceStorage::new(pool.clone(), test_cache());
        storage.set_presence(&user_id, "online", None).await.expect("set_presence should succeed");

        let row = sqlx::query_as::<_, (String, Option<String>, Option<i64>)>(
            "SELECT presence, status_msg, last_active_ts FROM presence WHERE user_id = $1",
        )
        .bind(&user_id)
        .fetch_one(&*pool)
        .await
        .expect("should find presence row");

        assert_eq!(row.0, "online");
        assert!(row.1.is_none());
        assert!(row.2.is_some());

        cleanup_presence_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_set_presence_offline() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@pres_test_offline_{suffix}:localhost");
        cleanup_presence_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let storage = PresenceStorage::new(pool.clone(), test_cache());
        storage.set_presence(&user_id, "offline", None).await.expect("set_presence should succeed");

        let row = sqlx::query_as::<_, (String, Option<String>, Option<i64>)>(
            "SELECT presence, status_msg, last_active_ts FROM presence WHERE user_id = $1",
        )
        .bind(&user_id)
        .fetch_one(&*pool)
        .await
        .expect("should find presence row");

        assert_eq!(row.0, "offline");

        cleanup_presence_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_set_presence_unavailable_with_status() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@pres_test_unavail_{suffix}:localhost");
        cleanup_presence_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let storage = PresenceStorage::new(pool.clone(), test_cache());
        storage
            .set_presence(&user_id, "unavailable", Some("Away from keyboard"))
            .await
            .expect("set_presence should succeed");

        let row = sqlx::query_as::<_, (String, Option<String>, Option<i64>)>(
            "SELECT presence, status_msg, last_active_ts FROM presence WHERE user_id = $1",
        )
        .bind(&user_id)
        .fetch_one(&*pool)
        .await
        .expect("should find presence row");

        assert_eq!(row.0, "unavailable");
        assert_eq!(row.1.as_deref(), Some("Away from keyboard"));
        assert!(row.2.is_some());

        cleanup_presence_data(&pool, &suffix).await;
    }

    // ================================================================
    // get_presence
    // ================================================================

    #[tokio::test]
    async fn test_get_presence_found() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@pres_test_get_found_{suffix}:localhost");
        cleanup_presence_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let storage = PresenceStorage::new(pool.clone(), test_cache());
        storage.set_presence(&user_id, "online", Some("Hello")).await.expect("set_presence should succeed");

        let result = storage.get_presence(&user_id).await.expect("get_presence should succeed");

        assert!(result.is_some());
        let (presence, status_msg) = result.unwrap();
        assert_eq!(presence, "online");
        assert_eq!(status_msg.as_deref(), Some("Hello"));

        cleanup_presence_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_presence_not_found() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@pres_test_get_miss_{suffix}:localhost");
        cleanup_presence_data(&pool, &suffix).await;
        // Note: intentionally NOT creating the user so presence is truly missing

        let storage = PresenceStorage::new(pool.clone(), test_cache());
        let result = storage.get_presence(&user_id).await.expect("get_presence should succeed");
        assert!(result.is_none());

        // get_presence_with_meta should also return None for unknown user
        let meta_result =
            storage.get_presence_with_meta(&user_id).await.expect("get_presence_with_meta should succeed");
        assert!(meta_result.is_none());

        cleanup_presence_data(&pool, &suffix).await;
    }

    // ================================================================
    // get_presence_with_meta
    // ================================================================

    #[tokio::test]
    async fn test_get_presence_with_meta_found() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@pres_test_meta_{suffix}:localhost");
        cleanup_presence_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let storage = PresenceStorage::new(pool.clone(), test_cache());
        storage.set_presence(&user_id, "online", Some("Working")).await.expect("set_presence should succeed");

        let result = storage.get_presence_with_meta(&user_id).await.expect("get_presence_with_meta should succeed");

        assert!(result.is_some());
        let (presence, status_msg, last_active_ts) = result.unwrap();
        assert_eq!(presence, "online");
        assert_eq!(status_msg.as_deref(), Some("Working"));
        assert!(last_active_ts.is_some());
        assert!(last_active_ts.unwrap() > 0);

        cleanup_presence_data(&pool, &suffix).await;
    }

    // ================================================================
    // get_presences
    // ================================================================

    #[tokio::test]
    async fn test_get_presences_multiple_users() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_a = format!("@pres_test_bulk_a_{suffix}:localhost");
        let user_b = format!("@pres_test_bulk_b_{suffix}:localhost");
        cleanup_presence_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_a).await;
        ensure_test_user(&pool, &user_b).await;

        let storage = PresenceStorage::new(pool.clone(), test_cache());
        storage.set_presence(&user_a, "online", None).await.expect("set_presence a");
        storage.set_presence(&user_b, "offline", Some("Busy")).await.expect("set_presence b");

        let map = storage.get_presences(&[user_a.clone(), user_b.clone()]).await.expect("get_presences should succeed");

        assert_eq!(map.len(), 2);
        assert_eq!(map.get(&user_a).map(|p| &*p.0), Some("online"));
        assert_eq!(map.get(&user_b).map(|p| &*p.0), Some("offline"));
        assert_eq!(map.get(&user_b).and_then(|p| p.1.as_deref()), Some("Busy"));

        cleanup_presence_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_presences_empty_batch() {
        let pool = test_pool().await;
        let storage = PresenceStorage::new(pool.clone(), test_cache());

        let map = storage.get_presences(&[]).await.expect("get_presences should succeed");
        assert!(map.is_empty());
    }

    // ================================================================
    // set_typing
    // ================================================================

    #[tokio::test]
    async fn test_set_typing_true() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@pres_test_typing_t_{suffix}:localhost");
        let room_id = format!("!test_room_typing_t_{suffix}:localhost");
        cleanup_presence_data(&pool, &suffix).await;

        let storage = PresenceStorage::new(pool.clone(), test_cache());
        storage.set_typing(&room_id, &user_id, true).await.expect("set_typing should succeed");

        let row = sqlx::query_as::<_, (bool, i64)>(
            "SELECT is_typing, last_active_ts FROM typing WHERE user_id = $1 AND room_id = $2",
        )
        .bind(&user_id)
        .bind(&room_id)
        .fetch_one(&*pool)
        .await
        .expect("should find typing row");

        assert!(row.0, "is_typing should be true");
        assert!(row.1 > 0);

        cleanup_presence_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_set_typing_false_removes_row() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@pres_test_typing_f_{suffix}:localhost");
        let room_id = format!("!test_room_typing_f_{suffix}:localhost");
        cleanup_presence_data(&pool, &suffix).await;

        let storage = PresenceStorage::new(pool.clone(), test_cache());
        storage.set_typing(&room_id, &user_id, true).await.expect("set_typing true");
        storage.set_typing(&room_id, &user_id, false).await.expect("set_typing false");

        let row = sqlx::query_as::<_, (bool,)>("SELECT is_typing FROM typing WHERE user_id = $1 AND room_id = $2")
            .bind(&user_id)
            .bind(&room_id)
            .fetch_optional(&*pool)
            .await
            .expect("query should succeed");

        assert!(row.is_none(), "typing row should be deleted");

        cleanup_presence_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_set_typing_updates_timestamp() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@pres_test_typing_upd_{suffix}:localhost");
        let room_id = format!("!test_room_typing_upd_{suffix}:localhost");
        cleanup_presence_data(&pool, &suffix).await;

        let storage = PresenceStorage::new(pool.clone(), test_cache());
        storage.set_typing(&room_id, &user_id, true).await.expect("first set_typing");

        let first_ts =
            sqlx::query_as::<_, (i64,)>("SELECT last_active_ts FROM typing WHERE user_id = $1 AND room_id = $2")
                .bind(&user_id)
                .bind(&room_id)
                .fetch_one(&*pool)
                .await
                .expect("query first ts")
                .0;

        // Small sleep to ensure different timestamps
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        storage.set_typing(&room_id, &user_id, true).await.expect("second set_typing");

        let second_ts =
            sqlx::query_as::<_, (i64,)>("SELECT last_active_ts FROM typing WHERE user_id = $1 AND room_id = $2")
                .bind(&user_id)
                .bind(&room_id)
                .fetch_one(&*pool)
                .await
                .expect("query second ts")
                .0;

        assert!(second_ts >= first_ts, "timestamp should be updated ({} >= {})", second_ts, first_ts);

        cleanup_presence_data(&pool, &suffix).await;
    }

    // ================================================================
    // add_subscription / remove_subscription
    // ================================================================

    #[tokio::test]
    async fn test_add_subscription_subscribes() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let subscriber = format!("@pres_test_sub_a_{suffix}:localhost");
        let target = format!("@pres_test_sub_t_{suffix}:localhost");
        cleanup_presence_data(&pool, &suffix).await;
        ensure_test_user(&pool, &subscriber).await;
        ensure_test_user(&pool, &target).await;

        let storage = PresenceStorage::new(pool.clone(), test_cache());
        storage.add_subscription(&subscriber, &target).await.expect("add_subscription should succeed");

        let subs = storage.get_subscriptions(&subscriber).await.expect("get_subscriptions should succeed");
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0], target);

        let followers = storage.get_subscribers(&target).await.expect("get_subscribers should succeed");
        assert_eq!(followers.len(), 1);
        assert_eq!(followers[0], subscriber);

        cleanup_presence_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_add_subscription_idempotent() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let subscriber = format!("@pres_test_idem_s_{suffix}:localhost");
        let target = format!("@pres_test_idem_t_{suffix}:localhost");
        cleanup_presence_data(&pool, &suffix).await;
        ensure_test_user(&pool, &subscriber).await;
        ensure_test_user(&pool, &target).await;

        let storage = PresenceStorage::new(pool.clone(), test_cache());
        storage.add_subscription(&subscriber, &target).await.expect("first add_subscription");
        storage.add_subscription(&subscriber, &target).await.expect("second add_subscription should also succeed");

        let subs = storage.get_subscriptions(&subscriber).await.expect("get_subscriptions should succeed");
        assert_eq!(subs.len(), 1, "should only have one subscription");

        cleanup_presence_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_remove_subscription_removes() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let subscriber = format!("@pres_test_rem_s_{suffix}:localhost");
        let target = format!("@pres_test_rem_t_{suffix}:localhost");
        cleanup_presence_data(&pool, &suffix).await;
        ensure_test_user(&pool, &subscriber).await;
        ensure_test_user(&pool, &target).await;

        let storage = PresenceStorage::new(pool.clone(), test_cache());
        storage.add_subscription(&subscriber, &target).await.expect("add_subscription");
        storage.remove_subscription(&subscriber, &target).await.expect("remove_subscription should succeed");

        let subs = storage.get_subscriptions(&subscriber).await.expect("get_subscriptions should succeed");
        assert!(subs.is_empty(), "subscriptions should be empty after removal");

        cleanup_presence_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_remove_subscription_idempotent() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let subscriber = format!("@pres_test_rem_ni_{suffix}:localhost");
        let target = format!("@pres_test_rem_nt_{suffix}:localhost");
        cleanup_presence_data(&pool, &suffix).await;
        ensure_test_user(&pool, &subscriber).await;
        ensure_test_user(&pool, &target).await;

        let storage = PresenceStorage::new(pool.clone(), test_cache());
        // Removing a non-existent subscription should succeed (no-op)
        let result = storage.remove_subscription(&subscriber, &target).await;
        assert!(result.is_ok(), "removing non-existent subscription should be ok");

        cleanup_presence_data(&pool, &suffix).await;
    }

    // ================================================================
    // get_subscriptions
    // ================================================================

    #[tokio::test]
    async fn test_get_subscriptions_returns_list() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let subscriber = format!("@pres_test_gsubs_s_{suffix}:localhost");
        let target_a = format!("@pres_test_gsubs_a_{suffix}:localhost");
        let target_b = format!("@pres_test_gsubs_b_{suffix}:localhost");
        cleanup_presence_data(&pool, &suffix).await;
        ensure_test_user(&pool, &subscriber).await;
        ensure_test_user(&pool, &target_a).await;
        ensure_test_user(&pool, &target_b).await;

        let storage = PresenceStorage::new(pool.clone(), test_cache());
        storage.add_subscription(&subscriber, &target_a).await.expect("add sub a");
        storage.add_subscription(&subscriber, &target_b).await.expect("add sub b");

        let subs = storage.get_subscriptions(&subscriber).await.expect("get_subscriptions should succeed");
        assert_eq!(subs.len(), 2);
        assert!(subs.contains(&target_a));
        assert!(subs.contains(&target_b));

        cleanup_presence_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_subscriptions_empty() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let subscriber = format!("@pres_test_gsubs_e_{suffix}:localhost");
        cleanup_presence_data(&pool, &suffix).await;
        ensure_test_user(&pool, &subscriber).await;

        let storage = PresenceStorage::new(pool.clone(), test_cache());
        let subs = storage.get_subscriptions(&subscriber).await.expect("get_subscriptions should succeed");
        assert!(subs.is_empty());

        cleanup_presence_data(&pool, &suffix).await;
    }

    // ================================================================
    // get_subscribers
    // ================================================================

    #[tokio::test]
    async fn test_get_subscribers_returns_list() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let target = format!("@pres_test_follow_t_{suffix}:localhost");
        let follower_a = format!("@pres_test_follow_a_{suffix}:localhost");
        let follower_b = format!("@pres_test_follow_b_{suffix}:localhost");
        cleanup_presence_data(&pool, &suffix).await;
        ensure_test_user(&pool, &target).await;
        ensure_test_user(&pool, &follower_a).await;
        ensure_test_user(&pool, &follower_b).await;

        let storage = PresenceStorage::new(pool.clone(), test_cache());
        storage.add_subscription(&follower_a, &target).await.expect("add sub a");
        storage.add_subscription(&follower_b, &target).await.expect("add sub b");

        let followers = storage.get_subscribers(&target).await.expect("get_subscribers should succeed");
        assert_eq!(followers.len(), 2);
        assert!(followers.contains(&follower_a));
        assert!(followers.contains(&follower_b));

        cleanup_presence_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_subscribers_empty() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let target = format!("@pres_test_follow_e_{suffix}:localhost");
        cleanup_presence_data(&pool, &suffix).await;
        ensure_test_user(&pool, &target).await;

        let storage = PresenceStorage::new(pool.clone(), test_cache());
        let followers = storage.get_subscribers(&target).await.expect("get_subscribers should succeed");
        assert!(followers.is_empty());

        cleanup_presence_data(&pool, &suffix).await;
    }

    // ================================================================
    // get_presence_batch
    // ================================================================

    #[tokio::test]
    async fn test_get_presence_batch_multiple_users() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_a = format!("@pres_test_batch_a_{suffix}:localhost");
        let user_b = format!("@pres_test_batch_b_{suffix}:localhost");
        let user_c = format!("@pres_test_batch_c_{suffix}:localhost");
        cleanup_presence_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_a).await;
        ensure_test_user(&pool, &user_b).await;

        let storage = PresenceStorage::new(pool.clone(), test_cache());
        storage.set_presence(&user_a, "online", None).await.expect("set_presence a");
        storage.set_presence(&user_b, "offline", Some("Sleeping")).await.expect("set_presence b");
        // user_c has no presence — should be silently omitted

        let results = storage
            .get_presence_batch(&[user_a.clone(), user_b.clone(), user_c.clone()])
            .await
            .expect("get_presence_batch should succeed");

        assert_eq!(results.len(), 2, "should return only users with presence data");
        let a = results.iter().find(|r| r.0 == user_a).expect("user_a present");
        assert_eq!(a.1, "online");
        assert!(a.2.is_none());
        let b = results.iter().find(|r| r.0 == user_b).expect("user_b present");
        assert_eq!(b.1, "offline");
        assert_eq!(b.2.as_deref(), Some("Sleeping"));

        cleanup_presence_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_presence_batch_with_meta() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user = format!("@pres_test_batch_m_{suffix}:localhost");
        cleanup_presence_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user).await;

        let storage = PresenceStorage::new(pool.clone(), test_cache());
        storage.set_presence(&user, "online", Some("At desk")).await.expect("set_presence");

        let results = storage
            .get_presence_batch_with_meta(&[user.clone()])
            .await
            .expect("get_presence_batch_with_meta should succeed");

        assert_eq!(results.len(), 1);
        let (uid, presence, status_msg, last_active) = &results[0];
        assert_eq!(uid, &user);
        assert_eq!(presence, "online");
        assert_eq!(status_msg.as_deref(), Some("At desk"));
        assert!(last_active.is_some());
        assert!(last_active.unwrap() > 0);

        cleanup_presence_data(&pool, &suffix).await;
    }

    // ================================================================
    // get_presence_snapshots
    // ================================================================

    #[tokio::test]
    async fn test_get_presence_snapshots_returns_data() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_a = format!("@pres_test_snap_a_{suffix}:localhost");
        let user_b = format!("@pres_test_snap_b_{suffix}:localhost");
        cleanup_presence_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_a).await;
        ensure_test_user(&pool, &user_b).await;

        let storage = PresenceStorage::new(pool.clone(), test_cache());
        storage.set_presence(&user_a, "online", Some("Available")).await.expect("set_presence a");
        storage.set_presence(&user_b, "unavailable", None).await.expect("set_presence b");

        let snapshots = storage
            .get_presence_snapshots(&[user_a.clone(), user_b.clone()])
            .await
            .expect("get_presence_snapshots should succeed");

        assert_eq!(snapshots.len(), 2);
        let snap_a = snapshots.get(&user_a).expect("snapshot for user_a");
        assert_eq!(snap_a.presence, "online");
        assert_eq!(snap_a.status_msg.as_deref(), Some("Available"));
        let snap_b = snapshots.get(&user_b).expect("snapshot for user_b");
        assert_eq!(snap_b.presence, "unavailable");
        assert!(snap_b.status_msg.is_none());

        cleanup_presence_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_presence_snapshots_empty() {
        let pool = test_pool().await;
        let storage = PresenceStorage::new(pool.clone(), test_cache());

        let snapshots = storage.get_presence_snapshots(&[]).await.expect("get_presence_snapshots should succeed");
        assert!(snapshots.is_empty());
    }
}
