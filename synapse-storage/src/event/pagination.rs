//! Pagination and cursor-based traversal methods for [`EventStorage`].

use super::models::{EventQueryFilter, RoomEvent};
use super::EventStorage;
use super::ROOM_EVENT_COLS;

impl EventStorage {
    /// See [`get_room_events_paginated`].
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

    /// S14: 增量同步水位线查询 —— 返回 `stream_ordering > after` 的最新
    /// `limit` 条事件，按 stream_ordering 升序排列。
    ///
    /// 语义说明：先按 stream_ordering 降序取最新 limit 条（保证客户端拿到
    /// 的是最新消息而非最旧消息），再翻转为升序输出。
    pub async fn get_room_events_after_stream_ordering(
        &self,
        room_id: &str,
        after: i64,
        limit: i64,
    ) -> Result<Vec<RoomEvent>, sqlx::Error> {
        let mut events = sqlx::query_as(&format!(
            "SELECT {ROOM_EVENT_COLS}
            FROM events
            WHERE room_id = $1 AND stream_ordering > $2
            ORDER BY stream_ordering DESC
            LIMIT $3
            "
        ))
        .bind(room_id)
        .bind(after)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;
        events.reverse();
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

    /// See [`find_event_id_by_timestamp`].
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

    /// See [`get_events_before_context`].
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

    /// See [`get_events_after_context`].
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

    /// ISSUE-06: 复合游标分页 —— 以 `(origin_server_ts, stream_ordering)`
    /// 元组精确定位页边界，同毫秒事件不再被跳过或重复。
    ///
    /// `from` 为 `Some((ts, Some(stream)))` 时使用元组比较；
    /// `Some((ts, None))`（legacy `t{ts}` token）保持旧的严格时间戳语义；
    /// `None` 取最新/最旧一页。
    pub async fn get_room_events_paginated_cursor(
        &self,
        room_id: &str,
        from: Option<(i64, Option<i64>)>,
        limit: i64,
        direction: &str,
    ) -> Result<Vec<RoomEvent>, sqlx::Error> {
        if matches!(from, Some((_, None))) {
            // legacy `t{ts}` token：保持旧的严格时间戳语义
            let from_ts = from.map(|(ts, _)| ts);
            return self.get_room_events_paginated(room_id, from_ts, limit, direction).await;
        }

        let events = match (direction, from) {
            ("f", Some((ts, Some(stream)))) => {
                sqlx::query_as(&format!(
                    "SELECT {ROOM_EVENT_COLS}
                    FROM events
                    WHERE room_id = $1
                      AND (origin_server_ts, stream_ordering) > ($2, $3)
                    ORDER BY origin_server_ts ASC, stream_ordering ASC
                    LIMIT $4
                    "
                ))
                .bind(room_id)
                .bind(ts)
                .bind(stream)
                .bind(limit)
                .fetch_all(&*self.pool)
                .await?
            }
            ("f", None) => {
                sqlx::query_as(&format!(
                    "SELECT {ROOM_EVENT_COLS}
                    FROM events
                    WHERE room_id = $1
                    ORDER BY origin_server_ts ASC, stream_ordering ASC
                    LIMIT $2
                    "
                ))
                .bind(room_id)
                .bind(limit)
                .fetch_all(&*self.pool)
                .await?
            }
            (_, Some((ts, Some(stream)))) => {
                sqlx::query_as(&format!(
                    "SELECT {ROOM_EVENT_COLS}
                    FROM events
                    WHERE room_id = $1
                      AND (origin_server_ts, stream_ordering) < ($2, $3)
                    ORDER BY origin_server_ts DESC, stream_ordering DESC
                    LIMIT $4
                    "
                ))
                .bind(room_id)
                .bind(ts)
                .bind(stream)
                .bind(limit)
                .fetch_all(&*self.pool)
                .await?
            }
            (_, Some((_, None))) => unreachable!("legacy ts-only tokens are delegated above"),
            (_, None) => {
                sqlx::query_as(&format!(
                    "SELECT {ROOM_EVENT_COLS}
                    FROM events
                    WHERE room_id = $1
                    ORDER BY origin_server_ts DESC, stream_ordering DESC
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

    /// See [`get_room_events_paginated_with_filter`].
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
