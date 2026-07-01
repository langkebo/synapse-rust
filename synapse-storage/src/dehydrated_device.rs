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
mod tests {
    use super::*;

    fn now_ms() -> i64 {
        chrono::Utc::now().timestamp_millis()
    }

    #[test]
    fn test_dehydrated_device_construction() {
        let device = DehydratedDevice {
            id: 1,
            user_id: "@alice:example.com".to_string(),
            device_id: "DEVICE123".to_string(),
            device_data: serde_json::json!({"key": "value"}),
            algorithm: "m.dehydration.v1".to_string(),
            account: Some(serde_json::json!({"account": "data"})),
            created_ts: now_ms(),
            updated_ts: now_ms(),
            expires_at: Some(now_ms() + 86_400_000),
        };
        assert_eq!(device.id, 1);
        assert_eq!(device.user_id, "@alice:example.com");
        assert_eq!(device.device_id, "DEVICE123");
        assert_eq!(device.algorithm, "m.dehydration.v1");
        assert!(device.account.is_some());
        assert!(device.expires_at.is_some());
    }

    #[test]
    fn test_dehydrated_device_optional_fields_none() {
        let device = DehydratedDevice {
            id: 2,
            user_id: "@bob:example.com".to_string(),
            device_id: "DEV456".to_string(),
            device_data: serde_json::json!({}),
            algorithm: "m.dehydration.v1".to_string(),
            account: None,
            created_ts: 0,
            updated_ts: 0,
            expires_at: None,
        };
        assert!(device.account.is_none());
        assert!(device.expires_at.is_none());
        assert_eq!(device.created_ts, 0);
    }

    #[test]
    fn test_upsert_dehydrated_device_params_construction() {
        let params = UpsertDehydratedDeviceParams {
            user_id: "@carol:example.com".to_string(),
            device_id: "DEV789".to_string(),
            device_data: serde_json::json!({"data": "payload"}),
            algorithm: "m.dehydration.v1".to_string(),
            account: Some(serde_json::json!({"k": "v"})),
            expires_at: Some(1_700_000_000_000),
        };
        assert_eq!(params.user_id, "@carol:example.com");
        assert_eq!(params.device_id, "DEV789");
        assert_eq!(params.algorithm, "m.dehydration.v1");
        assert!(params.account.is_some());
        assert_eq!(params.expires_at, Some(1_700_000_000_000));
    }

    #[test]
    fn test_upsert_dehydrated_device_params_no_expiry() {
        let params = UpsertDehydratedDeviceParams {
            user_id: "@dave:example.com".to_string(),
            device_id: "DEV000".to_string(),
            device_data: serde_json::json!({}),
            algorithm: "m.dehydration.v1".to_string(),
            account: None,
            expires_at: None,
        };
        assert!(params.account.is_none());
        assert!(params.expires_at.is_none());
    }

    #[test]
    fn test_dehydrated_device_clone_preserves_fields() {
        let device = DehydratedDevice {
            id: 5,
            user_id: "@eve:example.com".to_string(),
            device_id: "DEV111".to_string(),
            device_data: serde_json::json!({"a": 1}),
            algorithm: "algo".to_string(),
            account: None,
            created_ts: 100,
            updated_ts: 200,
            expires_at: None,
        };
        let cloned = device.clone();
        assert_eq!(cloned.id, device.id);
        assert_eq!(cloned.user_id, device.user_id);
        assert_eq!(cloned.device_data, device.device_data);
        assert_eq!(cloned.created_ts, device.created_ts);
    }
}
