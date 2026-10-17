//! `m.room.history_visibility` enforcement for `/sync`.
//!
//! The rule itself lives in [`crate::room::messaging::history_visibility`]; this
//! module only resolves it **for a whole sync at once** and applies it to the
//! room timelines.
//!
//! Until now `/sync` served every room's full timeline to any member, including
//! everything that happened before they joined — the one read path where the
//! leak needed no client action at all: a newly joined member's very first
//! (initial, `since`-less) sync handed them the pre-join history automatically.

use super::SyncService;
use crate::room::messaging::history_visibility::HistoryCut;
use std::collections::HashMap;
use synapse_common::ApiError;
use synapse_storage::event::RoomEvent;

impl SyncService {
    /// Resolve every room's [`HistoryCut`] for `user_id`.
    ///
    /// Deliberately batched, because a `/sync` spans every joined room while the
    /// per-room rule needs two reads:
    ///
    /// 1. **one** state-event batch for all rooms (`shared` and `world_readable`
    ///    need nothing further — and they are the overwhelming majority, since
    ///    `shared` is the room-version default);
    /// 2. then one boundary read **per remaining (restrictive) room only**, so a
    ///    sync whose rooms are all `shared` issues zero boundary queries.
    ///
    /// The naive per-room version would cost two queries for every room on every
    /// sync.
    pub(crate) async fn history_cuts(
        &self,
        room_ids: &[String],
        user_id: &str,
    ) -> Result<HashMap<String, HistoryCut>, ApiError> {
        // 实现在 `room::messaging::history_visibility::history_cuts_for_rooms`（单一来源，
        // 与 `/messages`、`/search` 共用）；本方法只负责把 `event_reader` 递过去。
        crate::room::messaging::history_visibility::history_cuts_for_rooms(&*self.event_reader, room_ids, user_id).await
    }

    /// Drop the timeline entries a room's policy hides from the caller.
    ///
    /// Kept as a free helper (not a method) so it is trivially testable and so the
    /// `/sync` assembly stays a single call site. Order is preserved; the vector
    /// is only rebuilt when something is actually hidden, which keeps the common
    /// `Unrestricted` case allocation-free.
    pub(crate) fn apply_history_cut(events: Vec<RoomEvent>, cut: HistoryCut) -> Vec<RoomEvent> {
        if cut == HistoryCut::Unrestricted {
            return events;
        }
        events.into_iter().filter(|event| cut.allows(event.stream_ordering)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn event(stream_ordering: Option<i64>) -> RoomEvent {
        RoomEvent {
            event_id: format!("$e{}", stream_ordering.unwrap_or(-1)),
            room_id: "!r:test".to_string(),
            user_id: "@u:test".to_string(),
            event_type: "m.room.message".to_string(),
            content: json!({}),
            state_key: None,
            depth: 1,
            origin_server_ts: 0,
            processed_ts: 0,
            not_before: 0,
            status: None,
            origin: "test".to_string(),
            stream_ordering,
            redacts: None,
            unsigned: None,
        }
    }

    #[test]
    fn unrestricted_keeps_every_event_in_order() {
        let events = vec![event(Some(1)), event(Some(2)), event(None)];
        let kept = SyncService::apply_history_cut(events, HistoryCut::Unrestricted);
        assert_eq!(kept.len(), 3);
        assert_eq!(kept[0].stream_ordering, Some(1));
        assert_eq!(kept[2].stream_ordering, None);
    }

    #[test]
    fn a_restrictive_cut_drops_pre_boundary_events_and_keeps_the_boundary_one() {
        let events = vec![event(Some(1)), event(Some(2)), event(Some(3)), event(Some(4))];
        let kept = SyncService::apply_history_cut(events, HistoryCut::From { stream_ordering: 3, origin_server_ts: 0 });
        let positions: Vec<Option<i64>> = kept.iter().map(|e| e.stream_ordering).collect();
        assert_eq!(positions, vec![Some(3), Some(4)], "the caller's own join event must survive");
    }

    #[test]
    fn a_restrictive_cut_drops_events_without_a_stream_ordering() {
        let events = vec![event(Some(1)), event(None), event(Some(5))];
        let kept = SyncService::apply_history_cut(events, HistoryCut::From { stream_ordering: 5, origin_server_ts: 0 });
        assert_eq!(kept.len(), 1, "an unknown position is not 'old enough to be public'");
        assert_eq!(kept[0].stream_ordering, Some(5));
    }

    #[test]
    fn nothing_hides_the_whole_timeline() {
        let events = vec![event(Some(1)), event(Some(2))];
        assert!(SyncService::apply_history_cut(events, HistoryCut::NOTHING).is_empty());
    }
}
