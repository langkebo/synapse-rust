// MSC4354 置顶的**联邦补发游标**（UG-7 catch-up，#20165 / #20233）。
//
// 置顶走的是增量 EDU 推送，EDU 没有位点 ⇒ 远端断连期间漏掉的那几条永远补不回来，
// 新入房的服务器也拿不到房间已有的置顶。这里用 `room_sticky_event_destinations`
// 记下「每个 destination 已经确认收到本服务器的最高 `revision`」：
//
//   * `last_revision = 0` ⇒ 全量补发（新入房或首次建游标）；
//   * 对账取 `room_sticky_events.revision > last_revision` 的行，**包含**
//     `is_sticky = false` 的墓碑；
//   * 只有发送方**确实把事务发了出去**才推进游标（见 `RoomService::run_sticky_catch_up_tick`）。
//
// 用 `revision`（序列）而不是 `updated_ts`（挂钟）作位点：同一毫秒内的两次写入会让
// `updated_ts` 并列（`>` 之后漏掉一条），时钟回拨则更糟。

use super::StickyEventStorage;
use synapse_common::current_timestamp_millis;

/// A `(room, destination)` pair that still owes the peer at least one sticky change.
#[derive(Debug, Clone)]
pub struct PendingStickyDelivery {
    /// Room the backlog belongs to.
    pub room_id: String,
    /// Remote server name (no leading `@`).
    pub destination: String,
    /// Highest revision already confirmed delivered; `0` means "send everything".
    pub last_revision: i64,
}

/// One sticky row to replay, in `revision` order.
#[derive(Debug, Clone)]
pub struct StickyDeliveryRow {
    /// Monotonic position of this row's last write.
    pub revision: i64,
    /// The event the pin points at.
    pub event_id: String,
    /// The pinned event type.
    pub event_type: String,
    /// `false` means this revision is a **removal** (tombstone), not a pin.
    pub is_sticky: bool,
    /// Client-requested duration of the pin, if any.
    pub sticky_duration_ms: Option<i64>,
    /// Absolute server-side expiry of the pin, if any.
    pub expires_at: Option<i64>,
}

impl StickyEventStorage {
    /// Create cursors for remote servers that already have joined members in rooms
    /// where **this** server owns sticky rows.
    ///
    /// This is the backstop for rooms that predate the cursor table: without it a
    /// long-lived deployment would never give existing peers a cursor, and the
    /// reconciliation loop would never see them. A fresh cursor is `0` ⇒ those
    /// peers receive one full (idempotent) replay of the room's pins.
    ///
    /// `ON CONFLICT DO NOTHING` makes it safe to run on every tick: a peer whose
    /// cursor has already advanced is left untouched.
    pub async fn seed_destination_cursors(&self, server_name: &str) -> Result<u64, sqlx::Error> {
        let now = current_timestamp_millis();
        // `substring(user_id FROM strpos(user_id, ':') + 1)` 取服务器段时保留端口：
        // `split_part(user_id, ':', 2)` 会把 `@u:example.com:8448` 截成 `example.com`。
        let result = sqlx::query!(
            r"
            INSERT INTO room_sticky_event_destinations
                (room_id, destination, last_revision, created_ts, updated_ts)
            SELECT DISTINCT e.room_id,
                   substring(m.user_id FROM strpos(m.user_id, ':') + 1),
                   0, $1::bigint, $1::bigint
            FROM room_sticky_events e
            JOIN room_memberships m
              ON m.room_id = e.room_id
             AND m.membership = 'join'
             AND m.user_id LIKE '@%:%'
            WHERE e.is_sticky = true
              AND (e.user_id = $2 OR e.user_id LIKE '%:' || $2)
              AND substring(m.user_id FROM strpos(m.user_id, ':') + 1) <> $2
            ON CONFLICT (room_id, destination) DO NOTHING
            ",
            now,
            server_name,
        )
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    /// Force `destination` to receive **everything** the room has on next pass.
    ///
    /// Called when a remote server joins a room (#20233): whatever it may have seen
    /// before the join is irrelevant, and whatever it missed while absent must be
    /// replayed. Resetting an already-current cursor only costs one idempotent
    /// replay, so a duplicate `send_join` is harmless.
    pub async fn reset_destination_cursor(&self, room_id: &str, destination: &str) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();
        sqlx::query!(
            r"
            INSERT INTO room_sticky_event_destinations
                (room_id, destination, last_revision, created_ts, updated_ts)
            VALUES ($1, $2, 0, $3, $3)
            ON CONFLICT (room_id, destination)
            DO UPDATE SET last_revision = 0, updated_ts = EXCLUDED.updated_ts
            ",
            room_id,
            destination,
            now,
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// `(room, destination)` pairs that still owe a peer at least one sticky change.
    ///
    /// Two `EXISTS` predicates keep the result honest:
    /// * the room must have a row **this server owns** past the peer's cursor —
    ///   other origins' rows are theirs to deliver, not ours;
    /// * the peer must still be joined — a departed server would otherwise hold a
    ///   backlog forever (and the room FK keeps the cursor row alive anyway).
    pub async fn pending_sticky_destinations(
        &self,
        server_name: &str,
        limit: i64,
    ) -> Result<Vec<PendingStickyDelivery>, sqlx::Error> {
        sqlx::query_as!(
            PendingStickyDelivery,
            r"
            SELECT d.room_id, d.destination, d.last_revision
            FROM room_sticky_event_destinations d
            WHERE EXISTS (
                      SELECT 1 FROM room_sticky_events e
                      WHERE e.room_id = d.room_id
                        AND e.revision > d.last_revision
                        AND (e.user_id = $1 OR e.user_id LIKE '%:' || $1)
                  )
              AND EXISTS (
                      SELECT 1 FROM room_memberships m
                      WHERE m.room_id = d.room_id
                        AND m.membership = 'join'
                        AND m.user_id LIKE '%:' || d.destination
                  )
            ORDER BY d.room_id, d.destination
            LIMIT $2
            ",
            server_name,
            limit,
        )
        .fetch_all(&*self.pool)
        .await
    }

    /// Sticky rows of `room_id` owned by `server_name` with `revision > after_revision`,
    /// oldest first.
    ///
    /// Tombstones (`is_sticky = false`) are included on purpose: skipping them would
    /// leave a cleared pin on the peer forever. Expired pins are **not** filtered
    /// here either — the caller must turn them into removals (its "now" is the one
    /// that matters), and filtering would silently advance the cursor past them.
    pub async fn pending_sticky_rows(
        &self,
        room_id: &str,
        server_name: &str,
        after_revision: i64,
        limit: i64,
    ) -> Result<Vec<StickyDeliveryRow>, sqlx::Error> {
        sqlx::query_as!(
            StickyDeliveryRow,
            r"
            SELECT e.revision, e.event_id, e.event_type, e.is_sticky,
                   e.sticky_duration_ms, e.expires_at
            FROM room_sticky_events e
            WHERE e.room_id = $1
              AND e.revision > $2
              AND (e.user_id = $3 OR e.user_id LIKE '%:' || $3)
            ORDER BY e.revision ASC
            LIMIT $4
            ",
            room_id,
            after_revision,
            server_name,
            limit,
        )
        .fetch_all(&*self.pool)
        .await
    }

    /// Record that `destination` is confirmed caught up to `last_revision`.
    ///
    /// `GREATEST` keeps the cursor monotonic: the periodic tick and the join-time
    /// backfill can run concurrently, and a stale writer must never push it back.
    pub async fn advance_destination_cursor(
        &self,
        room_id: &str,
        destination: &str,
        last_revision: i64,
    ) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();
        sqlx::query!(
            r"
            UPDATE room_sticky_event_destinations
            SET last_revision = GREATEST(last_revision, $3),
                updated_ts = $4
            WHERE room_id = $1 AND destination = $2
            ",
            room_id,
            destination,
            last_revision,
            now,
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }
}
