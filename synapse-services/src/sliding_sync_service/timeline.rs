use serde_json::Value;

use super::SlidingSyncService;
use crate::sync_helpers;

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
        let timeline = events.iter().map(sync_helpers::room_event_to_json).collect();
        Ok((timeline, limited, prev_batch))
    }
}
