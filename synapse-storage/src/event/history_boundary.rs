//! The `m.room.history_visibility` boundary lookup.
//!
//! Answers exactly one question — "since when was this user entitled to read this
//! room's history?" — which is what every visibility-aware read path needs.
//!
//! Kept in its own module rather than in `state.rs`: that file sits just under the
//! 500-line ceiling enforced by `scripts/ci/check_file_size_ratchet.py`, and adding
//! a query there pushed it over.

use super::models::EventStorage;

/// The member event that defines a caller's history boundary, in **both**
/// coordinates the server needs.
///
/// The list surfaces (`/messages`, `/sync`'s timeline, `/relations`, `/search`)
/// compare *stream positions*; the count surfaces (`unread_notifications`) compare
/// *wall clocks*, because a count is derived from a read marker, which carries a
/// timestamp. So one boundary has to be readable as either.
///
/// Both fields come from the **same row on purpose**. A caller's boundary is a
/// single member event, and if each surface looked up "the boundary" for itself
/// they could land on two different rows — an earlier `invite` for one, a later
/// `join` for the other — and then disagree about what the caller may see, which
/// is exactly the class of inconsistency this pair exists to prevent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::FromRow)]
pub struct MembershipBoundary {
    /// `stream_ordering` of the boundary member event.
    pub stream_ordering: i64,
    /// `origin_server_ts` of that same member event.
    pub origin_server_ts: i64,
}

impl EventStorage {
    /// See [`crate::event::reader::EventReader::get_membership_boundary`].
    ///
    /// Reads the user's `m.room.member` events straight out of the event log (not
    /// the `room_memberships` projection, which only carries wall-clock
    /// timestamps and therefore cannot be compared with an event's
    /// `stream_ordering`).
    ///
    /// The membership is taken from `content->>'membership'` rather than trusting
    /// the row to exist, so a malformed or redacted member event cannot widen
    /// visibility: it simply does not match any allowed value and is ignored.
    ///
    /// `ORDER BY stream_ordering LIMIT 1` — rather than `MIN(stream_ordering)` —
    /// is what makes both columns come from one row.
    pub async fn get_membership_boundary(
        &self,
        room_id: &str,
        user_id: &str,
        include_invites: bool,
    ) -> Result<Option<MembershipBoundary>, sqlx::Error> {
        let allowed: Vec<String> =
            if include_invites { vec!["invite".to_string(), "join".to_string()] } else { vec!["join".to_string()] };

        // No matching row ⇒ `None`, which is exactly the "never entitled" signal
        // callers must NOT interpret as "unrestricted".
        sqlx::query_as!(
            MembershipBoundary,
            r#"
            SELECT
                stream_ordering AS "stream_ordering!",
                origin_server_ts AS "origin_server_ts!"
            FROM events
            WHERE room_id = $1
              AND event_type = 'm.room.member'
              AND state_key = $2
              AND content->>'membership' = ANY($3)
            ORDER BY stream_ordering
            LIMIT 1
            "#,
            room_id,
            user_id,
            &allowed,
        )
        .fetch_optional(&*self.pool)
        .await
    }
}
