use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RoomRetentionPolicy {
    pub id: i64,
    pub room_id: String,
    pub max_lifetime: Option<i64>,
    pub min_lifetime: i64,
    pub is_expire_on_clients: bool,
    pub is_server_default: bool,
    pub created_ts: i64,
    pub updated_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ServerRetentionPolicy {
    pub id: i64,
    pub max_lifetime: Option<i64>,
    pub min_lifetime: i64,
    pub is_expire_on_clients: bool,
    pub created_ts: i64,
    pub updated_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RetentionCleanupQueueItem {
    pub id: i64,
    pub room_id: String,
    pub event_id: Option<String>,
    pub event_type: Option<String>,
    pub origin_server_ts: i64,
    pub scheduled_ts: i64,
    pub status: String,
    pub created_ts: i64,
    pub processed_ts: Option<i64>,
    pub error_message: Option<String>,
    pub retry_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RetentionCleanupLog {
    pub id: i64,
    pub room_id: String,
    pub events_deleted: i64,
    pub state_events_deleted: i64,
    pub media_deleted: i64,
    pub bytes_freed: i64,
    pub started_ts: i64,
    pub completed_ts: Option<i64>,
    pub status: String,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct DeletedEventIndex {
    pub id: i64,
    pub room_id: String,
    pub event_id: String,
    pub deletion_ts: i64,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RetentionStats {
    pub id: i64,
    pub room_id: String,
    pub total_events: i64,
    pub events_in_retention: i64,
    pub events_expired: i64,
    pub last_cleanup_ts: Option<i64>,
    pub next_cleanup_ts: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRoomRetentionPolicyRequest {
    pub room_id: String,
    pub max_lifetime: Option<i64>,
    pub min_lifetime: Option<i64>,
    pub is_expire_on_clients: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateRoomRetentionPolicyRequest {
    pub max_lifetime: Option<i64>,
    pub min_lifetime: Option<i64>,
    pub is_expire_on_clients: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateServerRetentionPolicyRequest {
    pub max_lifetime: Option<i64>,
    pub min_lifetime: Option<i64>,
    pub is_expire_on_clients: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectiveRetentionPolicy {
    pub max_lifetime: Option<i64>,
    pub min_lifetime: i64,
    pub is_expire_on_clients: bool,
}

#[derive(Clone)]
pub struct RetentionStorage {
    pool: Arc<PgPool>,
}

impl RetentionStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create_room_policy(
        &self,
        request: CreateRoomRetentionPolicyRequest,
    ) -> Result<RoomRetentionPolicy, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let row = sqlx::query_as::<_, RoomRetentionPolicy>(
            r"
            INSERT INTO room_retention_policies (
                room_id, max_lifetime, min_lifetime, is_expire_on_clients, is_server_default, created_ts, updated_ts
            )
            VALUES ($1, $2, $3, $4, FALSE, $5, $5)
            ON CONFLICT (room_id) DO UPDATE SET
                max_lifetime = EXCLUDED.max_lifetime,
                min_lifetime = EXCLUDED.min_lifetime,
                is_expire_on_clients = EXCLUDED.is_expire_on_clients,
                updated_ts = EXCLUDED.updated_ts
            RETURNING id, room_id, max_lifetime, min_lifetime, is_expire_on_clients, is_server_default, created_ts, updated_ts
            ",
        )
        .bind(&request.room_id)
        .bind(request.max_lifetime)
        .bind(request.min_lifetime.unwrap_or(0))
        .bind(request.is_expire_on_clients.unwrap_or(false))
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_room_policy(&self, room_id: &str) -> Result<Option<RoomRetentionPolicy>, sqlx::Error> {
        let row = sqlx::query_as::<_, RoomRetentionPolicy>(
            "SELECT id, room_id, max_lifetime, min_lifetime, is_expire_on_clients, is_server_default, created_ts, updated_ts FROM room_retention_policies WHERE room_id = $1",
        )
        .bind(room_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn update_room_policy(
        &self,
        room_id: &str,
        request: UpdateRoomRetentionPolicyRequest,
    ) -> Result<RoomRetentionPolicy, sqlx::Error> {
        let row = sqlx::query_as::<_, RoomRetentionPolicy>(
            r"
            UPDATE room_retention_policies SET
                max_lifetime = COALESCE($2, max_lifetime),
                min_lifetime = COALESCE($3, min_lifetime),
                is_expire_on_clients = COALESCE($4, is_expire_on_clients)
            WHERE room_id = $1
            RETURNING *
            ",
        )
        .bind(room_id)
        .bind(request.max_lifetime)
        .bind(request.min_lifetime)
        .bind(request.is_expire_on_clients)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn delete_room_policy(&self, room_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM room_retention_policies WHERE room_id = $1")
            .bind(room_id)
            .execute(&*self.pool)
            .await?;

        Ok(())
    }

    pub async fn get_server_policy(&self) -> Result<ServerRetentionPolicy, sqlx::Error> {
        let row = sqlx::query_as::<_, ServerRetentionPolicy>(
            "SELECT id, max_lifetime, min_lifetime, is_expire_on_clients, created_ts, updated_ts FROM server_retention_policy ORDER BY id LIMIT 1",
        )
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn update_server_policy(
        &self,
        request: UpdateServerRetentionPolicyRequest,
    ) -> Result<ServerRetentionPolicy, sqlx::Error> {
        let row = sqlx::query_as::<_, ServerRetentionPolicy>(
            r"
            UPDATE server_retention_policy SET
                max_lifetime = COALESCE($1, max_lifetime),
                min_lifetime = COALESCE($2, min_lifetime),
                is_expire_on_clients = COALESCE($3, is_expire_on_clients)
            WHERE id = (SELECT MIN(id) FROM server_retention_policy)
            RETURNING id, max_lifetime, min_lifetime, is_expire_on_clients, created_ts, updated_ts
            ",
        )
        .bind(request.max_lifetime)
        .bind(request.min_lifetime)
        .bind(request.is_expire_on_clients)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_effective_policy(&self, room_id: &str) -> Result<EffectiveRetentionPolicy, sqlx::Error> {
        let room_policy = self.get_room_policy(room_id).await?;
        let server_policy = self.get_server_policy().await?;

        Ok(EffectiveRetentionPolicy {
            max_lifetime: room_policy.as_ref().and_then(|p| p.max_lifetime).or(server_policy.max_lifetime),
            min_lifetime: room_policy.as_ref().map_or(server_policy.min_lifetime, |p| p.min_lifetime),
            is_expire_on_clients: room_policy
                .as_ref()
                .map_or(server_policy.is_expire_on_clients, |p| p.is_expire_on_clients),
        })
    }

    pub async fn delete_events_before(&self, room_id: &str, cutoff_ts: i64) -> Result<i64, sqlx::Error> {
        let result = sqlx::query(
            r"
            DELETE FROM events
            WHERE room_id = $1
            AND origin_server_ts < $2
            AND event_type NOT IN ('m.room.create', 'm.room.power_levels', 'm.room.join_rules', 'm.room.history_visibility')
            AND state_key IS NULL
            ",
        )
        .bind(room_id)
        .bind(cutoff_ts)
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected() as i64)
    }

    pub async fn get_rooms_with_policies(&self) -> Result<Vec<RoomRetentionPolicy>, sqlx::Error> {
        let rows = sqlx::query_as::<_, RoomRetentionPolicy>(
            "SELECT id, room_id, max_lifetime, min_lifetime, is_expire_on_clients, is_server_default, created_ts, updated_ts FROM room_retention_policies ORDER BY room_id",
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn get_server_policy_optional(&self) -> Result<Option<ServerRetentionPolicy>, sqlx::Error> {
        let row = sqlx::query_as::<_, ServerRetentionPolicy>(
            r"SELECT id, max_lifetime, min_lifetime, is_expire_on_clients, created_ts, updated_ts
               FROM server_retention_policy ORDER BY id LIMIT 1",
        )
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn upsert_server_policy(
        &self,
        request: UpdateServerRetentionPolicyRequest,
    ) -> Result<ServerRetentionPolicy, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        let row = sqlx::query_as::<_, ServerRetentionPolicy>(
            r#"
            INSERT INTO server_retention_policy (
                id, max_lifetime, min_lifetime, is_expire_on_clients, created_ts, updated_ts
            )
            VALUES ($1, $2, $3, $4, $5, $5)
            ON CONFLICT (id) DO UPDATE SET
                max_lifetime = EXCLUDED.max_lifetime,
                min_lifetime = EXCLUDED.min_lifetime,
                is_expire_on_clients = EXCLUDED.is_expire_on_clients,
                updated_ts = EXCLUDED.updated_ts
            RETURNING id, max_lifetime, min_lifetime, is_expire_on_clients, created_ts, updated_ts
            "#,
        )
        .bind(1_i64)
        .bind(request.max_lifetime)
        .bind(request.min_lifetime.unwrap_or(0))
        .bind(request.is_expire_on_clients.unwrap_or(false))
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn count_room_policies(&self) -> Result<i64, sqlx::Error> {
        let count =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM room_retention_policies").fetch_one(&*self.pool).await?;

        Ok(count)
    }

    pub async fn has_server_policy(&self) -> Result<bool, sqlx::Error> {
        let exists = sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM server_retention_policy)")
            .fetch_one(&*self.pool)
            .await?;

        Ok(exists)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_room_retention_policy_creation() {
        let policy = RoomRetentionPolicy {
            id: 1,
            room_id: "!room:example.com".to_string(),
            max_lifetime: Some(86_400_000),
            min_lifetime: 0,
            is_expire_on_clients: true,
            is_server_default: false,
            created_ts: 1234567890,
            updated_ts: 1234567890,
        };
        assert_eq!(policy.room_id, "!room:example.com");
        assert!(policy.max_lifetime.is_some());
    }

    #[test]
    fn test_room_retention_policy_max_lifetime_none() {
        let policy = RoomRetentionPolicy {
            id: 2,
            room_id: "!room2:example.com".to_string(),
            max_lifetime: None,
            min_lifetime: 0,
            is_expire_on_clients: false,
            is_server_default: false,
            created_ts: 1234567890,
            updated_ts: 1234567890,
        };
        assert!(policy.max_lifetime.is_none());
    }

    #[test]
    fn test_server_retention_policy_defaults() {
        let policy = ServerRetentionPolicy {
            id: 1,
            max_lifetime: Some(2592000000),
            min_lifetime: 0,
            is_expire_on_clients: true,
            created_ts: 1234567890,
            updated_ts: 1234567890,
        };
        assert!(policy.max_lifetime.is_some());
    }

    #[test]
    fn test_retention_cleanup_log() {
        let log = RetentionCleanupLog {
            id: 1,
            room_id: "!room:example.com".to_string(),
            events_deleted: 100,
            state_events_deleted: 10,
            media_deleted: 50,
            bytes_freed: 1048576,
            started_ts: 1234567890,
            completed_ts: Some(1234567990),
            status: "completed".to_string(),
            error_message: None,
        };
        assert_eq!(log.events_deleted, 100);
        assert!(log.completed_ts.is_some());
    }

    #[test]
    fn test_create_room_retention_policy_request() {
        let request = CreateRoomRetentionPolicyRequest {
            room_id: "!room:example.com".to_string(),
            max_lifetime: Some(86_400_000),
            min_lifetime: Some(0),
            is_expire_on_clients: Some(true),
        };
        assert_eq!(request.room_id, "!room:example.com");
    }

    #[test]
    fn test_effective_retention_policy() {
        let policy =
            EffectiveRetentionPolicy { max_lifetime: Some(86_400_000), min_lifetime: 0, is_expire_on_clients: true };
        assert_eq!(policy.max_lifetime, Some(86_400_000));
        assert!(policy.is_expire_on_clients);
    }

    #[test]
    fn test_effective_retention_policy_no_max_lifetime() {
        let policy = EffectiveRetentionPolicy { max_lifetime: None, min_lifetime: 0, is_expire_on_clients: false };
        assert!(policy.max_lifetime.is_none());
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use sqlx::PgPool;
    use sqlx::postgres::PgPoolOptions;

    async fn test_pool() -> Arc<PgPool> {
        let db_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&db_url)
            .await
            .expect("Failed to connect to test database");
        Arc::new(pool)
    }

    /// Insert a minimal room row to satisfy `room_retention_policies.room_id` FK →
    /// `rooms(room_id)` and `events.room_id` FK → `rooms(room_id)`.
    async fn ensure_test_room(pool: &PgPool, room_id: &str) {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            r#"INSERT INTO rooms (room_id, creator, created_ts)
               VALUES ($1, $2, $3)
               ON CONFLICT (room_id) DO NOTHING"#,
        )
        .bind(room_id)
        .bind("test_creator")
        .bind(now)
        .execute(pool)
        .await
        .expect("failed to create test room");
    }

    /// Insert a minimal event row for testing `delete_events_before`.
    async fn ensure_test_event(
        pool: &PgPool,
        event_id: &str,
        room_id: &str,
        sender: &str,
        origin_server_ts: i64,
    ) {
        sqlx::query(
            r#"INSERT INTO events (event_id, room_id, sender, event_type, content, origin_server_ts)
               VALUES ($1, $2, $3, 'm.room.message', '{}'::jsonb, $4)
               ON CONFLICT (event_id) DO NOTHING"#,
        )
        .bind(event_id)
        .bind(room_id)
        .bind(sender)
        .bind(origin_server_ts)
        .execute(pool)
        .await
        .expect("failed to create test event");
    }

    /// Remove a test room and all dependent rows (events, retention policies).
    async fn cleanup_room(pool: &PgPool, room_id: &str) {
        let _ = sqlx::query("DELETE FROM room_retention_policies WHERE room_id = $1")
            .bind(room_id)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM events WHERE room_id = $1")
            .bind(room_id)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM rooms WHERE room_id = $1")
            .bind(room_id)
            .execute(pool)
            .await;
    }

    // ------------------------------------------------------------------
    // 1. Create room retention policy
    // ------------------------------------------------------------------
    #[tokio::test]
    async fn test_create_room_policy() {
        let pool = test_pool().await;
        let storage = RetentionStorage::new(&pool);
        let room_id = &format!("!ret_create_{}:test.com", uuid::Uuid::new_v4());

        cleanup_room(&pool, room_id).await;
        ensure_test_room(&pool, room_id).await;

        let request = CreateRoomRetentionPolicyRequest {
            room_id: room_id.clone(),
            max_lifetime: Some(86_400_000),
            min_lifetime: Some(7_200_000),
            is_expire_on_clients: Some(true),
        };

        let policy = storage
            .create_room_policy(request)
            .await
            .expect("create_room_policy should succeed");

        assert!(policy.id > 0);
        assert_eq!(policy.room_id, *room_id);
        assert_eq!(policy.max_lifetime, Some(86_400_000));
        assert_eq!(policy.min_lifetime, 7_200_000);
        assert!(policy.is_expire_on_clients);
        assert!(!policy.is_server_default);
        assert!(policy.created_ts > 0);
        assert_eq!(policy.updated_ts, policy.created_ts);

        cleanup_room(&pool, room_id).await;
    }

    // ------------------------------------------------------------------
    // 2. Get room policy (found and not-found)
    // ------------------------------------------------------------------
    #[tokio::test]
    async fn test_get_room_policy_found() {
        let pool = test_pool().await;
        let storage = RetentionStorage::new(&pool);
        let room_id = &format!("!ret_get_{}:test.com", uuid::Uuid::new_v4());

        cleanup_room(&pool, room_id).await;
        ensure_test_room(&pool, room_id).await;

        storage
            .create_room_policy(CreateRoomRetentionPolicyRequest {
                room_id: room_id.clone(),
                max_lifetime: Some(3_600_000),
                min_lifetime: None,
                is_expire_on_clients: None,
            })
            .await
            .unwrap();

        let found = storage
            .get_room_policy(room_id)
            .await
            .expect("get_room_policy should succeed")
            .expect("policy should exist");

        assert_eq!(found.room_id, *room_id);
        assert_eq!(found.max_lifetime, Some(3_600_000));

        cleanup_room(&pool, room_id).await;
    }

    #[tokio::test]
    async fn test_get_room_policy_not_found() {
        let pool = test_pool().await;
        let storage = RetentionStorage::new(&pool);
        let room_id = &format!("!ret_nonexist_{}:test.com", uuid::Uuid::new_v4());

        let result = storage
            .get_room_policy(room_id)
            .await
            .expect("get_room_policy should succeed");

        assert!(result.is_none(), "non-existent room should return None");
    }

    // ------------------------------------------------------------------
    // 3. Update room policy
    // ------------------------------------------------------------------
    #[tokio::test]
    async fn test_update_room_policy() {
        let pool = test_pool().await;
        let storage = RetentionStorage::new(&pool);
        let room_id = &format!("!ret_update_{}:test.com", uuid::Uuid::new_v4());

        cleanup_room(&pool, room_id).await;
        ensure_test_room(&pool, room_id).await;

        storage
            .create_room_policy(CreateRoomRetentionPolicyRequest {
                room_id: room_id.clone(),
                max_lifetime: Some(86_400_000),
                min_lifetime: Some(0),
                is_expire_on_clients: Some(false),
            })
            .await
            .unwrap();

        // Update max_lifetime and is_expire_on_clients; leave min_lifetime unchanged (None)
        let updated = storage
            .update_room_policy(
                room_id,
                UpdateRoomRetentionPolicyRequest {
                    max_lifetime: Some(43_200_000),
                    min_lifetime: None,
                    is_expire_on_clients: Some(true),
                },
            )
            .await
            .expect("update_room_policy should succeed");

        assert_eq!(updated.max_lifetime, Some(43_200_000));
        assert_eq!(updated.min_lifetime, 0); // unchanged — COALESCE(NULL, min_lifetime)
        assert!(updated.is_expire_on_clients);

        // Cross-check via get
        let fetched = storage.get_room_policy(room_id).await.unwrap().unwrap();
        assert_eq!(fetched.max_lifetime, Some(43_200_000));
        assert!(fetched.is_expire_on_clients);

        cleanup_room(&pool, room_id).await;
    }

    // ------------------------------------------------------------------
    // 4. Delete room policy
    // ------------------------------------------------------------------
    #[tokio::test]
    async fn test_delete_room_policy() {
        let pool = test_pool().await;
        let storage = RetentionStorage::new(&pool);
        let room_id = &format!("!ret_delete_{}:test.com", uuid::Uuid::new_v4());

        cleanup_room(&pool, room_id).await;
        ensure_test_room(&pool, room_id).await;

        storage
            .create_room_policy(CreateRoomRetentionPolicyRequest {
                room_id: room_id.clone(),
                max_lifetime: Some(86_400_000),
                min_lifetime: None,
                is_expire_on_clients: None,
            })
            .await
            .unwrap();

        assert!(storage.get_room_policy(room_id).await.unwrap().is_some());

        storage.delete_room_policy(room_id).await.expect("delete should succeed");

        assert!(
            storage.get_room_policy(room_id).await.unwrap().is_none(),
            "policy should be gone after delete"
        );

        cleanup_room(&pool, room_id).await;
    }

    // ------------------------------------------------------------------
    // 5. List all rooms with policies (batch)
    // ------------------------------------------------------------------
    #[tokio::test]
    async fn test_get_rooms_with_policies() {
        let pool = test_pool().await;
        let storage = RetentionStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_a = &format!("!ret_batch_a_{}:test.com", suffix);
        let room_b = &format!("!ret_batch_b_{}:test.com", suffix);

        cleanup_room(&pool, room_a).await;
        cleanup_room(&pool, room_b).await;
        ensure_test_room(&pool, room_a).await;
        ensure_test_room(&pool, room_b).await;

        storage
            .create_room_policy(CreateRoomRetentionPolicyRequest {
                room_id: room_a.clone(),
                max_lifetime: Some(10_000_000),
                min_lifetime: None,
                is_expire_on_clients: None,
            })
            .await
            .unwrap();
        storage
            .create_room_policy(CreateRoomRetentionPolicyRequest {
                room_id: room_b.clone(),
                max_lifetime: Some(20_000_000),
                min_lifetime: None,
                is_expire_on_clients: None,
            })
            .await
            .unwrap();

        let policies = storage
            .get_rooms_with_policies()
            .await
            .expect("get_rooms_with_policies should succeed");

        let room_ids: Vec<_> = policies.iter().map(|p| p.room_id.as_str()).collect();
        assert!(room_ids.contains(&room_a.as_str()), "should contain room A");
        assert!(room_ids.contains(&room_b.as_str()), "should contain room B");

        cleanup_room(&pool, room_a).await;
        cleanup_room(&pool, room_b).await;
    }

    // ------------------------------------------------------------------
    // 6. Effective policy — room overrides server
    // ------------------------------------------------------------------
    #[tokio::test]
    async fn test_effective_policy_favors_room_over_server() {
        let pool = test_pool().await;
        let storage = RetentionStorage::new(&pool);
        let room_id = &format!("!ret_eff_{}:test.com", uuid::Uuid::new_v4());

        cleanup_room(&pool, room_id).await;
        ensure_test_room(&pool, room_id).await;

        // Seed server policy via upsert
        storage
            .upsert_server_policy(UpdateServerRetentionPolicyRequest {
                max_lifetime: Some(100_000_000),
                min_lifetime: Some(5_000),
                is_expire_on_clients: Some(false),
            })
            .await
            .unwrap();

        // Create room policy with conflicting values
        storage
            .create_room_policy(CreateRoomRetentionPolicyRequest {
                room_id: room_id.clone(),
                max_lifetime: Some(50_000_000),
                min_lifetime: Some(60_000),
                is_expire_on_clients: Some(true),
            })
            .await
            .unwrap();

        let effective = storage
            .get_effective_policy(room_id)
            .await
            .expect("get_effective_policy should succeed");

        // Room values take precedence
        assert_eq!(effective.max_lifetime, Some(50_000_000));
        assert_eq!(effective.min_lifetime, 60_000);
        assert!(effective.is_expire_on_clients);

        cleanup_room(&pool, room_id).await;
    }

    // ------------------------------------------------------------------
    // 7. Upsert server policy + has_server_policy
    // ------------------------------------------------------------------
    #[tokio::test]
    async fn test_upsert_and_has_server_policy() {
        let pool = test_pool().await;
        let storage = RetentionStorage::new(&pool);

        // Server policy always exists (seeded by migration with ON CONFLICT DO NOTHING)
        assert!(
            storage.has_server_policy().await.expect("has_server_policy should succeed"),
            "server_retention_policy should have the default row"
        );

        let policy = storage
            .upsert_server_policy(UpdateServerRetentionPolicyRequest {
                max_lifetime: Some(259_200_000),
                min_lifetime: Some(86_400_000),
                is_expire_on_clients: Some(true),
            })
            .await
            .expect("upsert_server_policy should succeed");

        assert_eq!(policy.id, 1);
        assert_eq!(policy.max_lifetime, Some(259_200_000));
        assert_eq!(policy.min_lifetime, 86_400_000);
        assert!(policy.is_expire_on_clients);

        let server = storage
            .get_server_policy_optional()
            .await
            .unwrap()
            .expect("default server policy should exist after upsert");
        assert_eq!(server.id, 1);
    }

    // ------------------------------------------------------------------
    // 8. Count room policies
    // ------------------------------------------------------------------
    #[tokio::test]
    async fn test_count_room_policies() {
        let pool = test_pool().await;
        let storage = RetentionStorage::new(&pool);
        let room_id = &format!("!ret_count_{}:test.com", uuid::Uuid::new_v4());

        cleanup_room(&pool, room_id).await;
        ensure_test_room(&pool, room_id).await;

        storage
            .create_room_policy(CreateRoomRetentionPolicyRequest {
                room_id: room_id.clone(),
                max_lifetime: None,
                min_lifetime: None,
                is_expire_on_clients: None,
            })
            .await
            .unwrap();

        let count = storage
            .count_room_policies()
            .await
            .expect("count_room_policies should succeed");

        assert!(count >= 1, "should count at least our newly-inserted policy");

        cleanup_room(&pool, room_id).await;
    }

    // ------------------------------------------------------------------
    // 9. Delete events before cutoff (purge)
    // ------------------------------------------------------------------
    #[tokio::test]
    async fn test_delete_events_before() {
        let pool = test_pool().await;
        let storage = RetentionStorage::new(&pool);
        let room_id = &format!("!ret_delev_{}:test.com", uuid::Uuid::new_v4());

        cleanup_room(&pool, room_id).await;
        ensure_test_room(&pool, room_id).await;

        let old_ts = chrono::Utc::now().timestamp_millis() - 86_400_000; // 1 day ago
        let recent_ts = chrono::Utc::now().timestamp_millis() - 3_600_000; // 1 hour ago

        ensure_test_event(
            &pool,
            &format!("evt_old_{}", uuid::Uuid::new_v4()),
            room_id,
            "@sender:test.com",
            old_ts,
        )
        .await;
        ensure_test_event(
            &pool,
            &format!("evt_recent_{}", uuid::Uuid::new_v4()),
            room_id,
            "@sender:test.com",
            recent_ts,
        )
        .await;

        // Cutoff at 12 hours ago — only the 1-day-old event is before it
        let cutoff = chrono::Utc::now().timestamp_millis() - 43_200_000;
        let deleted = storage
            .delete_events_before(room_id, cutoff)
            .await
            .expect("delete_events_before should succeed");

        assert!(deleted >= 1, "should delete at least the old event");

        cleanup_room(&pool, room_id).await;
    }

    // ------------------------------------------------------------------
    // 10. Round-trip: create → get → update → get → delete → verify gone
    // ------------------------------------------------------------------
    #[tokio::test]
    async fn test_round_trip_create_get_update_delete() {
        let pool = test_pool().await;
        let storage = RetentionStorage::new(&pool);
        let room_id = &format!("!ret_roundtrip_{}:test.com", uuid::Uuid::new_v4());

        cleanup_room(&pool, room_id).await;
        ensure_test_room(&pool, room_id).await;

        // Create
        let created = storage
            .create_room_policy(CreateRoomRetentionPolicyRequest {
                room_id: room_id.clone(),
                max_lifetime: Some(7_200_000),
                min_lifetime: Some(60_000),
                is_expire_on_clients: Some(false),
            })
            .await
            .unwrap();
        assert!(created.id > 0);

        // Get
        let got = storage.get_room_policy(room_id).await.unwrap().unwrap();
        assert_eq!(got.max_lifetime, Some(7_200_000));

        // Update
        let updated = storage
            .update_room_policy(
                room_id,
                UpdateRoomRetentionPolicyRequest {
                    max_lifetime: Some(14_400_000),
                    min_lifetime: None,
                    is_expire_on_clients: Some(true),
                },
            )
            .await
            .unwrap();
        assert_eq!(updated.max_lifetime, Some(14_400_000));
        assert!(updated.is_expire_on_clients);

        // Get again
        let got_again = storage.get_room_policy(room_id).await.unwrap().unwrap();
        assert_eq!(got_again.max_lifetime, Some(14_400_000));
        assert!(got_again.is_expire_on_clients);

        // Delete
        storage.delete_room_policy(room_id).await.unwrap();
        assert!(storage.get_room_policy(room_id).await.unwrap().is_none());

        cleanup_room(&pool, room_id).await;
    }
}
