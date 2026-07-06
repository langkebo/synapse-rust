//! Event creation methods for [`EventStorage`].

use super::models::{CreateEventParams, RoomEvent};
use super::EventStorage;

impl EventStorage {
    pub async fn create_event(
        &self,
        params: CreateEventParams,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> Result<RoomEvent, sqlx::Error> {
        let query = r"
            INSERT INTO events (event_id, room_id, sender, user_id, event_type, content, state_key, origin_server_ts, is_redacted, redacts)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, false, $9)
            RETURNING event_id, room_id, sender as user_id, event_type, content, state_key,
                      COALESCE(depth, 0) as depth, origin_server_ts, origin_server_ts as processed_at,
                      0::BIGINT as not_before, 'pending' as status, null as reference_image,
                      'self' as origin, stream_ordering, redacts
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
                .bind(params.redacts.as_deref())
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
                .bind(params.redacts.as_deref())
                .fetch_one(&*self.pool)
                .await
        }
    }

    /// Like `create_event` but also persists the event DAG metadata
    /// (`prev_events`, `auth_events`, `depth` columns in `events` plus rows
    /// in `event_edges`).  Callers that have the PDU's graph fields (notably
    /// the inbound federation transaction handler) should prefer this method
    /// so that `event_edges` is populated and `/get_missing_events` can walk
    /// the DAG.  Callers without graph data (locally-produced events where
    /// prev_events tracking is not yet wired) can continue to use
    /// `create_event`, which delegates here with empty arrays and depth 0.
    pub async fn create_event_with_graph(
        &self,
        params: CreateEventParams,
        prev_events: &[String],
        auth_events: &[String],
        depth: i64,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> Result<RoomEvent, sqlx::Error> {
        let prev_events_json = serde_json::to_value(prev_events).unwrap_or(serde_json::Value::Null);
        let auth_events_json = serde_json::to_value(auth_events).unwrap_or(serde_json::Value::Null);

        let query = r"
            INSERT INTO events (event_id, room_id, sender, user_id, event_type, content, state_key, origin_server_ts, is_redacted, redacts, depth, prev_events, auth_events)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, false, $9, $10, $11, $12)
            RETURNING event_id, room_id, sender as user_id, event_type, content, state_key,
                      COALESCE(depth, 0) as depth, origin_server_ts, origin_server_ts as processed_at,
                      0::BIGINT as not_before, 'pending' as status, null as reference_image,
                      'self' as origin, stream_ordering, redacts
            ";

        let event = if let Some(tx) = tx {
            let event = sqlx::query_as(query)
                .bind(&params.event_id)
                .bind(&params.room_id)
                .bind(&params.user_id)
                .bind(&params.user_id)
                .bind(&params.event_type)
                .bind(&params.content)
                .bind(params.state_key.as_deref())
                .bind(params.origin_server_ts)
                .bind(params.redacts.as_deref())
                .bind(depth)
                .bind(&prev_events_json)
                .bind(&auth_events_json)
                .fetch_one(&mut **tx)
                .await?;

            // Populate event_edges within the same transaction.
            if !prev_events.is_empty() {
                sqlx::query(
                    r"
                    INSERT INTO event_edges (event_id, prev_event_id, is_state)
                    SELECT $1, unnest($2::text[]), false
                    ON CONFLICT DO NOTHING
                    ",
                )
                .bind(&params.event_id)
                .bind(prev_events)
                .execute(&mut **tx)
                .await?;
            }
            event
        } else {
            let event = sqlx::query_as(query)
                .bind(&params.event_id)
                .bind(&params.room_id)
                .bind(&params.user_id)
                .bind(&params.user_id)
                .bind(&params.event_type)
                .bind(&params.content)
                .bind(params.state_key.as_deref())
                .bind(params.origin_server_ts)
                .bind(params.redacts.as_deref())
                .bind(depth)
                .bind(&prev_events_json)
                .bind(&auth_events_json)
                .fetch_one(&*self.pool)
                .await?;

            // Populate event_edges outside a transaction.
            if !prev_events.is_empty() {
                sqlx::query(
                    r"
                    INSERT INTO event_edges (event_id, prev_event_id, is_state)
                    SELECT $1, unnest($2::text[]), false
                    ON CONFLICT DO NOTHING
                    ",
                )
                .bind(&params.event_id)
                .bind(prev_events)
                .execute(&*self.pool)
                .await?;
            }
            event
        };

        Ok(event)
    }

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

    pub async fn get_room_create_event(&self, room_id: &str) -> Result<Option<RoomEvent>, sqlx::Error> {
        sqlx::query_as::<_, RoomEvent>(
            r"
            SELECT event_id, room_id, COALESCE(user_id, sender) as user_id, event_type, content, state_key,
                   COALESCE(depth, 0) as depth, COALESCE(origin_server_ts, 0) as origin_server_ts, COALESCE(origin_server_ts, 0) as processed_at,
                   COALESCE(not_before, 0) as not_before, status, reference_image, COALESCE(origin, 'self') as origin, stream_ordering, redacts
            FROM events
            WHERE room_id = $1 AND event_type = 'm.room.create'
            ORDER BY origin_server_ts ASC
            LIMIT 1
            ",
        )
        .bind(room_id)
        .fetch_optional(&*self.pool)
        .await
    }
}
