use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use synapse_common::current_timestamp_millis;

/// The `RoomTag` struct.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RoomTag {
    /// The `id` field.
    pub id: i32,
    /// The `user_id` field.
    pub user_id: String,
    /// The `room_id` field.
    pub room_id: String,
    /// The `tag` field.
    pub tag: String,
    #[sqlx(rename = "order_value")]
    /// The `order` field.
    pub order: Option<f64>,
    /// The `created_ts` field.
    pub created_ts: i64,
}

// ── Trait ───────────────────────────────────────────────────────────────

/// The `RoomTagStoreApi` trait.
#[async_trait]
pub trait RoomTagStoreApi: Send + Sync {
    /// See [`get_all_tags`].
    async fn get_all_tags(&self, user_id: &str) -> Result<Vec<RoomTag>, sqlx::Error>;
    /// See [`get_tags`].
    async fn get_tags(&self, user_id: &str, room_id: &str) -> Result<Vec<RoomTag>, sqlx::Error>;
    /// See [`add_tag`].
    async fn add_tag(&self, user_id: &str, room_id: &str, tag: &str, order: Option<f64>) -> Result<(), sqlx::Error>;
    /// See [`remove_tag`].
    async fn remove_tag(&self, user_id: &str, room_id: &str, tag: &str) -> Result<(), sqlx::Error>;
}

// ── Postgres implementation ─────────────────────────────────────────────

/// The `RoomTagStorage` struct.
#[derive(Clone)]
pub struct RoomTagStorage {
    pool: Arc<sqlx::PgPool>,
}

impl RoomTagStorage {
    /// See [`new`].
    pub fn new(pool: Arc<sqlx::PgPool>) -> Self {
        Self { pool }
    }

    /// Returns a reference to the database connection pool.
    pub fn pool(&self) -> &Arc<sqlx::PgPool> {
        &self.pool
    }

    /// See [`get_all_tags`].
    pub async fn get_all_tags(&self, user_id: &str) -> Result<Vec<RoomTag>, sqlx::Error> {
        sqlx::query_as::<_, RoomTag>(
            "SELECT id, user_id, room_id, tag, order_value, created_ts FROM room_tags WHERE user_id = $1 ORDER BY room_id, tag"
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await
    }

    /// See [`get_tags`].
    pub async fn get_tags(&self, user_id: &str, room_id: &str) -> Result<Vec<RoomTag>, sqlx::Error> {
        sqlx::query_as::<_, RoomTag>(
            "SELECT id, user_id, room_id, tag, order_value, created_ts FROM room_tags WHERE user_id = $1 AND room_id = $2 ORDER BY tag"
        )
        .bind(user_id)
        .bind(room_id)
        .fetch_all(&*self.pool)
        .await
    }

    /// See [`add_tag`].
    pub async fn add_tag(
        &self,
        user_id: &str,
        room_id: &str,
        tag: &str,
        order: Option<f64>,
    ) -> Result<(), sqlx::Error> {
        let created_ts = current_timestamp_millis();
        sqlx::query(
            "INSERT INTO room_tags (user_id, room_id, tag, order_value, created_ts) VALUES ($1, $2, $3, $4, $5) ON CONFLICT (user_id, room_id, tag) DO UPDATE SET order_value = EXCLUDED.order_value"
        )
        .bind(user_id)
        .bind(room_id)
        .bind(tag)
        .bind(order)
        .bind(created_ts)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    /// See [`remove_tag`].
    pub async fn remove_tag(&self, user_id: &str, room_id: &str, tag: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM room_tags WHERE user_id = $1 AND room_id = $2 AND tag = $3")
            .bind(user_id)
            .bind(room_id)
            .bind(tag)
            .execute(&*self.pool)
            .await?;
        Ok(())
    }
}

// ── Trait delegation ────────────────────────────────────────────────────

#[async_trait]
impl RoomTagStoreApi for RoomTagStorage {
    async fn get_all_tags(&self, user_id: &str) -> Result<Vec<RoomTag>, sqlx::Error> {
        self.get_all_tags(user_id).await
    }

    async fn get_tags(&self, user_id: &str, room_id: &str) -> Result<Vec<RoomTag>, sqlx::Error> {
        self.get_tags(user_id, room_id).await
    }

    async fn add_tag(&self, user_id: &str, room_id: &str, tag: &str, order: Option<f64>) -> Result<(), sqlx::Error> {
        self.add_tag(user_id, room_id, tag, order).await
    }

    async fn remove_tag(&self, user_id: &str, room_id: &str, tag: &str) -> Result<(), sqlx::Error> {
        self.remove_tag(user_id, room_id, tag).await
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod db_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use sqlx::postgres::PgPoolOptions;
    use std::env;
    use std::sync::Arc;
    use std::time::Duration;

    async fn test_pool() -> Arc<sqlx::PgPool> {
        let db_url = env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:5432/synapse_test".to_string());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .acquire_timeout(Duration::from_secs(30))
            .connect(&db_url)
            .await
            .expect("Failed to connect to test database");
        Arc::new(pool)
    }

    fn make_suffix() -> String {
        uuid::Uuid::new_v4().simple().to_string()
    }

    #[tokio::test]
    async fn get_all_tags_empty_for_new_user() {
        let pool = test_pool().await;
        let storage = RoomTagStorage::new(pool);
        let suffix = make_suffix();
        let user_id = format!("@roomtag_empty_{suffix}:test");
        assert!(storage.get_all_tags(&user_id).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn add_tag_then_get_tags() {
        let pool = test_pool().await;
        let storage = RoomTagStorage::new(pool.clone());
        let suffix = make_suffix();
        let user_id = format!("@roomtag_add_{suffix}:test");
        let room_id = format!("!room_{suffix}:test");

        storage.add_tag(&user_id, &room_id, "m.favourite", Some(0.5)).await.unwrap();
        let tags = storage.get_tags(&user_id, &room_id).await.unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].tag, "m.favourite");
        assert_eq!(tags[0].order, Some(0.5));

        let _ = sqlx::query("DELETE FROM room_tags WHERE user_id = $1 AND room_id = $2")
            .bind(&user_id)
            .bind(&room_id)
            .execute(pool.as_ref())
            .await;
    }

    #[tokio::test]
    async fn add_tag_upserts_existing_tag() {
        let pool = test_pool().await;
        let storage = RoomTagStorage::new(pool.clone());
        let suffix = make_suffix();
        let user_id = format!("@roomtag_upsert_{suffix}:test");
        let room_id = format!("!room_{suffix}:test");

        storage.add_tag(&user_id, &room_id, "m.lowpriority", Some(0.1)).await.unwrap();
        storage.add_tag(&user_id, &room_id, "m.lowpriority", Some(0.9)).await.unwrap();
        let tags = storage.get_tags(&user_id, &room_id).await.unwrap();
        assert_eq!(tags.len(), 1, "upsert must not create a duplicate row");
        assert_eq!(tags[0].order, Some(0.9));

        let _ = sqlx::query("DELETE FROM room_tags WHERE user_id = $1 AND room_id = $2")
            .bind(&user_id)
            .bind(&room_id)
            .execute(pool.as_ref())
            .await;
    }

    #[tokio::test]
    async fn remove_tag_deletes_record() {
        let pool = test_pool().await;
        let storage = RoomTagStorage::new(pool.clone());
        let suffix = make_suffix();
        let user_id = format!("@roomtag_remove_{suffix}:test");
        let room_id = format!("!room_{suffix}:test");

        storage.add_tag(&user_id, &room_id, "m.favourite", None).await.unwrap();
        storage.remove_tag(&user_id, &room_id, "m.favourite").await.unwrap();
        assert!(storage.get_tags(&user_id, &room_id).await.unwrap().is_empty());

        let _ = sqlx::query("DELETE FROM room_tags WHERE user_id = $1 AND room_id = $2")
            .bind(&user_id)
            .bind(&room_id)
            .execute(pool.as_ref())
            .await;
    }

    #[tokio::test]
    async fn get_all_tags_returns_multiple_tags() {
        let pool = test_pool().await;
        let storage = RoomTagStorage::new(pool.clone());
        let suffix = make_suffix();
        let user_id = format!("@roomtag_multi_{suffix}:test");
        let room_id = format!("!room_{suffix}:test");

        storage.add_tag(&user_id, &room_id, "m.favourite", None).await.unwrap();
        storage.add_tag(&user_id, &room_id, "m.lowpriority", None).await.unwrap();
        let all = storage.get_all_tags(&user_id).await.unwrap();
        assert_eq!(all.len(), 2);

        let _ = sqlx::query("DELETE FROM room_tags WHERE user_id = $1 AND room_id = $2")
            .bind(&user_id)
            .bind(&room_id)
            .execute(pool.as_ref())
            .await;
    }
}
