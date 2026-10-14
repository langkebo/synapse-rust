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
use std::collections::HashMap;
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
    /// Only events at or after this boundary are visible **or countable**.
    ///
    /// Both coordinates describe the **same** `m.room.member` event, looked up
    /// once (see [`EventReader::get_membership_boundary`]): the list surfaces
    /// compare `stream_ordering` ([`Self::allows`]), the count surfaces compare
    /// `origin_server_ts` ([`Self::unread_floor_ts`]). They travel together so a
    /// caller cannot resolve one axis and forget the other — which is how the
    /// unread counts ended up leaking hidden history after every list surface had
    /// been fixed.
    ///
    /// [`HistoryCut::NOTHING`] is the "was never entitled" case: stream orderings
    /// are positive and no real timestamp reaches `i64::MAX`, so it matches
    /// nothing on either axis.
    From {
        /// `stream_ordering` of the boundary member event.
        stream_ordering: i64,
        /// `origin_server_ts` of that same member event.
        origin_server_ts: i64,
    },
}

impl HistoryCut {
    /// A cut that hides everything: the caller has no qualifying membership.
    pub const NOTHING: Self = Self::From { stream_ordering: i64::MAX, origin_server_ts: i64::MAX };

    /// Whether an event at `stream_ordering` is inside this cut.
    ///
    /// An event with no recorded `stream_ordering` is hidden whenever the cut
    /// restricts: "unknown position" must not be treated as "old enough to be
    /// public".
    pub fn allows(self, stream_ordering: Option<i64>) -> bool {
        match self {
            Self::Unrestricted => true,
            Self::From { stream_ordering: from, .. } => stream_ordering.is_some_and(|position| position >= from),
        }
    }

    /// The **wall-clock** floor the count surfaces (`unread_notifications`) must
    /// apply, i.e. "the earliest event this caller may have counted at all".
    ///
    /// `0` for [`HistoryCut::Unrestricted`], because the count queries take
    /// `GREATEST(read marker, floor)` and every real timestamp is above zero — so
    /// the read marker keeps deciding, exactly as before this floor existed.
    pub fn unread_floor_ts(self) -> i64 {
        match self {
            Self::Unrestricted => 0,
            Self::From { origin_server_ts, .. } => origin_server_ts,
        }
    }
}

/// Turn the storage-layer boundary row into a cut, keeping both of its
/// coordinates together.
fn cut_from_boundary(boundary: Option<synapse_storage::event::MembershipBoundary>) -> HistoryCut {
    boundary.map_or(HistoryCut::NOTHING, |boundary| HistoryCut::From {
        stream_ordering: boundary.stream_ordering,
        origin_server_ts: boundary.origin_server_ts,
    })
}

/// The rule, expressed against any [`EventReader`].
///
/// This is **the** implementation: `MessagingService` (`/messages`、`/event`、`/context`)、
/// `SyncService`（`/sync` 时间线）与 `SearchService`（`/search`）都调它，避免同一判定出现
/// 第二份实现 —— 读路径各写一份是本缺口最初能长期存在的原因。
///
/// 一个 `invited`/`joined` 房间多一次边界查询；`shared`/`world_readable`（绝大多数）**零额外查询**。
pub(crate) async fn history_cut_with(
    reader: &dyn synapse_storage::event::EventReader,
    room_id: &str,
    user_id: &str,
) -> Result<HistoryCut, ApiError> {
    let visibility = room_history_visibility(reader, room_id).await?;
    if !visibility.restricts_history() {
        return Ok(HistoryCut::Unrestricted);
    }

    let boundary = reader
        .get_membership_boundary(room_id, user_id, visibility.counts_invites())
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to resolve history visibility boundary", e))?;

    Ok(cut_from_boundary(boundary))
}

/// [`history_cut_with`] for many rooms at once, batched.
///
/// 一次 `/sync` / 一页 `/search` 会覆盖多个房间，逐房间照搬单房间规则要 **2N** 次查询；
/// 这里改成 ① 一次 `get_state_events_by_type_batch` 取全部房间的可见性值，
/// ② 只对 `invited`/`joined` 的房间再做边界查询。`shared` 是房间版本默认值，
/// 故绝大多数调用的第二步**执行 0 次**。
pub(crate) async fn history_cuts_for_rooms(
    reader: &dyn synapse_storage::event::EventReader,
    room_ids: &[String],
    user_id: &str,
) -> Result<HashMap<String, HistoryCut>, ApiError> {
    if room_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let state_by_room = reader
        .get_state_events_by_type_batch(room_ids, HISTORY_VISIBILITY_EVENT_TYPE)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to read room history visibility", e))?;

    let mut cuts: HashMap<String, HistoryCut> = HashMap::with_capacity(room_ids.len());
    let mut restrictive: Vec<(String, bool)> = Vec::new();

    for room_id in room_ids {
        if let Some(cut) = cuts.get(room_id) {
            // 同一房间可能被调用方重复传入（例如某一页里同一房间多条命中）。
            let _ = cut;
            continue;
        }
        let visibility = visibility_from_state_events(state_by_room.get(room_id));
        if visibility.restricts_history() {
            restrictive.push((room_id.clone(), visibility.counts_invites()));
        } else {
            cuts.insert(room_id.clone(), HistoryCut::Unrestricted);
        }
    }

    for (room_id, include_invites) in restrictive {
        let boundary = reader
            .get_membership_boundary(&room_id, user_id, include_invites)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to resolve history visibility boundary", e))?;
        cuts.insert(room_id, cut_from_boundary(boundary));
    }

    Ok(cuts)
}

/// The room's effective history visibility, read from the **current
/// `m.room.history_visibility` state event**.
///
/// 不读 `rooms.history_visibility` 投影：投影只在建房与 summary 更新时写，客户端事后改
/// 可见性（power level 100 的普通状态事件）会被旧值误判。
///
/// No state event ⇒ [`HistoryVisibility::Shared`], the same fail-open default
/// [`HistoryVisibility::parse`] applies to an unreadable value.
async fn room_history_visibility(
    reader: &dyn synapse_storage::event::EventReader,
    room_id: &str,
) -> Result<HistoryVisibility, ApiError> {
    let state_event = reader
        .get_state_event(room_id, HISTORY_VISIBILITY_EVENT_TYPE, "")
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to get room history visibility", e))?;

    Ok(visibility_from_state_event(state_event.as_ref()))
}

/// 从一条 `m.room.history_visibility` 状态事件取策略。见 [`room_history_visibility`] 的默认值说明。
fn visibility_from_state_event(event: Option<&synapse_storage::event::StateEvent>) -> HistoryVisibility {
    event
        .and_then(|event| event.content.get("history_visibility"))
        .and_then(|value| value.as_str())
        .map_or(HistoryVisibility::Shared, HistoryVisibility::parse)
}

/// 从批量查询结果里取策略（每个房间至多一条：该状态事件只有空 state key）。
fn visibility_from_state_events(events: Option<&Vec<synapse_storage::event::StateEvent>>) -> HistoryVisibility {
    visibility_from_state_event(events.and_then(|events| events.first()))
}

impl MessagingService {
    /// What `user_id` may see of `room_id`'s history. See [`history_cut_with`].
    pub(crate) async fn history_cut(&self, room_id: &str, user_id: &str) -> Result<HistoryCut, ApiError> {
        history_cut_with(&*self.event_reader, room_id, user_id).await
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
        let cut = HistoryCut::From { stream_ordering: 100, origin_server_ts: 0 };
        assert!(cut.allows(Some(100)), "the boundary event itself is the caller's own join/invite");
        assert!(cut.allows(Some(101)));
        assert!(!cut.allows(Some(99)));
    }

    #[test]
    fn a_missing_stream_ordering_is_hidden_when_the_cut_restricts() {
        // "unknown position" must not be treated as "old enough to be public".
        assert!(!HistoryCut::From { stream_ordering: 1, origin_server_ts: 0 }.allows(None));
        assert!(!HistoryCut::NOTHING.allows(None));
    }

    #[test]
    fn nothing_hides_every_real_event() {
        assert!(!HistoryCut::NOTHING.allows(Some(0)));
        assert!(!HistoryCut::NOTHING.allows(Some(i64::MAX - 1)));
    }

    /// The **count** axis must be neutral exactly when the cut is, and otherwise
    /// must be the boundary event's own timestamp — never a number derived from
    /// the stream position, which the count queries cannot compare against.
    #[test]
    fn the_unread_floor_is_the_boundary_timestamp_or_zero() {
        assert_eq!(HistoryCut::Unrestricted.unread_floor_ts(), 0, "no policy ⇒ the read marker keeps deciding");
        assert_eq!(
            HistoryCut::From { stream_ordering: 100, origin_server_ts: 1_700_000_000_000 }.unread_floor_ts(),
            1_700_000_000_000,
            "the floor is the boundary event's wall clock, not its stream ordering"
        );
        assert_eq!(
            HistoryCut::NOTHING.unread_floor_ts(),
            i64::MAX,
            "`never entitled` must floor out every real event, so the count is zero"
        );
    }
}
