use super::types::*;
use super::SyncService;
use crate::common::*;
use crate::map_internal;

use serde_json::json;

impl SyncService {
    pub async fn get_room_messages(
        &self,
        room_id: &str,
        user_id: &str,
        from: &str,
        limit: i64,
        _dir: &str,
    ) -> ApiResult<serde_json::Value> {
        if !self
            .member_storage
            .is_member(room_id, user_id)
            .await
            .map_err(map_internal!("Failed to check membership"))?
        {
            return Err(ApiError::forbidden("You are not a member of this room".to_string()));
        }

        let events = self
            .event_storage
            .get_room_events(room_id, limit)
            .await
            .map_err(map_internal!("Failed to get messages"))?;

        let event_list: Vec<serde_json::Value> =
            events.iter().map(|e| Self::event_to_json(e, SyncEventFormat::Client)).collect();

        let end_token = events.last().map_or_else(
            || format!("t{}", chrono::Utc::now().timestamp_millis()),
            |e| format!("t{}", e.origin_server_ts),
        );

        Ok(json!({
            "chunk": event_list,
            "start": from,
            "end": end_token
        }))
    }

    pub async fn get_public_rooms(&self, limit: i64, _since: Option<&str>) -> ApiResult<serde_json::Value> {
        let rooms =
            self.room_storage.get_public_rooms(limit).await.map_err(map_internal!("Failed to get public rooms"))?;

        let room_list: Vec<serde_json::Value> = rooms
            .iter()
            .map(|r| {
                json!({
                    "room_id": r.room_id,
                    "name": r.name,
                    "topic": r.topic,
                    "canonical_alias": r.canonical_alias,
                    "is_public": r.is_public,
                    "join_rule": r.join_rule
                })
            })
            .collect();

        let next_batch = if room_list.len() as i64 >= limit {
            Some(format!("p{}", chrono::Utc::now().timestamp_millis()))
        } else {
            None
        };

        let mut response = json!({
            "chunk": room_list,
            "total_room_count_estimate": room_list.len() as i64
        });

        if let Some(batch) = next_batch {
            response["next_batch"] = json!(batch);
        }

        Ok(response)
    }

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
            .event_storage
            .get_room_events_since_batch(&room_ids, since_ts, limit)
            .await
            .map_err(map_internal!("Failed to get events"))?;

        let mut chunk = vec![];
        for room_events in events.values() {
            for event in room_events {
                chunk.push(Self::event_to_json(event, SyncEventFormat::Client));
            }
        }

        let end_token = format!("s{}", chrono::Utc::now().timestamp_millis());

        Ok(json!({
            "start": from,
            "end": end_token,
            "chunk": chunk
        }))
    }
}
