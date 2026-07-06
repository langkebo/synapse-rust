// Invite Blocking Storage - MSC4380
// Allows room admins to control who can be invited to a room
// Following project field naming standards

use async_trait::async_trait;
use sqlx::PgPool;
use std::sync::Arc;

#[async_trait]
pub trait InviteBlocklistStoreApi: Send + Sync {
    async fn set_invite_blocklist(&self, room_id: &str, user_ids: Vec<String>) -> Result<(), sqlx::Error>;
    async fn get_invite_blocklist(&self, room_id: &str) -> Result<Vec<String>, sqlx::Error>;
    async fn is_user_blocked(&self, room_id: &str, user_id: &str) -> Result<bool, sqlx::Error>;
    async fn set_invite_allowlist(&self, room_id: &str, user_ids: Vec<String>) -> Result<(), sqlx::Error>;
    async fn get_invite_allowlist(&self, room_id: &str) -> Result<Vec<String>, sqlx::Error>;
    async fn is_user_allowed(&self, room_id: &str, user_id: &str) -> Result<bool, sqlx::Error>;
    async fn has_any_invite_restriction(&self, room_id: &str) -> Result<bool, sqlx::Error>;
    async fn get_global_invite_blocklist(&self) -> Result<Vec<serde_json::Value>, sqlx::Error>;
    async fn get_global_invite_allowlist(&self) -> Result<Vec<serde_json::Value>, sqlx::Error>;
}

#[derive(Clone)]
pub struct InviteBlocklistStorage {
    pool: Arc<PgPool>,
}

impl InviteBlocklistStorage {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }

    /// Set the invite blocklist for a room (users that cannot be invited)
    pub async fn set_invite_blocklist(&self, room_id: &str, user_ids: Vec<String>) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        // Clear existing blocklist
        sqlx::query("DELETE FROM room_invite_blocklist WHERE room_id = $1").bind(room_id).execute(&*self.pool).await?;

        // Insert new blocklist
        if !user_ids.is_empty() {
            sqlx::query(
                r"
                INSERT INTO room_invite_blocklist (room_id, user_id, created_ts)
                SELECT $1, unnest($2::text[]), $3
                ON CONFLICT DO NOTHING
                ",
            )
            .bind(room_id)
            .bind(&user_ids)
            .bind(now)
            .execute(&*self.pool)
            .await?;
        }

        Ok(())
    }

    /// Get the invite blocklist for a room
    pub async fn get_invite_blocklist(&self, room_id: &str) -> Result<Vec<String>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (String,)>(
            r"
            SELECT user_id FROM room_invite_blocklist WHERE room_id = $1
            ",
        )
        .bind(room_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.0).collect())
    }

    /// Check if a user is blocked from being invited
    pub async fn is_user_blocked(&self, room_id: &str, user_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query_as::<_, (String,)>(
            r"
            SELECT user_id FROM room_invite_blocklist
            WHERE room_id = $1 AND user_id = $2
            ",
        )
        .bind(room_id)
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(result.is_some())
    }

    /// Set the invite allowlist for a room (only these users can be invited)
    pub async fn set_invite_allowlist(&self, room_id: &str, user_ids: Vec<String>) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        // Clear existing allowlist
        sqlx::query("DELETE FROM room_invite_allowlist WHERE room_id = $1").bind(room_id).execute(&*self.pool).await?;

        // Insert new allowlist
        if !user_ids.is_empty() {
            sqlx::query(
                r"
                INSERT INTO room_invite_allowlist (room_id, user_id, created_ts)
                SELECT $1, unnest($2::text[]), $3
                ON CONFLICT DO NOTHING
                ",
            )
            .bind(room_id)
            .bind(&user_ids)
            .bind(now)
            .execute(&*self.pool)
            .await?;
        }

        Ok(())
    }

    /// Get the invite allowlist for a room
    pub async fn get_invite_allowlist(&self, room_id: &str) -> Result<Vec<String>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (String,)>(
            r"
            SELECT user_id FROM room_invite_allowlist WHERE room_id = $1
            ",
        )
        .bind(room_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.0).collect())
    }

    /// Check if a user is allowed to be invited (when allowlist is set)
    pub async fn is_user_allowed(&self, room_id: &str, user_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query_as::<_, (String,)>(
            r"
            SELECT user_id FROM room_invite_allowlist
            WHERE room_id = $1 AND user_id = $2
            ",
        )
        .bind(room_id)
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(result.is_some())
    }

    /// Check if invite blocking is enabled for a room
    pub async fn has_any_invite_restriction(&self, room_id: &str) -> Result<bool, sqlx::Error> {
        let blocklist = sqlx::query("SELECT 1 FROM room_invite_blocklist WHERE room_id = $1 LIMIT 1")
            .bind(room_id)
            .fetch_optional(&*self.pool)
            .await?;

        if blocklist.is_some() {
            return Ok(true);
        }

        let allowlist = sqlx::query("SELECT 1 FROM room_invite_allowlist WHERE room_id = $1 LIMIT 1")
            .bind(room_id)
            .fetch_optional(&*self.pool)
            .await?;

        Ok(allowlist.is_some())
    }

    /// Get global invite blocklist (all rooms)
    pub async fn get_global_invite_blocklist(&self) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (String, String, i64)>(
            r"
            SELECT room_id, user_id, created_ts FROM room_invite_blocklist
            ORDER BY created_ts DESC
            ",
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(room_id, user_id, created_ts)| {
                serde_json::json!({
                    "room_id": room_id,
                    "user_id": user_id,
                    "created_ts": created_ts
                })
            })
            .collect())
    }

    /// Get global invite allowlist (all rooms)
    pub async fn get_global_invite_allowlist(&self) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (String, String, i64)>(
            r"
            SELECT room_id, user_id, created_ts FROM room_invite_allowlist
            ORDER BY created_ts DESC
            ",
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(room_id, user_id, created_ts)| {
                serde_json::json!({
                    "room_id": room_id,
                    "user_id": user_id,
                    "created_ts": created_ts
                })
            })
            .collect())
    }
}

#[async_trait]
impl InviteBlocklistStoreApi for InviteBlocklistStorage {
    async fn set_invite_blocklist(&self, room_id: &str, user_ids: Vec<String>) -> Result<(), sqlx::Error> {
        self.set_invite_blocklist(room_id, user_ids).await
    }
    async fn get_invite_blocklist(&self, room_id: &str) -> Result<Vec<String>, sqlx::Error> {
        self.get_invite_blocklist(room_id).await
    }
    async fn is_user_blocked(&self, room_id: &str, user_id: &str) -> Result<bool, sqlx::Error> {
        self.is_user_blocked(room_id, user_id).await
    }
    async fn set_invite_allowlist(&self, room_id: &str, user_ids: Vec<String>) -> Result<(), sqlx::Error> {
        self.set_invite_allowlist(room_id, user_ids).await
    }
    async fn get_invite_allowlist(&self, room_id: &str) -> Result<Vec<String>, sqlx::Error> {
        self.get_invite_allowlist(room_id).await
    }
    async fn is_user_allowed(&self, room_id: &str, user_id: &str) -> Result<bool, sqlx::Error> {
        self.is_user_allowed(room_id, user_id).await
    }
    async fn has_any_invite_restriction(&self, room_id: &str) -> Result<bool, sqlx::Error> {
        self.has_any_invite_restriction(room_id).await
    }
    async fn get_global_invite_blocklist(&self) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        self.get_global_invite_blocklist().await
    }
    async fn get_global_invite_allowlist(&self) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        self.get_global_invite_allowlist().await
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_user_id_format() {
        let valid_users = vec!["@user:localhost", "@alice:example.com"];

        for user in valid_users {
            assert!(user.starts_with('@'), "User ID should start with @");
            assert!(user.contains(':'), "User ID should contain : separator");
        }
    }

    #[test]
    fn test_room_id_format() {
        let valid_rooms = vec!["!room:localhost", "!abc123:matrix.org"];

        for room in valid_rooms {
            assert!(room.starts_with('!'), "Room ID should start with !");
            assert!(room.contains(':'), "Room ID should contain : separator");
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

    /// Insert a minimal room row so the FK constraint on
    /// room_invite_blocklist/room_invite_allowlist.room_id is satisfied.
    async fn ensure_test_room(pool: &PgPool, room_id: &str) {
        let now = chrono::Utc::now().timestamp_millis();
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

    /// Cleanup blocklist rows for a given room (idempotent, skips errors).
    async fn cleanup_blocklist(pool: &PgPool, room_id: &str) {
        sqlx::query("DELETE FROM room_invite_blocklist WHERE room_id = $1").bind(room_id).execute(pool).await.ok();
    }

    /// Cleanup allowlist rows for a given room (idempotent, skips errors).
    async fn cleanup_allowlist(pool: &PgPool, room_id: &str) {
        sqlx::query("DELETE FROM room_invite_allowlist WHERE room_id = $1").bind(room_id).execute(pool).await.ok();
    }

    #[tokio::test]
    async fn test_set_and_get_invite_blocklist() {
        let pool = test_pool().await;
        let storage = InviteBlocklistStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_bl_{suffix}:test.com");
        let user_a = format!("@user_a_{suffix}:test.com");
        let user_b = format!("@user_b_{suffix}:test.com");

        cleanup_blocklist(&pool, &room_id).await;
        ensure_test_room(&pool, &room_id).await;

        storage
            .set_invite_blocklist(&room_id, vec![user_a.clone(), user_b.clone()])
            .await
            .expect("set_invite_blocklist should succeed");

        let blocklist = storage.get_invite_blocklist(&room_id).await.expect("get_invite_blocklist should succeed");

        assert_eq!(blocklist.len(), 2);
        assert!(blocklist.contains(&user_a), "blocklist should contain user_a");
        assert!(blocklist.contains(&user_b), "blocklist should contain user_b");

        cleanup_blocklist(&pool, &room_id).await;
    }

    #[tokio::test]
    async fn test_is_user_blocked_positive_and_negative() {
        let pool = test_pool().await;
        let storage = InviteBlocklistStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_blocked_{suffix}:test.com");
        let blocked_user = format!("@blocked_{suffix}:test.com");
        let free_user = format!("@free_{suffix}:test.com");

        cleanup_blocklist(&pool, &room_id).await;
        ensure_test_room(&pool, &room_id).await;

        storage
            .set_invite_blocklist(&room_id, vec![blocked_user.clone()])
            .await
            .expect("set_invite_blocklist should succeed");

        assert!(
            storage.is_user_blocked(&room_id, &blocked_user).await.expect("is_user_blocked should succeed"),
            "blocked user should be reported as blocked"
        );

        assert!(
            !storage.is_user_blocked(&room_id, &free_user).await.expect("is_user_blocked should succeed"),
            "non-blocked user should not be reported as blocked"
        );

        cleanup_blocklist(&pool, &room_id).await;
    }

    #[tokio::test]
    async fn test_set_invite_blocklist_overwrites_previous() {
        let pool = test_pool().await;
        let storage = InviteBlocklistStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_overwrite_{suffix}:test.com");
        let user_a = format!("@user_a_{suffix}:test.com");
        let user_b = format!("@user_b_{suffix}:test.com");
        let user_c = format!("@user_c_{suffix}:test.com");

        cleanup_blocklist(&pool, &room_id).await;
        ensure_test_room(&pool, &room_id).await;

        // Set initial blocklist with A and B
        storage
            .set_invite_blocklist(&room_id, vec![user_a.clone(), user_b.clone()])
            .await
            .expect("first set should succeed");

        // Overwrite with only C
        storage.set_invite_blocklist(&room_id, vec![user_c.clone()]).await.expect("second set should succeed");

        let blocklist = storage.get_invite_blocklist(&room_id).await.expect("get_invite_blocklist should succeed");

        assert_eq!(blocklist.len(), 1, "blocklist should have exactly 1 entry after overwrite");
        assert!(blocklist.contains(&user_c), "blocklist should contain only user_c");
        assert!(!blocklist.contains(&user_a), "blocklist should not contain user_a");
        assert!(!blocklist.contains(&user_b), "blocklist should not contain user_b");

        cleanup_blocklist(&pool, &room_id).await;
    }

    #[tokio::test]
    async fn test_set_and_get_invite_allowlist() {
        let pool = test_pool().await;
        let storage = InviteBlocklistStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_al_{suffix}:test.com");
        let user_a = format!("@allowed_a_{suffix}:test.com");
        let user_b = format!("@allowed_b_{suffix}:test.com");

        cleanup_allowlist(&pool, &room_id).await;
        ensure_test_room(&pool, &room_id).await;

        storage
            .set_invite_allowlist(&room_id, vec![user_a.clone(), user_b.clone()])
            .await
            .expect("set_invite_allowlist should succeed");

        let allowlist = storage.get_invite_allowlist(&room_id).await.expect("get_invite_allowlist should succeed");

        assert_eq!(allowlist.len(), 2);
        assert!(allowlist.contains(&user_a), "allowlist should contain user_a");
        assert!(allowlist.contains(&user_b), "allowlist should contain user_b");

        cleanup_allowlist(&pool, &room_id).await;
    }

    #[tokio::test]
    async fn test_is_user_allowed_positive_and_negative() {
        let pool = test_pool().await;
        let storage = InviteBlocklistStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_allowed_{suffix}:test.com");
        let allowed_user = format!("@allowed_{suffix}:test.com");
        let not_allowed_user = format!("@not_allowed_{suffix}:test.com");

        cleanup_allowlist(&pool, &room_id).await;
        ensure_test_room(&pool, &room_id).await;

        storage
            .set_invite_allowlist(&room_id, vec![allowed_user.clone()])
            .await
            .expect("set_invite_allowlist should succeed");

        assert!(
            storage.is_user_allowed(&room_id, &allowed_user).await.expect("is_user_allowed should succeed"),
            "allowed user should be reported as allowed"
        );

        assert!(
            !storage.is_user_allowed(&room_id, &not_allowed_user).await.expect("is_user_allowed should succeed"),
            "non-allowed user should not be reported as allowed"
        );

        cleanup_allowlist(&pool, &room_id).await;
    }

    #[tokio::test]
    async fn test_has_any_invite_restriction() {
        let pool = test_pool().await;
        let storage = InviteBlocklistStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let room_no_restrict = format!("!room_none_{suffix}:test.com");
        let room_block = format!("!room_block_{suffix}:test.com");
        let room_allow = format!("!room_allow_{suffix}:test.com");
        let room_both = format!("!room_both_{suffix}:test.com");
        let user = format!("@user_{suffix}:test.com");

        // Ensure all rooms exist
        for rid in [&room_no_restrict, &room_block, &room_allow, &room_both] {
            cleanup_blocklist(&pool, rid).await;
            cleanup_allowlist(&pool, rid).await;
            ensure_test_room(&pool, rid).await;
        }

        // Room with no restrictions
        assert!(
            !storage
                .has_any_invite_restriction(&room_no_restrict)
                .await
                .expect("has_any_invite_restriction should succeed"),
            "room with no entries should have no restrictions"
        );

        // Room with only blocklist
        storage
            .set_invite_blocklist(&room_block, vec![user.clone()])
            .await
            .expect("set_invite_blocklist should succeed");
        assert!(
            storage.has_any_invite_restriction(&room_block).await.expect("has_any_invite_restriction should succeed"),
            "room with blocklist should have restrictions"
        );

        // Room with only allowlist
        storage
            .set_invite_allowlist(&room_allow, vec![user.clone()])
            .await
            .expect("set_invite_allowlist should succeed");
        assert!(
            storage.has_any_invite_restriction(&room_allow).await.expect("has_any_invite_restriction should succeed"),
            "room with allowlist should have restrictions"
        );

        // Room with both
        storage
            .set_invite_blocklist(&room_both, vec![format!("@other_{suffix}:test.com")])
            .await
            .expect("set_invite_blocklist for both should succeed");
        storage
            .set_invite_allowlist(&room_both, vec![user.clone()])
            .await
            .expect("set_invite_allowlist for both should succeed");
        assert!(
            storage.has_any_invite_restriction(&room_both).await.expect("has_any_invite_restriction should succeed"),
            "room with both lists should have restrictions"
        );

        // Cleanup
        for rid in [&room_no_restrict, &room_block, &room_allow, &room_both] {
            cleanup_blocklist(&pool, rid).await;
            cleanup_allowlist(&pool, rid).await;
        }
    }

    #[tokio::test]
    async fn test_get_global_invite_blocklist() {
        let pool = test_pool().await;
        let storage = InviteBlocklistStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let room_a = format!("!room_ga_{suffix}:test.com");
        let room_b = format!("!room_gb_{suffix}:test.com");
        let user_a = format!("@global_user_a_{suffix}:test.com");
        let user_b = format!("@global_user_b_{suffix}:test.com");

        cleanup_blocklist(&pool, &room_a).await;
        cleanup_blocklist(&pool, &room_b).await;
        ensure_test_room(&pool, &room_a).await;
        ensure_test_room(&pool, &room_b).await;

        storage
            .set_invite_blocklist(&room_a, vec![user_a.clone()])
            .await
            .expect("set blocklist for room_a should succeed");
        storage
            .set_invite_blocklist(&room_b, vec![user_b.clone()])
            .await
            .expect("set blocklist for room_b should succeed");

        let global = storage.get_global_invite_blocklist().await.expect("get_global_invite_blocklist should succeed");

        assert!(global.len() >= 2, "global blocklist should have at least 2 entries across 2 rooms");

        let room_ids: Vec<&str> = global.iter().map(|v| v["room_id"].as_str().unwrap()).collect();
        assert!(room_ids.contains(&room_a.as_str()), "global should contain room_a");
        assert!(room_ids.contains(&room_b.as_str()), "global should contain room_b");

        let user_ids: Vec<&str> = global.iter().map(|v| v["user_id"].as_str().unwrap()).collect();
        assert!(user_ids.contains(&user_a.as_str()), "global should contain user_a");
        assert!(user_ids.contains(&user_b.as_str()), "global should contain user_b");

        cleanup_blocklist(&pool, &room_a).await;
        cleanup_blocklist(&pool, &room_b).await;
    }

    #[tokio::test]
    async fn test_get_global_invite_allowlist() {
        let pool = test_pool().await;
        let storage = InviteBlocklistStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let room_a = format!("!room_gal_a_{suffix}:test.com");
        let room_b = format!("!room_gal_b_{suffix}:test.com");
        let user_a = format!("@global_al_a_{suffix}:test.com");
        let user_b = format!("@global_al_b_{suffix}:test.com");

        cleanup_allowlist(&pool, &room_a).await;
        cleanup_allowlist(&pool, &room_b).await;
        ensure_test_room(&pool, &room_a).await;
        ensure_test_room(&pool, &room_b).await;

        storage
            .set_invite_allowlist(&room_a, vec![user_a.clone()])
            .await
            .expect("set allowlist for room_a should succeed");
        storage
            .set_invite_allowlist(&room_b, vec![user_b.clone()])
            .await
            .expect("set allowlist for room_b should succeed");

        let global = storage.get_global_invite_allowlist().await.expect("get_global_invite_allowlist should succeed");

        assert_eq!(global.len(), 2, "global allowlist should have 2 entries across 2 rooms");

        let room_ids: Vec<&str> = global.iter().map(|v| v["room_id"].as_str().unwrap()).collect();
        assert!(room_ids.contains(&room_a.as_str()));
        assert!(room_ids.contains(&room_b.as_str()));

        let user_ids: Vec<&str> = global.iter().map(|v| v["user_id"].as_str().unwrap()).collect();
        assert!(user_ids.contains(&user_a.as_str()));
        assert!(user_ids.contains(&user_b.as_str()));

        cleanup_allowlist(&pool, &room_a).await;
        cleanup_allowlist(&pool, &room_b).await;
    }

    #[tokio::test]
    async fn test_round_trip_blocklist_set_and_clear() {
        let pool = test_pool().await;
        let storage = InviteBlocklistStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_clear_{suffix}:test.com");
        let user = format!("@to_clear_{suffix}:test.com");

        cleanup_blocklist(&pool, &room_id).await;
        ensure_test_room(&pool, &room_id).await;

        // Set blocklist
        storage.set_invite_blocklist(&room_id, vec![user.clone()]).await.expect("set_invite_blocklist should succeed");

        let blocklist = storage.get_invite_blocklist(&room_id).await.expect("get_invite_blocklist should succeed");
        assert_eq!(blocklist.len(), 1, "blocklist should have 1 entry after set");

        // Clear by setting empty vec
        storage
            .set_invite_blocklist(&room_id, vec![])
            .await
            .expect("set_invite_blocklist with empty vec should succeed");

        let cleared = storage.get_invite_blocklist(&room_id).await.expect("get_invite_blocklist should succeed");
        assert!(cleared.is_empty(), "blocklist should be empty after setting empty vec");

        cleanup_blocklist(&pool, &room_id).await;
    }
}
