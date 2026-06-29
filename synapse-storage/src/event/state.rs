use super::models::*;

impl EventStorage {
    pub async fn get_state_event(
        &self,
        room_id: &str,
        event_type: &str,
        state_key: &str,
    ) -> Result<Option<StateEvent>, sqlx::Error> {
        sqlx::query_as::<_, StateEvent>(
            r"
            SELECT event_id, room_id, COALESCE(sender, user_id) as sender, event_type, content, state_key,
                   COALESCE(unsigned, '{}'::jsonb) as unsigned,
                   COALESCE(is_redacted, false) as is_redacted,
                   COALESCE(origin_server_ts, 0) as origin_server_ts,
                   depth, NULL::BIGINT as processed_at, not_before, status, reference_image, origin, user_id, stream_ordering
            FROM events
            WHERE room_id = $1
              AND event_type = $2
              AND state_key = $3
              AND state_key IS NOT NULL
            ORDER BY origin_server_ts DESC
            LIMIT 1
            ",
        )
        .bind(room_id)
        .bind(event_type)
        .bind(state_key)
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn get_state_events(&self, room_id: &str) -> Result<Vec<StateEvent>, sqlx::Error> {
        sqlx::query_as::<_, StateEvent>(
            r"
            SELECT event_id, room_id, COALESCE(sender, user_id) as sender, event_type, content, state_key,
                   COALESCE(unsigned, '{}'::jsonb) as unsigned,
                   COALESCE(is_redacted, false) as is_redacted,
                   COALESCE(origin_server_ts, 0) as origin_server_ts,
                   depth, NULL::BIGINT as processed_at, not_before, status, reference_image, origin, user_id, stream_ordering
            FROM (
                SELECT DISTINCT ON (event_type, state_key)
                       event_id, room_id, COALESCE(sender, user_id) as sender, event_type, content, state_key,
                       unsigned, is_redacted, origin_server_ts, depth, not_before, status, reference_image, origin, user_id, stream_ordering
                FROM events
                WHERE room_id = $1
                  AND state_key IS NOT NULL
                ORDER BY event_type, state_key, origin_server_ts DESC
            ) s
            ORDER BY origin_server_ts DESC
            ",
        )
        .bind(room_id)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn get_state_events_at_or_before(
        &self,
        room_id: &str,
        origin_server_ts: i64,
    ) -> Result<Vec<StateEvent>, sqlx::Error> {
        sqlx::query_as::<_, StateEvent>(
            r"
            SELECT event_id, room_id, COALESCE(sender, user_id) as sender, event_type, content, state_key,
                   COALESCE(unsigned, '{}'::jsonb) as unsigned,
                   COALESCE(is_redacted, false) as is_redacted,
                   COALESCE(origin_server_ts, 0) as origin_server_ts,
                   depth, NULL::BIGINT as processed_at, not_before, status, reference_image, origin, user_id, stream_ordering
            FROM (
                SELECT DISTINCT ON (event_type, state_key)
                       event_id, room_id, COALESCE(sender, user_id) as sender, event_type, content, state_key,
                       unsigned, is_redacted, origin_server_ts, depth, not_before, status, reference_image, origin, user_id, stream_ordering
                FROM events
                WHERE room_id = $1
                  AND state_key IS NOT NULL
                  AND origin_server_ts <= $2
                ORDER BY event_type, state_key, origin_server_ts DESC, event_id DESC
            ) s
            ORDER BY origin_server_ts DESC, event_id ASC
            ",
        )
        .bind(room_id)
        .bind(origin_server_ts)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn get_state_events_by_type(
        &self,
        room_id: &str,
        event_type: &str,
    ) -> Result<Vec<StateEvent>, sqlx::Error> {
        sqlx::query_as::<_, StateEvent>(
            r"
            SELECT event_id, room_id, COALESCE(sender, user_id) as sender, event_type, content, state_key,
                   COALESCE(unsigned, '{}'::jsonb) as unsigned,
                   COALESCE(is_redacted, false) as is_redacted,
                   COALESCE(origin_server_ts, 0) as origin_server_ts,
                   depth, NULL::BIGINT as processed_at, not_before, status, reference_image, origin, user_id, stream_ordering
            FROM (
                SELECT DISTINCT ON (state_key)
                       event_id, room_id, COALESCE(sender, user_id) as sender, event_type, content, state_key,
                       unsigned, is_redacted, origin_server_ts, depth, not_before, status, reference_image, origin, user_id, stream_ordering
                FROM events
                WHERE room_id = $1
                  AND event_type = $2
                  AND state_key IS NOT NULL
                ORDER BY state_key, origin_server_ts DESC
                LIMIT 5000
            ) s
            ORDER BY origin_server_ts DESC
            ",
        )
        .bind(room_id)
        .bind(event_type)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn get_state_events_by_type_batch(
        &self,
        room_ids: &[String],
        event_type: &str,
    ) -> Result<std::collections::HashMap<String, Vec<StateEvent>>, sqlx::Error> {
        if room_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let events: Vec<StateEvent> = sqlx::query_as(
            r"
            SELECT event_id, room_id, COALESCE(sender, user_id) as sender, event_type, content, state_key,
                   COALESCE(unsigned, '{}'::jsonb) as unsigned,
                   COALESCE(is_redacted, false) as is_redacted,
                   COALESCE(origin_server_ts, 0) as origin_server_ts,
                   depth, NULL::BIGINT as processed_at, not_before, status, reference_image, origin, user_id, stream_ordering
            FROM (
                SELECT DISTINCT ON (room_id, state_key)
                       event_id, room_id, COALESCE(sender, user_id) as sender, event_type, content, state_key,
                       unsigned, is_redacted, origin_server_ts, depth, not_before, status, reference_image, origin, user_id, stream_ordering
                FROM events
                WHERE room_id = ANY($1)
                  AND event_type = $2
                  AND state_key IS NOT NULL
                ORDER BY room_id, state_key, origin_server_ts DESC
            ) s
            ORDER BY room_id, origin_server_ts DESC
            LIMIT 50000
            ",
        )
        .bind(room_ids)
        .bind(event_type)
        .fetch_all(&*self.pool)
        .await?;

        Ok(Self::group_state_events(room_ids, events))
    }

    pub async fn get_state_events_since_batch(
        &self,
        room_ids: &[String],
        since: i64,
    ) -> Result<std::collections::HashMap<String, Vec<StateEvent>>, sqlx::Error> {
        if room_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let events: Vec<StateEvent> = sqlx::query_as(
            r"
            SELECT event_id, room_id, COALESCE(sender, user_id) as sender, event_type, content, state_key,
                   COALESCE(unsigned, '{}'::jsonb) as unsigned,
                   COALESCE(is_redacted, false) as is_redacted,
                   COALESCE(origin_server_ts, 0) as origin_server_ts,
                   depth, NULL::BIGINT as processed_at, not_before, status, reference_image, origin, user_id, stream_ordering
            FROM (
                SELECT DISTINCT ON (room_id, event_type, state_key)
                       event_id, room_id, COALESCE(sender, user_id) as sender, event_type, content, state_key,
                       unsigned, is_redacted, origin_server_ts, depth, not_before, status, reference_image, origin, user_id, stream_ordering
                FROM events
                WHERE room_id = ANY($1)
                  AND state_key IS NOT NULL
                  AND origin_server_ts > $2
                ORDER BY room_id, event_type, state_key, origin_server_ts DESC
            ) s
            ORDER BY room_id, origin_server_ts DESC
            LIMIT 50000
            ",
        )
        .bind(room_ids)
        .bind(since)
        .fetch_all(&*self.pool)
        .await?;

        Ok(Self::group_state_events(room_ids, events))
    }

    pub async fn get_state_events_since_stream_batch(
        &self,
        room_ids: &[String],
        since_stream_ordering: i64,
    ) -> Result<std::collections::HashMap<String, Vec<StateEvent>>, sqlx::Error> {
        if room_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let events: Vec<StateEvent> = sqlx::query_as(
            r"
            SELECT event_id, room_id, COALESCE(sender, user_id) as sender, event_type, content, state_key,
                   COALESCE(unsigned, '{}'::jsonb) as unsigned,
                   COALESCE(is_redacted, false) as is_redacted,
                   COALESCE(origin_server_ts, 0) as origin_server_ts,
                   depth, NULL::BIGINT as processed_at, not_before, status, reference_image, origin, user_id, stream_ordering
            FROM (
                SELECT DISTINCT ON (room_id, event_type, state_key)
                       event_id, room_id, COALESCE(sender, user_id) as sender, event_type, content, state_key,
                       unsigned, is_redacted, origin_server_ts, depth, not_before, status, reference_image, origin, user_id, stream_ordering
                FROM events
                WHERE room_id = ANY($1)
                  AND state_key IS NOT NULL
                  AND stream_ordering > $2
                ORDER BY room_id, event_type, state_key, stream_ordering DESC
            ) s
            ORDER BY room_id, stream_ordering DESC
            LIMIT 50000
            ",
        )
        .bind(room_ids)
        .bind(since_stream_ordering)
        .fetch_all(&*self.pool)
        .await?;

        Ok(Self::group_state_events(room_ids, events))
    }

    pub async fn get_state_change_timestamps_batch(
        &self,
        room_ids: &[String],
        since: i64,
    ) -> Result<std::collections::HashMap<String, i64>, sqlx::Error> {
        if room_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let rows: Vec<(String, i64)> = sqlx::query_as(
            r"
            SELECT room_id, MAX(origin_server_ts) AS latest_ts
            FROM events
            WHERE room_id = ANY($1)
              AND state_key IS NOT NULL
              AND origin_server_ts > $2
            GROUP BY room_id
            ",
        )
        .bind(room_ids)
        .bind(since)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().collect())
    }

    pub async fn get_state_change_timestamps_since_stream_batch(
        &self,
        room_ids: &[String],
        since_stream_ordering: i64,
    ) -> Result<std::collections::HashMap<String, i64>, sqlx::Error> {
        if room_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let rows: Vec<(String, i64)> = sqlx::query_as(
            r"
            SELECT room_id, MAX(origin_server_ts) AS latest_ts
            FROM events
            WHERE room_id = ANY($1)
              AND state_key IS NOT NULL
              AND stream_ordering > $2
            GROUP BY room_id
            ",
        )
        .bind(room_ids)
        .bind(since_stream_ordering)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().collect())
    }

    pub async fn get_state_events_batch(
        &self,
        room_ids: &[String],
    ) -> Result<std::collections::HashMap<String, Vec<StateEvent>>, sqlx::Error> {
        if room_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let events: Vec<StateEvent> = sqlx::query_as(
            r"
            SELECT event_id, room_id, COALESCE(sender, user_id) as sender, event_type, content, state_key,
                   COALESCE(unsigned, '{}'::jsonb) as unsigned,
                   COALESCE(is_redacted, false) as is_redacted,
                   COALESCE(origin_server_ts, 0) as origin_server_ts,
                   depth, NULL::BIGINT as processed_at, not_before, status, reference_image, origin, user_id, stream_ordering
            FROM (
                SELECT DISTINCT ON (room_id, event_type, state_key)
                       event_id, room_id, COALESCE(sender, user_id) as sender, event_type, content, state_key,
                       unsigned, is_redacted, origin_server_ts, depth, not_before, status, reference_image, origin, user_id, stream_ordering
                FROM events
                WHERE room_id = ANY($1)
                  AND state_key IS NOT NULL
                ORDER BY room_id, event_type, state_key, origin_server_ts DESC
            ) s
            ORDER BY room_id, origin_server_ts DESC
            ",
        )
        .bind(room_ids)
        .fetch_all(&*self.pool)
        .await?;

        let mut result: std::collections::HashMap<String, Vec<StateEvent>> =
            std::collections::HashMap::with_capacity(room_ids.len());
        for id in room_ids {
            result.insert(id.clone(), Vec::new());
        }

        for event in events {
            if let Some(room_events) = result.get_mut(&event.room_id) {
                room_events.push(event);
            }
        }

        Ok(result)
    }

    // -----------------------------------------------------------------------
    // Membership state keys
    // -----------------------------------------------------------------------

    pub async fn get_membership_state_keys_since_batch(
        &self,
        room_ids: &[String],
        since: i64,
    ) -> Result<std::collections::HashMap<String, std::collections::HashSet<String>>, sqlx::Error> {
        if room_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let rows: Vec<(String, String)> = sqlx::query_as(
            r"
            SELECT room_id, state_key
            FROM (
                SELECT DISTINCT ON (room_id, state_key)
                       room_id, state_key
                FROM events
                WHERE room_id = ANY($1)
                  AND origin_server_ts > $2
                  AND event_type = 'm.room.member'
                  AND state_key IS NOT NULL
                ORDER BY room_id, state_key, origin_server_ts DESC
            ) recent_membership
            ",
        )
        .bind(room_ids)
        .bind(since)
        .fetch_all(&*self.pool)
        .await?;

        let mut result: std::collections::HashMap<String, std::collections::HashSet<String>> =
            room_ids.iter().map(|room_id| (room_id.clone(), std::collections::HashSet::new())).collect();

        for (room_id, state_key) in rows {
            result.entry(room_id).or_default().insert(state_key);
        }

        Ok(result)
    }

    pub async fn get_membership_state_keys_since_stream_batch(
        &self,
        room_ids: &[String],
        since_stream_ordering: i64,
    ) -> Result<std::collections::HashMap<String, std::collections::HashSet<String>>, sqlx::Error> {
        if room_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let rows: Vec<(String, String)> = sqlx::query_as(
            r"
            SELECT room_id, state_key
            FROM (
                SELECT DISTINCT ON (room_id, state_key)
                       room_id, state_key
                FROM events
                WHERE room_id = ANY($1)
                  AND stream_ordering > $2
                  AND event_type = 'm.room.member'
                  AND state_key IS NOT NULL
                ORDER BY room_id, state_key, stream_ordering DESC
            ) recent_membership
            ",
        )
        .bind(room_ids)
        .bind(since_stream_ordering)
        .fetch_all(&*self.pool)
        .await?;

        let mut result: std::collections::HashMap<String, std::collections::HashSet<String>> =
            room_ids.iter().map(|room_id| (room_id.clone(), std::collections::HashSet::new())).collect();

        for (room_id, state_key) in rows {
            result.entry(room_id).or_default().insert(state_key);
        }

        Ok(result)
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn group_state_events(
        room_ids: &[String],
        events: Vec<StateEvent>,
    ) -> std::collections::HashMap<String, Vec<StateEvent>> {
        let mut result: std::collections::HashMap<String, Vec<StateEvent>> =
            std::collections::HashMap::with_capacity(room_ids.len());
        for id in room_ids {
            result.insert(id.clone(), Vec::new());
        }

        for event in events {
            result.entry(event.room_id.clone()).or_default().push(event);
        }

        result
    }
}
