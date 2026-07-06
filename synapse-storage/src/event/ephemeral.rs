//! Ephemeral event methods for [`EventStorage`] — typing notifications,
//! read receipts, and other room-level transient state.

use std::collections::HashMap;

use super::models::RoomEphemeralEvent;
use super::EventStorage;

impl EventStorage {
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

    pub async fn get_ephemeral_events_batch(
        &self,
        room_ids: &[String],
        now: i64,
        limit: i64,
    ) -> Result<HashMap<String, Vec<RoomEphemeralEvent>>, sqlx::Error> {
        let mut result: HashMap<String, Vec<RoomEphemeralEvent>> =
            room_ids.iter().cloned().map(|room_id| (room_id, Vec::new())).collect();
        if room_ids.is_empty() {
            return Ok(result);
        }

        let rows = sqlx::query_as::<_, (String, String, String, serde_json::Value, i64, i64)>(
            r"
            SELECT room_id, event_type, user_id, content, stream_id, created_ts
            FROM (
                SELECT
                    room_id,
                    event_type,
                    user_id,
                    content,
                    stream_id,
                    created_ts,
                    ROW_NUMBER() OVER (
                        PARTITION BY room_id
                        ORDER BY stream_id DESC
                    ) AS rn
                FROM room_ephemeral
                WHERE room_id = ANY($1)
                  AND (expires_at IS NULL OR expires_at > $2)
            ) ranked
            WHERE rn <= $3
            ORDER BY room_id, stream_id DESC
            ",
        )
        .bind(room_ids)
        .bind(now)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;

        for (room_id, event_type, user_id, content, stream_id, created_ts) in rows {
            if let Some(events) = result.get_mut(&room_id) {
                events.push(RoomEphemeralEvent { event_type, user_id, content, stream_id, created_ts });
            }
        }

        Ok(result)
    }
}
