use super::types::*;
use super::SyncService;
use crate::map_internal;
use synapse_common::current_timestamp_millis;
use synapse_common::*;
use synapse_storage::event::SinceFilter;

use serde_json::json;

impl SyncService {
    pub async fn get_events(&self, user_id: &str, from: &str, _timeout: u64) -> ApiResult<serde_json::Value> {
        let room_ids =
            self.member_storage.get_joined_rooms(user_id).await.map_err(map_internal!("Failed to get rooms"))?;

        let since_ts: i64 = from
            .trim_start_matches('s')
            .trim_start_matches('t')
            .parse()
            .map_err(|_| ApiError::invalid_input("Invalid 'from' token".to_string()))?;

        let limit = 100i64;
        let events = self
            .event_reader
            .get_room_events_batch_since(&room_ids, SinceFilter::OriginServerTs(since_ts), limit)
            .await
            .map_err(map_internal!("Failed to get events"))?;

        let mut chunk = vec![];
        for room_events in events.values() {
            for event in room_events {
                chunk.push(Self::event_to_json(event, SyncEventFormat::Client));
            }
        }

        // 时间戳 token 统一用 `t` 前缀（与 /messages 的 legacy token 一致）。
        // `s` 前缀保留给 SyncToken 的 stream_id 语义，避免两者冲突（审查 2.1.2）。
        let end_token = format!("t{}", current_timestamp_millis());

        Ok(json!({
            "start": from,
            "end": end_token,
            "chunk": chunk
        }))
    }
}
