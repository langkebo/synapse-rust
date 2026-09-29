//! Unread-count queries over the `events` table.
//!
//! These methods were moved here from `RoomStorage` to respect the storage
//! layer boundary defined in project rules §7.1: `RoomStorage` must not
//! access event data directly. They operate on the `events` table, so they
//! belong on `EventStorage`.

use super::models::*;
use crate::room::RoomUnreadCounts;

impl EventStorage {
    /// Get unread notification and highlight counts for a user in a room.
    ///
    /// P1-7: `last_read_ts` falls back to `read_markers.origin_server_ts` when
    /// the referenced event has been purged (the LEFT JOIN returns NULL).
    /// Without this fallback, `last_read_ts` would collapse to 0 and every
    /// remaining event in the room — including already-read local events that
    /// survived `purge_history` — would be counted as unread (count bloat).
    ///
    /// D-98: the highlight branch matches the reader's user id **literally** via
    /// `strpos(...) > 0`. It used to build a `LIKE '%<user_id>%'` pattern, but `_`
    /// and `%` are `LIKE` metacharacters and Matrix localparts may contain `_` ⇒ a
    /// *different* user id differing only at an underscore position was counted as
    /// a mention of the reader (`%` would widen the pattern to "match anything").
    pub async fn get_unread_counts(&self, room_id: &str, user_id: &str) -> Result<RoomUnreadCounts, sqlx::Error> {
        // R4 ①：`$1`（参数）与两个 `COALESCE(COUNT(...), 0)` 都**没有关系来源** ⇒ sqlx 推成可空。
        // 谁保证非空：`room_id` 是本函数的 `&str` 参数（Rust 侧不可为 NULL）；两个计数是
        // `COALESCE(..., 0)` 的**恒非空**表达式（空窗口落到字面量 0）。
        sqlx::query_as!(
            RoomUnreadCounts,
            r#"
            WITH last_read AS (
                SELECT COALESCE(MAX(e.origin_server_ts), MAX(rm.origin_server_ts), 0) AS last_read_ts
                FROM read_markers rm
                LEFT JOIN events e ON e.event_id = rm.event_id
                WHERE rm.room_id = $1 AND rm.user_id = $2
            )
            SELECT
                $1 AS "room_id!",
                COALESCE(COUNT(ev.event_id) FILTER (
                    WHERE COALESCE(ev.user_id, ev.sender) != $2
                      AND ev.state_key IS NULL
                      AND ev.origin_server_ts > lr.last_read_ts
                ), 0) AS "notification_count!",
                COALESCE(COUNT(ev.event_id) FILTER (
                    WHERE COALESCE(ev.user_id, ev.sender) != $2
                      AND ev.state_key IS NULL
                      AND ev.origin_server_ts > lr.last_read_ts
                      AND (
                        strpos(ev.content::text, $2) > 0
                        OR strpos(ev.content::text, '@room') > 0
                      )
                ), 0) AS "highlight_count!"
            FROM last_read lr
            LEFT JOIN events ev
              ON ev.room_id = $1
             AND COALESCE(ev.user_id, ev.sender) != $2
             AND ev.state_key IS NULL
             AND ev.soft_failed = FALSE
             AND ev.origin_server_ts > lr.last_read_ts
            GROUP BY lr.last_read_ts
            "#,
            room_id,
            user_id,
        )
        .fetch_one(&*self.pool)
        .await
    }

    /// Batch variant of [`get_unread_counts`](Self::get_unread_counts).
    ///
    /// P1-7: like the single-room variant, `last_read_ts` falls back to
    /// `read_markers.origin_server_ts` when the referenced event is purged.
    /// D-98: the highlight branch matches the user id literally (`strpos`), not
    /// through a `LIKE` pattern — see [`Self::get_unread_counts`].
    pub async fn get_unread_counts_batch(
        &self,
        room_ids: &[String],
        user_id: &str,
    ) -> Result<Vec<RoomUnreadCounts>, sqlx::Error> {
        if room_ids.is_empty() {
            return Ok(Vec::new());
        }

        // R4 ①：`tr.room_id` 来自 `SELECT UNNEST($2::text[])`（集合返回函数 ⇒ 无关系来源），
        // 两个 `COALESCE(COUNT(...), 0)` 同理 ⇒ 三列都需断言。谁保证非空：`room_id` 取自
        // 调用方 `&[String]`（Rust 侧不可为 NULL），计数是 `COALESCE(..., 0)` 的恒非空表达式。
        sqlx::query_as!(
            RoomUnreadCounts,
            r#"
            WITH target_rooms AS (
                SELECT UNNEST($2::text[]) AS room_id
            ),
            last_reads AS (
                SELECT tr.room_id,
                       COALESCE(MAX(e.origin_server_ts), MAX(rm.origin_server_ts), 0) AS last_read_ts
                FROM target_rooms tr
                LEFT JOIN read_markers rm
                  ON rm.room_id = tr.room_id
                 AND rm.user_id = $1
                LEFT JOIN events e
                  ON e.event_id = rm.event_id
                GROUP BY tr.room_id
            )
            SELECT
                tr.room_id AS "room_id!",
                COALESCE(COUNT(ev.event_id) FILTER (
                    WHERE COALESCE(ev.user_id, ev.sender) != $1
                      AND ev.state_key IS NULL
                      AND ev.origin_server_ts > lr.last_read_ts
                ), 0) AS "notification_count!",
                COALESCE(COUNT(ev.event_id) FILTER (
                    WHERE COALESCE(ev.user_id, ev.sender) != $1
                      AND ev.state_key IS NULL
                      AND ev.origin_server_ts > lr.last_read_ts
                      AND (
                        strpos(ev.content::text, $1) > 0
                        OR strpos(ev.content::text, '@room') > 0
                      )
                ), 0) AS "highlight_count!"
            FROM target_rooms tr
            LEFT JOIN last_reads lr
              ON lr.room_id = tr.room_id
            LEFT JOIN events ev
              ON ev.room_id = tr.room_id
             AND COALESCE(ev.user_id, ev.sender) != $1
             AND ev.state_key IS NULL
             AND ev.soft_failed = FALSE
             AND ev.origin_server_ts > lr.last_read_ts
            GROUP BY tr.room_id, lr.last_read_ts
            "#,
            user_id,
            room_ids,
        )
        .fetch_all(&*self.pool)
        .await
    }
}
