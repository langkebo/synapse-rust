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
        let row: Option<(String,)> = sqlx::query_as(
            r"
            SELECT event_id
            FROM room_event_txn_dedup
            WHERE user_id = $1 AND room_id = $2 AND txn_id = $3
            ",
        )
        .bind(user_id)
        .bind(room_id)
        .bind(txn_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row.map(|(event_id,)| event_id))
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
        let result = sqlx::query(
            r"
            INSERT INTO room_event_txn_dedup (user_id, room_id, txn_id, event_id, created_ts)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (user_id, room_id, txn_id) DO NOTHING
            ",
        )
        .bind(user_id)
        .bind(room_id)
        .bind(txn_id)
        .bind(event_id)
        .bind(synapse_common::current_timestamp_millis())
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Best-effort removal of a losing duplicate event after a txn race.
    pub async fn delete_event_by_id(&self, event_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM events WHERE event_id = $1").bind(event_id).execute(&*self.pool).await?;
        Ok(())
    }
}
