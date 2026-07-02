use serde::{Deserialize, Serialize};
use std::sync::Arc;
use synapse_common::ApiError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyEvent {
    pub user_id: String,
    pub device_id: Option<String>,
    pub operation: String,
    pub key_id: Option<String>,
    pub room_id: Option<String>,
    pub details: Option<serde_json::Value>,
    pub ip_address: Option<String>,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct KeyAuditEntry {
    pub id: i64,
    pub user_id: String,
    pub device_id: Option<String>,
    pub operation: String,
    pub key_id: Option<String>,
    pub room_id: Option<String>,
    pub details: Option<serde_json::Value>,
    pub ip_address: Option<String>,
    pub created_ts: i64,
}

#[derive(Clone)]
pub struct E2eeAuditStorage {
    pool: Arc<sqlx::PgPool>,
}

impl E2eeAuditStorage {
    pub fn new(pool: &Arc<sqlx::PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn log_key_operation(&self, event: &KeyEvent) -> Result<(), ApiError> {
        sqlx::query(
            r"
            INSERT INTO e2ee_audit_log
            (user_id, device_id, action, operation, key_id, room_id, details, ip_address, created_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ",
        )
        .bind(&event.user_id)
        .bind(&event.device_id)
        .bind(&event.operation)
        .bind(&event.operation)
        .bind(&event.key_id)
        .bind(&event.room_id)
        .bind(&event.details)
        .bind(&event.ip_address)
        .bind(event.timestamp)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to log key operation", &e))?;

        Ok(())
    }

    pub async fn get_key_history(&self, user_id: &str) -> Result<Vec<KeyAuditEntry>, ApiError> {
        sqlx::query_as::<_, KeyAuditEntry>(
            r"
            SELECT id, user_id, device_id, operation, key_id, room_id, details, ip_address, created_ts
            FROM e2ee_audit_log
            WHERE user_id = $1
            ORDER BY created_ts DESC, id DESC
            LIMIT 100
            ",
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get key history", &e))
    }

    pub async fn get_key_history_paginated(
        &self,
        user_id: &str,
        limit: i64,
        from_ts: Option<i64>,
        from_id: Option<i64>,
    ) -> Result<Vec<KeyAuditEntry>, ApiError> {
        if let (Some(ts), Some(id)) = (from_ts, from_id) {
            sqlx::query_as::<_, KeyAuditEntry>(
                r"
                SELECT id, user_id, device_id, operation, key_id, room_id, details, ip_address, created_ts
                FROM e2ee_audit_log
                WHERE user_id = $1 AND (created_ts < $2 OR (created_ts = $2 AND id < $3))
                ORDER BY created_ts DESC, id DESC
                LIMIT $4
                ",
            )
            .bind(user_id)
            .bind(ts)
            .bind(id)
            .bind(limit)
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get key history", &e))
        } else {
            sqlx::query_as::<_, KeyAuditEntry>(
                r"
                SELECT id, user_id, device_id, operation, key_id, room_id, details, ip_address, created_ts
                FROM e2ee_audit_log
                WHERE user_id = $1
                ORDER BY created_ts DESC, id DESC
                LIMIT $2
                ",
            )
            .bind(user_id)
            .bind(limit)
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get key history", &e))
        }
    }

    pub async fn get_operations_by_type(&self, operation: &str, limit: i64) -> Result<Vec<KeyAuditEntry>, ApiError> {
        sqlx::query_as::<_, KeyAuditEntry>(
            r"
            SELECT id, user_id, device_id, operation, key_id, room_id, details, ip_address, created_ts
            FROM e2ee_audit_log
            WHERE operation = $1
            ORDER BY created_ts DESC, id DESC
            LIMIT $2
            ",
        )
        .bind(operation)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get operations", &e))
    }

    pub async fn get_user_device_history(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<Vec<KeyAuditEntry>, ApiError> {
        sqlx::query_as::<_, KeyAuditEntry>(
            r"
            SELECT id, user_id, device_id, operation, key_id, room_id, details, ip_address, created_ts
            FROM e2ee_audit_log
            WHERE user_id = $1 AND device_id = $2
            ORDER BY created_ts DESC, id DESC
            LIMIT 50
            ",
        )
        .bind(user_id)
        .bind(device_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get device history", &e))
    }

    pub async fn cleanup_old_logs(&self, days_to_keep: i64) -> Result<u64, ApiError> {
        let cutoff_ts = chrono::Utc::now().timestamp_millis() - (days_to_keep * 24 * 60 * 60 * 1000);
        let result = sqlx::query("DELETE FROM e2ee_audit_log WHERE created_ts < $1")
            .bind(cutoff_ts)
            .execute(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to cleanup logs", &e))?;

        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;
    use std::sync::Arc;

    async fn test_pool() -> Arc<sqlx::PgPool> {
        let db_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
        let pool =
            PgPoolOptions::new().max_connections(2).connect(&db_url).await.expect("Failed to connect to test database");
        Arc::new(pool)
    }

    fn make_test_event(user_id: &str, operation: &str, device_id: &str) -> KeyEvent {
        KeyEvent {
            user_id: user_id.to_string(),
            device_id: Some(device_id.to_string()),
            operation: operation.to_string(),
            key_id: Some(format!("key_{}", uuid::Uuid::new_v4())),
            room_id: Some("!test_room:example.com".to_string()),
            details: Some(serde_json::json!({"method": "m.room.encryption"})),
            ip_address: Some("127.0.0.1".to_string()),
            timestamp: chrono::Utc::now().timestamp_millis(),
        }
    }

    #[tokio::test]
    async fn test_log_key_operation_full_round_trip() {
        let pool = test_pool().await;
        let storage = E2eeAuditStorage::new(&pool);
        let user_id = format!("@roundtrip_{}:example.com", uuid::Uuid::new_v4());

        let event_ts = chrono::Utc::now().timestamp_millis() - 3600000; // 1 hour ago (recent, safe from cleanup)

        let event = KeyEvent {
            user_id: user_id.clone(),
            device_id: Some("ROUNDTRIP_DEVICE".to_string()),
            operation: "cross_sign".to_string(),
            key_id: Some("ed25519:master_key".to_string()),
            room_id: Some("!rt_room:example.com".to_string()),
            details: Some(serde_json::json!({"type": "master", "verified": true})),
            ip_address: Some("10.0.0.1".to_string()),
            timestamp: event_ts,
        };

        storage.log_key_operation(&event).await.expect("log_key_operation should succeed");

        let history = storage.get_key_history(&user_id).await.expect("get_key_history should succeed");
        assert_eq!(history.len(), 1, "should have exactly one audit entry");
        let entry = &history[0];

        assert_eq!(entry.user_id, user_id);
        assert_eq!(entry.device_id.as_deref(), Some("ROUNDTRIP_DEVICE"));
        assert_eq!(entry.operation, "cross_sign");
        assert_eq!(entry.key_id.as_deref(), Some("ed25519:master_key"));
        assert_eq!(entry.room_id.as_deref(), Some("!rt_room:example.com"));
        assert_eq!(entry.ip_address.as_deref(), Some("10.0.0.1"));
        assert_eq!(entry.created_ts, event_ts);
        assert!(entry.id > 0);
        assert!(entry.details.is_some());
    }

    #[tokio::test]
    async fn test_get_key_history_user_isolation() {
        let pool = test_pool().await;
        let storage = E2eeAuditStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let user_a = format!("@alice_{}:example.com", suffix);
        let user_b = format!("@bob_{}:example.com", suffix);

        let event_a = make_test_event(&user_a, "upload_device_keys", "DEV_A");
        let event_b = make_test_event(&user_b, "upload_one_time_keys", "DEV_B");

        storage.log_key_operation(&event_a).await.expect("log a should succeed");
        storage.log_key_operation(&event_b).await.expect("log b should succeed");

        let history_a = storage.get_key_history(&user_a).await.expect("get_key_history a should succeed");
        assert!(!history_a.is_empty(), "user_a should have audit entries");
        assert!(history_a.iter().all(|e| e.user_id == user_a), "all entries should belong to user_a");

        let history_b = storage.get_key_history(&user_b).await.expect("get_key_history b should succeed");
        assert!(!history_b.is_empty(), "user_b should have audit entries");
        assert!(history_b.iter().all(|e| e.user_id == user_b), "all entries should belong to user_b");
    }

    #[tokio::test]
    async fn test_get_operations_by_type_filtering() {
        let pool = test_pool().await;
        let storage = E2eeAuditStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let user_id = format!("@optype_{}:example.com", suffix);

        // Use UUID-based operation names to avoid cross-test interference.
        let op_upload = format!("upload_dev_keys_{}", suffix);
        let op_cross = format!("cross_sign_{}", suffix);

        // Insert events with different operations
        storage
            .log_key_operation(&make_test_event(&user_id, &op_upload, "DEV_A"))
            .await
            .expect("upload log should succeed");
        storage
            .log_key_operation(&make_test_event(&user_id, &op_cross, "DEV_A"))
            .await
            .expect("cross log should succeed");
        storage
            .log_key_operation(&make_test_event(&user_id, &op_upload, "DEV_A"))
            .await
            .expect("upload2 log should succeed");

        // Query by operation type
        let uploads =
            storage.get_operations_by_type(&op_upload, 100).await.expect("get_operations_by_type should succeed");
        assert_eq!(uploads.len(), 2, "should find 2 upload events");
        assert!(uploads.iter().all(|e| e.operation == op_upload));

        let crosses =
            storage.get_operations_by_type(&op_cross, 100).await.expect("get_operations_by_type should succeed");
        assert_eq!(crosses.len(), 1, "should find 1 cross_sign event");
        assert_eq!(crosses[0].operation, op_cross);
    }

    #[tokio::test]
    async fn test_get_key_history_paginated_with_cursor() {
        let pool = test_pool().await;
        let storage = E2eeAuditStorage::new(&pool);
        let user_id = format!("@paged_{}:example.com", uuid::Uuid::new_v4());

        // Insert events with known timestamps in ascending order (oldest first).
        // Since results are ORDER BY created_ts DESC, the first page will
        // return the most recent (highest ts) entries.
        let base_ts = chrono::Utc::now().timestamp_millis();
        for i in 0..5 {
            let mut event = make_test_event(&user_id, "query_keys", &format!("DEV_PAGED_{}", i));
            event.timestamp = base_ts + i * 1000;
            storage.log_key_operation(&event).await.expect("log should succeed");
        }

        // Page 1: get first 2 entries (most recent)
        let page1 = storage.get_key_history_paginated(&user_id, 2, None, None).await.expect("page1 should succeed");
        assert_eq!(page1.len(), 2, "page1 should have 2 entries");

        // Page 2: cursor from last entry of page1
        let cursor_ts = page1.last().unwrap().created_ts;
        let cursor_id = page1.last().unwrap().id;
        let page2 = storage
            .get_key_history_paginated(&user_id, 2, Some(cursor_ts), Some(cursor_id))
            .await
            .expect("page2 should succeed");
        assert_eq!(page2.len(), 2, "page2 should have 2 entries");

        // Page 3: cursor from last entry of page2, should get the remaining 1
        let cursor_ts3 = page2.last().unwrap().created_ts;
        let cursor_id3 = page2.last().unwrap().id;
        let page3 = storage
            .get_key_history_paginated(&user_id, 2, Some(cursor_ts3), Some(cursor_id3))
            .await
            .expect("page3 should succeed");
        assert_eq!(page3.len(), 1, "page3 should have 1 remaining entry");

        // Verify no overlap between pages (all IDs should be distinct)
        let ids1: Vec<i64> = page1.iter().map(|e| e.id).collect();
        let ids2: Vec<i64> = page2.iter().map(|e| e.id).collect();
        let ids3: Vec<i64> = page3.iter().map(|e| e.id).collect();
        for id in &ids2 {
            assert!(!ids1.contains(id), "page2 entry {} should not be in page1", id);
        }
        for id in &ids3 {
            assert!(!ids1.contains(id) && !ids2.contains(id), "page3 entry {} should not be in page1 or page2", id);
        }
    }

    #[tokio::test]
    async fn test_get_user_device_history_filtering() {
        let pool = test_pool().await;
        let storage = E2eeAuditStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let user_id = format!("@devhist_{}:example.com", suffix);

        // Insert events for device A
        storage
            .log_key_operation(&make_test_event(&user_id, "upload_device_keys", "DEVICE_A"))
            .await
            .expect("a1 log should succeed");
        storage
            .log_key_operation(&make_test_event(&user_id, "update_device_keys", "DEVICE_A"))
            .await
            .expect("a2 log should succeed");

        // Insert event for device B
        storage
            .log_key_operation(&make_test_event(&user_id, "upload_device_keys", "DEVICE_B"))
            .await
            .expect("b log should succeed");

        // Query device A history
        let dev_a = storage
            .get_user_device_history(&user_id, "DEVICE_A")
            .await
            .expect("get_user_device_history A should succeed");
        assert_eq!(dev_a.len(), 2, "should find 2 entries for DEVICE_A");
        assert!(dev_a.iter().all(|e| e.device_id.as_deref() == Some("DEVICE_A")));

        // Query device B history
        let dev_b = storage
            .get_user_device_history(&user_id, "DEVICE_B")
            .await
            .expect("get_user_device_history B should succeed");
        assert_eq!(dev_b.len(), 1, "should find 1 entry for DEVICE_B");
        assert_eq!(dev_b[0].device_id.as_deref(), Some("DEVICE_B"));

        // Query non-existent device
        let dev_x = storage
            .get_user_device_history(&user_id, "NONEXISTENT")
            .await
            .expect("get_user_device_history X should succeed");
        assert!(dev_x.is_empty(), "nonexistent device should return empty");
    }

    #[tokio::test]
    async fn test_cleanup_old_logs_removes_old_events() {
        let pool = test_pool().await;
        let storage = E2eeAuditStorage::new(&pool);
        let user_id = format!("@cleanup_{}:example.com", uuid::Uuid::new_v4());

        let now_ms = chrono::Utc::now().timestamp_millis();
        let one_day_ms: i64 = 24 * 60 * 60 * 1000;

        // Count pre-existing events for this user (should be 0).
        let before = storage.get_key_history(&user_id).await.expect("get_key_history before should succeed");
        assert!(before.is_empty(), "no events should exist for this user before test");

        // Insert an old event (31 days ago)
        let mut old_event = make_test_event(&user_id, "old_operation", "DEV_OLD");
        old_event.timestamp = now_ms - (31 * one_day_ms);
        storage.log_key_operation(&old_event).await.expect("old log should succeed");

        // Insert a recent event (1 day ago)
        let mut recent_event = make_test_event(&user_id, "recent_operation", "DEV_RECENT");
        recent_event.timestamp = now_ms - one_day_ms;
        storage.log_key_operation(&recent_event).await.expect("recent log should succeed");

        // Cleanup logs older than 7 days — should remove the 31-day-old event
        // but keep the 1-day-old event.
        let deleted = storage.cleanup_old_logs(7).await.expect("cleanup should succeed");
        assert!(deleted >= 1, "should have deleted at least 1 old event");

        // Verify recent event still exists
        let history = storage.get_key_history(&user_id).await.expect("get_key_history should succeed");
        assert_eq!(history.len(), 1, "only the recent event should remain");
        assert_eq!(history[0].operation, "recent_operation");
        assert_eq!(history[0].created_ts, recent_event.timestamp);
    }

    #[tokio::test]
    async fn test_batch_audit_logging_returns_ordered_by_recency() {
        let pool = test_pool().await;
        let storage = E2eeAuditStorage::new(&pool);
        let user_id = format!("@batch_{}:example.com", uuid::Uuid::new_v4());

        let base_ts = chrono::Utc::now().timestamp_millis();
        let mut timestamps: Vec<i64> = Vec::new();

        // Insert events with staggered timestamps
        for i in 0..5 {
            let mut event = make_test_event(&user_id, &format!("batch_op_{}", i), &format!("DEV_{}", i));
            event.timestamp = base_ts + i * 100;
            timestamps.push(event.timestamp);
            storage.log_key_operation(&event).await.expect("batch log should succeed");
        }

        let history = storage.get_key_history(&user_id).await.expect("get_key_history should succeed");
        assert_eq!(history.len(), 5, "should have all 5 audit entries");

        // Verify descending order by created_ts (most recent first).
        for i in 0..4 {
            assert!(
                history[i].created_ts >= history[i + 1].created_ts,
                "entries should be ordered by created_ts descending: {} (idx {}) >= {} (idx {})",
                history[i].created_ts,
                i,
                history[i + 1].created_ts,
                i + 1
            );
        }

        // Verify all inserted timestamps are present
        let mut returned_ts: Vec<i64> = history.iter().map(|e| e.created_ts).collect();
        returned_ts.sort();
        timestamps.sort();
        assert_eq!(returned_ts, timestamps, "all timestamps should be present");
    }
}
