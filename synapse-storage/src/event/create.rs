//! Event creation methods for [`EventStorage`].

use super::models::{CreateEventParams, RoomEvent};
use super::EventStorage;

impl EventStorage {
    /// See [`create_event`].
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
                      0::BIGINT as not_before, 'pending' as status,
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
    ///
    /// P2-1 Optimization (2026-09-23):
    /// - Combined two-step insert in single transaction (event row + edges)
    /// - Uses unnest() to batch edge inserts in one round-trip
    /// - Reduces transaction overhead vs. multiple individual INSERTs
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

        // P2-1: Insert event row first, then batch edge inserts in same txn
        let insert_event_query = r"
            INSERT INTO events (event_id, room_id, sender, user_id, event_type, content, state_key, origin_server_ts, is_redacted, redacts, depth, prev_events, auth_events)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, false, $9, $10, $11, $12)
            RETURNING event_id, room_id, sender as user_id, event_type, content, state_key,
                      COALESCE(depth, 0) as depth, origin_server_ts, origin_server_ts as processed_at,
                      0::BIGINT as not_before, 'pending' as status,
                      'self' as origin, stream_ordering, redacts
        ";

        // P2-1: Batch insert all prev_edges in a single round-trip using unnest()
        let insert_edges_query = r"
            INSERT INTO event_edges (event_id, prev_event_id, is_state)
            SELECT $1, unnest($2::text[]), false
            WHERE $2 IS NOT NULL AND $2 != '[]'
            ON CONFLICT DO NOTHING
        ";

        let event = if let Some(tx) = tx {
            let event = sqlx::query_as(insert_event_query)
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

            // P2-1: Batch edge inserts in same transaction
            if !prev_events.is_empty() {
                sqlx::query(insert_edges_query)
                    .bind(&params.event_id)
                    .bind(prev_events)
                    .execute(&mut **tx)
                    .await?;
            }
            event
        } else {
            // No caller transaction: wrap the event row and its DAG edges in a
            // local transaction so that a failed `event_edges` insert cannot
            // leave an orphaned `events` row behind (B8).
            let mut local_tx = self.pool.begin().await?;

            let event = sqlx::query_as(insert_event_query)
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
                .fetch_one(&mut *local_tx)
                .await?;

            // P2-1: Batch edge inserts in same local transaction
            if !prev_events.is_empty() {
                sqlx::query(insert_edges_query)
                    .bind(&params.event_id)
                    .bind(prev_events)
                    .execute(&mut *local_tx)
                    .await?;
            }

            local_tx.commit().await?;
            event
        };

        Ok(event)
    }

    /// Create a state event with MSC4242 `prev_state_events` (state DAG edges).
    ///
    /// This is the MSC4242 State DAG equivalent of `create_event_with_graph`:
    /// it stores `prev_state_events` in addition to `prev_events` and
    /// `auth_events`, forming the state DAG distinct from the room DAG.
    ///
    /// - `prev_events`: room DAG edges (all events)
    /// - `auth_events`: authorization events (sender-specified for v1-v11;
    ///   ignored / server-calculated for MSC4242 room versions)
    /// - `prev_state_events`: state DAG edges (state events only, MSC4242)
    ///
    /// For non-MSC4242 room versions, use `create_event_with_graph` instead
    /// (which leaves `prev_state_events` NULL).
    ///
    /// P2-1 Optimization (2026-09-23):
    /// - Batch inserts room edges and state edges in separate unnest() calls
    pub async fn create_state_event_with_dag(
        &self,
        params: CreateEventParams,
        prev_events: &[String],
        auth_events: &[String],
        prev_state_events: &[String],
        depth: i64,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> Result<RoomEvent, sqlx::Error> {
        let prev_events_json = serde_json::to_value(prev_events).unwrap_or(serde_json::Value::Null);
        let auth_events_json = serde_json::to_value(auth_events).unwrap_or(serde_json::Value::Null);
        let prev_state_events_json = serde_json::to_value(prev_state_events).unwrap_or(serde_json::Value::Null);

        let insert_event_query = r"
            INSERT INTO events (event_id, room_id, sender, user_id, event_type, content, state_key,
                                origin_server_ts, is_redacted, redacts, depth,
                                prev_events, auth_events, prev_state_events)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, false, $9, $10, $11, $12, $13)
            RETURNING event_id, room_id, sender as user_id, event_type, content, state_key,
                      COALESCE(depth, 0) as depth, origin_server_ts, origin_server_ts as processed_at,
                      0::BIGINT as not_before, 'pending' as status,
                      'self' as origin, stream_ordering, redacts
        ";

        // P2-1: Batch room DAG edges
        let insert_room_edges_query = r"
            INSERT INTO event_edges (event_id, prev_event_id, is_state)
            SELECT $1, unnest($2::text[]), false
            WHERE $2 IS NOT NULL AND $2 != '[]'
            ON CONFLICT DO NOTHING
        ";

        // P2-1: Batch state DAG edges
        let insert_state_edges_query = r"
            INSERT INTO event_edges (event_id, prev_event_id, is_state)
            SELECT $1, unnest($2::text[]), true
            WHERE $2 IS NOT NULL AND $2 != '[]'
            ON CONFLICT DO NOTHING
        ";

        let event = if let Some(tx) = tx {
            let event = sqlx::query_as(insert_event_query)
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
                .bind(&prev_state_events_json)
                .fetch_one(&mut **tx)
                .await?;

            // P2-1: Batch room DAG edges
            if !prev_events.is_empty() {
                sqlx::query(insert_room_edges_query)
                    .bind(&params.event_id)
                    .bind(prev_events)
                    .execute(&mut **tx)
                    .await?;
            }
            // P2-1: Batch state DAG edges
            if !prev_state_events.is_empty() {
                sqlx::query(insert_state_edges_query)
                    .bind(&params.event_id)
                    .bind(prev_state_events)
                    .execute(&mut **tx)
                    .await?;
            }
            event
        } else {
            // 无调用方事务：事件行与两组 DAG 边必须在**同一本地事务**里落库，
            // 否则 `event_edges` 插入失败会留下孤立 `events` 行（`/get_missing_events`
            // 永远走不到它）。这与 B8 给 `create_event_with_graph` 关掉的是同一个半写窗口，
            // 此前因为两处插入逻辑重复而漏掉了这一条路径（2026-09-23 实测；
            // 红证明 `create_state_event_with_dag_rolls_back_event_when_edges_insert_fails`）。
            let mut local_tx = self.pool.begin().await?;

            let event = sqlx::query_as(insert_event_query)
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
                .bind(&prev_state_events_json)
                .fetch_one(&mut *local_tx)
                .await?;

            // P2-1: Batch room DAG edges
            if !prev_events.is_empty() {
                sqlx::query(insert_room_edges_query)
                    .bind(&params.event_id)
                    .bind(prev_events)
                    .execute(&mut *local_tx)
                    .await?;
            }
            // P2-1: Batch state DAG edges
            if !prev_state_events.is_empty() {
                sqlx::query(insert_state_edges_query)
                    .bind(&params.event_id)
                    .bind(prev_state_events)
                    .execute(&mut *local_tx)
                    .await?;
            }

            local_tx.commit().await?;
            event
        };

        Ok(event)
    }

    /// See [`upsert_power_levels_event`].
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

    /// See [`get_room_create_event`].
    pub async fn get_room_create_event(&self, room_id: &str) -> Result<Option<RoomEvent>, sqlx::Error> {
        sqlx::query_as::<_, RoomEvent>(
            r"
            SELECT event_id, room_id, COALESCE(user_id, sender) as user_id, event_type, content, state_key,
                   COALESCE(depth, 0) as depth, COALESCE(origin_server_ts, 0) as origin_server_ts, COALESCE(origin_server_ts, 0) as processed_at,
                   COALESCE(not_before, 0) as not_before, status, COALESCE(origin, 'self') as origin, stream_ordering, redacts
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
