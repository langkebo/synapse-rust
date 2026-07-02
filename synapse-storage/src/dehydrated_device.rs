use serde_json::Value;
use sqlx::{Pool, Postgres};
use std::sync::Arc;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DehydratedDevice {
    pub id: i64,
    pub user_id: String,
    pub device_id: String,
    pub device_data: Value,
    pub algorithm: String,
    pub account: Option<Value>,
    pub created_ts: i64,
    pub updated_ts: i64,
    pub expires_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct UpsertDehydratedDeviceParams {
    pub user_id: String,
    pub device_id: String,
    pub device_data: Value,
    pub algorithm: String,
    pub account: Option<Value>,
    pub expires_at: Option<i64>,
}

#[derive(Clone)]
pub struct DehydratedDeviceStorage {
    pool: Arc<Pool<Postgres>>,
}

impl DehydratedDeviceStorage {
    pub fn new(pool: &Arc<Pool<Postgres>>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn get_by_user(&self, user_id: &str) -> Result<Option<DehydratedDevice>, sqlx::Error> {
        sqlx::query_as::<_, DehydratedDevice>(
            r"
            SELECT id, user_id, device_id, device_data, algorithm, account, created_ts, updated_ts, expires_at
            FROM dehydrated_devices
            WHERE user_id = $1
              AND (expires_at IS NULL OR expires_at > $2)
            ORDER BY updated_ts DESC
            LIMIT 1
            ",
        )
        .bind(user_id)
        .bind(chrono::Utc::now().timestamp_millis())
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn upsert_for_user(&self, params: UpsertDehydratedDeviceParams) -> Result<DehydratedDevice, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            r"
            DELETE FROM dehydrated_devices
            WHERE user_id = $1
            ",
        )
        .bind(&params.user_id)
        .execute(&mut *tx)
        .await?;

        let record = sqlx::query_as::<_, DehydratedDevice>(
            r"
            INSERT INTO dehydrated_devices (
                user_id,
                device_id,
                device_data,
                algorithm,
                account,
                created_ts,
                updated_ts,
                expires_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $6, $7)
            RETURNING id, user_id, device_id, device_data, algorithm, account, created_ts, updated_ts, expires_at
            ",
        )
        .bind(&params.user_id)
        .bind(&params.device_id)
        .bind(&params.device_data)
        .bind(&params.algorithm)
        .bind(&params.account)
        .bind(now)
        .bind(params.expires_at)
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(record)
    }

    pub async fn delete_by_user(&self, user_id: &str) -> Result<u64, sqlx::Error> {
        let mut tx = self.pool.begin().await?;

        // Best-effort cleanup of any pending to-device messages addressed to a
        // dehydrated device for this user. We don't know the device_id ahead
        // of time, so we join via dehydrated_devices in a single statement.
        sqlx::query(
            r"
            DELETE FROM to_device_messages
            WHERE recipient_user_id = $1
              AND recipient_device_id IN (
                  SELECT device_id FROM dehydrated_devices WHERE user_id = $1
              )
            ",
        )
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

        let rows = sqlx::query(
            r"
            DELETE FROM dehydrated_devices
            WHERE user_id = $1
            ",
        )
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .map(|result| result.rows_affected())?;

        tx.commit().await?;
        Ok(rows)
    }

    /// Sweep dehydrated devices whose `expires_at` has passed.
    ///
    /// Removes any pending to-device messages addressed to those devices, then
    /// deletes the device rows themselves. Returns the number of devices
    /// removed.
    pub async fn sweep_expired(&self) -> Result<u64, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            r"
            DELETE FROM to_device_messages
            WHERE (recipient_user_id, recipient_device_id) IN (
                SELECT user_id, device_id FROM dehydrated_devices
                WHERE expires_at IS NOT NULL AND expires_at <= $1
            )
            ",
        )
        .bind(now)
        .execute(&mut *tx)
        .await?;

        let rows = sqlx::query(
            r"
            DELETE FROM dehydrated_devices
            WHERE expires_at IS NOT NULL AND expires_at <= $1
            ",
        )
        .bind(now)
        .execute(&mut *tx)
        .await
        .map(|result| result.rows_affected())?;

        tx.commit().await?;
        Ok(rows)
    }

    /// Fetch a batch of to-device messages addressed to the dehydrated device,
    /// in stream order, after the given cursor.
    ///
    /// Returns `(events, next_stream_id)` where `next_stream_id` is the highest
    /// `stream_id` returned (suitable as the next `next_batch` cursor) or
    /// `since_stream_id` when nothing was returned.
    pub async fn claim_to_device_events(
        &self,
        user_id: &str,
        device_id: &str,
        since_stream_id: i64,
        limit: i64,
    ) -> Result<(Vec<Value>, i64), sqlx::Error> {
        use sqlx::Row;

        let rows = sqlx::query(
            r"
            SELECT stream_id, sender_user_id, event_type, content, message_id
            FROM to_device_messages
            WHERE recipient_user_id = $1
              AND recipient_device_id = $2
              AND stream_id > $3
            ORDER BY stream_id ASC
            LIMIT $4
            ",
        )
        .bind(user_id)
        .bind(device_id)
        .bind(since_stream_id)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;

        let mut max_stream_id = since_stream_id;
        let mut events = Vec::with_capacity(rows.len());
        for row in rows {
            let stream_id: i64 = row.get("stream_id");
            let sender: String = row.get("sender_user_id");
            let event_type: String = row.get("event_type");
            let content: Value = row.get("content");
            let message_id: Option<String> = row.get("message_id");

            if stream_id > max_stream_id {
                max_stream_id = stream_id;
            }

            let mut event = serde_json::Map::new();
            event.insert("type".to_string(), Value::String(event_type));
            event.insert("sender".to_string(), Value::String(sender));
            event.insert("content".to_string(), content);
            if let Some(mid) = message_id {
                event.insert("message_id".to_string(), Value::String(mid));
            }
            events.push(Value::Object(event));
        }

        Ok((events, max_stream_id))
    }

    /// Claim a single one-time key (or fallback key, if no OTK is available)
    /// from the dehydrated device for the given algorithm prefix
    /// (e.g. `"signed_curve25519"`).
    ///
    /// Returns the `(key_id, key_payload)` pair, or `None` when no key is
    /// available. One-time keys are consumed (deleted) on claim per Matrix
    /// semantics; fallback keys are reusable and left in place.
    ///
    /// The whole read-modify-write happens inside a serialisable transaction
    /// with `FOR UPDATE` to prevent two concurrent claims from returning the
    /// same OTK.
    pub async fn claim_one_time_key(
        &self,
        user_id: &str,
        device_id: &str,
        algorithm: &str,
    ) -> Result<Option<(String, Value)>, sqlx::Error> {
        use sqlx::Row;
        let mut tx = self.pool.begin().await?;

        let row = sqlx::query(
            r"
            SELECT id, device_data
            FROM dehydrated_devices
            WHERE user_id = $1 AND device_id = $2
            FOR UPDATE
            ",
        )
        .bind(user_id)
        .bind(device_id)
        .fetch_optional(&mut *tx)
        .await?;

        let Some(row) = row else {
            tx.rollback().await?;
            return Ok(None);
        };

        let id: i64 = row.get("id");
        let mut device_data: Value = row.get("device_data");

        let prefix = format!("{algorithm}:");

        // 1) Try one-time keys first; consume on success.
        if let Some(otk_obj) = device_data.get_mut("one_time_keys").and_then(|v| v.as_object_mut()) {
            if let Some(matched_id) = otk_obj.keys().find(|k| k.starts_with(&prefix)).cloned() {
                if let Some(payload) = otk_obj.remove(&matched_id) {
                    let remaining_with_prefix = otk_obj.keys().filter(|k| k.starts_with(&prefix)).count();
                    sqlx::query("UPDATE dehydrated_devices SET device_data = $1, updated_ts = $2 WHERE id = $3")
                        .bind(&device_data)
                        .bind(chrono::Utc::now().timestamp_millis())
                        .bind(id)
                        .execute(&mut *tx)
                        .await?;
                    tx.commit().await?;
                    if remaining_with_prefix < 5 {
                        ::tracing::warn!(
                            "Dehydrated OTK stock low for {}:{} ({}) — {} remaining",
                            user_id,
                            device_id,
                            algorithm,
                            remaining_with_prefix
                        );
                    }
                    return Ok(Some((matched_id, payload)));
                }
            }
        }

        // 2) Fall back to fallback keys; do not consume.
        if let Some(fb_obj) = device_data.get("fallback_keys").and_then(|v| v.as_object()) {
            if let Some((matched_id, payload)) =
                fb_obj.iter().find(|(k, _)| k.starts_with(&prefix)).map(|(k, v)| (k.clone(), v.clone()))
            {
                tx.rollback().await?;
                return Ok(Some((matched_id, payload)));
            }
        }

        tx.rollback().await?;
        Ok(None)
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use serde_json::json;
    use sqlx::postgres::PgPoolOptions;
    use std::env;

    async fn test_pool() -> Arc<Pool<Postgres>> {
        let db_url = env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&db_url)
            .await
            .expect("Failed to connect to test database");
        Arc::new(pool)
    }

    /// Clean up dehydrated devices and to_device messages for a given user.
    async fn cleanup_user(pool: &Pool<Postgres>, user_id: &str) {
        let _ = sqlx::query("DELETE FROM to_device_messages WHERE recipient_user_id = $1")
            .bind(user_id)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM dehydrated_devices WHERE user_id = $1")
            .bind(user_id)
            .execute(pool)
            .await;
    }

    /// Build device_data JSON with one_time_keys and fallback_keys for OTK tests.
    fn make_device_data_with_keys(otk_alg: &str, otk_id: &str, otk_val: &Value, _fb_alg: &str, fb_id: &str, fb_val: &Value) -> Value {
        json!({
            "one_time_keys": { otk_id: otk_val },
            "fallback_keys": { fb_id: fb_val },
            "algorithm": otk_alg
        })
    }

    /// Direct insert of a to-device message for testing claim_to_device_events.
    async fn insert_to_device_msg(
        pool: &Pool<Postgres>,
        sender: &str,
        recipient_user: &str,
        recipient_device: &str,
        event_type: &str,
        content: &Value,
        stream_id: i64,
    ) {
        sqlx::query(
            r#"
            INSERT INTO to_device_messages
                (sender_user_id, sender_device_id, recipient_user_id, recipient_device_id,
                 event_type, content, stream_id, created_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(sender)
        .bind("TEST_SENDER_DEV")
        .bind(recipient_user)
        .bind(recipient_device)
        .bind(event_type)
        .bind(content)
        .bind(stream_id)
        .bind(chrono::Utc::now().timestamp_millis())
        .execute(pool)
        .await
        .expect("Failed to insert to_device_messages row");
    }

    #[tokio::test]
    async fn test_get_by_user_returns_none_when_no_device() {
        let pool = test_pool().await;
        let storage = DehydratedDeviceStorage::new(&pool);
        let user_id = format!("@no_device_{}:test", uuid::Uuid::new_v4());

        cleanup_user(&pool, &user_id).await;

        let result = storage.get_by_user(&user_id).await.expect("get_by_user should succeed");
        assert!(result.is_none(), "Should return None for user with no device");

        cleanup_user(&pool, &user_id).await;
    }

    #[tokio::test]
    async fn test_upsert_and_get_by_user() {
        let pool = test_pool().await;
        let storage = DehydratedDeviceStorage::new(&pool);
        let user_id = format!("@upsert_get_{}:test", uuid::Uuid::new_v4());

        cleanup_user(&pool, &user_id).await;

        let device_data = json!({"algorithms": ["m.olm.v1.curve25519-aes-sha2"]});
        let params = UpsertDehydratedDeviceParams {
            user_id: user_id.clone(),
            device_id: "DEV_UPSERT_GET".to_string(),
            device_data: device_data.clone(),
            algorithm: "m.dehydrated_device_v1".to_string(),
            account: Some(json!({"test": "account"})),
            expires_at: None,
        };

        let created = storage.upsert_for_user(params).await.expect("upsert should succeed");
        assert_eq!(created.user_id, user_id);
        assert_eq!(created.device_id, "DEV_UPSERT_GET");
        assert_eq!(created.device_data, device_data);
        assert_eq!(created.algorithm, "m.dehydrated_device_v1");
        assert_eq!(created.account, Some(json!({"test": "account"})));
        assert!(created.expires_at.is_none());
        assert!(created.created_ts > 0);
        assert!(created.updated_ts > 0);
        assert!(created.id > 0);

        // Retrieve via get_by_user
        let fetched = storage.get_by_user(&user_id).await.expect("get_by_user should succeed");
        assert!(fetched.is_some(), "Should find the upserted device");
        let fetched = fetched.unwrap();
        assert_eq!(fetched.id, created.id);
        assert_eq!(fetched.user_id, user_id);
        assert_eq!(fetched.device_id, "DEV_UPSERT_GET");
        assert_eq!(fetched.device_data, device_data);
        assert_eq!(fetched.algorithm, "m.dehydrated_device_v1");
        assert_eq!(fetched.account, Some(json!({"test": "account"})));

        cleanup_user(&pool, &user_id).await;
    }

    #[tokio::test]
    async fn test_get_by_user_respects_expiry() {
        let pool = test_pool().await;
        let storage = DehydratedDeviceStorage::new(&pool);
        let user_id = format!("@expired_get_{}:test", uuid::Uuid::new_v4());

        cleanup_user(&pool, &user_id).await;

        let past = chrono::Utc::now().timestamp_millis() - 100_000; // expired
        let params = UpsertDehydratedDeviceParams {
            user_id: user_id.clone(),
            device_id: "DEV_EXPIRED".to_string(),
            device_data: json!({"alg": "test"}),
            algorithm: "m.dehydrated_device_v1".to_string(),
            account: None,
            expires_at: Some(past),
        };

        storage.upsert_for_user(params).await.expect("upsert should succeed");

        let fetched = storage.get_by_user(&user_id).await.expect("get_by_user should succeed");
        assert!(fetched.is_none(), "Should NOT return expired device");

        cleanup_user(&pool, &user_id).await;
    }

    #[tokio::test]
    async fn test_upsert_replaces_existing() {
        let pool = test_pool().await;
        let storage = DehydratedDeviceStorage::new(&pool);
        let user_id = format!("@upsert_replace_{}:test", uuid::Uuid::new_v4());

        cleanup_user(&pool, &user_id).await;

        // First upsert
        let params1 = UpsertDehydratedDeviceParams {
            user_id: user_id.clone(),
            device_id: "DEV_FIRST".to_string(),
            device_data: json!({"version": 1}),
            algorithm: "m.dehydrated_device_v1".to_string(),
            account: None,
            expires_at: None,
        };
        let created1 = storage.upsert_for_user(params1).await.expect("first upsert should succeed");
        assert_eq!(created1.device_id, "DEV_FIRST");

        // Second upsert for same user — should delete first and create new
        let params2 = UpsertDehydratedDeviceParams {
            user_id: user_id.clone(),
            device_id: "DEV_SECOND".to_string(),
            device_data: json!({"version": 2}),
            algorithm: "m.dehydrated_device_v2".to_string(),
            account: None,
            expires_at: None,
        };
        let created2 = storage.upsert_for_user(params2).await.expect("second upsert should succeed");
        assert_eq!(created2.device_id, "DEV_SECOND");
        assert_ne!(created2.id, created1.id);

        // Verify only the second device exists
        let fetched = storage.get_by_user(&user_id).await.expect("get_by_user should succeed");
        assert!(fetched.is_some(), "Should find the second device");
        let fetched = fetched.unwrap();
        assert_eq!(fetched.id, created2.id);
        assert_eq!(fetched.device_id, "DEV_SECOND");
        assert_eq!(fetched.algorithm, "m.dehydrated_device_v2");

        // Verify the first device is gone by checking total count
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM dehydrated_devices WHERE user_id = $1")
            .bind(&user_id)
            .fetch_one(&*pool)
            .await
            .expect("count query should succeed");
        assert_eq!(count.0, 1, "Only one device should exist for this user after second upsert");

        cleanup_user(&pool, &user_id).await;
    }

    #[tokio::test]
    async fn test_delete_by_user_removes_device() {
        let pool = test_pool().await;
        let storage = DehydratedDeviceStorage::new(&pool);
        let user_id = format!("@delete_test_{}:test", uuid::Uuid::new_v4());

        cleanup_user(&pool, &user_id).await;

        let params = UpsertDehydratedDeviceParams {
            user_id: user_id.clone(),
            device_id: "DEV_DELETE".to_string(),
            device_data: json!({"test": true}),
            algorithm: "m.dehydrated_device_v1".to_string(),
            account: None,
            expires_at: None,
        };
        storage.upsert_for_user(params).await.expect("upsert should succeed");

        let deleted = storage.delete_by_user(&user_id).await.expect("delete should succeed");
        assert_eq!(deleted, 1, "Should delete one device");

        let fetched = storage.get_by_user(&user_id).await.expect("get_by_user should succeed");
        assert!(fetched.is_none(), "Device should be deleted");

        cleanup_user(&pool, &user_id).await;
    }

    #[tokio::test]
    async fn test_delete_by_user_returns_zero_when_no_device() {
        let pool = test_pool().await;
        let storage = DehydratedDeviceStorage::new(&pool);
        let user_id = format!("@delete_none_{}:test", uuid::Uuid::new_v4());

        cleanup_user(&pool, &user_id).await;

        let deleted = storage.delete_by_user(&user_id).await.expect("delete should succeed");
        assert_eq!(deleted, 0, "Should return zero for non-existent user");

        cleanup_user(&pool, &user_id).await;
    }

    #[tokio::test]
    async fn test_sweep_expired_removes_expired() {
        let pool = test_pool().await;
        let storage = DehydratedDeviceStorage::new(&pool);
        let user_id = format!("@sweep_expired_{}:test", uuid::Uuid::new_v4());

        cleanup_user(&pool, &user_id).await;

        // Create a device, then set expires_at to the past via raw SQL
        let params = UpsertDehydratedDeviceParams {
            user_id: user_id.clone(),
            device_id: "DEV_SWEEP".to_string(),
            device_data: json!({"test": true}),
            algorithm: "m.dehydrated_device_v1".to_string(),
            account: None,
            expires_at: None,
        };
        storage.upsert_for_user(params).await.expect("upsert should succeed");

        let past = chrono::Utc::now().timestamp_millis() - 86_400_000;
        sqlx::query("UPDATE dehydrated_devices SET expires_at = $1 WHERE user_id = $2")
            .bind(past)
            .bind(&user_id)
            .execute(&*pool)
            .await
            .expect("update expires_at");

        // Verify it exists (expired, so get_by_user won't return it, but it's in the DB)
        let count_before: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM dehydrated_devices WHERE user_id = $1")
                .bind(&user_id)
                .fetch_one(&*pool)
                .await
                .expect("count query should succeed");
        assert_eq!(count_before.0, 1, "Device should exist before sweep");

        let swept = storage.sweep_expired().await.expect("sweep should succeed");
        assert!(swept >= 1, "Should sweep at least 1 device, got {}", swept);

        // Verify it's gone from DB
        let count_after: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM dehydrated_devices WHERE user_id = $1")
                .bind(&user_id)
                .fetch_one(&*pool)
                .await
                .expect("count query should succeed");
        assert_eq!(count_after.0, 0, "Device should be gone after sweep");

        cleanup_user(&pool, &user_id).await;
    }

    #[tokio::test]
    async fn test_sweep_expired_keeps_valid() {
        let pool = test_pool().await;
        let storage = DehydratedDeviceStorage::new(&pool);
        let user_id = format!("@sweep_keep_{}:test", uuid::Uuid::new_v4());

        cleanup_user(&pool, &user_id).await;

        let future = chrono::Utc::now().timestamp_millis() + 86_400_000; // 1 day in future

        let params = UpsertDehydratedDeviceParams {
            user_id: user_id.clone(),
            device_id: "DEV_VALID".to_string(),
            device_data: json!({"test": true}),
            algorithm: "m.dehydrated_device_v1".to_string(),
            account: None,
            expires_at: Some(future),
        };
        storage.upsert_for_user(params).await.expect("upsert should succeed");

        let _ = storage.sweep_expired().await.expect("sweep should succeed");
        // _swept may or may not include our device — depends on other tests

        let fetched = storage.get_by_user(&user_id).await.expect("get_by_user should succeed");
        assert!(fetched.is_some(), "Valid (future-expiry) device should survive sweep");

        cleanup_user(&pool, &user_id).await;
    }

    #[tokio::test]
    async fn test_claim_to_device_events_pagination() {
        let pool = test_pool().await;
        let storage = DehydratedDeviceStorage::new(&pool);
        let user_id = format!("@claim_td_{}:test", uuid::Uuid::new_v4());
        let device_id = "DEV_CLAIM_TD";

        cleanup_user(&pool, &user_id).await;

        // Create a dehydrated device first so the infrastructure is consistent
        let params = UpsertDehydratedDeviceParams {
            user_id: user_id.clone(),
            device_id: device_id.to_string(),
            device_data: json!({"test": true}),
            algorithm: "m.dehydrated_device_v1".to_string(),
            account: None,
            expires_at: None,
        };
        storage.upsert_for_user(params).await.expect("upsert should succeed");

        // Insert 5 to_device messages with sequential stream_ids
        for i in 0..5 {
            insert_to_device_msg(
                &pool,
                "@sender:test",
                &user_id,
                device_id,
                "m.room.message",
                &json!({"body": format!("msg {}", i)}),
                100 + i,
            )
            .await;
        }

        // Fetch first 3 (stream_id > 0)
        let (events, max_id) = storage
            .claim_to_device_events(&user_id, device_id, 0, 3)
            .await
            .expect("claim should succeed");
        assert_eq!(events.len(), 3);
        assert_eq!(max_id, 102);

        // Fetch remaining 2 (stream_id > 102)
        let (events2, max_id2) = storage
            .claim_to_device_events(&user_id, device_id, max_id, 3)
            .await
            .expect("second claim should succeed");
        assert_eq!(events2.len(), 2);
        assert_eq!(max_id2, 104);

        // No more messages
        let (events3, max_id3) = storage
            .claim_to_device_events(&user_id, device_id, max_id2, 3)
            .await
            .expect("third claim should succeed");
        assert_eq!(events3.len(), 0);
        assert_eq!(max_id3, max_id2); // cursor unchanged when empty

        cleanup_user(&pool, &user_id).await;
    }

    #[tokio::test]
    async fn test_claim_to_device_events_empty_when_no_messages() {
        let pool = test_pool().await;
        let storage = DehydratedDeviceStorage::new(&pool);
        let user_id = format!("@claim_empty_{}:test", uuid::Uuid::new_v4());
        let device_id = "DEV_CLAIM_EMPTY";

        cleanup_user(&pool, &user_id).await;

        // Create a dehydrated device
        let params = UpsertDehydratedDeviceParams {
            user_id: user_id.clone(),
            device_id: device_id.to_string(),
            device_data: json!({"test": true}),
            algorithm: "m.dehydrated_device_v1".to_string(),
            account: None,
            expires_at: None,
        };
        storage.upsert_for_user(params).await.expect("upsert should succeed");

        let (events, max_id) = storage
            .claim_to_device_events(&user_id, device_id, 0, 10)
            .await
            .expect("claim should succeed");
        assert_eq!(events.len(), 0);
        assert_eq!(max_id, 0); // cursor unchanged when empty

        cleanup_user(&pool, &user_id).await;
    }

    #[tokio::test]
    async fn test_claim_one_time_key_claims_otk_and_consumes() {
        let pool = test_pool().await;
        let storage = DehydratedDeviceStorage::new(&pool);
        let user_id = format!("@otk_claim_{}:test", uuid::Uuid::new_v4());
        let device_id = "DEV_OTK_CLAIM";

        cleanup_user(&pool, &user_id).await;

        let otk_key = json!({"key": "otk_secret_key", "signatures": {"@device:test": {"ed25519:1": "sig"}}});
        let fb_key = json!({"key": "fb_secret_key", "signatures": {"@device:test": {"ed25519:1": "sig"}}});
        let device_data = make_device_data_with_keys(
            "signed_curve25519",
            "signed_curve25519:AAAAAQ",
            &otk_key,
            "signed_curve25519",
            "signed_curve25519:BBBB",
            &fb_key,
        );

        let params = UpsertDehydratedDeviceParams {
            user_id: user_id.clone(),
            device_id: device_id.to_string(),
            device_data,
            algorithm: "m.dehydrated_device_v1".to_string(),
            account: None,
            expires_at: None,
        };
        storage.upsert_for_user(params).await.expect("upsert should succeed");

        // Claim the OTK
        let result = storage
            .claim_one_time_key(&user_id, device_id, "signed_curve25519")
            .await
            .expect("claim_one_time_key should succeed");
        assert!(result.is_some(), "Should return a key");
        let (key_id, key_payload) = result.unwrap();
        assert_eq!(key_id, "signed_curve25519:AAAAAQ");
        assert_eq!(key_payload, otk_key);

        // Second claim should fall back to fallback key (OTK was consumed)
        let result2 = storage
            .claim_one_time_key(&user_id, device_id, "signed_curve25519")
            .await
            .expect("second claim_one_time_key should succeed");
        assert!(result2.is_some(), "Should return the fallback key");
        let (key_id2, key_payload2) = result2.unwrap();
        assert_eq!(key_id2, "signed_curve25519:BBBB");
        assert_eq!(key_payload2, fb_key);

        cleanup_user(&pool, &user_id).await;
    }

    #[tokio::test]
    async fn test_claim_one_time_key_none_when_no_device() {
        let pool = test_pool().await;
        let storage = DehydratedDeviceStorage::new(&pool);
        let user_id = format!("@otk_nodev_{}:test", uuid::Uuid::new_v4());

        cleanup_user(&pool, &user_id).await;

        let result = storage
            .claim_one_time_key(&user_id, "NONEXISTENT_DEV", "signed_curve25519")
            .await
            .expect("claim_one_time_key should succeed");
        assert!(result.is_none(), "Should return None when device doesn't exist");

        cleanup_user(&pool, &user_id).await;
    }

    #[tokio::test]
    async fn test_claim_one_time_key_none_when_no_matching_algorithm() {
        let pool = test_pool().await;
        let storage = DehydratedDeviceStorage::new(&pool);
        let user_id = format!("@otk_noalg_{}:test", uuid::Uuid::new_v4());
        let device_id = "DEV_OTK_NOALG";

        cleanup_user(&pool, &user_id).await;

        let otk_key = json!({"key": "some_key"});
        let fb_key = json!({"key": "some_fb_key"});
        let device_data = make_device_data_with_keys(
            "signed_curve25519",
            "signed_curve25519:AAAAAQ",
            &otk_key,
            "signed_curve25519",
            "signed_curve25519:BBBB",
            &fb_key,
        );

        let params = UpsertDehydratedDeviceParams {
            user_id: user_id.clone(),
            device_id: device_id.to_string(),
            device_data,
            algorithm: "m.dehydrated_device_v1".to_string(),
            account: None,
            expires_at: None,
        };
        storage.upsert_for_user(params).await.expect("upsert should succeed");

        // Claim with different algorithm — no matching key
        let result = storage
            .claim_one_time_key(&user_id, device_id, "signed_ed25519")
            .await
            .expect("claim_one_time_key should succeed");
        assert!(result.is_none(), "Should return None for non-matching algorithm");

        cleanup_user(&pool, &user_id).await;
    }

    #[tokio::test]
    async fn test_claim_one_time_key_fallback_only_no_otk() {
        let pool = test_pool().await;
        let storage = DehydratedDeviceStorage::new(&pool);
        let user_id = format!("@otk_fbonly_{}:test", uuid::Uuid::new_v4());
        let device_id = "DEV_OTK_FBONLY";

        cleanup_user(&pool, &user_id).await;

        let fb_key = json!({"key": "reusable_fallback", "fallback": true});
        let device_data = json!({
            "fallback_keys": {
                "signed_curve25519:CCCC": &fb_key
            }
        });

        let params = UpsertDehydratedDeviceParams {
            user_id: user_id.clone(),
            device_id: device_id.to_string(),
            device_data,
            algorithm: "m.dehydrated_device_v1".to_string(),
            account: None,
            expires_at: None,
        };
        storage.upsert_for_user(params).await.expect("upsert should succeed");

        // No OTKs available, should get fallback key
        let result = storage
            .claim_one_time_key(&user_id, device_id, "signed_curve25519")
            .await
            .expect("claim_one_time_key should succeed");
        assert!(result.is_some(), "Should return fallback key when no OTK");
        let (key_id, key_payload) = result.unwrap();
        assert_eq!(key_id, "signed_curve25519:CCCC");
        assert_eq!(key_payload, fb_key);

        // Second claim — fallback keys are not consumed, should return again
        let result2 = storage
            .claim_one_time_key(&user_id, device_id, "signed_curve25519")
            .await
            .expect("second claim should succeed");
        assert!(result2.is_some(), "Fallback key should be reusable");
        let (key_id2, _) = result2.unwrap();
        assert_eq!(key_id2, "signed_curve25519:CCCC");

        cleanup_user(&pool, &user_id).await;
    }

    #[tokio::test]
    async fn test_delete_by_user_cleans_up_to_device_messages() {
        let pool = test_pool().await;
        let storage = DehydratedDeviceStorage::new(&pool);
        let user_id = format!("@del_cleanup_{}:test", uuid::Uuid::new_v4());
        let device_id = "DEV_DEL_CLEANUP";

        cleanup_user(&pool, &user_id).await;

        // Create a dehydrated device
        let params = UpsertDehydratedDeviceParams {
            user_id: user_id.clone(),
            device_id: device_id.to_string(),
            device_data: json!({"test": true}),
            algorithm: "m.dehydrated_device_v1".to_string(),
            account: None,
            expires_at: None,
        };
        storage.upsert_for_user(params).await.expect("upsert should succeed");

        // Insert to_device messages for this user's dehydrated device
        insert_to_device_msg(
            &pool, "@sender:test", &user_id, device_id,
            "m.room.message", &json!({"body": "msg1"}), 200,
        ).await;
        insert_to_device_msg(
            &pool, "@sender:test", &user_id, device_id,
            "m.room.message", &json!({"body": "msg2"}), 201,
        ).await;

        // Verify messages exist
        let msg_count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM to_device_messages WHERE recipient_user_id = $1 AND recipient_device_id = $2")
                .bind(&user_id)
                .bind(device_id)
                .fetch_one(&*pool)
                .await
                .expect("count should succeed");
        assert_eq!(msg_count.0, 2, "Should have 2 to_device messages before delete");

        // Delete should also clean up to_device_messages
        let deleted = storage.delete_by_user(&user_id).await.expect("delete should succeed");
        assert_eq!(deleted, 1, "Should delete one device");

        // Verify messages are gone
        let msg_count_after: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM to_device_messages WHERE recipient_user_id = $1 AND recipient_device_id = $2")
                .bind(&user_id)
                .bind(device_id)
                .fetch_one(&*pool)
                .await
                .expect("count should succeed");
        assert_eq!(msg_count_after.0, 0, "to_device messages should be cleaned up");

        cleanup_user(&pool, &user_id).await;
    }
}
