use super::models::*;
use sqlx::{Postgres, QueryBuilder};

impl EventStorage {
    pub async fn get_room_events_since(
        &self,
        room_id: &str,
        since: i64,
        limit: i64,
    ) -> Result<Vec<RoomEvent>, sqlx::Error> {
        let events = sqlx::query_as(
            r"
            SELECT event_id, room_id, COALESCE(user_id, sender) as user_id, event_type, content, state_key,
                   COALESCE(depth, 0) as depth, COALESCE(origin_server_ts, 0) as origin_server_ts, COALESCE(origin_server_ts, 0) as processed_at,
                   COALESCE(not_before, 0) as not_before, status, reference_image, COALESCE(NULLIF(NULLIF(BTRIM(origin), ''), 'undefined'), 'self') as origin, stream_ordering
            FROM events WHERE room_id = $1 AND origin_server_ts > $2
            ORDER BY origin_server_ts ASC
            LIMIT $3
            ",
        )
        .bind(room_id)
        .bind(since)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;
        Ok(events)
    }

    pub async fn get_events_since(&self, since: i64, limit: i64) -> Result<Vec<RoomEvent>, sqlx::Error> {
        let events = sqlx::query_as(
            r"
            SELECT event_id, room_id, COALESCE(user_id, sender) as user_id, event_type, content, state_key,
                   COALESCE(depth, 0) as depth, COALESCE(origin_server_ts, 0) as origin_server_ts, COALESCE(origin_server_ts, 0) as processed_at,
                   COALESCE(not_before, 0) as not_before, status, reference_image, COALESCE(NULLIF(NULLIF(BTRIM(origin), ''), 'undefined'), 'self') as origin, stream_ordering
            FROM events WHERE origin_server_ts > $1
            ORDER BY origin_server_ts ASC
            LIMIT $2
            ",
        )
        .bind(since)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;
        Ok(events)
    }

    pub async fn get_room_events_batch(
        &self,
        room_ids: &[String],
        limit_per_room: i64,
    ) -> Result<std::collections::HashMap<String, Vec<RoomEvent>>, sqlx::Error> {
        self.get_room_events_batch_inner(room_ids, None, None, limit_per_room, None).await
    }

    pub async fn get_room_events_batch_filtered(
        &self,
        room_ids: &[String],
        limit_per_room: i64,
        filter: &EventQueryFilter,
    ) -> Result<std::collections::HashMap<String, Vec<RoomEvent>>, sqlx::Error> {
        self.get_room_events_batch_inner(room_ids, None, None, limit_per_room, Some(filter)).await
    }

    pub async fn get_room_events_since_batch(
        &self,
        room_ids: &[String],
        since: i64,
        limit_per_room: i64,
    ) -> Result<std::collections::HashMap<String, Vec<RoomEvent>>, sqlx::Error> {
        self.get_room_events_batch_inner(room_ids, Some(since), None, limit_per_room, None).await
    }

    pub async fn get_room_events_since_stream_batch(
        &self,
        room_ids: &[String],
        since_stream_ordering: i64,
        limit_per_room: i64,
    ) -> Result<std::collections::HashMap<String, Vec<RoomEvent>>, sqlx::Error> {
        self.get_room_events_batch_inner(room_ids, None, Some(since_stream_ordering), limit_per_room, None).await
    }

    pub async fn get_room_events_since_batch_filtered(
        &self,
        room_ids: &[String],
        since: i64,
        limit_per_room: i64,
        filter: &EventQueryFilter,
    ) -> Result<std::collections::HashMap<String, Vec<RoomEvent>>, sqlx::Error> {
        self.get_room_events_batch_inner(room_ids, Some(since), None, limit_per_room, Some(filter)).await
    }

    pub async fn get_room_events_since_stream_batch_filtered(
        &self,
        room_ids: &[String],
        since_stream_ordering: i64,
        limit_per_room: i64,
        filter: &EventQueryFilter,
    ) -> Result<std::collections::HashMap<String, Vec<RoomEvent>>, sqlx::Error> {
        self.get_room_events_batch_inner(room_ids, None, Some(since_stream_ordering), limit_per_room, Some(filter))
            .await
    }

    async fn get_room_events_batch_inner(
        &self,
        room_ids: &[String],
        since_ts: Option<i64>,
        since_stream: Option<i64>,
        limit_per_room: i64,
        filter: Option<&EventQueryFilter>,
    ) -> Result<std::collections::HashMap<String, Vec<RoomEvent>>, sqlx::Error> {
        if room_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let mut query = QueryBuilder::<Postgres>::new(
            r"
            SELECT event_id, room_id, user_id, event_type, content, state_key,
                   depth, origin_server_ts, processed_at, not_before, status, reference_image, origin, stream_ordering
            FROM (
                SELECT
                    event_id,
                    room_id,
                    COALESCE(user_id, sender) as user_id,
                    event_type,
                    content,
                    state_key,
                    COALESCE(depth, 0) as depth,
                    COALESCE(origin_server_ts, 0) as origin_server_ts,
                    COALESCE(origin_server_ts, 0) as processed_at,
                    COALESCE(not_before, 0) as not_before,
                    status,
                    reference_image,
                    COALESCE(origin, 'self') as origin,
                    stream_ordering,
                    ROW_NUMBER() OVER (
                        PARTITION BY room_id
                        ORDER BY stream_ordering ASC
                    ) AS rn
                FROM events
                WHERE room_id = ANY(
            ",
        );
        query.push_bind(room_ids);
        query.push(")");

        if let Some(since_stream) = since_stream {
            query.push(" AND stream_ordering > ");
            query.push_bind(since_stream);
        } else if let Some(since_ts) = since_ts {
            query.push(" AND origin_server_ts > ");
            query.push_bind(since_ts);
        }

        if let Some(filter) = filter {
            if let Some(types) = Self::non_empty_filter_values(filter.types.as_deref()) {
                query.push(" AND event_type = ANY(");
                query.push_bind(types);
                query.push(")");
            }

            if let Some(not_types) = Self::non_empty_filter_values(filter.not_types.as_deref()) {
                query.push(" AND NOT (event_type = ANY(");
                query.push_bind(not_types);
                query.push("))");
            }

            if let Some(senders) = Self::non_empty_filter_values(filter.senders.as_deref()) {
                query.push(" AND COALESCE(user_id, sender) = ANY(");
                query.push_bind(senders);
                query.push(")");
            }

            if let Some(not_senders) = Self::non_empty_filter_values(filter.not_senders.as_deref()) {
                query.push(" AND NOT (COALESCE(user_id, sender) = ANY(");
                query.push_bind(not_senders);
                query.push("))");
            }
        }

        query.push(
            r"
            ) ranked
            WHERE rn <=
            ",
        );
        query.push_bind(limit_per_room);
        query.push(
            r"
            ORDER BY room_id, origin_server_ts DESC
            ",
        );

        let events: Vec<RoomEvent> = query.build_query_as().fetch_all(&*self.pool).await?;

        Ok(Self::group_room_events(room_ids, events, limit_per_room))
    }

    pub async fn get_events_batch(&self, event_ids: &[String]) -> Result<Vec<RoomEvent>, sqlx::Error> {
        if event_ids.is_empty() {
            return Ok(Vec::new());
        }

        sqlx::query_as(
            r"
            SELECT event_id, room_id, COALESCE(user_id, sender) as user_id, event_type, content, state_key,
                   COALESCE(depth, 0) as depth, COALESCE(origin_server_ts, 0) as origin_server_ts, COALESCE(origin_server_ts, 0) as processed_at,
                   COALESCE(not_before, 0) as not_before, status, reference_image, COALESCE(origin, 'self') as origin, stream_ordering
            FROM events
            WHERE event_id = ANY($1)
            ",
        )
        .bind(event_ids)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn get_events_map(
        &self,
        event_ids: &[String],
    ) -> Result<std::collections::HashMap<String, RoomEvent>, sqlx::Error> {
        if event_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let events = self.get_events_batch(event_ids).await?;

        Ok(events.into_iter().map(|e| (e.event_id.clone(), e)).collect())
    }

    pub async fn has_room_events_since(&self, room_ids: &[String], since: i64) -> Result<bool, sqlx::Error> {
        if room_ids.is_empty() {
            return Ok(false);
        }

        let row = sqlx::query_scalar::<_, i32>(
            r"
            SELECT 1
            FROM events
            WHERE room_id = ANY($1)
              AND origin_server_ts > $2
            LIMIT 1
            ",
        )
        .bind(room_ids)
        .bind(since)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row.is_some())
    }

    pub async fn get_latest_events_for_rooms(
        &self,
        room_ids: &[String],
        _limit_per_room: i64,
    ) -> Result<std::collections::HashMap<String, RoomEvent>, sqlx::Error> {
        if room_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let events: Vec<RoomEvent> = sqlx::query_as(
            r"
            SELECT DISTINCT ON (room_id)
                   event_id, room_id, COALESCE(user_id, sender) as user_id, event_type, content, state_key,
                   COALESCE(depth, 0) as depth, COALESCE(origin_server_ts, 0) as origin_server_ts,
                   COALESCE(origin_server_ts, 0) as processed_at,
                   COALESCE(not_before, 0) as not_before, status, reference_image, COALESCE(origin, 'self') as origin, stream_ordering
            FROM events
            WHERE room_id = ANY($1)
            ORDER BY room_id, origin_server_ts DESC
            ",
        )
        .bind(room_ids)
        .fetch_all(&*self.pool)
        .await?;

        Ok(events.into_iter().map(|e| (e.room_id.clone(), e)).collect())
    }

    pub async fn get_room_message_counts_batch(
        &self,
        room_ids: &[String],
    ) -> Result<std::collections::HashMap<String, i64>, sqlx::Error> {
        if room_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let rows: Vec<(String, i64)> = sqlx::query_as(
            r"
            SELECT room_id, COUNT(*) as count
            FROM events
            WHERE room_id = ANY($1) AND event_type = 'm.room.message'
            GROUP BY room_id
            ",
        )
        .bind(room_ids)
        .fetch_all(&*self.pool)
        .await?;

        let mut result: std::collections::HashMap<String, i64> = room_ids.iter().map(|id| (id.clone(), 0)).collect();

        for (room_id, count) in rows {
            result.insert(room_id, count);
        }

        Ok(result)
    }

    pub async fn get_max_stream_ordering(&self) -> Result<i64, sqlx::Error> {
        let result: Option<(i64,)> =
            sqlx::query_as("SELECT COALESCE(MAX(stream_ordering), 0) FROM events").fetch_optional(&*self.pool).await?;
        Ok(result.map_or(0, |r| r.0))
    }

    pub async fn get_max_origin_server_ts_for_room(&self, room_id: &str) -> Result<i64, sqlx::Error> {
        let result: Option<(i64,)> =
            sqlx::query_as("SELECT COALESCE(MAX(origin_server_ts), 0) FROM events WHERE room_id = $1")
                .bind(room_id)
                .fetch_optional(&*self.pool)
                .await?;
        Ok(result.map_or(0, |r| r.0))
    }

    pub async fn get_events_since_stream_ordering(
        &self,
        room_id: &str,
        since_stream_ordering: i64,
        limit: i64,
    ) -> Result<Vec<RoomEvent>, sqlx::Error> {
        sqlx::query_as::<_, RoomEvent>(
            r"
            SELECT event_id, room_id, sender as user_id, event_type, content, state_key,
                   COALESCE(depth, 0) as depth, origin_server_ts, origin_server_ts as processed_at,
                   COALESCE(not_before, 0) as not_before, status, reference_image,
                   COALESCE(NULLIF(NULLIF(BTRIM(origin), ''), 'undefined'), 'self') as origin, stream_ordering
            FROM events
            WHERE room_id = $1
              AND stream_ordering > $2
              AND is_redacted = false
            ORDER BY stream_ordering ASC
            LIMIT $3
            ",
        )
        .bind(room_id)
        .bind(since_stream_ordering)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn get_room_events_by_stream_range(
        &self,
        room_id: &str,
        from_stream: i64,
        to_stream: i64,
        limit: i64,
        direction: &str,
    ) -> Result<Vec<RoomEvent>, sqlx::Error> {
        let (op, order) = if direction == "b" { ("<", "DESC") } else { (">", "ASC") };
        let query = format!(
            r"
            SELECT event_id, room_id, sender as user_id, event_type, content, state_key,
                   COALESCE(depth, 0) as depth, origin_server_ts, origin_server_ts as processed_at,
                   COALESCE(not_before, 0) as not_before, status, reference_image,
                   COALESCE(NULLIF(NULLIF(BTRIM(origin), ''), 'undefined'), 'self') as origin, stream_ordering
            FROM events
            WHERE room_id = $1
              AND stream_ordering {op} $2
              AND stream_ordering <= $4
              AND is_redacted = false
            ORDER BY stream_ordering {order}
            LIMIT $3
            "
        );
        sqlx::query_as::<_, RoomEvent>(&query)
            .bind(room_id)
            .bind(from_stream)
            .bind(limit)
            .bind(to_stream)
            .fetch_all(&*self.pool)
            .await
    }

    // -----------------------------------------------------------------------
    // Room encryption check
    // -----------------------------------------------------------------------

    /// Check whether a room has an `m.room.encryption` state event.
    pub async fn check_room_has_encryption(&self, room_id: &str) -> Result<bool, sqlx::Error> {
        let row: Option<(i32,)> = sqlx::query_as(
            "SELECT 1 FROM events WHERE room_id = $1 AND event_type = 'm.room.encryption' AND state_key IS NOT NULL LIMIT 1",
        )
        .bind(room_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row.is_some())
    }

    // -----------------------------------------------------------------------
    // Event queue (pending / processing / failed)
    // -----------------------------------------------------------------------

    /// Get pending events for a room (used by the message queue endpoint).
    pub async fn get_pending_room_events(&self, room_id: &str, limit: i64) -> Result<Vec<RoomEvent>, sqlx::Error> {
        sqlx::query_as::<_, RoomEvent>(
            r"
            SELECT event_id, room_id, sender as user_id, event_type, content, state_key,
                   COALESCE(depth, 0) as depth, origin_server_ts, origin_server_ts as processed_at,
                   COALESCE(not_before, 0) as not_before, status, reference_image,
                   COALESCE(NULLIF(NULLIF(BTRIM(origin), ''), 'undefined'), 'self') as origin, stream_ordering
            FROM events
            WHERE room_id = $1 AND status = 'pending'
            ORDER BY origin_server_ts ASC
            LIMIT $2
            ",
        )
        .bind(room_id)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await
    }

    /// Count events in a room by status (e.g. "processing", "failed").
    pub async fn count_room_events_by_status(&self, room_id: &str, status: &str) -> Result<i64, sqlx::Error> {
        let result: Option<(i64,)> = sqlx::query_as("SELECT COUNT(*) FROM events WHERE room_id = $1 AND status = $2")
            .bind(room_id)
            .bind(status)
            .fetch_optional(&*self.pool)
            .await?;
        Ok(result.map_or(0, |r| r.0))
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn non_empty_filter_values(values: Option<&[String]>) -> Option<&[String]> {
        values.filter(|entries| !entries.is_empty())
    }

    fn group_room_events(
        room_ids: &[String],
        events: Vec<RoomEvent>,
        limit_per_room: i64,
    ) -> std::collections::HashMap<String, Vec<RoomEvent>> {
        let mut result: std::collections::HashMap<String, Vec<RoomEvent>> =
            std::collections::HashMap::with_capacity(room_ids.len());
        for id in room_ids {
            result.insert(id.clone(), Vec::new());
        }

        for event in events {
            if let Some(room_events) = result.get_mut(&event.room_id) {
                if room_events.len() < limit_per_room as usize {
                    room_events.push(event);
                }
            }
        }

        result
    }
}
