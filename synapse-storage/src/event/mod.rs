pub(crate) mod api;
pub mod batch;
pub(crate) mod create;
pub(crate) mod dag;
pub(crate) mod ephemeral;
pub(crate) mod models;
pub(crate) mod pagination;
pub(crate) mod redaction;
pub(crate) mod search;
pub(crate) mod signature;
pub mod state;

pub use api::EventStoreApi;
pub use models::*;

use std::sync::Arc;

use sqlx::{Pool, Postgres};

/// Canonical 15-column SELECT list for `RoomEvent` deserialization.
///
/// Used by `event/mod.rs` and `event/batch.rs` to avoid hand-rolling the same
/// column list across 15+ query methods. Mirrors the pattern already in use
/// for `StateEvent` via `STATE_EVENT_OUTER_COLS` / `STATE_EVENT_INNER_COLS`
/// in `event/state.rs`.
///
/// The `COALESCE(...)` expressions preserve backward-compatible null handling:
/// - `user_id` falls back to `sender` for legacy events
/// - `depth` / `origin_server_ts` / `not_before` default to 0
/// - `origin` normalizes empty/`undefined` strings to `'self'`
///
/// Ref: TDD落地执行清单 §8.2 ARC-1..5 (Problem #2 SQL Column Boilerplate)
pub(crate) const ROOM_EVENT_COLS: &str = "\
    event_id, room_id, COALESCE(user_id, sender) as user_id, event_type, content, state_key, \
    COALESCE(depth, 0) as depth, COALESCE(origin_server_ts, 0) as origin_server_ts, \
    COALESCE(origin_server_ts, 0) as processed_at, COALESCE(not_before, 0) as not_before, \
    status, reference_image, COALESCE(NULLIF(NULLIF(BTRIM(origin), ''), 'undefined'), 'self') as origin, \
    stream_ordering, redacts";

impl EventStorage {
    pub fn new(pool: &Arc<Pool<Postgres>>, server_name: String) -> Self {
        Self { pool: pool.clone(), server_name }
    }

    pub async fn get_event(&self, event_id: &str) -> Result<Option<RoomEvent>, sqlx::Error> {
        let event = sqlx::query_as::<_, RoomEvent>(
            r"
            SELECT event_id, room_id, sender as user_id, event_type, content, state_key,
                   COALESCE(depth, 0) as depth, origin_server_ts, origin_server_ts as processed_at,
                   COALESCE(not_before, 0) as not_before, status, reference_image, COALESCE(origin, 'self') as origin, stream_ordering, redacts
            FROM events WHERE event_id = $1
            ",
        )
        .bind(event_id)
        .fetch_optional(&*self.pool)
        .await?;
        Ok(event)
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

    /// Count `m.room.message` events sent in the last 24 hours.
    pub async fn get_daily_message_count(&self) -> Result<i64, sqlx::Error> {
        let cutoff = chrono::Utc::now().timestamp_millis() - 24 * 60 * 60 * 1000;
        let count = sqlx::query_scalar::<_, i64>(
            r"
            SELECT COALESCE(COUNT(*), 0) FROM events
            WHERE event_type = 'm.room.message' AND origin_server_ts >= $1
            ",
        )
        .bind(cutoff)
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
    // Power levels
    // -----------------------------------------------------------------------

    pub async fn count_room_events(&self, room_id: &str) -> Result<i64, sqlx::Error> {
        let count = sqlx::query_scalar::<_, i64>(
            r"
            SELECT COALESCE(COUNT(*), 0) FROM events WHERE room_id = $1
            ",
        )
        .bind(room_id)
        .fetch_one(&*self.pool)
        .await?;
        Ok(count)
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod db_tests;
