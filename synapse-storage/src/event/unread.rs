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
    /// `visibility_floor_ts` is the caller's `m.room.history_visibility` boundary
    /// expressed as a **wall clock** (see
    /// [`EventReader::get_membership_boundary`](crate::event::EventReader::get_membership_boundary));
    /// pass `0` for "no policy", which is what `shared` / `world_readable` mean.
    /// The window's lower bound is `GREATEST(read marker, floor)`.
    ///
    /// The floor is load-bearing, not defensive. A member who joined later has
    /// **no read marker** in that room — joining does not write one, only a client
    /// receipt does — so `last_read_ts` collapsed to `0` and every *pre-join*
    /// message was counted as unread. The client then showed "4 unread" over a
    /// timeline that listed one message: history it may not read leaked through
    /// the **count** while every list surface hid it.
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
    ///
    /// Membership is a precondition, not a filter (`EXISTS … room_memberships`).
    ///
    /// Upstream derives unread from **push actions**, and
    /// `BulkPushRuleEvaluator.action_for_pushing_events` only iterates
    /// `get_local_users_in_room` (release-v1.161, `bulk_push_rule_evaluator.py:189`)
    /// ⇒ a user who is not in the room has no push actions and therefore **no
    /// unread**, however readable the room is.
    ///
    /// This computed-on-read design has no such writer, so the rule has to live in
    /// the query. Without it, `GET /rooms/{roomId}/unread_count` — whose gate is
    /// `ensure_room_view_access`, i.e. *visibility*, not membership — would answer a
    /// total stranger in a `world_readable` room with the room's entire non-state
    /// history as their "unread count": `last_read_ts` stays `0` (a non-member has
    /// no read marker) and nothing else stops the count. `invite` is included so an
    /// invited user still sees the invite; `join` is the normal case.
    pub async fn get_unread_counts(
        &self,
        room_id: &str,
        user_id: &str,
        visibility_floor_ts: i64,
    ) -> Result<RoomUnreadCounts, sqlx::Error> {
        // R4 ①：`$1`（参数）与两个 `COALESCE(COUNT(...), 0)` 都**没有关系来源** ⇒ sqlx 推成可空。
        // 谁保证非空：`room_id` 是本函数的 `&str` 参数（Rust 侧不可为 NULL）；两个计数是
        // `COALESCE(..., 0)` 的**恒非空**表达式（空窗口落到字面量 0）。
        sqlx::query_as!(
            RoomUnreadCounts,
            r#"
            WITH last_read AS (
                SELECT GREATEST(
                           COALESCE(MAX(e.origin_server_ts), MAX(rm.origin_server_ts), 0),
                           $3
                       ) AS last_read_ts
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
             AND EXISTS (
                 SELECT 1 FROM room_memberships m
                 WHERE m.room_id = $1
                   AND m.user_id = $2
                   AND m.membership IN ('join', 'invite')
             )
            GROUP BY lr.last_read_ts
            "#,
            room_id,
            user_id,
            visibility_floor_ts,
        )
        .fetch_one(&*self.pool)
        .await
    }

    /// Batch variant of [`get_unread_counts`](Self::get_unread_counts).
    ///
    /// `room_floors` pairs every room with its visibility floor (see the
    /// single-room variant). A `(room_id, floor_ts)` pair rather than two parallel
    /// slices: misaligning two arrays would silently move one room's floor onto
    /// another room, and that is a count-only history leak in whichever direction
    /// it slips.
    ///
    /// P1-7: like the single-room variant, `last_read_ts` falls back to
    /// `read_markers.origin_server_ts` when the referenced event is purged.
    /// D-98: the highlight branch matches the user id literally (`strpos`), not
    /// through a `LIKE` pattern — see [`Self::get_unread_counts`].
    pub async fn get_unread_counts_batch(
        &self,
        room_floors: &[(String, i64)],
        user_id: &str,
    ) -> Result<Vec<RoomUnreadCounts>, sqlx::Error> {
        if room_floors.is_empty() {
            return Ok(Vec::new());
        }

        let room_ids: Vec<&str> = room_floors.iter().map(|(room_id, _)| room_id.as_str()).collect();
        let floors: Vec<i64> = room_floors.iter().map(|(_, floor_ts)| *floor_ts).collect();

        // R4 ①：`tr.room_id` 来自 `SELECT UNNEST($2::text[])`（集合返回函数 ⇒ 无关系来源），
        // 两个 `COALESCE(COUNT(...), 0)` 同理 ⇒ 三列都需断言。谁保证非空：`room_id` 取自
        // 调用方 `&[String]`（Rust 侧不可为 NULL），计数是 `COALESCE(..., 0)` 的恒非空表达式。
        //
        // 两个 `UNNEST` 在同一 SELECT 列表里由 PostgreSQL **成对拉链**求值（7.2.1.4），
        // 所以第 N 行的 floor 必属于第 N 个 room_id —— 这正是选择"成对入参"的落点。
        sqlx::query_as!(
            RoomUnreadCounts,
            r#"
            WITH target_rooms AS (
                SELECT UNNEST($2::text[]) AS room_id, UNNEST($3::bigint[]) AS floor_ts
            ),
            last_reads AS (
                SELECT tr.room_id,
                       GREATEST(
                           COALESCE(MAX(e.origin_server_ts), MAX(rm.origin_server_ts), 0),
                           tr.floor_ts
                       ) AS last_read_ts
                FROM target_rooms tr
                LEFT JOIN read_markers rm
                  ON rm.room_id = tr.room_id
                 AND rm.user_id = $1
                LEFT JOIN events e
                  ON e.event_id = rm.event_id
                GROUP BY tr.room_id, tr.floor_ts
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
             AND EXISTS (
                 SELECT 1 FROM room_memberships m
                 WHERE m.room_id = tr.room_id
                   AND m.user_id = $1
                   AND m.membership IN ('join', 'invite')
             )
            GROUP BY tr.room_id, lr.last_read_ts
            "#,
            user_id,
            &room_ids as &[&str],
            &floors,
        )
        .fetch_all(&*self.pool)
        .await
    }
}
