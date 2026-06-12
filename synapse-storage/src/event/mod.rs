pub mod batch;
pub(crate) mod models;
pub mod state;

pub use models::*;

use std::sync::Arc;

use sqlx::{Pool, Postgres};

impl EventStorage {
    pub fn new(pool: &Arc<Pool<Postgres>>, server_name: String) -> Self {
        Self { pool: pool.clone(), server_name }
    }

    pub async fn create_event(
        &self,
        params: CreateEventParams,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> Result<RoomEvent, sqlx::Error> {
        let query = r"
            INSERT INTO events (event_id, room_id, sender, user_id, event_type, content, state_key, origin_server_ts, is_redacted)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, false)
            RETURNING event_id, room_id, sender as user_id, event_type, content, state_key,
                      COALESCE(depth, 0) as depth, origin_server_ts, origin_server_ts as processed_at,
                      0::BIGINT as not_before, 'pending' as status, null as reference_image,
                      'self' as origin, stream_ordering
            ";

        if let Some(tx) = tx {
            sqlx::query_as(query)
                .bind(&params.event_id)
                .bind(&params.room_id)
                .bind(&params.user_id)
                .bind(&params.user_id)
                .bind(&params.event_type)
                .bind(&params.content)
                .bind(params.state_key.as_deref())
                .bind(params.origin_server_ts)
                .fetch_one(&mut **tx)
                .await
        } else {
            sqlx::query_as(query)
                .bind(&params.event_id)
                .bind(&params.room_id)
                .bind(&params.user_id)
                .bind(&params.user_id)
                .bind(&params.event_type)
                .bind(&params.content)
                .bind(params.state_key.as_deref())
                .bind(params.origin_server_ts)
                .fetch_one(&*self.pool)
                .await
        }
    }

    pub async fn get_event(&self, event_id: &str) -> Result<Option<RoomEvent>, sqlx::Error> {
        let event = sqlx::query_as::<_, RoomEvent>(
            r"
            SELECT event_id, room_id, sender as user_id, event_type, content, state_key,
                   COALESCE(depth, 0) as depth, origin_server_ts, origin_server_ts as processed_at,
                   COALESCE(not_before, 0) as not_before, status, reference_image, COALESCE(origin, 'self') as origin, stream_ordering
            FROM events WHERE event_id = $1
            ",
        )
        .bind(event_id)
        .fetch_optional(&*self.pool)
        .await?;
        Ok(event)
    }

    pub async fn add_ephemeral_event(
        &self,
        room_id: &str,
        user_id: &str,
        event_type: &str,
        content: &serde_json::Value,
        stream_id: i64,
    ) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        self.upsert_ephemeral_event(room_id, user_id, event_type, content, stream_id, now, None).await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_ephemeral_event(
        &self,
        room_id: &str,
        user_id: &str,
        event_type: &str,
        content: &serde_json::Value,
        stream_id: i64,
        created_ts: i64,
        expires_at: Option<i64>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r"
            INSERT INTO room_ephemeral (room_id, event_type, user_id, content, stream_id, created_ts, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (room_id, event_type, user_id) DO UPDATE
            SET content = EXCLUDED.content,
                stream_id = EXCLUDED.stream_id,
                created_ts = EXCLUDED.created_ts,
                expires_at = EXCLUDED.expires_at
            ",
        )
        .bind(room_id)
        .bind(event_type)
        .bind(user_id)
        .bind(content)
        .bind(stream_id)
        .bind(created_ts)
        .bind(expires_at)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_ephemeral_event(
        &self,
        room_id: &str,
        event_type: &str,
        user_id: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r"
            DELETE FROM room_ephemeral
            WHERE room_id = $1 AND event_type = $2 AND user_id = $3
            ",
        )
        .bind(room_id)
        .bind(event_type)
        .bind(user_id)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_ephemeral_events(
        &self,
        room_id: &str,
        now: i64,
        limit: i64,
    ) -> Result<Vec<RoomEphemeralEvent>, sqlx::Error> {
        sqlx::query_as::<_, RoomEphemeralEvent>(
            r"
            SELECT event_type, user_id, content, stream_id, created_ts
            FROM room_ephemeral
            WHERE room_id = $1
              AND (expires_at IS NULL OR expires_at > $2)
            ORDER BY stream_id DESC
            LIMIT $3
            ",
        )
        .bind(room_id)
        .bind(now)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn delete_events_before(&self, room_id: &str, timestamp: i64) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            "DELETE FROM events WHERE room_id = $1 AND origin_server_ts < $2 AND event_type != 'm.room.create'",
        )
        .bind(room_id)
        .bind(timestamp)
        .execute(&*self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    pub async fn get_room_events(&self, room_id: &str, limit: i64) -> Result<Vec<RoomEvent>, sqlx::Error> {
        let events = sqlx::query_as(
            r"
            SELECT event_id, room_id, COALESCE(user_id, sender) as user_id, event_type, content, state_key,
                   COALESCE(depth, 0) as depth, COALESCE(origin_server_ts, 0) as origin_server_ts, COALESCE(origin_server_ts, 0) as processed_at,
                   COALESCE(not_before, 0) as not_before, status, reference_image, COALESCE(NULLIF(NULLIF(BTRIM(origin), ''), 'undefined'), 'self') as origin, stream_ordering
            FROM events WHERE room_id = $1
            ORDER BY origin_server_ts DESC, stream_ordering DESC NULLS LAST, event_id DESC
            LIMIT $2
            ",
        )
        .bind(room_id)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;
        Ok(events)
    }

    pub async fn get_room_events_paginated(
        &self,
        room_id: &str,
        from: Option<i64>,
        limit: i64,
        direction: &str,
    ) -> Result<Vec<RoomEvent>, sqlx::Error> {
        let events = match (direction, from) {
            ("f", Some(from_ts)) => {
                sqlx::query_as(
                    r"
                    SELECT event_id, room_id, COALESCE(user_id, sender) as user_id, event_type, content, state_key,
                           COALESCE(depth, 0) as depth, COALESCE(origin_server_ts, 0) as origin_server_ts, COALESCE(origin_server_ts, 0) as processed_at,
                           COALESCE(not_before, 0) as not_before, status, reference_image, COALESCE(NULLIF(NULLIF(BTRIM(origin), ''), 'undefined'), 'self') as origin, stream_ordering
                    FROM events
                    WHERE room_id = $1 AND origin_server_ts > $2
                    ORDER BY origin_server_ts ASC
                    LIMIT $3
                    ",
                )
                .bind(room_id)
                .bind(from_ts)
                .bind(limit)
                .fetch_all(&*self.pool)
                .await?
            }
            ("f", None) => {
                sqlx::query_as(
                    r"
                    SELECT event_id, room_id, COALESCE(user_id, sender) as user_id, event_type, content, state_key,
                           COALESCE(depth, 0) as depth, COALESCE(origin_server_ts, 0) as origin_server_ts, COALESCE(origin_server_ts, 0) as processed_at,
                           COALESCE(not_before, 0) as not_before, status, reference_image, COALESCE(NULLIF(NULLIF(BTRIM(origin), ''), 'undefined'), 'self') as origin, stream_ordering
                    FROM events
                    WHERE room_id = $1
                    ORDER BY origin_server_ts ASC
                    LIMIT $2
                    ",
                )
                .bind(room_id)
                .bind(limit)
                .fetch_all(&*self.pool)
                .await?
            }
            (_, Some(from_ts)) => {
                sqlx::query_as(
                    r"
                    SELECT event_id, room_id, COALESCE(user_id, sender) as user_id, event_type, content, state_key,
                           COALESCE(depth, 0) as depth, COALESCE(origin_server_ts, 0) as origin_server_ts, COALESCE(origin_server_ts, 0) as processed_at,
                           COALESCE(not_before, 0) as not_before, status, reference_image, COALESCE(NULLIF(NULLIF(BTRIM(origin), ''), 'undefined'), 'self') as origin, stream_ordering
                    FROM events
                    WHERE room_id = $1 AND origin_server_ts < $2
                    ORDER BY origin_server_ts DESC
                    LIMIT $3
                    ",
                )
                .bind(room_id)
                .bind(from_ts)
                .bind(limit)
                .fetch_all(&*self.pool)
                .await?
            }
            (_, None) => {
                sqlx::query_as(
                    r"
                    SELECT event_id, room_id, COALESCE(user_id, sender) as user_id, event_type, content, state_key,
                           COALESCE(depth, 0) as depth, COALESCE(origin_server_ts, 0) as origin_server_ts, COALESCE(origin_server_ts, 0) as processed_at,
                           COALESCE(not_before, 0) as not_before, status, reference_image, COALESCE(NULLIF(NULLIF(BTRIM(origin), ''), 'undefined'), 'self') as origin, stream_ordering
                    FROM events
                    WHERE room_id = $1
                    ORDER BY origin_server_ts DESC
                    LIMIT $2
                    ",
                )
                .bind(room_id)
                .bind(limit)
                .fetch_all(&*self.pool)
                .await?
            }
        };

        Ok(events)
    }

    /// Find the event closest to a given timestamp
    /// Used by MSC3030 timestamp_to_event endpoint
    pub async fn find_event_by_timestamp(
        &self,
        room_id: &str,
        ts: i64,
    ) -> Result<Option<serde_json::Value>, sqlx::Error> {
        // First try to find an event exactly at or before the timestamp
        let event = sqlx::query_as::<_, (String, i64)>(
            r"
            SELECT event_id, origin_server_ts
            FROM events
            WHERE room_id = $1
              AND origin_server_ts IS NOT NULL
              AND origin_server_ts <= $2
            ORDER BY origin_server_ts DESC
            LIMIT 1
            ",
        )
        .bind(room_id)
        .bind(ts)
        .fetch_optional(&*self.pool)
        .await?;

        if let Some((event_id, origin_server_ts)) = event {
            // Get the full event content
            let full_event = sqlx::query_as::<_, (serde_json::Value,)>(
                r"
                SELECT content
                FROM events
                WHERE event_id = $1
                ",
            )
            .bind(&event_id)
            .fetch_optional(&*self.pool)
            .await?;

            if let Some((content,)) = full_event {
                let mut result = serde_json::Map::new();
                result.insert("event_id".to_string(), serde_json::Value::String(event_id));
                result.insert("origin_server_ts".to_string(), serde_json::Value::Number(origin_server_ts.into()));
                // Merge content into result
                if let serde_json::Value::Object(obj) = content {
                    for (k, v) in obj {
                        result.insert(k, v);
                    }
                }
                return Ok(Some(serde_json::Value::Object(result)));
            }
        }

        Ok(None)
    }

    pub async fn get_room_events_by_type(
        &self,
        room_id: &str,
        event_type: &str,
        limit: i64,
    ) -> Result<Vec<RoomEvent>, sqlx::Error> {
        let events = sqlx::query_as(
            r"
            SELECT event_id, room_id, COALESCE(user_id, sender) as user_id, event_type, content, state_key,
                   COALESCE(depth, 0) as depth, COALESCE(origin_server_ts, 0) as origin_server_ts, COALESCE(origin_server_ts, 0) as processed_at,
                   COALESCE(not_before, 0) as not_before, status, reference_image, COALESCE(NULLIF(NULLIF(BTRIM(origin), ''), 'undefined'), 'self') as origin, stream_ordering
            FROM events WHERE room_id = $1 AND event_type = $2
            ORDER BY origin_server_ts DESC
            LIMIT $3
            ",
        )
        .bind(room_id)
        .bind(event_type)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;
        Ok(events)
    }

    pub async fn get_sender_events(&self, user_id: &str, limit: i64) -> Result<Vec<RoomEvent>, sqlx::Error> {
        let events = sqlx::query_as(
            r"
            SELECT event_id, room_id, COALESCE(user_id, sender) as user_id, event_type, content, state_key,
                   COALESCE(depth, 0) as depth, COALESCE(origin_server_ts, 0) as origin_server_ts, COALESCE(origin_server_ts, 0) as processed_at,
                   COALESCE(not_before, 0) as not_before, status, reference_image, COALESCE(NULLIF(NULLIF(BTRIM(origin), ''), 'undefined'), 'self') as origin, stream_ordering
            FROM events WHERE COALESCE(user_id, sender) = $1
            ORDER BY origin_server_ts DESC
            LIMIT $2
            ",
        )
        .bind(user_id)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;
        Ok(events)
    }

    pub async fn get_room_message_count(&self, room_id: &str) -> Result<i64, sqlx::Error> {
        let count = sqlx::query_scalar::<_, i64>(
            r"
            SELECT COALESCE(COUNT(*), 0) FROM events WHERE room_id = $1 AND event_type = 'm.room.message'
            ",
        )
        .bind(room_id)
        .fetch_one(&*self.pool)
        .await?;
        Ok(count)
    }

    pub async fn get_total_message_count(&self) -> Result<i64, sqlx::Error> {
        let count = sqlx::query_scalar::<_, i64>(
            r"
            SELECT COALESCE(COUNT(*), 0) FROM events WHERE event_type = 'm.room.message'
            ",
        )
        .fetch_one(&*self.pool)
        .await?;
        Ok(count)
    }

    pub async fn delete_room_events(&self, room_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r"
            DELETE FROM events WHERE room_id = $1
            ",
        )
        .bind(room_id)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Report / redact
    // -----------------------------------------------------------------------

    pub async fn report_event(
        &self,
        event_id: &str,
        room_id: &str,
        _reported_user_id: &str,
        reporter_user_id: &str,
        reason: Option<&str>,
        score: i32,
    ) -> Result<i64, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let row = sqlx::query_as::<_, EventReportId>(
            r"
            INSERT INTO event_reports (event_id, room_id, reporter_user_id, reason, score, received_ts)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id
            ",
        )
        .bind(event_id)
        .bind(room_id)
        .bind(reporter_user_id)
        .bind(reason)
        .bind(score)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;
        Ok(row.id)
    }

    pub async fn update_event_report_score(&self, report_id: i64, score: i32) -> Result<(), sqlx::Error> {
        sqlx::query(
            r"
            UPDATE event_reports SET score = $1 WHERE id = $2
            ",
        )
        .bind(score)
        .bind(report_id)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn update_event_report_score_by_event(&self, event_id: &str, score: i32) -> Result<(), sqlx::Error> {
        sqlx::query(
            r"
            UPDATE event_reports SET score = $1 WHERE event_id = $2
            ",
        )
        .bind(score)
        .bind(event_id)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_event_report(&self, event_id: &str) -> Result<Vec<EventReport>, sqlx::Error> {
        sqlx::query_as::<_, EventReport>(
            r"
            SELECT id, event_id, room_id, reporter_user_id, reason, score, received_ts, resolved_at, resolved_by
            FROM event_reports WHERE event_id = $1 ORDER BY received_ts DESC
            ",
        )
        .bind(event_id)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn redact_event_content(&self, event_id: &str) -> Result<(), sqlx::Error> {
        let redacted_content = serde_json::json!({});
        sqlx::query("UPDATE events SET content = $1, is_redacted = true WHERE event_id = $2")
            .bind(redacted_content)
            .bind(event_id)
            .execute(&*self.pool)
            .await?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Event signatures
    // -----------------------------------------------------------------------

    /// Save (upsert) an event signature.
    #[allow(clippy::too_many_arguments)]
    pub async fn save_event_signature(
        &self,
        event_id: &str,
        user_id: &str,
        device_id: &str,
        signature: &str,
        key_id: &str,
        algorithm: &str,
        created_ts: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r"
            INSERT INTO event_signatures (id, event_id, user_id, device_id, signature, key_id, algorithm, created_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (event_id, user_id, device_id, key_id) DO UPDATE
            SET signature = EXCLUDED.signature,
                algorithm = EXCLUDED.algorithm,
                created_ts = EXCLUDED.created_ts
            ",
        )
        .bind(uuid::Uuid::new_v4())
        .bind(event_id)
        .bind(user_id)
        .bind(device_id)
        .bind(signature)
        .bind(key_id)
        .bind(algorithm)
        .bind(created_ts)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    /// Get all signatures for an event.
    pub async fn get_event_signatures(&self, event_id: &str) -> Result<Vec<EventSignature>, sqlx::Error> {
        sqlx::query_as::<_, EventSignature>(
            r"
            SELECT id, event_id, user_id, device_id, signature, key_id, created_ts
            FROM event_signatures
            WHERE event_id = $1
            ",
        )
        .bind(event_id)
        .fetch_all(&*self.pool)
        .await
    }

    // -----------------------------------------------------------------------
    // Power levels
    // -----------------------------------------------------------------------

    pub async fn upsert_power_levels_event(
        &self,
        event_id: &str,
        room_id: &str,
        user_id: &str,
        content: serde_json::Value,
        origin_server_ts: i64,
        sender: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r"
            INSERT INTO events (event_id, room_id, user_id, event_type, content, state_key, origin_server_ts, sender, unsigned)
            VALUES ($1, $2, $3, 'm.room.power_levels', $4, '', $5, $6, '{}'::jsonb)
            ON CONFLICT (event_id) DO UPDATE SET content = $4
            ",
        )
        .bind(event_id)
        .bind(room_id)
        .bind(user_id)
        .bind(content)
        .bind(origin_server_ts)
        .bind(sender)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Context
    // -----------------------------------------------------------------------

    pub async fn get_events_before_context(
        &self,
        room_id: &str,
        before_ts: i64,
        limit: i64,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let rows = sqlx::query(
            r"
            SELECT event_id, event_type AS type, COALESCE(user_id, sender) AS sender, content, origin_server_ts
            FROM events
            WHERE room_id = $1 AND origin_server_ts < $2
            ORDER BY origin_server_ts DESC
            LIMIT $3
            ",
        )
        .bind(room_id)
        .bind(before_ts)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;

        use sqlx::Row;
        Ok(rows
            .iter()
            .map(|r| {
                serde_json::json!({
                    "event_id": r.get::<String, _>("event_id"),
                    "type": r.get::<String, _>("type"),
                    "sender": r.get::<String, _>("sender"),
                    "content": r.get::<serde_json::Value, _>("content"),
                    "origin_server_ts": r.get::<i64, _>("origin_server_ts")
                })
            })
            .collect())
    }

    pub async fn get_events_after_context(
        &self,
        room_id: &str,
        after_ts: i64,
        limit: i64,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let rows = sqlx::query(
            r"
            SELECT event_id, event_type AS type, COALESCE(user_id, sender) AS sender, content, origin_server_ts
            FROM events
            WHERE room_id = $1 AND origin_server_ts > $2
            ORDER BY origin_server_ts ASC
            LIMIT $3
            ",
        )
        .bind(room_id)
        .bind(after_ts)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;

        use sqlx::Row;
        Ok(rows
            .iter()
            .map(|r| {
                serde_json::json!({
                    "event_id": r.get::<String, _>("event_id"),
                    "type": r.get::<String, _>("type"),
                    "sender": r.get::<String, _>("sender"),
                    "content": r.get::<serde_json::Value, _>("content"),
                    "origin_server_ts": r.get::<i64, _>("origin_server_ts")
                })
            })
            .collect())
    }

    // -----------------------------------------------------------------------
    // Search
    // -----------------------------------------------------------------------

    pub async fn search_room_messages_admin(
        &self,
        room_id: &str,
        search_pattern: &str,
        limit: i64,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let rows = sqlx::query(
            r"
            SELECT event_id, event_type, sender, content, origin_server_ts
            FROM events
            WHERE room_id = $1 AND event_type = 'm.room.message' AND LOWER(content::text) LIKE $2 AND is_redacted = false
            ORDER BY origin_server_ts DESC
            LIMIT $3
            ",
        )
        .bind(room_id)
        .bind(search_pattern)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;

        use sqlx::Row;
        Ok(rows
            .iter()
            .map(|r| {
                serde_json::json!({
                    "event_id": r.get::<String, _>("event_id"),
                    "type": r.get::<String, _>("event_type"),
                    "sender": r.get::<String, _>("sender"),
                    "content": r.get::<serde_json::Value, _>("content"),
                    "origin_server_ts": r.get::<i64, _>("origin_server_ts")
                })
            })
            .collect())
    }

    // -----------------------------------------------------------------------
    // Extremities
    // -----------------------------------------------------------------------

    pub async fn get_forward_extremities_count(&self, room_id: &str) -> Result<i64, sqlx::Error> {
        let count: i64 = sqlx::query_scalar(
            r"
            SELECT COUNT(*) FROM events
            WHERE room_id = $1
            AND state_key IS NOT NULL
            AND event_id NOT IN (
                SELECT content->>'prev_event_id' FROM events
                WHERE room_id = $1 AND content->>'prev_event_id' IS NOT NULL
            )
            ",
        )
        .bind(room_id)
        .fetch_one(&*self.pool)
        .await?;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_room_event_struct() {
        let event = RoomEvent {
            event_id: "$event123:example.com".to_string(),
            room_id: "!room123:example.com".to_string(),
            user_id: "@alice:example.com".to_string(),
            event_type: "m.room.message".to_string(),
            content: json!({"msgtype": "m.text", "body": "Hello"}),
            state_key: None,
            depth: 1,
            origin_server_ts: 1234567890,
            processed_ts: 1234567891,
            not_before: 0,
            status: None,
            reference_image: None,
            origin: "self".to_string(),
            stream_ordering: Some(1),
        };

        assert_eq!(event.event_id, "$event123:example.com");
        assert_eq!(event.room_id, "!room123:example.com");
        assert_eq!(event.event_type, "m.room.message");
        assert!(event.state_key.is_none());
    }

    #[test]
    fn test_state_event_struct() {
        let event = StateEvent {
            event_id: "$state123:example.com".to_string(),
            room_id: "!room123:example.com".to_string(),
            sender: "@alice:example.com".to_string(),
            event_type: Some("m.room.member".to_string()),
            content: json!({"membership": "join"}),
            state_key: Some("@bob:example.com".to_string()),
            unsigned: None,
            is_redacted: Some(false),
            origin_server_ts: 1234567890,
            depth: Some(1),
            processed_ts: Some(1234567891),
            not_before: Some(0),
            status: None,
            reference_image: None,
            origin: Some("self".to_string()),
            user_id: Some("@alice:example.com".to_string()),
            stream_ordering: Some(1),
        };

        assert_eq!(event.event_type, Some("m.room.member".to_string()));
        assert!(event.state_key.is_some());
        assert_eq!(event.state_key.unwrap(), "@bob:example.com");
    }

    #[test]
    fn test_create_event_params() {
        let params = CreateEventParams {
            event_id: "$new_event:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            user_id: "@user:example.com".to_string(),
            event_type: "m.room.message".to_string(),
            content: json!({"msgtype": "m.text", "body": "Test"}),
            state_key: None,
            origin_server_ts: 1234567890,
        };

        assert_eq!(params.event_id, "$new_event:example.com");
        assert_eq!(params.event_type, "m.room.message");
        assert!(params.state_key.is_none());
    }

    #[test]
    fn test_create_event_params_with_state_key() {
        let params = CreateEventParams {
            event_id: "$state_event:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            user_id: "@user:example.com".to_string(),
            event_type: "m.room.member".to_string(),
            content: json!({"membership": "join"}),
            state_key: Some("@user:example.com".to_string()),
            origin_server_ts: 1234567890,
        };

        assert_eq!(params.event_type, "m.room.member");
        assert!(params.state_key.is_some());
    }

    #[test]
    fn test_event_report_struct() {
        let report = EventReport {
            id: 1,
            event_id: "$event:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            reporter_user_id: "@reporter:example.com".to_string(),
            reason: Some("Spam".to_string()),
            score: -50,
            received_ts: 1234567890,
            resolved_ts: None,
            resolved_by: None,
        };

        assert_eq!(report.id, 1);
        assert_eq!(report.reason, Some("Spam".to_string()));
        assert!(report.resolved_ts.is_none());
    }

    #[test]
    fn test_event_report_id_struct() {
        let report_id = EventReportId { id: 42 };
        assert_eq!(report_id.id, 42);
    }

    #[test]
    fn test_event_content_serialization() {
        let content = json!({
            "msgtype": "m.text",
            "body": "Hello, World!",
            "format": "org.matrix.custom.html",
            "formatted_body": "<b>Hello, World!</b>"
        });

        let event = RoomEvent {
            event_id: "$event:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            user_id: "@user:example.com".to_string(),
            event_type: "m.room.message".to_string(),
            content,
            state_key: None,
            depth: 0,
            origin_server_ts: 0,
            processed_ts: 0,
            not_before: 0,
            status: None,
            reference_image: None,
            origin: "self".to_string(),
            stream_ordering: Some(0),
        };

        assert_eq!(event.content["msgtype"], "m.text");
        assert_eq!(event.content["body"], "Hello, World!");
    }

    #[test]
    fn test_event_types() {
        let message_type = "m.room.message";
        let member_type = "m.room.member";
        let create_type = "m.room.create";
        let power_levels_type = "m.room.power_levels";

        assert!(message_type.starts_with("m.room."));
        assert!(member_type.starts_with("m.room."));
        assert!(create_type.starts_with("m.room."));
        assert!(power_levels_type.starts_with("m.room."));
    }

    #[test]
    fn test_state_event_with_is_redacted() {
        let event = StateEvent {
            event_id: "$redacted:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            sender: "@alice:example.com".to_string(),
            event_type: Some("m.room.message".to_string()),
            content: json!({}),
            state_key: None,
            unsigned: Some(json!({"redacted_because": {}})),
            is_redacted: Some(true),
            origin_server_ts: 1234567890,
            depth: None,
            processed_ts: None,
            not_before: None,
            status: None,
            reference_image: None,
            origin: None,
            user_id: None,
            stream_ordering: None,
        };

        assert!(event.is_redacted.unwrap_or(false));
        assert!(event.unsigned.is_some());
    }
}
