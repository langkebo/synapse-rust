//! ISSUE-03: Durable transaction-id dedup for client-sent room events.
//!
//! Backed by the `room_event_txn_dedup` table (migration
//! `20260810160000_create_room_event_txn_dedup.sql`). The PRIMARY KEY on
//! `(user_id, room_id, txn_id)` is the source of truth; the 1h-TTL cache
//! in the route layer remains only as a fast path.

use super::EventStorage;

impl EventStorage {
    /// Look up the event previously recorded for `(user_id, room_id, txn_id)`.
    pub async fn get_event_id_by_txn(
        &self,
        user_id: &str,
        room_id: &str,
        txn_id: &str,
    ) -> Result<Option<String>, sqlx::Error> {
        // `event_id` 是 `TEXT NOT NULL` ⇒ `query_scalar!` 给 `String`，`fetch_optional` 正好是
        // `Option<String>`（R6 ⑤：元组投影本就不能用 `query_as!`）。
        sqlx::query_scalar!(
            r"
            SELECT event_id
            FROM room_event_txn_dedup
            WHERE user_id = $1 AND room_id = $2 AND txn_id = $3
            ",
            user_id,
            room_id,
            txn_id,
        )
        .fetch_optional(&*self.pool)
        .await
    }

    /// Record the `txn_id → event_id` mapping. Returns `true` when the row
    /// was inserted, `false` when the `(user_id, room_id, txn_id)` triple
    /// was already taken by a concurrent/earlier send (ON CONFLICT DO
    /// NOTHING), in which case the caller must resolve the winner via
    /// [`Self::get_event_id_by_txn`].
    pub async fn record_event_txn(
        &self,
        user_id: &str,
        room_id: &str,
        txn_id: &str,
        event_id: &str,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r"
            INSERT INTO room_event_txn_dedup (user_id, room_id, txn_id, event_id, created_ts)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (user_id, room_id, txn_id) DO NOTHING
            ",
            user_id,
            room_id,
            txn_id,
            event_id,
            synapse_common::current_timestamp_millis(),
        )
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Same as [`Self::record_event_txn`] but enlists in the caller's
    /// transaction, so the dedup marker commits atomically with the event it
    /// points at (A4).
    ///
    /// This closes the window the two-phase `begin_txn`/`finish_txn` protocol
    /// left open: with the marker written in a *separate* transaction after the
    /// event commit, a crash in between left a visible event with no dedup row
    /// (client retries created a second copy). Here the caller writes the event
    /// and the marker in one transaction: either both become visible or neither.
    ///
    /// Returns `true` when the row was inserted, `false` on a concurrent
    /// duplicate. Because `ON CONFLICT DO NOTHING` blocks until the conflicting
    /// transaction settles, a `false` here means the *other* transaction
    /// committed its row — the caller can roll back its own (never-committed)
    /// event and read the winner via [`Self::get_event_id_by_txn`].
    pub async fn record_event_txn_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        user_id: &str,
        room_id: &str,
        txn_id: &str,
        event_id: &str,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r"
            INSERT INTO room_event_txn_dedup (user_id, room_id, txn_id, event_id, created_ts)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (user_id, room_id, txn_id) DO NOTHING
            ",
            user_id,
            room_id,
            txn_id,
            event_id,
            synapse_common::current_timestamp_millis(),
        )
        .execute(&mut **tx)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// B-8: mark a losing duplicate event as soft-failed instead of physically
    /// deleting it.
    ///
    /// Replaces the prior `delete_event_by_id` hard-delete path.  The losing
    /// event's row remains in the `events` table (preserving the FK graph and
    /// audit trail) but is hidden from consumer read paths that filter on
    /// `soft_failed = FALSE`.  See migration
    /// `20260906010000_add_events_soft_failed.sql` for the schema change.
    ///
    /// Idempotent: calling on an already soft-failed event is a no-op (the
    /// `UPDATE ... WHERE soft_failed = FALSE` simply matches zero rows and
    /// returns `Ok(())`).  This is deliberate so that retries from
    /// `send_message_with_txn` after a partial failure are safe.
    pub async fn mark_event_soft_failed(&self, event_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r"
            UPDATE events
            SET soft_failed = TRUE
            WHERE event_id = $1 AND soft_failed = FALSE
            ",
            event_id,
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }
}
