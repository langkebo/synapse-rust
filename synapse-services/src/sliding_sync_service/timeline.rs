use serde_json::Value;
use synapse_common::generate_pagination_token;
use synapse_storage::event::RoomEvent;

use super::SlidingSyncService;
use crate::sync_helpers;

impl SlidingSyncService {
    /// S14/SS-10: `since_stream` 为上一轮同步记录的事件流水水位线。
    /// - 初始同步（None）：保持原语义，取最新 N 条；
    /// - 增量同步（Some(wm)）：只取 `stream_ordering > wm` 的事件，
    ///   不再把客户端已收事件重复下发。
    pub(super) async fn build_timeline(
        &self,
        room_id: &str,
        timeline_limit: Option<u32>,
        since_stream: Option<i64>,
    ) -> Result<(Vec<Value>, bool, Option<String>), sqlx::Error> {
        let Some(limit) = timeline_limit.filter(|limit| *limit > 0) else {
            return Ok((Vec::new(), false, None));
        };

        match since_stream {
            Some(watermark) => {
                // 多取一条用于判断是否还有更早的未送达事件（limited 语义）；
                // 查询按 stream_ordering 升序返回水位线之后的最新事件。
                let mut fetched = self
                    .event_reader
                    .get_room_events_after_stream_ordering(room_id, watermark, i64::from(limit) + 1)
                    .await?;
                let limited = fetched.len() > limit as usize;
                if limited {
                    // 存在缺口：丢弃最旧的一条，客户端可凭 prev_batch 回翻。
                    fetched.remove(0);
                }
                Ok(Self::timeline_from_events(fetched, limited))
            }
            None => {
                let mut fetched =
                    self.event_reader.get_room_events_paginated(room_id, None, i64::from(limit) + 1, "b").await?;
                let limited = fetched.len() > limit as usize;
                if limited {
                    fetched.truncate(limit as usize);
                }
                fetched.reverse();
                Ok(Self::timeline_from_events(fetched, limited))
            }
        }
    }

    fn timeline_from_events(events: Vec<RoomEvent>, limited: bool) -> (Vec<Value>, bool, Option<String>) {
        let prev_batch =
            events.first().map(|event| generate_pagination_token(event.origin_server_ts, event.stream_ordering));
        let timeline = events.iter().map(sync_helpers::room_event_to_json).collect();
        (timeline, limited, prev_batch)
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
            reference_image: None,
            origin: "ex.com".into(),
            stream_ordering,
            redacts: None,
        }
    }

    #[test]
    fn timeline_prev_batch_uses_composite_token() {
        // ISSUE 2.1.1: sliding sync timeline 的 prev_batch 也应为复合 token。
        let events = vec![make_room_event(Some(100))];
        let (_timeline, _limited, prev_batch) = SlidingSyncService::timeline_from_events(events, false);
        assert_eq!(prev_batch.as_deref(), Some("t1700000000000_100"));
    }

    #[test]
    fn timeline_prev_batch_falls_back_to_legacy_when_no_stream() {
        let events = vec![make_room_event(None)];
        let (_timeline, _limited, prev_batch) = SlidingSyncService::timeline_from_events(events, false);
        assert_eq!(prev_batch.as_deref(), Some("t1700000000000"));
    }
}
