// Sticky Event Storage - MSC4354
// Stores is_sticky event metadata for rooms
// Following project field naming standards

use async_trait::async_trait;
use sqlx::PgPool;
use std::sync::Arc;
use synapse_common::current_timestamp_millis;

#[async_trait]
pub trait StickyEventStoreApi: Send + Sync {
    async fn set_is_sticky_event(
        &self,
        room_id: &str,
        user_id: &str,
        event_id: &str,
        event_type: &str,
        is_sticky: bool,
    ) -> Result<(), sqlx::Error>;
    async fn get_is_sticky_event(
        &self,
        room_id: &str,
        user_id: &str,
        event_type: &str,
    ) -> Result<Option<StickyEvent>, sqlx::Error>;
    async fn get_all_is_sticky_events(&self, room_id: &str, user_id: &str) -> Result<Vec<StickyEvent>, sqlx::Error>;
    async fn clear_is_sticky_event(&self, room_id: &str, user_id: &str, event_type: &str) -> Result<(), sqlx::Error>;
    async fn get_rooms_with_is_sticky_events(&self, user_id: &str) -> Result<Vec<String>, sqlx::Error>;
}

#[derive(Clone)]
pub struct StickyEventStorage {
    pool: Arc<PgPool>,
}

impl StickyEventStorage {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }

    /// Set is_sticky event metadata for a room
    pub async fn set_is_sticky_event(
        &self,
        room_id: &str,
        user_id: &str,
        event_id: &str,
        event_type: &str,
        is_sticky: bool,
    ) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();

        sqlx::query(
            r"
            INSERT INTO room_sticky_events (room_id, user_id, event_id, event_type, is_sticky, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (room_id, user_id, event_type)
            DO UPDATE SET event_id = EXCLUDED.event_id, is_sticky = EXCLUDED.is_sticky, updated_ts = EXCLUDED.updated_ts
            ",
        )
        .bind(room_id)
        .bind(user_id)
        .bind(event_id)
        .bind(event_type)
        .bind(is_sticky)
        .bind(now)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// Get is_sticky event for a room and user by event type
    pub async fn get_is_sticky_event(
        &self,
        room_id: &str,
        user_id: &str,
        event_type: &str,
    ) -> Result<Option<StickyEvent>, sqlx::Error> {
        let result = sqlx::query_as::<_, (String, String, String, String, bool, i64, i64)>(
            r"
            SELECT room_id, user_id, event_id, event_type, is_sticky, created_ts, updated_ts
            FROM room_sticky_events
            WHERE room_id = $1 AND user_id = $2 AND event_type = $3 AND is_sticky = true
            ",
        )
        .bind(room_id)
        .bind(user_id)
        .bind(event_type)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(result.map(|(room_id, user_id, event_id, event_type, is_sticky, created_ts, updated_ts)| StickyEvent {
            room_id,
            user_id,
            event_id,
            event_type,
            is_sticky,
            created_ts,
            updated_ts,
        }))
    }

    /// Get all is_sticky events for a room and user
    pub async fn get_all_is_sticky_events(
        &self,
        room_id: &str,
        user_id: &str,
    ) -> Result<Vec<StickyEvent>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (String, String, String, String, bool, i64, i64)>(
            r"
            SELECT room_id, user_id, event_id, event_type, is_sticky, created_ts, updated_ts
            FROM room_sticky_events
            WHERE room_id = $1 AND user_id = $2 AND is_sticky = true
            ORDER BY event_type
            ",
        )
        .bind(room_id)
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(room_id, user_id, event_id, event_type, is_sticky, created_ts, updated_ts)| StickyEvent {
                room_id,
                user_id,
                event_id,
                event_type,
                is_sticky,
                created_ts,
                updated_ts,
            })
            .collect())
    }

    /// Clear is_sticky event for a room, user, and event type
    pub async fn clear_is_sticky_event(
        &self,
        room_id: &str,
        user_id: &str,
        event_type: &str,
    ) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();

        sqlx::query(
            r"
            UPDATE room_sticky_events
            SET is_sticky = false, updated_ts = $4
            WHERE room_id = $1 AND user_id = $2 AND event_type = $3
            ",
        )
        .bind(room_id)
        .bind(user_id)
        .bind(event_type)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// Get rooms with is_sticky events for a user (for sync)
    pub async fn get_rooms_with_is_sticky_events(&self, user_id: &str) -> Result<Vec<String>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (String,)>(
            r"
            SELECT DISTINCT room_id FROM room_sticky_events
            WHERE user_id = $1 AND is_sticky = true
            ",
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.0).collect())
    }
}

/// Sticky Event - Following project field naming standards
/// - created_ts: NOT NULL, milliseconds timestamp
/// - updated_ts: NOT NULL (or NULLABLE if appropriate), milliseconds timestamp
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StickyEvent {
    pub room_id: String,
    pub user_id: String,
    pub event_id: String,
    pub event_type: String,
    pub is_sticky: bool,
    pub created_ts: i64,
    pub updated_ts: i64,
}

#[async_trait]
impl StickyEventStoreApi for StickyEventStorage {
    async fn set_is_sticky_event(
        &self,
        room_id: &str,
        user_id: &str,
        event_id: &str,
        event_type: &str,
        is_sticky: bool,
    ) -> Result<(), sqlx::Error> {
        self.set_is_sticky_event(room_id, user_id, event_id, event_type, is_sticky).await
    }
    async fn get_is_sticky_event(
        &self,
        room_id: &str,
        user_id: &str,
        event_type: &str,
    ) -> Result<Option<StickyEvent>, sqlx::Error> {
        self.get_is_sticky_event(room_id, user_id, event_type).await
    }
    async fn get_all_is_sticky_events(&self, room_id: &str, user_id: &str) -> Result<Vec<StickyEvent>, sqlx::Error> {
        self.get_all_is_sticky_events(room_id, user_id).await
    }
    async fn clear_is_sticky_event(&self, room_id: &str, user_id: &str, event_type: &str) -> Result<(), sqlx::Error> {
        self.clear_is_sticky_event(room_id, user_id, event_type).await
    }
    async fn get_rooms_with_is_sticky_events(&self, user_id: &str) -> Result<Vec<String>, sqlx::Error> {
        self.get_rooms_with_is_sticky_events(user_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_sticky_event_struct() {
        let event = StickyEvent {
            room_id: "!room:localhost".to_string(),
            user_id: "@user:localhost".to_string(),
            event_id: "$event:localhost".to_string(),
            event_type: "m.room.message".to_string(),
            is_sticky: true,
            created_ts: 1700000000000i64,
            updated_ts: 1700000000000i64,
        };

        assert_eq!(event.room_id, "!room:localhost");
        assert!(event.is_sticky);
    }

    #[test]
    fn test_event_type_validation() {
        let valid_types = vec!["m.room.message", "m.room.topic", "m.room.avatar"];

        for event_type in valid_types {
            assert!(event_type.starts_with("m.") || event_type.starts_with("com."));
        }
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;

    async fn test_pool() -> Arc<PgPool> {
        let db_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
        let pool =
            PgPoolOptions::new().max_connections(2).connect(&db_url).await.expect("Failed to connect to test database");
        Arc::new(pool)
    }

    /// Insert a minimal user row to satisfy the FK from room_sticky_events.user_id.
    async fn ensure_test_user(pool: &PgPool, user_id: &str) {
        let now = current_timestamp_millis();
        let username = user_id.strip_prefix('@').and_then(|u| u.split(':').next()).unwrap_or("testuser");
        sqlx::query(
            r#"INSERT INTO users (user_id, username, created_ts)
               VALUES ($1, $2, $3)
               ON CONFLICT (user_id) DO NOTHING"#,
        )
        .bind(user_id)
        .bind(username)
        .bind(now)
        .execute(pool)
        .await
        .expect("failed to create test user");
    }

    /// Insert a minimal room row to satisfy the FK from room_sticky_events.room_id.
    async fn ensure_test_room(pool: &PgPool, room_id: &str) {
        let now = current_timestamp_millis();
        sqlx::query(
            r#"INSERT INTO rooms (room_id, created_ts)
               VALUES ($1, $2)
               ON CONFLICT (room_id) DO NOTHING"#,
        )
        .bind(room_id)
        .bind(now)
        .execute(pool)
        .await
        .expect("failed to create test room");
    }

    /// Delete sticky events for a given room+user combo (idempotent cleanup).
    async fn cleanup_sticky(pool: &PgPool, room_id: &str, user_id: &str) {
        sqlx::query("DELETE FROM room_sticky_events WHERE room_id = $1 AND user_id = $2")
            .bind(room_id)
            .bind(user_id)
            .execute(pool)
            .await
            .expect("failed to cleanup sticky events");
    }

    // --- Tests ---

    #[tokio::test]
    async fn test_set_and_get_sticky_event() {
        let pool = test_pool().await;
        let storage = StickyEventStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let room_id = &format!("!room_get_{suffix}:test.com");
        let user_id = &format!("@user_get_{suffix}:test.com");
        let event_id = &format!("$event_get_{suffix}:test.com");
        let event_type = "m.room.message";

        // Cleanup at start (handles leftover from previous aborted run)
        cleanup_sticky(&pool, room_id, user_id).await;
        ensure_test_user(&pool, user_id).await;
        ensure_test_room(&pool, room_id).await;

        storage.set_is_sticky_event(room_id, user_id, event_id, event_type, true).await.expect("set should succeed");

        let found = storage
            .get_is_sticky_event(room_id, user_id, event_type)
            .await
            .expect("get should succeed")
            .expect("sticky event should be found");

        assert_eq!(found.room_id, *room_id);
        assert_eq!(found.user_id, *user_id);
        assert_eq!(found.event_id, *event_id);
        assert_eq!(found.event_type, event_type);
        assert!(found.is_sticky);
        assert!(found.created_ts > 0);
        assert!(found.updated_ts > 0);

        // Cleanup at end
        cleanup_sticky(&pool, room_id, user_id).await;
    }

    #[tokio::test]
    async fn test_get_sticky_event_not_found() {
        let pool = test_pool().await;
        let storage = StickyEventStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let room_id = &format!("!room_nf_{suffix}:test.com");
        let user_id = &format!("@user_nf_{suffix}:test.com");

        ensure_test_user(&pool, user_id).await;
        ensure_test_room(&pool, room_id).await;

        let result = storage.get_is_sticky_event(room_id, user_id, "m.room.topic").await.expect("query should succeed");

        assert!(result.is_none(), "non-existent event should return None");

        cleanup_sticky(&pool, room_id, user_id).await;
    }

    #[tokio::test]
    async fn test_update_sticky_event() {
        let pool = test_pool().await;
        let storage = StickyEventStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let room_id = &format!("!room_upd_{suffix}:test.com");
        let user_id = &format!("@user_upd_{suffix}:test.com");
        let event_id_a = &format!("$event_a_{suffix}:test.com");
        let event_id_b = &format!("$event_b_{suffix}:test.com");
        let event_type = "m.room.message";

        cleanup_sticky(&pool, room_id, user_id).await;
        ensure_test_user(&pool, user_id).await;
        ensure_test_room(&pool, room_id).await;

        // Set initial sticky event
        storage
            .set_is_sticky_event(room_id, user_id, event_id_a, event_type, true)
            .await
            .expect("first set should succeed");

        // Update with a different event_id (same room/user/type triggers ON CONFLICT DO UPDATE)
        storage
            .set_is_sticky_event(room_id, user_id, event_id_b, event_type, true)
            .await
            .expect("second set should succeed");

        let found = storage
            .get_is_sticky_event(room_id, user_id, event_type)
            .await
            .expect("get should succeed")
            .expect("updated sticky event should be found");

        assert_eq!(found.event_id, *event_id_b, "event_id should be updated to the new value");

        cleanup_sticky(&pool, room_id, user_id).await;
    }

    #[tokio::test]
    async fn test_clear_sticky_event() {
        let pool = test_pool().await;
        let storage = StickyEventStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let room_id = &format!("!room_clr_{suffix}:test.com");
        let user_id = &format!("@user_clr_{suffix}:test.com");
        let event_id = &format!("$event_clr_{suffix}:test.com");
        let event_type = "m.room.message";

        cleanup_sticky(&pool, room_id, user_id).await;
        ensure_test_user(&pool, user_id).await;
        ensure_test_room(&pool, room_id).await;

        // Set and verify
        storage.set_is_sticky_event(room_id, user_id, event_id, event_type, true).await.expect("set should succeed");
        assert!(storage.get_is_sticky_event(room_id, user_id, event_type).await.unwrap().is_some());

        // Clear it
        storage.clear_is_sticky_event(room_id, user_id, event_type).await.expect("clear should succeed");

        // Should no longer be returned (is_sticky = false)
        let result = storage.get_is_sticky_event(room_id, user_id, event_type).await.expect("query should succeed");
        assert!(result.is_none(), "cleared event should not be returned");

        cleanup_sticky(&pool, room_id, user_id).await;
    }

    #[tokio::test]
    async fn test_get_all_sticky_events() {
        let pool = test_pool().await;
        let storage = StickyEventStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let room_id = &format!("!room_all_{suffix}:test.com");
        let user_id = &format!("@user_all_{suffix}:test.com");
        let event_id1 = &format!("$ev1_{suffix}:test.com");
        let event_id2 = &format!("$ev2_{suffix}:test.com");

        cleanup_sticky(&pool, room_id, user_id).await;
        ensure_test_user(&pool, user_id).await;
        ensure_test_room(&pool, room_id).await;

        storage
            .set_is_sticky_event(room_id, user_id, event_id1, "m.room.message", true)
            .await
            .expect("set 1 should succeed");
        storage
            .set_is_sticky_event(room_id, user_id, event_id2, "m.room.topic", true)
            .await
            .expect("set 2 should succeed");

        let all = storage.get_all_is_sticky_events(room_id, user_id).await.expect("get_all should succeed");

        assert_eq!(all.len(), 2, "should return both sticky events");
        assert!(all.iter().any(|e| e.event_type == "m.room.message"));
        assert!(all.iter().any(|e| e.event_type == "m.room.topic"));
        assert!(all.iter().all(|e| e.is_sticky));

        cleanup_sticky(&pool, room_id, user_id).await;
    }

    #[tokio::test]
    async fn test_get_rooms_with_sticky_events() {
        let pool = test_pool().await;
        let storage = StickyEventStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let user_id = &format!("@user_rooms_{suffix}:test.com");
        let room_a = &format!("!room_a_{suffix}:test.com");
        let room_b = &format!("!room_b_{suffix}:test.com");
        let event_id = &format!("$ev_rooms_{suffix}:test.com");

        ensure_test_user(&pool, user_id).await;
        ensure_test_room(&pool, room_a).await;
        ensure_test_room(&pool, room_b).await;

        // Cleanup both room+user combos
        cleanup_sticky(&pool, room_a, user_id).await;
        cleanup_sticky(&pool, room_b, user_id).await;

        storage
            .set_is_sticky_event(room_a, user_id, event_id, "m.room.message", true)
            .await
            .expect("set room_a should succeed");
        storage
            .set_is_sticky_event(room_b, user_id, event_id, "m.room.topic", true)
            .await
            .expect("set room_b should succeed");

        let rooms = storage.get_rooms_with_is_sticky_events(user_id).await.expect("get_rooms should succeed");

        assert_eq!(rooms.len(), 2, "should return both rooms");
        assert!(rooms.contains(&room_a.to_string()));
        assert!(rooms.contains(&room_b.to_string()));

        cleanup_sticky(&pool, room_a, user_id).await;
        cleanup_sticky(&pool, room_b, user_id).await;
    }

    #[tokio::test]
    async fn test_sticky_event_round_trip() {
        let pool = test_pool().await;
        let storage = StickyEventStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let room_id = &format!("!room_rt_{suffix}:test.com");
        let user_id = &format!("@user_rt_{suffix}:test.com");
        let event_id = &format!("$ev_rt_{suffix}:test.com");
        let event_type = "m.room.message";

        cleanup_sticky(&pool, room_id, user_id).await;
        ensure_test_user(&pool, user_id).await;
        ensure_test_room(&pool, room_id).await;

        // Set -> Get -> Clear -> Get (should be gone) -> Set again -> Get (should be back)
        storage.set_is_sticky_event(room_id, user_id, event_id, event_type, true).await.expect("set 1 should succeed");

        let found = storage
            .get_is_sticky_event(room_id, user_id, event_type)
            .await
            .unwrap()
            .expect("should be found after set 1");
        assert_eq!(found.event_id, *event_id);

        storage.clear_is_sticky_event(room_id, user_id, event_type).await.expect("clear should succeed");

        let after_clear = storage.get_is_sticky_event(room_id, user_id, event_type).await.unwrap();
        assert!(after_clear.is_none(), "should be gone after clear");

        // Re-set (same room/user/type — row still exists with is_sticky=false, ON CONFLICT DO UPDATE flips it back)
        storage.set_is_sticky_event(room_id, user_id, event_id, event_type, true).await.expect("set 2 should succeed");

        let found_again = storage
            .get_is_sticky_event(room_id, user_id, event_type)
            .await
            .unwrap()
            .expect("should be found after set 2");
        assert_eq!(found_again.event_id, *event_id);
        assert!(found_again.is_sticky);

        cleanup_sticky(&pool, room_id, user_id).await;
    }
}
