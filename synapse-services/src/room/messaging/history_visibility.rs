//! `m.room.history_visibility` enforcement.
//!
//! The value is stored on the room, echoed in room summaries and protected as an
//! auth event, but **nothing enforced it on the read paths**: `/messages`,
//! `/event` and `/context` served a room's entire history to any current member,
//! including everything that happened before they were ever involved. This module
//! turns the stored value into a single, testable decision so every read path can
//! share one rule instead of each inventing its own.
//!
//! ## The rule
//!
//! `m.room.history_visibility` is evaluated against the membership the caller
//! had **when the event happened** (upstream
//! `synapse/visibility.py::_check_membership`). Because this codebase does not
//! keep per-event state groups for every event, the same decision is expressed
//! as a single **stream-ordering boundary**, which is exact whenever the room's
//! visibility value has not changed over its lifetime:
//!
//! | value | who may read | boundary |
//! | --- | --- | --- |
//! | `world_readable` | anyone | none (no filtering) |
//! | `shared` | members | none (no filtering) |
//! | `invited` | from the moment they were **invited** | first `invite`/`join` event |
//! | `joined` | from the moment they **joined** | first `join` event |
//!
//! Events strictly before that boundary are "history from before the caller was
//! involved" and are hidden. A caller with no qualifying membership event was
//! never entitled and therefore sees nothing — which is also the correct answer
//! for a non-member reading a public room whose policy is `invited`/`joined`.
//!
//! ## Deliberate simplifications (per-event state is not consulted)
//!
//! * Changing the value mid-room applies the **current** value to the whole
//!   timeline. Upstream would use the value in force at each event, so a
//!   `shared`→`joined` change hides earlier events here that upstream keeps
//!   visible for members. That direction is *stricter*, never leakier.
//! * A user who joined, left and rejoined keeps their original entitlement
//!   (upstream takes the "most joined" of the previous and current membership for
//!   the same reason), so events during the gap are visible here.

use super::service::MessagingService;
use synapse_common::ApiError;

/// The state event that carries the policy.
const HISTORY_VISIBILITY_EVENT_TYPE: &str = "m.room.history_visibility";

/// The effective `m.room.history_visibility` of a room.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryVisibility {
    /// Anyone, member or not, may read the whole history.
    WorldReadable,
    /// Every member may read the whole history (the room-version default).
    Shared,
    /// Members may read from the point they were invited.
    Invited,
    /// Members may read from the point they joined.
    Joined,
}

impl HistoryVisibility {
    /// Parse the state event's `history_visibility` value.
    ///
    /// Missing, unknown and malformed values all mean [`Self::Shared`], matching
    /// upstream `get_effective_room_visibility_from_state`. That is a
    /// **fail-open** choice on purpose: the value comes from an auth-protected
    /// state event, so an unreadable value means "no policy recorded", not "hide
    /// everything". A fail-closed default would black out history for every room
    /// whose visibility event is redacted.
    pub fn parse(raw: &str) -> Self {
        match raw {
            "world_readable" => Self::WorldReadable,
            "invited" => Self::Invited,
            "joined" => Self::Joined,
            _ => Self::Shared,
        }
    }

    /// Whether this value constrains which slice of the history a member sees.
    pub fn restricts_history(self) -> bool {
        matches!(self, Self::Invited | Self::Joined)
    }

    /// Whether `invite` counts towards the boundary (only `invited` does).
    pub fn counts_invites(self) -> bool {
        matches!(self, Self::Invited)
    }
}

/// How much of a room's history a specific caller may see.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryCut {
    /// No filtering: every event is visible.
    Unrestricted,
    /// Only events whose `stream_ordering` is `>= from` are visible.
    ///
    /// [`HistoryCut::NOTHING`] (`from == i64::MAX`) is the "was never entitled"
    /// case: stream orderings are positive, so it matches nothing.
    From(i64),
}

impl HistoryCut {
    /// A cut that hides everything: the caller has no qualifying membership.
    pub const NOTHING: Self = Self::From(i64::MAX);

    /// Whether an event at `stream_ordering` is inside this cut.
    ///
    /// An event with no recorded `stream_ordering` is hidden whenever the cut
    /// restricts: "unknown position" must not be treated as "old enough to be
    /// public".
    pub fn allows(self, stream_ordering: Option<i64>) -> bool {
        match self {
            Self::Unrestricted => true,
            Self::From(from) => stream_ordering.is_some_and(|position| position >= from),
        }
    }
}

impl MessagingService {
    /// The room's effective history visibility.
    ///
    /// Read from the **current `m.room.history_visibility` state event**, not from
    /// the `rooms.history_visibility` projection: the projection is written at
    /// room creation and by room-summary updates, so a client that changes the
    /// value later (it is an ordinary state event with power level 100) would keep
    /// being judged by the stale copy.
    ///
    /// No state event ⇒ [`HistoryVisibility::Shared`], the same fail-open default
    /// [`HistoryVisibility::parse`] applies to an unreadable value.
    async fn room_history_visibility(&self, room_id: &str) -> Result<HistoryVisibility, ApiError> {
        let state_event = self
            .event_reader
            .get_state_event(room_id, HISTORY_VISIBILITY_EVENT_TYPE, "")
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get room history visibility", e))?;

        let raw = state_event
            .as_ref()
            .and_then(|event| event.content.get("history_visibility"))
            .and_then(|value| value.as_str());

        Ok(raw.map_or(HistoryVisibility::Shared, HistoryVisibility::parse))
    }

    /// What `user_id` may see of `room_id`'s history.
    ///
    /// One extra query for `invited`/`joined` rooms and **no** extra query for
    /// `shared`/`world_readable`, which is the overwhelming majority.
    pub(crate) async fn history_cut(&self, room_id: &str, user_id: &str) -> Result<HistoryCut, ApiError> {
        let visibility = self.room_history_visibility(room_id).await?;
        if !visibility.restricts_history() {
            return Ok(HistoryCut::Unrestricted);
        }

        let boundary = self
            .event_reader
            .get_membership_boundary_stream_ordering(room_id, user_id, visibility.counts_invites())
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to resolve history visibility boundary", e))?;

        Ok(boundary.map_or(HistoryCut::NOTHING, HistoryCut::From))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_recognises_every_specified_value() {
        assert_eq!(HistoryVisibility::parse("world_readable"), HistoryVisibility::WorldReadable);
        assert_eq!(HistoryVisibility::parse("shared"), HistoryVisibility::Shared);
        assert_eq!(HistoryVisibility::parse("invited"), HistoryVisibility::Invited);
        assert_eq!(HistoryVisibility::parse("joined"), HistoryVisibility::Joined);
    }

    #[test]
    fn parse_falls_back_to_shared_for_unknown_or_empty_values() {
        // Fail-open is deliberate (see the type docs): an unreadable value must not
        // black out a room's history.
        for raw in ["", "JOINED", "joined ", "public", "world-readable"] {
            assert_eq!(HistoryVisibility::parse(raw), HistoryVisibility::Shared, "input {raw:?}");
        }
    }

    #[test]
    fn only_invited_and_joined_restrict_history() {
        assert!(!HistoryVisibility::WorldReadable.restricts_history());
        assert!(!HistoryVisibility::Shared.restricts_history());
        assert!(HistoryVisibility::Invited.restricts_history());
        assert!(HistoryVisibility::Joined.restricts_history());
    }

    #[test]
    fn only_invited_counts_invites_towards_the_boundary() {
        assert!(HistoryVisibility::Invited.counts_invites());
        assert!(!HistoryVisibility::Joined.counts_invites());
    }

    #[test]
    fn unrestricted_allows_even_an_event_without_a_stream_ordering() {
        assert!(HistoryCut::Unrestricted.allows(None));
        assert!(HistoryCut::Unrestricted.allows(Some(0)));
    }

    #[test]
    fn from_is_inclusive_at_the_boundary_and_strict_before_it() {
        let cut = HistoryCut::From(100);
        assert!(cut.allows(Some(100)), "the boundary event itself is the caller's own join/invite");
        assert!(cut.allows(Some(101)));
        assert!(!cut.allows(Some(99)));
    }

    #[test]
    fn a_missing_stream_ordering_is_hidden_when_the_cut_restricts() {
        // "unknown position" must not be treated as "old enough to be public".
        assert!(!HistoryCut::From(1).allows(None));
        assert!(!HistoryCut::NOTHING.allows(None));
    }

    #[test]
    fn nothing_hides_every_real_event() {
        assert!(!HistoryCut::NOTHING.allows(Some(0)));
        assert!(!HistoryCut::NOTHING.allows(Some(i64::MAX - 1)));
    }
}
