use serde_json::{json, Value};
use synapse_storage::RoomEvent;

use super::SlidingSyncService;

impl SlidingSyncService {
    pub(super) async fn build_timeline(
        &self,
        room_id: &str,
        timeline_limit: Option<u32>,
    ) -> Result<(Vec<Value>, bool, Option<String>), sqlx::Error> {
        let Some(limit) = timeline_limit.filter(|limit| *limit > 0) else {
            return Ok((Vec::new(), false, None));
        };

        let mut events = self.event_storage.get_room_events_paginated(room_id, None, i64::from(limit) + 1, "b").await?;
        let limited = events.len() > limit as usize;
        if limited {
            events.truncate(limit as usize);
        }
        events.reverse();

        let prev_batch = events.first().map(|event| format!("t{}", event.origin_server_ts));
        let timeline = events.iter().map(Self::room_event_to_json).collect();
        Ok((timeline, limited, prev_batch))
    }

    fn room_event_to_json(event: &RoomEvent) -> Value {
        let now = chrono::Utc::now().timestamp_millis();
        let age = now.saturating_sub(event.origin_server_ts);
        let mut obj = json!({
            "type": event.event_type,
            "content": event.content,
            "sender": event.user_id,
            "origin_server_ts": event.origin_server_ts,
            "event_id": event.event_id,
            "room_id": event.room_id,
            "unsigned": {
                "age": age
            }
        });
        if let Some(state_key) = &event.state_key {
            obj["state_key"] = json!(state_key);
        }
        obj
    }
}
