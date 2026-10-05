use super::models::*;
use super::ROOM_EVENT_COLS;
use sqlx::{Pool, Postgres};
use std::sync::Arc;
use synapse_common::current_timestamp_millis;

impl EventStorage {
    /// See [`new`].
    pub fn new(pool: &Arc<Pool<Postgres>>, server_name: String) -> Self {
        Self { pool: pool.clone(), server_name }
    }

    /// See [`get_event`].
    pub async fn get_event(&self, event_id: &str) -> Result<Option<RoomEvent>, sqlx::Error> {
        // R4：`COALESCE(depth, 0)` / `COALESCE(not_before, 0)` / `COALESCE(origin, 'self')` 都是
        // "无关系来源的表达式" ⇒ 宏推可空，而 `RoomEvent` 三个字段非 `Option` ⇒ 断言非空
        // （`COALESCE` 自身即保证）。`sender` 是 NOT NULL 列，别名成 `user_id` 仍保持非空。
        let event = sqlx::query_as!(
            RoomEvent,
            r#"
            SELECT event_id, room_id, sender as user_id, event_type, content, state_key,
                   COALESCE(depth, 0) AS "depth!", origin_server_ts,
                   origin_server_ts AS "processed_ts",
                   COALESCE(not_before, 0) AS "not_before!", status,
                   COALESCE(origin, 'self') AS "origin!", stream_ordering, redacts
            FROM events WHERE event_id = $1
            "#,
            event_id,
        )
        .fetch_optional(&*self.pool)
        .await?;
        Ok(event)
    }

    /// Purge historical events before the given timestamp.
    ///
    /// **Security (P0)**: Only remote/federated events are deleted. Local
    /// events (those whose `origin` is `'self'`, NULL, empty, or
    /// `'undefined'`) are always preserved to prevent accidental deletion of
    /// locally-originated outbound events. This mirrors the Element Synapse
    /// v1.156 purge history safety fix.
    ///
    /// When `dry_run` is `true`, returns the count of events that *would* be
    /// deleted without actually removing any rows. This enables admin
    /// pre-flight inspection of a purge history operation.
    pub async fn delete_remote_events_before(
        &self,
        room_id: &str,
        timestamp: i64,
        dry_run: bool,
    ) -> Result<u64, sqlx::Error> {
        if dry_run {
            let count = self.count_events_before(room_id, timestamp).await?;
            return Ok(count as u64);
        }
        let result = sqlx::query!(
            r#"
            DELETE FROM events
            WHERE room_id = $1
              AND origin_server_ts < $2
              AND event_type != 'm.room.create'
              AND COALESCE(NULLIF(NULLIF(BTRIM(origin), ''), 'undefined'), 'self') != 'self'
            "#,
            room_id,
            timestamp,
        )
        .execute(&*self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// Count historical events before the given timestamp that would be
    /// purged by [`delete_remote_events_before`]. Does not mutate state.
    ///
    /// Applies the same security filter as `delete_remote_events_before`: only
    /// remote/federated events (origin != 'self') are counted, and
    /// `m.room.create` events are always excluded.
    pub async fn count_events_before(&self, room_id: &str, timestamp: i64) -> Result<i64, sqlx::Error> {
        // R4：`COUNT(*)` 无关系来源 ⇒ 推可空；`COALESCE(..., 0)` 保证非空 ⇒ 断言。
        let count: i64 = sqlx::query_scalar!(
            r#"
            SELECT COALESCE(COUNT(*), 0) AS "count!" FROM events
            WHERE room_id = $1
              AND origin_server_ts < $2
              AND event_type != 'm.room.create'
              AND COALESCE(NULLIF(NULLIF(BTRIM(origin), ''), 'undefined'), 'self') != 'self'
            "#,
            room_id,
            timestamp,
        )
        .fetch_one(&*self.pool)
        .await?;
        Ok(count)
    }

    /// See [`get_room_events`].
    pub async fn get_room_events(&self, room_id: &str, limit: i64) -> Result<Vec<RoomEvent>, sqlx::Error> {
        let events = sqlx::query_as(&format!(
            "SELECT {ROOM_EVENT_COLS}
            FROM events WHERE room_id = $1
            ORDER BY origin_server_ts DESC, stream_ordering DESC NULLS LAST, event_id DESC
            LIMIT $2
            "
        ))
        .bind(room_id)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;
        Ok(events)
    }

    /// See [`get_room_events_by_type`].
    pub async fn get_room_events_by_type(
        &self,
        room_id: &str,
        event_type: &str,
        limit: i64,
    ) -> Result<Vec<RoomEvent>, sqlx::Error> {
        let events = sqlx::query_as(&format!(
            "SELECT {ROOM_EVENT_COLS}
            FROM events WHERE room_id = $1 AND event_type = $2
            ORDER BY origin_server_ts DESC
            LIMIT $3
            "
        ))
        .bind(room_id)
        .bind(event_type)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;
        Ok(events)
    }

    /// See [`get_sender_events`].
    pub async fn get_sender_events(&self, user_id: &str, limit: i64) -> Result<Vec<RoomEvent>, sqlx::Error> {
        let events = sqlx::query_as(&format!(
            "SELECT {ROOM_EVENT_COLS}
            FROM events WHERE COALESCE(user_id, sender) = $1
            ORDER BY origin_server_ts DESC
            LIMIT $2
            "
        ))
        .bind(user_id)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;
        Ok(events)
    }

    /// See [`get_room_message_count`].
    pub async fn get_room_message_count(&self, room_id: &str) -> Result<i64, sqlx::Error> {
        let count: i64 = sqlx::query_scalar!(
            r#"
            SELECT COALESCE(COUNT(*), 0) AS "count!" FROM events
            WHERE room_id = $1 AND event_type = 'm.room.message'
            "#,
            room_id,
        )
        .fetch_one(&*self.pool)
        .await?;
        Ok(count)
    }

    /// See [`get_total_message_count`].
    ///
    /// B4: this full-table `COUNT(*)` over `events` is only exercised by the
    /// storage db_tests; no production caller exists. Gate it behind
    /// `cfg(test)` so the unbounded scan is not reachable from the server
    /// binary, while the test coverage remains.
    #[cfg(test)]
    pub async fn get_total_message_count(&self) -> Result<i64, sqlx::Error> {
        let count: i64 = sqlx::query_scalar!(
            r#"
            SELECT COALESCE(COUNT(*), 0) AS "count!" FROM events WHERE event_type = 'm.room.message'
            "#,
        )
        .fetch_one(&*self.pool)
        .await?;
        Ok(count)
    }

    /// Count `m.room.message` events sent in the last 24 hours.
    pub async fn get_daily_message_count(&self) -> Result<i64, sqlx::Error> {
        let cutoff = current_timestamp_millis() - 24 * 60 * 60 * 1000;
        let count: i64 = sqlx::query_scalar!(
            r#"
            SELECT COALESCE(COUNT(*), 0) AS "count!" FROM events
            WHERE event_type = 'm.room.message' AND origin_server_ts >= $1
            "#,
            cutoff,
        )
        .fetch_one(&*self.pool)
        .await?;
        Ok(count)
    }

    /// See [`delete_room_events`].
    pub async fn delete_room_events(&self, room_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r"
            DELETE FROM events WHERE room_id = $1
            ",
            room_id,
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Power levels
    // -----------------------------------------------------------------------

    /// See [`count_room_events`].
    pub async fn count_room_events(&self, room_id: &str) -> Result<i64, sqlx::Error> {
        let count: i64 = sqlx::query_scalar!(
            r#"
            SELECT COALESCE(COUNT(*), 0) AS "count!" FROM events WHERE room_id = $1
            "#,
            room_id,
        )
        .fetch_one(&*self.pool)
        .await?;
        Ok(count)
    }

    /// See [`get_room_stats`].
    ///
    /// Computes the room-summary counters in a single SQL aggregate so callers
    /// never have to load a room's full event history into memory.
    pub async fn get_room_stats(&self, room_id: &str) -> Result<RoomEventStats, sqlx::Error> {
        // R4：`COUNT(*)` 及各 `FILTER` 分支均为无关系来源的聚合 ⇒ 推可空 ⇒ 断言非空
        // （`COUNT` 自身即保证非空）。
        let stats = sqlx::query_as!(
            RoomEventStats,
            r#"
            SELECT
                COUNT(*) AS "total_events!",
                COUNT(*) FILTER (WHERE state_key IS NOT NULL) AS "total_state_events!",
                COUNT(*) FILTER (WHERE event_type = 'm.room.message') AS "total_messages!",
                COUNT(*) FILTER (
                    WHERE event_type = 'm.room.message'
                      AND content->>'msgtype' IN ('m.image', 'm.video', 'm.file', 'm.audio')
                ) AS "total_media!"
            FROM events WHERE room_id = $1
            "#,
            room_id,
        )
        .fetch_one(&*self.pool)
        .await?;
        Ok(stats)
    }
}
