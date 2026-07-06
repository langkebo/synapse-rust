//! Search and full-text search methods for [`EventStorage`].

use sqlx::{Postgres, QueryBuilder};

use super::models::RoomEvent;
use super::EventStorage;

impl EventStorage {
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

    #[allow(clippy::too_many_arguments)]
    pub async fn search_joined_room_events(
        &self,
        joined_rooms: &[String],
        search_pattern: &str,
        rooms: Option<&[String]>,
        not_rooms: Option<&[String]>,
        event_types: Option<&[String]>,
        senders: Option<&[String]>,
        cursor: Option<(&str, i64)>,
        limit: i64,
    ) -> Result<Vec<(String, String, String, String, serde_json::Value, i64)>, sqlx::Error> {
        if joined_rooms.is_empty() {
            return Ok(Vec::new());
        }

        let mut query_builder = QueryBuilder::<Postgres>::new(
            "SELECT event_id, room_id, sender, event_type, content, origin_server_ts FROM events WHERE ",
        );

        query_builder.push("(LOWER(content::text) LIKE ");
        query_builder.push_bind(search_pattern);
        query_builder.push(" OR LOWER(sender) LIKE ");
        query_builder.push_bind(search_pattern);
        query_builder.push(")");

        query_builder.push(" AND room_id IN (");
        {
            let mut separated = query_builder.separated(", ");
            for room in joined_rooms {
                separated.push_bind(room);
            }
        }
        query_builder.push(")");

        let has_explicit_types = event_types.is_some_and(|types| !types.is_empty());
        if !has_explicit_types {
            query_builder.push(" AND event_type = 'm.room.message'");
        }

        if let Some(rooms) = rooms.filter(|rooms| !rooms.is_empty()) {
            query_builder.push(" AND room_id IN (");
            {
                let mut separated = query_builder.separated(", ");
                for room in rooms {
                    separated.push_bind(room);
                }
            }
            query_builder.push(")");
        }

        if let Some(not_rooms) = not_rooms.filter(|rooms| !rooms.is_empty()) {
            query_builder.push(" AND room_id NOT IN (");
            {
                let mut separated = query_builder.separated(", ");
                for room in not_rooms {
                    separated.push_bind(room);
                }
            }
            query_builder.push(")");
        }

        if let Some(event_types) = event_types.filter(|types| !types.is_empty()) {
            query_builder.push(" AND event_type IN (");
            {
                let mut separated = query_builder.separated(", ");
                for event_type in event_types {
                    separated.push_bind(event_type);
                }
            }
            query_builder.push(")");
        }

        if let Some(senders) = senders.filter(|senders| !senders.is_empty()) {
            query_builder.push(" AND sender IN (");
            {
                let mut separated = query_builder.separated(", ");
                for sender in senders {
                    separated.push_bind(sender);
                }
            }
            query_builder.push(")");
        }

        if let Some((event_id, origin_server_ts)) = cursor {
            query_builder.push(" AND (origin_server_ts, event_id) < (");
            query_builder.push_bind(origin_server_ts);
            query_builder.push(", ");
            query_builder.push_bind(event_id);
            query_builder.push(")");
        }

        query_builder.push(" ORDER BY origin_server_ts DESC, event_id DESC LIMIT ");
        query_builder.push_bind(limit);

        query_builder.build_query_as().fetch_all(&*self.pool).await
    }

    pub async fn search_postgres_messages(
        &self,
        user_id: &str,
        query: &str,
        rank_cursor: Option<f64>,
        origin_server_ts_cursor: Option<i64>,
        event_id_cursor: Option<&str>,
        limit: i64,
    ) -> Result<Vec<(String, String, String, String, serde_json::Value, i64, f64)>, sqlx::Error> {
        if let (Some(rank), Some(origin_server_ts), Some(event_id)) =
            (rank_cursor, origin_server_ts_cursor, event_id_cursor)
        {
            sqlx::query_as::<_, (String, String, String, String, serde_json::Value, i64, f64)>(
                r"
                SELECT
                    e.event_id,
                    e.room_id,
                    e.sender,
                    e.event_type,
                    e.content,
                    e.origin_server_ts,
                    ts_rank(to_tsvector('english', e.content), plainto_tsquery('english', $2)) as rank
                FROM events e
                INNER JOIN room_memberships rm ON e.room_id = rm.room_id AND rm.user_id = $1 AND rm.membership = 'join'
                WHERE e.event_type = 'm.room.message'
                    AND e.stream_ordering > 0
                    AND to_tsvector('english', e.content) @@ plainto_tsquery('english', $2)
                    AND (
                        ts_rank(to_tsvector('english', e.content), plainto_tsquery('english', $2)) < $3
                        OR (
                            ts_rank(to_tsvector('english', e.content), plainto_tsquery('english', $2)) = $3
                            AND e.origin_server_ts < $4
                        )
                        OR (
                            ts_rank(to_tsvector('english', e.content), plainto_tsquery('english', $2)) = $3
                            AND e.origin_server_ts = $4
                            AND e.event_id < $5
                        )
                    )
                ORDER BY rank DESC, e.origin_server_ts DESC, e.event_id DESC
                LIMIT $6
                ",
            )
            .bind(user_id)
            .bind(query)
            .bind(rank)
            .bind(origin_server_ts)
            .bind(event_id)
            .bind(limit)
            .fetch_all(&*self.pool)
            .await
        } else {
            sqlx::query_as::<_, (String, String, String, String, serde_json::Value, i64, f64)>(
                r"
                SELECT
                    e.event_id,
                    e.room_id,
                    e.sender,
                    e.event_type,
                    e.content,
                    e.origin_server_ts,
                    ts_rank(to_tsvector('english', e.content), plainto_tsquery('english', $2)) as rank
                FROM events e
                INNER JOIN room_memberships rm ON e.room_id = rm.room_id AND rm.user_id = $1 AND rm.membership = 'join'
                WHERE e.event_type = 'm.room.message'
                    AND e.stream_ordering > 0
                    AND to_tsvector('english', e.content) @@ plainto_tsquery('english', $2)
                ORDER BY rank DESC, e.origin_server_ts DESC, e.event_id DESC
                LIMIT $3
                ",
            )
            .bind(user_id)
            .bind(query)
            .bind(limit)
            .fetch_all(&*self.pool)
            .await
        }
    }

    pub async fn create_postgres_fts_index(&self) -> Result<(), sqlx::Error> {
        sqlx::query(
            r"
            CREATE INDEX IF NOT EXISTS events_fts_idx
            ON events
            USING GIN (to_tsvector('english', content))
            WHERE event_type = 'm.room.message' AND stream_ordering > 0
            ",
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    /// Room-scoped postgres full-text search.
    /// Named differently from `search_postgres_messages` (which is user-scoped
    /// and returns tuples) to avoid method name collision.
    pub async fn search_room_postgres_messages(
        &self,
        room_id: &str,
        search_term: &str,
        limit: i64,
    ) -> Result<Vec<RoomEvent>, sqlx::Error> {
        let events = sqlx::query_as(
            r"
            SELECT event_id, room_id, COALESCE(user_id, sender) as user_id, event_type, content, state_key,
                   COALESCE(depth, 0) as depth, COALESCE(origin_server_ts, 0) as origin_server_ts, COALESCE(origin_server_ts, 0) as processed_at,
                   COALESCE(not_before, 0) as not_before, status, reference_image, COALESCE(origin, 'self') as origin, stream_ordering, redacts
            FROM events
            WHERE room_id = $1
              AND event_type = 'm.room.message'
              AND to_tsvector('english', content) @@ plainto_tsquery('english', $2)
            ORDER BY origin_server_ts DESC
            LIMIT $3
            ",
        )
        .bind(room_id)
        .bind(search_term)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;
        Ok(events)
    }
}
