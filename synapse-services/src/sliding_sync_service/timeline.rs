use serde_json::Value;
use synapse_common::generate_pagination_token;
use synapse_storage::event::RoomEvent;

use super::SlidingSyncService;
use crate::sync_helpers::{self, TimelineOrder};

impl SlidingSyncService {
    /// S14/SS-10: `since_stream` 为上一轮同步记录的事件流水水位线。
    /// - 初始同步（None）：保持原语义，取最新 N 条；
    /// - 增量同步（Some(wm)）：只取 `stream_ordering > wm` 的事件，
    ///   不再把客户端已收事件重复下发。
    ///
    /// 「取哪 N 条 / 输出顺序 / `limited`」全部交给 [`sync_helpers::timeline_window`] ——
    /// 与 `/sync` 用的是同一个函数（ARCH-02 的共享语义基座）。这里只负责两件本路径特有的事：
    /// **查多少条**（多取 1 条用于判断 `limited`）和**入参顺序**（两条查询方向不同）。
    pub(super) async fn build_timeline(
        &self,
        room_id: &str,
        timeline_limit: Option<u32>,
        since_stream: Option<i64>,
    ) -> Result<(Vec<Value>, bool, Option<String>), sqlx::Error> {
        let Some(limit) = timeline_limit.filter(|limit| *limit > 0) else {
            return Ok((Vec::new(), false, None));
        };
        let limit = limit as usize;

        let (fetched, incoming) = match since_stream {
            Some(watermark) => (
                // 多取一条用于判断是否还有更早的未送达事件（limited 语义）。
                self.event_reader.get_room_events_after_stream_ordering(room_id, watermark, limit as i64 + 1).await?,
                // 该查询已内部翻转为升序。
                TimelineOrder::OldestFirst,
            ),
            None => (
                self.event_reader.get_room_events_paginated(room_id, None, limit as i64 + 1, "b").await?,
                // `direction=b` ⇒ 最新在前。
                TimelineOrder::NewestFirst,
            ),
        };

        Ok(Self::timeline_from_events(fetched, limit, incoming))
    }

    fn timeline_from_events(
        events: Vec<RoomEvent>,
        limit: usize,
        incoming: TimelineOrder,
    ) -> (Vec<Value>, bool, Option<String>) {
        let window = sync_helpers::timeline_window(events, limit, incoming);
        let prev_batch =
            window.events.first().map(|event| generate_pagination_token(event.origin_server_ts, event.stream_ordering));
        let timeline = window.events.iter().map(sync_helpers::room_event_to_json).collect();
        (timeline, window.limited, prev_batch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_room_event(stream_ordering: Option<i64>) -> RoomEvent {
        RoomEvent {
            event_id: "$ev1:ex.com".into(),
            room_id: "!room:ex.com".into(),
            user_id: "@alice:ex.com".into(),
            event_type: "m.room.message".into(),
            content: serde_json::json!({"body": "hello"}),
            state_key: None,
            depth: 5,
            origin_server_ts: 1700000000000,
            processed_ts: 1700000001000,
            not_before: 0,
            status: None,
            origin: "ex.com".into(),
            stream_ordering,
            redacts: None,
            unsigned: None,
        }
    }

    #[test]
    fn timeline_prev_batch_uses_composite_token() {
        // ISSUE 2.1.1: sliding sync timeline 的 prev_batch 也应为复合 token。
        let events = vec![make_room_event(Some(100))];
        let (_timeline, _limited, prev_batch) =
            SlidingSyncService::timeline_from_events(events, 10, TimelineOrder::OldestFirst);
        assert_eq!(prev_batch.as_deref(), Some("t1700000000000_100"));
    }

    #[test]
    fn timeline_prev_batch_falls_back_to_legacy_when_no_stream() {
        let events = vec![make_room_event(None)];
        let (_timeline, _limited, prev_batch) =
            SlidingSyncService::timeline_from_events(events, 10, TimelineOrder::OldestFirst);
        assert_eq!(prev_batch.as_deref(), Some("t1700000000000"));
    }

    /// 裁剪语义现在只有一份实现（`sync_helpers::timeline_window`），这里锁住两条
    /// 路径的**入参顺序差异**：新在前 ⇒ 保留最新 N 条并翻成升序；已升序 ⇒ 丢最旧的。
    #[test]
    fn timeline_window_truncates_newest_and_returns_ascending() {
        let newest_first = vec![make_room_event(Some(5)), make_room_event(Some(4)), make_room_event(Some(3))];
        let (_timeline, limited, prev_batch) =
            SlidingSyncService::timeline_from_events(newest_first, 2, TimelineOrder::NewestFirst);
        assert!(limited);
        // 最新 2 条的升序 ⇒ 起点是 stream_ordering=4。
        assert_eq!(prev_batch.as_deref(), Some("t1700000000000_4"));

        let oldest_first = vec![make_room_event(Some(3)), make_room_event(Some(4)), make_room_event(Some(5))];
        let (_timeline, limited, prev_batch) =
            SlidingSyncService::timeline_from_events(oldest_first, 2, TimelineOrder::OldestFirst);
        assert!(limited);
        assert_eq!(prev_batch.as_deref(), Some("t1700000000000_4"));
    }
}
