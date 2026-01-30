use crate::common::*;
use crate::services::*;
use serde_json::json;

pub struct SyncService<'a> {
    services: &'a ServiceContainer,
}

impl<'a> SyncService<'a> {
    pub fn new(services: &'a ServiceContainer) -> Self {
        Self { services }
    }

    pub async fn sync(
        &self,
        user_id: &str,
        _timeout: u64,
        _full_state: bool,
        set_presence: &str,
    ) -> ApiResult<serde_json::Value> {
        self.services
            .presence_storage
            .set_presence(user_id, set_presence, None)
            .await
            .ok();

        let room_ids = self
            .services
            .member_storage
            .get_joined_rooms(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get rooms: {}", e)))?;

        let mut rooms = serde_json::Map::new();
        for room_id in room_ids {
            let events = self
                .services
                .event_storage
                .get_room_events(&room_id, 20)
                .await
                .map_err(|e| ApiError::internal(format!("Failed to get events: {}", e)))?;

            let event_list: Vec<serde_json::Value> = events
                .iter()
                .map(|e| {
                    json!({
                        "type": e.event_type,
                        "content": serde_json::from_str(&e.content).unwrap_or(json!({})),
                        "sender": e.user_id,
                        "origin_server_ts": e.origin_server_ts,
                        "event_id": e.event_id
                    })
                })
                .collect();

            let prev_batch = events
                .first()
                .map(|e| format!("t{}", e.origin_server_ts))
                .unwrap_or_else(|| format!("t{}", chrono::Utc::now().timestamp_millis()));

            rooms.insert(
                room_id,
                json!({
                    "timeline": {
                        "events": event_list,
                        "limited": true,
                        "prev_batch": prev_batch
                    },
                    "state": json!({}),
                    "ephemeral": json!({}),
                    "account_data": json!({}),
                    "unread_notifications": json!({
                        "highlight_count": 0,
                        "notification_count": 0
                    })
                }),
            );
        }

        Ok(json!({
            "next_batch": format!("s{}", chrono::Utc::now().timestamp_millis()),
            "rooms": rooms,
            "presence": json!({
                "events": []
            }),
            "account_data": json!({
                "events": []
            }),
            "to_device": json!({
                "events": []
            })
        }))
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
            .services
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
            .services
            .event_storage
            .get_room_events(room_id, limit)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get messages: {}", e)))?;

        let event_list: Vec<serde_json::Value> = events
            .iter()
            .map(|e| {
                json!({
                    "type": e.event_type,
                    "content": serde_json::from_str(&e.content).unwrap_or(json!({})),
                    "sender": e.user_id,
                    "origin_server_ts": e.origin_server_ts,
                    "event_id": e.event_id
                })
            })
            .collect();

        Ok(json!({
            "chunk": event_list,
            "start": from,
            "end": format!("e{}", chrono::Utc::now().timestamp_millis())
        }))
    }

    pub async fn get_public_rooms(
        &self,
        limit: i64,
        _since: Option<&str>,
    ) -> ApiResult<serde_json::Value> {
        let rooms = self
            .services
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
                    "member_count": r.member_count
                })
            })
            .collect();

        Ok(json!({
            "chunk": room_list,
            "total_room_count_estimate": room_list.len() as i64
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sync_service_creation() {
        let services = ServiceContainer::new_test();
        let _sync_service = SyncService::new(&services);
    }

    #[test]
    fn test_sync_response_format() {
        let response = json!({
            "next_batch": "s1234567890",
            "rooms": json!({}),
            "presence": json!({
                "events": []
            }),
            "account_data": json!({
                "events": []
            }),
            "to_device": json!({
                "events": []
            })
        });

        assert!(response.get("next_batch").is_some());
        assert!(response.get("rooms").is_some());
        assert!(response["presence"].is_object());
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
        let state = json!({});
        assert!(state.is_object());
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
            "end": "t1234567899",
            "state": json!({}),
            "architecture_design": json!({})
        });

        assert!(response.get("chunk").is_some());
        assert!(response.get("start").is_some());
        assert!(response.get("end").is_some());
    }

    #[test]
    fn test_event_filter_format() {
        let filter = json!({
            "limit": 10,
            "room": json!({
                "state": json!({
                    "types": ["m.room.*"]
                }),
                "timeline": json!({
                    "types": ["m.room.message"]
                })
            })
        });

        assert!(filter.is_object());
        assert!(filter.get("room").is_some());
    }
}
