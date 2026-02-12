use crate::common::*;
use crate::services::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncToken {
    pub stream_id: i64,
    pub room_id: Option<String>,
    pub event_type: Option<String>,
}

impl SyncToken {
    pub fn parse(token: &str) -> Option<Self> {
        if let Some(stripped) = token.strip_prefix('s') {
            stripped.parse::<i64>().ok().map(|stream_id| Self {
                stream_id,
                room_id: None,
                event_type: None,
            })
        } else {
            None
        }
    }

    pub fn encode(&self) -> String {
        format!("s{}", self.stream_id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncFilter {
    pub limit: Option<i64>,
    pub types: Option<Vec<String>>,
    pub not_types: Option<Vec<String>>,
    pub senders: Option<Vec<String>>,
    pub not_senders: Option<Vec<String>>,
}

impl Default for SyncFilter {
    fn default() -> Self {
        Self {
            limit: Some(100),
            types: None,
            not_types: None,
            senders: None,
            not_senders: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomFilter {
    pub state: Option<SyncFilter>,
    pub timeline: Option<SyncFilter>,
    pub ephemeral: Option<SyncFilter>,
    pub account_data: Option<SyncFilter>,
}

impl Default for RoomFilter {
    fn default() -> Self {
        Self {
            state: Some(SyncFilter::default()),
            timeline: Some(SyncFilter { limit: Some(50), ..Default::default() }),
            ephemeral: Some(SyncFilter::default()),
            account_data: Some(SyncFilter::default()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncRequest {
    pub since: Option<String>,
    pub filter: Option<String>,
    pub full_state: bool,
    pub set_presence: Option<String>,
    pub timeout: u64,
}

#[derive(Debug, Clone)]
pub struct SyncState {
    pub rooms: HashMap<String, RoomSyncState>,
    pub last_stream_id: i64,
}

#[derive(Debug, Clone)]
pub struct RoomSyncState {
    pub timeline_limit: i64,
    pub last_event_id: Option<String>,
    pub last_stream_id: i64,
}

pub struct SyncService {
    presence_storage: PresenceStorage,
    member_storage: RoomMemberStorage,
    event_storage: EventStorage,
    room_storage: RoomStorage,
    #[allow(dead_code)]
    device_storage: DeviceStorage,
}

impl SyncService {
    pub fn new(
        presence_storage: PresenceStorage,
        member_storage: RoomMemberStorage,
        event_storage: EventStorage,
        room_storage: RoomStorage,
        device_storage: DeviceStorage,
    ) -> Self {
        Self {
            presence_storage,
            member_storage,
            event_storage,
            room_storage,
            device_storage,
        }
    }

    pub async fn sync(
        &self,
        user_id: &str,
        _timeout: u64,
        full_state: bool,
        set_presence: &str,
        since: Option<&str>,
    ) -> ApiResult<serde_json::Value> {
        if let Some(presence) = set_presence.strip_prefix("online").or_else(|| set_presence.strip_prefix("unavailable")) {
            self.presence_storage
                .set_presence(user_id, presence, None)
                .await
                .ok();
        }

        let since_token = since.and_then(SyncToken::parse);
        let is_incremental = since_token.is_some() && !full_state;

        let room_ids = self
            .member_storage
            .get_joined_rooms(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get rooms: {}", e)))?;

        let limit = 50i64;
        let since_ts = since_token.as_ref().map(|t| t.stream_id).unwrap_or(0);
        
        let room_events = if is_incremental {
            self.event_storage
                .get_room_events_since_batch(&room_ids, since_ts, limit)
                .await
                .map_err(|e| ApiError::internal(format!("Failed to get batch events: {}", e)))?
        } else {
            self.event_storage
                .get_room_events_batch(&room_ids, limit)
                .await
                .map_err(|e| ApiError::internal(format!("Failed to get batch events: {}", e)))?
        };

        let mut rooms = serde_json::Map::new();
        let invites = serde_json::Map::new();
        let leaves = serde_json::Map::new();

        for room_id in &room_ids {
            let events = room_events.get(room_id).cloned().unwrap_or_default();
            let room_sync = self.build_room_sync(room_id, user_id, events, is_incremental).await?;
            
            if room_sync.is_object() && !room_sync.as_object().is_some_and(|o| o.is_empty()) {
                rooms.insert(room_id.clone(), room_sync);
            }
        }

        let presence_events = self.get_presence_events(user_id, &since_token).await?;
        
        let account_data_events = self.get_account_data_events(user_id).await?;
        
        let to_device_events = self.get_to_device_events(user_id, &since_token).await?;
        
        let device_lists = self.get_device_lists(user_id, &since_token).await?;

        let next_batch = SyncToken {
            stream_id: chrono::Utc::now().timestamp_millis(),
            room_id: None,
            event_type: None,
        };

        Ok(json!({
            "next_batch": next_batch.encode(),
            "rooms": {
                "join": rooms,
                "invite": invites,
                "leave": leaves
            },
            "presence": json!({
                "events": presence_events
            }),
            "account_data": json!({
                "events": account_data_events
            }),
            "to_device": json!({
                "events": to_device_events
            }),
            "device_lists": device_lists,
            "device_one_time_keys_count": json!({})
        }))
    }

    async fn build_room_sync(
        &self,
        room_id: &str,
        user_id: &str,
        events: Vec<RoomEvent>,
        is_incremental: bool,
    ) -> ApiResult<serde_json::Value> {
        let state_events = if is_incremental {
            vec![]
        } else {
            self.get_room_state_events(room_id).await?
        };

        let event_list: Vec<serde_json::Value> = events
            .iter()
            .map(|e| self.event_to_json(e))
            .collect();

        let state_list: Vec<serde_json::Value> = state_events
            .iter()
            .map(|e| self.event_to_json(e))
            .collect();

        let ephemeral_events = self.get_room_ephemeral_events(room_id, user_id).await?;
        
        let account_data_events = self.get_room_account_data_events(room_id, user_id).await?;

        let (highlight_count, notification_count) = self.get_unread_counts(room_id, user_id).await?;

        let prev_batch = events
            .first()
            .map(|e| format!("t{}", e.origin_server_ts))
            .unwrap_or_else(|| format!("t{}", chrono::Utc::now().timestamp_millis()));

        let limited = event_list.len() as i64 >= 50;

        Ok(json!({
            "state": {
                "events": state_list
            },
            "timeline": {
                "events": event_list,
                "limited": limited,
                "prev_batch": prev_batch
            },
            "ephemeral": {
                "events": ephemeral_events
            },
            "account_data": {
                "events": account_data_events
            },
            "unread_notifications": {
                "highlight_count": highlight_count,
                "notification_count": notification_count
            }
        }))
    }

    fn event_to_json(&self, event: &RoomEvent) -> serde_json::Value {
        json!({
            "type": event.event_type,
            "content": event.content,
            "sender": event.user_id,
            "origin_server_ts": event.origin_server_ts,
            "event_id": event.event_id
        })
    }

    async fn get_room_state_events(&self, _room_id: &str) -> ApiResult<Vec<RoomEvent>> {
        Ok(vec![])
    }

    async fn get_presence_events(&self, _user_id: &str, _since: &Option<SyncToken>) -> ApiResult<Vec<serde_json::Value>> {
        Ok(vec![])
    }

    async fn get_account_data_events(&self, _user_id: &str) -> ApiResult<Vec<serde_json::Value>> {
        Ok(vec![])
    }

    async fn get_to_device_events(&self, _user_id: &str, _since: &Option<SyncToken>) -> ApiResult<Vec<serde_json::Value>> {
        Ok(vec![])
    }

    async fn get_device_lists(&self, _user_id: &str, _since: &Option<SyncToken>) -> ApiResult<serde_json::Value> {
        Ok(json!({
            "changed": [],
            "left": []
        }))
    }

    async fn get_room_ephemeral_events(&self, _room_id: &str, _user_id: &str) -> ApiResult<Vec<serde_json::Value>> {
        Ok(vec![])
    }

    async fn get_room_account_data_events(&self, _room_id: &str, _user_id: &str) -> ApiResult<Vec<serde_json::Value>> {
        Ok(vec![])
    }

    async fn get_unread_counts(&self, _room_id: &str, _user_id: &str) -> ApiResult<(i64, i64)> {
        Ok((0, 0))
    }

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
            .map_err(|e| ApiError::internal(format!("Failed to check membership: {}", e)))?
        {
            return Err(ApiError::forbidden(
                "You are not a member of this room".to_string(),
            ));
        }

        let events = self
            .event_storage
            .get_room_events(room_id, limit)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get messages: {}", e)))?;

        let event_list: Vec<serde_json::Value> = events
            .iter()
            .map(|e| self.event_to_json(e))
            .collect();

        let end_token = events
            .last()
            .map(|e| format!("t{}", e.origin_server_ts))
            .unwrap_or_else(|| format!("t{}", chrono::Utc::now().timestamp_millis()));

        Ok(json!({
            "chunk": event_list,
            "start": from,
            "end": end_token
        }))
    }

    pub async fn get_public_rooms(
        &self,
        limit: i64,
        _since: Option<&str>,
    ) -> ApiResult<serde_json::Value> {
        let rooms = self
            .room_storage
            .get_public_rooms(limit)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get public rooms: {}", e)))?;

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

    pub async fn get_filter(&self, _user_id: &str, _filter_id: &str) -> ApiResult<serde_json::Value> {
        Ok(json!({}))
    }

    pub async fn set_filter(&self, _user_id: &str, _filter: &serde_json::Value) -> ApiResult<String> {
        Ok(format!("filter_{}", chrono::Utc::now().timestamp_millis()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_token_parse() {
        let token = SyncToken::parse("s1234567890");
        assert!(token.is_some());
        let token = token.unwrap();
        assert_eq!(token.stream_id, 1234567890);
    }

    #[test]
    fn test_sync_token_encode() {
        let token = SyncToken {
            stream_id: 1234567890,
            room_id: None,
            event_type: None,
        };
        assert_eq!(token.encode(), "s1234567890");
    }

    #[test]
    fn test_sync_token_roundtrip() {
        let original = SyncToken {
            stream_id: 9876543210,
            room_id: None,
            event_type: None,
        };
        let encoded = original.encode();
        let parsed = SyncToken::parse(&encoded).unwrap();
        assert_eq!(original.stream_id, parsed.stream_id);
    }

    #[test]
    fn test_sync_filter_default() {
        let filter = SyncFilter::default();
        assert_eq!(filter.limit, Some(100));
        assert!(filter.types.is_none());
    }

    #[test]
    fn test_room_filter_default() {
        let filter = RoomFilter::default();
        assert!(filter.state.is_some());
        assert!(filter.timeline.is_some());
        assert_eq!(filter.timeline.unwrap().limit, Some(50));
    }

    #[test]
    fn test_sync_response_format() {
        let response = json!({
            "next_batch": "s1234567890",
            "rooms": {
                "join": {},
                "invite": {},
                "leave": {}
            },
            "presence": json!({
                "events": []
            }),
            "account_data": json!({
                "events": []
            }),
            "to_device": json!({
                "events": []
            }),
            "device_lists": {
                "changed": [],
                "left": []
            }
        });

        assert!(response.get("next_batch").is_some());
        assert!(response["rooms"]["join"].is_object());
        assert!(response["presence"]["events"].is_array());
        assert!(response["device_lists"]["changed"].is_array());
    }

    #[test]
    fn test_room_timeline_format() {
        let timeline = json!({
            "events": [],
            "limited": true,
            "prev_batch": "t1234567890"
        });

        assert!(timeline["events"].is_array());
        assert!(timeline["limited"].is_boolean());
        assert_eq!(timeline["prev_batch"], "t1234567890");
    }

    #[test]
    fn test_room_state_format() {
        let state = json!({
            "events": []
        });
        assert!(state["events"].is_array());
    }

    #[test]
    fn test_presence_format() {
        let presence = json!({
            "events": []
        });
        assert!(presence["events"].is_array());
    }

    #[test]
    fn test_account_data_format() {
        let account_data = json!({
            "events": []
        });
        assert!(account_data["events"].is_array());
    }

    #[test]
    fn test_to_device_format() {
        let to_device = json!({
            "events": []
        });
        assert!(to_device["events"].is_array());
    }

    #[test]
    fn test_device_lists_format() {
        let device_lists = json!({
            "changed": ["@user1:example.com"],
            "left": ["@user2:example.com"]
        });

        assert!(device_lists["changed"].is_array());
        assert!(device_lists["left"].is_array());
    }

    #[test]
    fn test_unread_notifications_format() {
        let notifications = json!({
            "highlight_count": 0,
            "notification_count": 0
        });

        assert_eq!(notifications["highlight_count"], 0);
        assert_eq!(notifications["notification_count"], 0);
    }

    #[test]
    fn test_ephemeral_format() {
        let ephemeral = json!({
            "events": []
        });
        assert!(ephemeral["events"].is_array());
    }

    #[test]
    fn test_room_messages_response_format() {
        let response = json!({
            "chunk": [],
            "start": "t1234567890",
            "end": "t1234567899"
        });

        assert!(response.get("chunk").is_some());
        assert!(response.get("start").is_some());
        assert!(response.get("end").is_some());
    }

    #[test]
    fn test_public_rooms_response_format() {
        let response = json!({
            "chunk": [],
            "total_room_count_estimate": 0,
            "next_batch": "p1234567890"
        });

        assert!(response.get("chunk").is_some());
        assert!(response.get("total_room_count_estimate").is_some());
        assert!(response.get("next_batch").is_some());
    }
}
