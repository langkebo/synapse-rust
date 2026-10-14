//! The `m.room.history_visibility` boundary lookup.
//!
//! Answers exactly one question — "since which `stream_ordering` was this user
//! entitled to read this room's history?" — which is what every visibility-aware
//! read path needs. Kept in its own module rather than in `state.rs`: that file
//! sits just under the 500-line ceiling enforced by
//! `scripts/ci/check_file_size_ratchet.py`, and adding a query there pushed it
//! over.

use super::models::EventStorage;

impl EventStorage {
    /// See [`crate::event::reader::EventReader::get_membership_boundary_stream_ordering`].
    ///
    /// Reads the user's `m.room.member` events straight out of the event log (not
    /// the `room_memberships` projection, which only carries wall-clock
    /// timestamps and therefore cannot be compared with an event's
    /// `stream_ordering`).
    ///
    /// The membership is taken from `content->>'membership'` rather than trusting
    /// the row to exist, so a malformed or redacted member event cannot widen
    /// visibility: it simply does not match any allowed value and is ignored.
    pub async fn get_membership_boundary_stream_ordering(
        &self,
        room_id: &str,
        user_id: &str,
        include_invites: bool,
    ) -> Result<Option<i64>, sqlx::Error> {
        let allowed: Vec<String> =
            if include_invites { vec!["invite".to_string(), "join".to_string()] } else { vec!["join".to_string()] };

        // `MIN` over an empty set is SQL NULL ⇒ `fetch_one` yields `None`, which is
        // exactly the "never entitled" signal callers must NOT interpret as
        // "unrestricted".
        sqlx::query_scalar!(
            r#"
            SELECT MIN(stream_ordering) AS "boundary?"
            FROM events
            WHERE room_id = $1
              AND event_type = 'm.room.member'
              AND state_key = $2
              AND content->>'membership' = ANY($3)
            "#,
            room_id,
            user_id,
            &allowed,
        )
        .fetch_one(&*self.pool)
        .await
    }
}
