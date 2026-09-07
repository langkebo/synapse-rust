use serde_json::Value;
use sqlx::{Pool, Postgres, Row};
use std::collections::HashSet;
use std::sync::Arc;
use synapse_common::current_timestamp_millis;
use synapse_common::map_database;
use synapse_common::ApiError;

#[derive(Debug, Clone)]
/// The `ToDeviceMessage` type.
pub struct ToDeviceMessage<'a> {
    /// The `sender_user_id` field.
    /// The `sender_device_id` field.
    /// The `recipient_user_id` field.
    /// The `recipient_device_id` field.
    /// The `event_type` field.
    /// The `message_id` field.
    /// The `content` field.
    pub sender_user_id: &'a str,
    /// The `sender_device_id` field.
    /// The `recipient_user_id` field.
    /// The `recipient_device_id` field.
    /// The `event_type` field.
    /// The `message_id` field.
    /// The `content` field.
    pub sender_device_id: &'a str,
    /// The `recipient_user_id` field.
    /// The `recipient_device_id` field.
    /// The `event_type` field.
    /// The `message_id` field.
    /// The `content` field.
    pub recipient_user_id: &'a str,
    /// The `recipient_device_id` field.
    /// The `event_type` field.
    /// The `message_id` field.
    /// The `content` field.
    pub recipient_device_id: &'a str,
    /// The `event_type` field.
    /// The `message_id` field.
    /// The `content` field.
    pub event_type: &'a str,
    /// The `message_id` field.
    /// The `content` field.
    pub message_id: Option<&'a str>,
    /// The `content` field.
    pub content: Value,
}

#[derive(Clone)]
/// The `ToDeviceStorage` type.
pub struct ToDeviceStorage {
    pool: Arc<Pool<Postgres>>,
}

/// (see code)
impl ToDeviceStorage {
    /// See [`new`].
    pub fn new(pool: &Arc<Pool<Postgres>>) -> Self {
        Self { pool: pool.clone() }
    }

    /// See [`device_exists`].
    pub async fn device_exists(&self, user_id: &str, device_id: &str) -> Result<bool, ApiError> {
        // Accept either a regular device or a (non-expired) dehydrated device
        // (MSC3814) as a valid recipient — without this, to-device messages
        // addressed to a dehydrated device id are silently dropped.
        let now = current_timestamp_millis();
        let result = sqlx::query(
            r"
            SELECT 1 AS hit FROM devices
                WHERE user_id = $1 AND device_id = $2
            UNION ALL
            SELECT 1 AS hit FROM dehydrated_devices
                WHERE user_id = $1 AND device_id = $2
                  AND (expires_at IS NULL OR expires_at > $3)
            LIMIT 1
            ",
        )
        .bind(user_id)
        .bind(device_id)
        .bind(now)
        .fetch_optional(&*self.pool)
        .await
        .map_err(map_database!("device_exists"))?;

        Ok(result.is_some())
    }

    /// See [`record_transaction`].
    pub async fn record_transaction(
        &self,
        sender_user_id: &str,
        sender_device_id: &str,
        message_id: &str,
    ) -> Result<bool, ApiError> {
        let now = current_timestamp_millis();
        let row = sqlx::query(
            r"
            INSERT INTO to_device_transactions (sender_user_id, sender_device_id, message_id, created_ts)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (sender_user_id, sender_device_id, message_id) DO NOTHING
            RETURNING id
            ",
        )
        .bind(sender_user_id)
        .bind(sender_device_id)
        .bind(message_id)
        .bind(now)
        .fetch_optional(&*self.pool)
        .await
        .map_err(map_database!("record_transaction"))?;
        Ok(row.is_some())
    }

    /// See [`cleanup_old_transactions`].
    pub async fn cleanup_old_transactions(&self, max_age_ms: i64) -> Result<u64, ApiError> {
        let cutoff = current_timestamp_millis() - max_age_ms;
        let result = sqlx::query(
            r"
            DELETE FROM to_device_transactions
            WHERE created_ts < $1
            ",
        )
        .bind(cutoff)
        .execute(&*self.pool)
        .await
        .map_err(map_database!("cleanup_old_transactions"))?;

        Ok(result.rows_affected())
    }

    /// See [`add_message`].
    pub async fn add_message(&self, msg: ToDeviceMessage<'_>) -> Result<(), ApiError> {
        if !self.device_exists(msg.recipient_user_id, msg.recipient_device_id).await? {
            ::tracing::warn!(
                "Skipping to-device message for non-existent device: {}:{}",
                msg.recipient_user_id,
                msg.recipient_device_id
            );
            return Ok(());
        }

        let now = current_timestamp_millis();
        sqlx::query(
            r"
            INSERT INTO to_device_messages (
                sender_user_id,
                sender_device_id,
                recipient_user_id,
                recipient_device_id,
                event_type,
                content,
                message_id,
                stream_id,
                created_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, nextval('to_device_stream_id_seq'), $8)
            ",
        )
        .bind(msg.sender_user_id)
        .bind(msg.sender_device_id)
        .bind(msg.recipient_user_id)
        .bind(msg.recipient_device_id)
        .bind(msg.event_type)
        .bind(msg.content)
        .bind(msg.message_id)
        .bind(now)
        .execute(&*self.pool)
        .await
        .map_err(map_database!("add_message"))?;

        Ok(())
    }

    /// Batch insert of to-device messages. Eliminates the N+1 round-trips that
    /// `add_message` would incur when sending to many devices.
    ///
    /// Each message's recipient device is still checked for existence (so we
    /// silently skip recipients whose devices no longer exist, matching the
    /// behaviour of `add_message`). All checked-and-existing messages are then
    /// persisted in a single `INSERT ... VALUES (...), (...), ...` statement.
    ///
    /// The existence check is done once for all distinct (user_id, device_id)
    /// pairs via a single query (`device_exists_batch`), replacing the
    /// per-message round-trip pattern.
    ///
    /// Returns the number of messages actually inserted.
    pub async fn add_messages_batch(&self, messages: &[ToDeviceMessage<'_>]) -> Result<usize, ApiError> {
        if messages.is_empty() {
            return Ok(0);
        }

        // Stage 1: collect distinct (user_id, device_id) pairs and filter to
        // those that exist in a single round-trip.
        let mut distinct: Vec<(String, String)> = Vec::with_capacity(messages.len());
        let mut seen: HashSet<(String, String)> = HashSet::new();
        for msg in messages {
            let key = (msg.recipient_user_id.to_string(), msg.recipient_device_id.to_string());
            if seen.insert(key.clone()) {
                distinct.push(key);
            }
        }
        let existing = self.device_exists_batch(&distinct).await?;

        // Stage 2: keep only messages whose (user, device) exists, with warn
        // for skipped recipients to match add_message semantics.
        let mut kept: Vec<&ToDeviceMessage<'_>> = Vec::with_capacity(messages.len());
        for msg in messages {
            if existing.contains(&(msg.recipient_user_id.to_string(), msg.recipient_device_id.to_string())) {
                kept.push(msg);
            } else {
                tracing::warn!(
                    "Skipping to-device message for non-existent device: {}:{}",
                    msg.recipient_user_id,
                    msg.recipient_device_id
                );
            }
        }
        if kept.is_empty() {
            return Ok(0);
        }

        // Stage 3: batch insert. stream_id is assigned by the DB via nextval()
        // for each row (one nextval() call per row, even in a single statement).
        let now = current_timestamp_millis();
        let mut qb = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "INSERT INTO to_device_messages \
             (sender_user_id, sender_device_id, recipient_user_id, recipient_device_id, \
              event_type, content, message_id, stream_id, created_ts) ",
        );
        qb.push_values(kept.iter().copied(), |mut b, msg| {
            b.push_bind(msg.sender_user_id)
                .push_bind(msg.sender_device_id)
                .push_bind(msg.recipient_user_id)
                .push_bind(msg.recipient_device_id)
                .push_bind(msg.event_type)
                .push_bind(&msg.content)
                .push_bind(msg.message_id)
                .push("nextval('to_device_stream_id_seq')")
                .push_bind(now);
        });

        let result = qb.build().execute(&*self.pool).await.map_err(map_database!("add_messages_batch"))?;

        Ok(result.rows_affected() as usize)
    }

    /// Batch check whether (user_id, device_id) pairs exist in either
    /// `devices` or (non-expired) `dehydrated_devices` (MSC3814).
    ///
    /// Single round-trip using `unnest($1::text[], $2::text[])` to expand the
    /// pairs and `UNION` to fold both tables. Returns the set of pairs that
    /// have at least one match.
    pub async fn device_exists_batch(&self, pairs: &[(String, String)]) -> Result<HashSet<(String, String)>, ApiError> {
        if pairs.is_empty() {
            return Ok(HashSet::new());
        }
        let user_ids: Vec<String> = pairs.iter().map(|(u, _)| u.clone()).collect();
        let device_ids: Vec<String> = pairs.iter().map(|(_, d)| d.clone()).collect();
        let now = current_timestamp_millis();

        let rows = sqlx::query(
            r"
            SELECT user_id, device_id FROM devices
                WHERE (user_id, device_id) IN (
                    SELECT * FROM unnest($1::text[], $2::text[]) AS t(user_id, device_id)
                )
            UNION
            SELECT user_id, device_id FROM dehydrated_devices
                WHERE (user_id, device_id) IN (
                    SELECT * FROM unnest($1::text[], $2::text[]) AS t(user_id, device_id)
                )
                  AND (expires_at IS NULL OR expires_at > $3)
            ",
        )
        .bind(&user_ids)
        .bind(&device_ids)
        .bind(now)
        .fetch_all(&*self.pool)
        .await
        .map_err(map_database!("device_exists_batch"))?;

        let mut out = HashSet::with_capacity(rows.len());
        for row in rows {
            let user_id: String = row.get("user_id");
            let device_id: String = row.get("device_id");
            out.insert((user_id, device_id));
        }
        Ok(out)
    }

    /// See [`get_messages`].
    pub async fn get_messages(&self, user_id: &str, device_id: &str) -> Result<Vec<Value>, ApiError> {
        let rows = sqlx::query(
            r"
            SELECT id, stream_id, sender_user_id, event_type, content, message_id, created_ts
            FROM to_device_messages
            WHERE recipient_user_id = $1 AND recipient_device_id = $2
            ORDER BY stream_id ASC
            ",
        )
        .bind(user_id)
        .bind(device_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(map_database!("get_messages"))?;

        let mut messages = Vec::new();
        for row in rows {
            let event_type: String = row.get("event_type");
            let sender_user_id: String = row.get("sender_user_id");
            let content: Value = row.get("content");
            let message_id: Option<String> = row.get("message_id");
            messages.push(serde_json::json!({
                "type": event_type,
                "sender": sender_user_id,
                "content": content
            }));
            if let Some(mid) = message_id {
                if let Some(obj) = messages.last_mut().and_then(|v| v.as_object_mut()) {
                    obj.insert("message_id".to_string(), serde_json::json!(mid));
                }
            }
        }

        Ok(messages)
    }

    /// See [`get_messages_since`].
    pub async fn get_messages_since(
        &self,
        user_id: &str,
        device_id: &str,
        since_stream_id: i64,
        limit: i64,
    ) -> Result<(Vec<Value>, i64), ApiError> {
        let rows = sqlx::query(
            r"
            SELECT sender_user_id, event_type, content, message_id, stream_id
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
        .await
        .map_err(map_database!("get_messages_since"))?;

        let mut max_stream_id = since_stream_id;
        let mut messages = Vec::with_capacity(rows.len());
        for row in rows {
            let sender_user_id: String = row.get("sender_user_id");
            let event_type: String = row.get("event_type");
            let content: Value = row.get("content");
            let message_id: Option<String> = row.get("message_id");
            let stream_id: i64 = row.get("stream_id");
            if stream_id > max_stream_id {
                max_stream_id = stream_id;
            }

            let mut msg = serde_json::json!({
                "type": event_type,
                "sender": sender_user_id,
                "content": content
            });
            if let Some(mid) = message_id {
                if let Some(obj) = msg.as_object_mut() {
                    obj.insert("message_id".to_string(), serde_json::json!(mid));
                }
            }
            messages.push(msg);
        }

        Ok((messages, max_stream_id))
    }

    /// See [`get_current_stream_id`].
    pub async fn get_current_stream_id(&self, user_id: &str, device_id: &str) -> Result<i64, ApiError> {
        let max_id: Option<i64> = sqlx::query_scalar(
            r"
            SELECT COALESCE(MAX(stream_id), 0)
            FROM to_device_messages
            WHERE recipient_user_id = $1
              AND recipient_device_id = $2
            ",
        )
        .bind(user_id)
        .bind(device_id)
        .fetch_optional(&*self.pool)
        .await
        .map_err(map_database!("get_current_stream_id"))?;

        Ok(max_id.unwrap_or(0))
    }

    /// See [`has_messages_since`].
    pub async fn has_messages_since(
        &self,
        user_id: &str,
        device_id: &str,
        since_stream_id: i64,
    ) -> Result<bool, ApiError> {
        let row = sqlx::query(
            r"
            SELECT 1
            FROM to_device_messages
            WHERE recipient_user_id = $1
              AND recipient_device_id = $2
              AND stream_id > $3
            LIMIT 1
            ",
        )
        .bind(user_id)
        .bind(device_id)
        .bind(since_stream_id)
        .fetch_optional(&*self.pool)
        .await
        .map_err(map_database!("has_messages_since"))?;

        Ok(row.is_some())
    }

    /// See [`get_and_delete_messages`].
    pub async fn get_and_delete_messages(&self, user_id: &str, device_id: &str) -> Result<Vec<Value>, ApiError> {
        // E2EE-10: Wrap DELETE...RETURNING in a CTE so we can apply
        // ORDER BY stream_id ASC to the returned rows.  PostgreSQL's
        // DELETE ... RETURNING does not guarantee row order; without
        // this, to-device messages may be delivered out of sequence,
        // causing race conditions in key exchange protocols.
        let rows = sqlx::query(
            r"
            WITH deleted AS (
                DELETE FROM to_device_messages
                WHERE recipient_user_id = $1 AND recipient_device_id = $2
                RETURNING id, stream_id, sender_user_id, event_type, content, message_id, created_ts
            )
            SELECT id, stream_id, sender_user_id, event_type, content, message_id, created_ts
            FROM deleted
            ORDER BY stream_id ASC
            ",
        )
        .bind(user_id)
        .bind(device_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(map_database!("get_and_delete_messages"))?;

        let mut messages = Vec::new();
        for row in rows {
            let event_type: String = row.get("event_type");
            let sender_user_id: String = row.get("sender_user_id");
            let content: Value = row.get("content");
            let message_id: Option<String> = row.get("message_id");
            messages.push(serde_json::json!({
                "type": event_type,
                "sender": sender_user_id,
                "content": content
            }));
            if let Some(mid) = message_id {
                if let Some(obj) = messages.last_mut().and_then(|v| v.as_object_mut()) {
                    obj.insert("message_id".to_string(), serde_json::json!(mid));
                }
            }
        }

        Ok(messages)
    }

    /// See [`delete_messages`].
    pub async fn delete_messages(&self, ids: &[i64]) -> Result<(), ApiError> {
        sqlx::query(
            r"
            DELETE FROM to_device_messages
            WHERE id = ANY($1)
            ",
        )
        .bind(ids)
        .execute(&*self.pool)
        .await
        .map_err(map_database!("delete_messages"))?;

        Ok(())
    }

    /// See [`delete_messages_up_to`].
    pub async fn delete_messages_up_to(&self, user_id: &str, device_id: &str, stream_id: i64) -> Result<(), ApiError> {
        sqlx::query(
            r"
            DELETE FROM to_device_messages
            WHERE recipient_user_id = $1
              AND recipient_device_id = $2
              AND stream_id <= $3
            ",
        )
        .bind(user_id)
        .bind(device_id)
        .bind(stream_id)
        .execute(&*self.pool)
        .await
        .map_err(map_database!("delete_messages_up_to"))?;

        Ok(())
    }
}
