//! Pagination and cursor-based traversal methods for [`EventStorage`].

use super::models::{EventQueryFilter, RoomEvent};
use super::EventStorage;
use super::ROOM_EVENT_COLS;

impl EventStorage {
    pub async fn get_room_events_paginated(
        &self,
        room_id: &str,
        from: Option<i64>,
        limit: i64,
        direction: &str,
    ) -> Result<Vec<RoomEvent>, sqlx::Error> {
        let events = match (direction, from) {
            ("f", Some(from_ts)) => {
                sqlx::query_as(&format!(
                    "SELECT {ROOM_EVENT_COLS}
                    FROM events
                    WHERE room_id = $1 AND origin_server_ts > $2
                    ORDER BY origin_server_ts ASC
                    LIMIT $3
                    "
                ))
                .bind(room_id)
                .bind(from_ts)
                .bind(limit)
                .fetch_all(&*self.pool)
                .await?
            }
            ("f", None) => {
                sqlx::query_as(&format!(
                    "SELECT {ROOM_EVENT_COLS}
                    FROM events
                    WHERE room_id = $1
                    ORDER BY origin_server_ts ASC
                    LIMIT $2
                    "
                ))
                .bind(room_id)
                .bind(limit)
                .fetch_all(&*self.pool)
                .await?
            }
            (_, Some(from_ts)) => {
                sqlx::query_as(&format!(
                    "SELECT {ROOM_EVENT_COLS}
                    FROM events
                    WHERE room_id = $1 AND origin_server_ts < $2
                    ORDER BY origin_server_ts DESC
                    LIMIT $3
                    "
                ))
                .bind(room_id)
                .bind(from_ts)
                .bind(limit)
                .fetch_all(&*self.pool)
                .await?
            }
            (_, None) => {
                sqlx::query_as(&format!(
                    "SELECT {ROOM_EVENT_COLS}
                    FROM events
                    WHERE room_id = $1
                    ORDER BY origin_server_ts DESC
                    LIMIT $2
                    "
                ))
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

    pub async fn find_event_id_by_timestamp(
        &self,
        room_id: &str,
        ts: i64,
        forward: bool,
    ) -> Result<Option<(String, i64)>, sqlx::Error> {
        if forward {
            sqlx::query_as::<_, (String, i64)>(
                r"
                SELECT event_id, origin_server_ts
                FROM events
                WHERE room_id = $1
                  AND origin_server_ts IS NOT NULL
                  AND origin_server_ts >= $2
                ORDER BY origin_server_ts ASC
                LIMIT 1
                ",
            )
            .bind(room_id)
            .bind(ts)
            .fetch_optional(&*self.pool)
            .await
        } else {
            sqlx::query_as::<_, (String, i64)>(
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
            .await
        }
    }

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

    pub async fn get_room_events_paginated_with_filter(
        &self,
        room_id: &str,
        from: Option<&str>,
        to: Option<&str>,
        limit: i64,
        filter: Option<&EventQueryFilter>,
    ) -> Result<Vec<RoomEvent>, sqlx::Error> {
        if to.is_some() {
            tracing::warn!("EventStorage::get_room_events_paginated_with_filter: 'to' parameter not yet supported");
        }
        if filter.is_some() {
            tracing::warn!("EventStorage::get_room_events_paginated_with_filter: 'filter' parameter not yet supported");
        }
        let from_ts = from.and_then(|f| f.parse::<i64>().ok());
        self.get_room_events_paginated(room_id, from_ts, limit, "b").await
    }
}
