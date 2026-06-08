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
        sqlx::query_as!(
            DehydratedDevice,
            r#"
            SELECT id, user_id, device_id, device_data AS "device_data!", algorithm AS "algorithm!",
                   account, created_ts AS "created_ts!", updated_ts AS "updated_ts!", expires_at
            FROM dehydrated_devices
            WHERE user_id = $1
              AND (expires_at IS NULL OR expires_at > $2)
            ORDER BY updated_ts DESC
            LIMIT 1
            "#,
            user_id,
            chrono::Utc::now().timestamp_millis()
        )
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn upsert_for_user(&self, params: UpsertDehydratedDeviceParams) -> Result<DehydratedDevice, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let mut tx = self.pool.begin().await?;

        sqlx::query!(
            r"
            DELETE FROM dehydrated_devices
            WHERE user_id = $1
            ",
            &params.user_id
        )
        .execute(&mut *tx)
        .await?;

        let record = sqlx::query_as!(
            DehydratedDevice,
            r#"
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
            RETURNING id, user_id, device_id, device_data AS "device_data!", algorithm AS "algorithm!",
                      account, created_ts AS "created_ts!", updated_ts AS "updated_ts!", expires_at
            "#,
            &params.user_id,
            &params.device_id,
            &params.device_data,
            &params.algorithm,
            params.account.as_ref(),
            now,
            params.expires_at
        )
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
        sqlx::query!(
            r"
            DELETE FROM to_device_messages
            WHERE recipient_user_id = $1
              AND recipient_device_id IN (
                  SELECT device_id FROM dehydrated_devices WHERE user_id = $1
              )
            ",
            user_id
        )
        .execute(&mut *tx)
        .await?;

        let rows = sqlx::query!(
            r"
            DELETE FROM dehydrated_devices
            WHERE user_id = $1
            ",
            user_id
        )
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

        sqlx::query!(
            r"
            DELETE FROM to_device_messages
            WHERE (recipient_user_id, recipient_device_id) IN (
                SELECT user_id, device_id FROM dehydrated_devices
                WHERE expires_at IS NOT NULL AND expires_at <= $1
            )
            ",
            now
        )
        .execute(&mut *tx)
        .await?;

        let rows = sqlx::query!(
            r"
            DELETE FROM dehydrated_devices
            WHERE expires_at IS NOT NULL AND expires_at <= $1
            ",
            now
        )
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
        let rows = sqlx::query!(
            r#"SELECT stream_id AS "stream_id!", sender_user_id AS "sender_user_id!",
                      event_type AS "event_type!", content AS "content!", message_id
            FROM to_device_messages
            WHERE recipient_user_id = $1
              AND recipient_device_id = $2
              AND stream_id > $3
            ORDER BY stream_id ASC
            LIMIT $4"#,
            user_id,
            device_id,
            since_stream_id,
            limit
        )
        .fetch_all(&*self.pool)
        .await?;

        let mut max_stream_id = since_stream_id;
        let mut events = Vec::with_capacity(rows.len());
        for row in rows {
            if row.stream_id > max_stream_id {
                max_stream_id = row.stream_id;
            }

            let mut event = serde_json::Map::new();
            event.insert("type".to_string(), Value::String(row.event_type));
            event.insert("sender".to_string(), Value::String(row.sender_user_id));
            event.insert("content".to_string(), row.content);
            if let Some(mid) = row.message_id {
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
        let mut tx = self.pool.begin().await?;

        let row = sqlx::query!(
            r#"SELECT id AS "id!", device_data AS "device_data!"
            FROM dehydrated_devices
            WHERE user_id = $1 AND device_id = $2
            FOR UPDATE"#,
            user_id,
            device_id
        )
        .fetch_optional(&mut *tx)
        .await?;

        let Some(row) = row else {
            tx.rollback().await?;
            return Ok(None);
        };

        let id: i64 = row.id;
        let mut device_data: Value = row.device_data;

        let prefix = format!("{algorithm}:");

        // 1) Try one-time keys first; consume on success.
        if let Some(otk_obj) = device_data.get_mut("one_time_keys").and_then(|v| v.as_object_mut()) {
            if let Some(matched_id) = otk_obj.keys().find(|k| k.starts_with(&prefix)).cloned() {
                if let Some(payload) = otk_obj.remove(&matched_id) {
                    let remaining_with_prefix = otk_obj.keys().filter(|k| k.starts_with(&prefix)).count();
                    sqlx::query!(
                        r#"UPDATE dehydrated_devices SET device_data = $1, updated_ts = $2 WHERE id = $3"#,
                        &device_data,
                        chrono::Utc::now().timestamp_millis(),
                        id
                    )
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
