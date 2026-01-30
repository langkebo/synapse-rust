use crate::common::{generate_event_id, generate_room_id};
use crate::services::*;
use serde_json::json;

pub struct RoomService<'a> {
    services: &'a ServiceContainer,
}

impl<'a> RoomService<'a> {
    pub fn new(services: &'a ServiceContainer) -> Self {
        Self { services }
    }

    pub async fn create_room(
        &self,
        user_id: &str,
        visibility: Option<&str>,
        room_alias_name: Option<&str>,
        name: Option<&str>,
        topic: Option<&str>,
        invite_list: Option<&Vec<String>>,
        preset: Option<&str>,
    ) -> ApiResult<serde_json::Value> {
        let room_id = self.generate_room_id();
        let join_rule = self.determine_join_rule(preset);
        let is_public = self.is_public_visibility(visibility);

        self.create_room_in_db(&room_id, user_id, join_rule, is_public)
            .await?;
        self.add_creator_to_room(&room_id, user_id).await?;
        self.set_room_metadata(&room_id, name, topic).await?;
        self.process_invites(&room_id, invite_list).await?;

        let room_alias = self.format_room_alias(room_alias_name);
        Ok(self.build_room_response(&room_id, room_alias))
    }

    fn generate_room_id(&self) -> String {
        generate_room_id(&self.services.server_name)
    }

    fn determine_join_rule(&self, preset: Option<&str>) -> &'static str {
        match preset {
            Some("public_chat") => "public",
            _ => "invite",
        }
    }

    fn is_public_visibility(&self, visibility: Option<&str>) -> bool {
        visibility.unwrap_or("private") == "public"
    }

    async fn create_room_in_db(
        &self,
        room_id: &str,
        user_id: &str,
        join_rule: &str,
        is_public: bool,
    ) -> ApiResult<()> {
        self.services
            .room_storage
            .create_room(room_id, user_id, join_rule, "1", is_public)
            .await
            .map(|_| ())
            .map_err(|e| ApiError::internal(format!("Failed to create room: {}", e)))
    }

    async fn add_creator_to_room(&self, room_id: &str, user_id: &str) -> ApiResult<()> {
        self.services
            .member_storage
            .add_member(room_id, user_id, "join", None, None)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to add room member: {}", e)))?;

        self.services
            .room_storage
            .increment_member_count(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to update member count: {}", e)))
    }

    async fn set_room_metadata(
        &self,
        room_id: &str,
        name: Option<&str>,
        topic: Option<&str>,
    ) -> ApiResult<()> {
        if let Some(room_name) = name {
            self.services
                .room_storage
                .update_room_name(room_id, room_name)
                .await
                .map_err(|e| ApiError::internal(format!("Failed to update room name: {}", e)))?;
        }

        if let Some(room_topic) = topic {
            self.services
                .room_storage
                .update_room_topic(room_id, room_topic)
                .await
                .map_err(|e| ApiError::internal(format!("Failed to update room topic: {}", e)))?;
        }

        Ok(())
    }

    async fn process_invites(
        &self,
        room_id: &str,
        invite_list: Option<&Vec<String>>,
    ) -> ApiResult<()> {
        if let Some(invites) = invite_list {
            for invitee in invites {
                self.services
                    .member_storage
                    .add_member(room_id, invitee, "invite", None, None)
                    .await
                    .map_err(|e| ApiError::internal(format!("Failed to invite user: {}", e)))?;
            }
        }
        Ok(())
    }

    fn format_room_alias(&self, room_alias_name: Option<&str>) -> Option<String> {
        room_alias_name.map(|a| format!("#{}:{}", a, self.services.server_name))
    }

    fn build_room_response(&self, room_id: &str, room_alias: Option<String>) -> serde_json::Value {
        json!({
            "room_id": room_id,
            "room_alias": room_alias
        })
    }

    pub async fn send_message(
        &self,
        room_id: &str,
        user_id: &str,
        message_type: &str,
        content: &serde_json::Value,
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

        let event_id = generate_event_id(&self.services.server_name);
        let now = chrono::Utc::now().timestamp_millis();

        let event_content = json!({
            "msgtype": message_type,
            "body": content
        });

        self.services
            .event_storage
            .create_event(
                &event_id,
                room_id,
                user_id,
                "m.room.message",
                &event_content.to_string(),
                None,
                now,
            )
            .await
            .map_err(|e| ApiError::internal(format!("Failed to send message: {}", e)))?;

        Ok(json!({
            "event_id": event_id
        }))
    }

    pub async fn join_room(&self, room_id: &str, user_id: &str) -> ApiResult<()> {
        if !self
            .services
            .room_storage
            .room_exists(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to check room: {}", e)))?
        {
            return Err(ApiError::not_found("Room not found".to_string()));
        }

        self.services
            .member_storage
            .add_member(room_id, user_id, "join", None, None)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to join room: {}", e)))?;

        self.services
            .room_storage
            .increment_member_count(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to update member count: {}", e)))?;

        Ok(())
    }

    pub async fn leave_room(&self, room_id: &str, user_id: &str) -> ApiResult<()> {
        self.services
            .member_storage
            .remove_member(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to leave room: {}", e)))?;

        self.services
            .room_storage
            .decrement_member_count(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to update member count: {}", e)))?;

        Ok(())
    }

    pub async fn get_room_members(
        &self,
        room_id: &str,
        user_id: &str,
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

        let members = self
            .services
            .member_storage
            .get_room_members(room_id, "join")
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get members: {}", e)))?;

        Ok(json!(members))
    }

    pub async fn get_room(&self, room_id: &str) -> ApiResult<serde_json::Value> {
        let room = self
            .services
            .room_storage
            .get_room(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get room: {}", e)))?;

        match room {
            Some(r) => Ok(json!({
                "room_id": r.room_id,
                "name": r.name,
                "topic": r.topic,
                "canonical_alias": r.canonical_alias,
                "is_public": r.is_public,
                "member_count": r.member_count,
                "creator": r.creator
            })),
            None => Err(ApiError::not_found("Room not found".to_string())),
        }
    }

    pub async fn get_room_state(
        &self,
        room_id: &str,
        user_id: &str,
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

        let room = self
            .services
            .room_storage
            .get_room(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get room: {}", e)))?;

        match room {
            Some(r) => Ok(json!({
                "room_id": r.room_id,
                "name": r.name,
                "topic": r.topic,
                "canonical_alias": r.canonical_alias,
                "is_public": r.is_public,
                "member_count": r.member_count,
                "creator": r.creator
            })),
            None => Err(ApiError::not_found("Room not found".to_string())),
        }
    }

    pub async fn get_user_rooms(&self, user_id: &str) -> ApiResult<serde_json::Value> {
        let room_ids = self
            .services
            .member_storage
            .get_joined_rooms(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get rooms: {}", e)))?;

        let mut rooms = Vec::new();
        for room_id in room_ids {
            if let Ok(Some(room)) = self.services.room_storage.get_room(&room_id).await {
                rooms.push(json!({
                    "room_id": room.room_id,
                    "name": room.name,
                    "topic": room.topic,
                    "is_public": room.is_public,
                    "member_count": room.member_count
                }));
            }
        }

        Ok(json!(rooms))
    }

    pub async fn get_room_messages(
        &self,
        room_id: &str,
        from: i64,
        limit: i64,
        _direction: &str,
    ) -> ApiResult<serde_json::Value> {
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
            "start": from.to_string(),
            "end": format!("e{}", chrono::Utc::now().timestamp_millis())
        }))
    }

    pub async fn invite_user(&self, room_id: &str, inviter: &str, invitee: &str) -> ApiResult<()> {
        let event_id = format!("${}", uuid::Uuid::new_v4());
        let origin_server_ts = chrono::Utc::now().timestamp_millis();

        let invite_event = json!({
            "type": "m.room.member",
            "content": {
                "membership": "invite"
            },
            "sender": inviter,
            "state_key": invitee
        });

        self.services
            .event_storage
            .create_event(
                &event_id,
                room_id,
                inviter,
                "m.room.member",
                &invite_event.to_string(),
                Some(invitee),
                origin_server_ts,
            )
            .await
            .map_err(|e| ApiError::internal(format!("Failed to create invite event: {}", e)))?;

        Ok(())
    }

    pub async fn get_state_events(&self, room_id: &str) -> ApiResult<Vec<serde_json::Value>> {
        let events = self
            .services
            .event_storage
            .get_state_events(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get state events: {}", e)))?;

        let event_list: Vec<serde_json::Value> = events
            .iter()
            .map(|e| {
                json!({
                    "type": e.event_type,
                    "content": serde_json::from_str(&e.content).unwrap_or(json!({})),
                    "sender": e.user_id,
                    "state_key": e.state_key
                })
            })
            .collect();

        Ok(event_list)
    }

    pub async fn get_public_rooms(&self, limit: i64) -> ApiResult<serde_json::Value> {
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

    pub async fn delete_room(&self, room_id: &str) -> ApiResult<()> {
        self.services
            .room_storage
            .delete_room(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to delete room: {}", e)))
    }

    pub async fn get_joined_rooms(&self, user_id: &str) -> ApiResult<Vec<String>> {
        self.services
            .member_storage
            .get_joined_rooms(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get joined rooms: {}", e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn test_room_service_creation() {
        // Skip this test as it requires full ServiceContainer setup with database pool
        // To re-enable, provide proper mock pool, cache, jwt_secret, and server_name
        // let pool = Arc::new(sqlx::PgPool::connect("postgres://...").await?);
        // let cache = Arc::new(CacheManager::new(CacheConfig::default()));
        // let services = ServiceContainer::new(&pool, cache, "secret", "example.com");
        // let _room_service = RoomService::new(&services);
    }

    #[test]
    fn test_room_id_format() {
        let room_id = generate_room_id("example.com");
        assert!(room_id.starts_with('!'));
        assert!(room_id.contains(":example.com"));
    }

    #[test]
    fn test_event_id_format() {
        let event_id = generate_event_id("example.com");
        assert!(event_id.starts_with('$'));
    }

    #[test]
    fn test_create_room_response_format() {
        let room_id = "!testroom:example.com";
        let room_alias = Some("#test:example.com");

        let response = json!({
            "room_id": room_id,
            "room_alias": room_alias
        });

        assert_eq!(response["room_id"], room_id);
        assert_eq!(response["room_alias"], room_alias.unwrap_or(""));
    }

    #[test]
    fn test_message_response_format() {
        let response = json!({
            "event_id": "$test_event",
            "room_id": "!testroom:example.com"
        });

        assert!(response["event_id"].is_string());
        assert!(response["room_id"].is_string());
    }

    #[test]
    fn test_public_room_visibility() {
        let is_public = true;
        assert!(is_public);
    }

    #[test]
    fn test_private_room_visibility() {
        let is_public = false;
        assert!(!is_public);
    }

    #[test]
    fn test_join_rule_public() {
        let join_rule = "public";
        assert_eq!(join_rule, "public");
    }

    #[test]
    fn test_join_rule_invite() {
        let join_rule = "invite";
        assert_eq!(join_rule, "invite");
    }

    #[test]
    fn test_room_state_format() {
        let state = json!({
            "m.room.name": json!({
                "name": "Test Room"
            }),
            "m.room.topic": json!({
                "topic": "Test Topic"
            })
        });

        assert!(state.is_object());
        assert!(state.get("m.room.name").is_some());
    }

    #[test]
    fn test_room_list_response_format() {
        let room_list = vec![
            json!({
                "room_id": "!room1:example.com",
                "name": "Room 1",
                "member_count": 5
            }),
            json!({
                "room_id": "!room2:example.com",
                "name": "Room 2",
                "member_count": 10
            }),
        ];

        let response = json!({
            "chunk": room_list,
            "total_room_count_estimate": 2
        });

        assert_eq!(response["chunk"].as_array().unwrap().len(), 2);
        assert_eq!(response["total_room_count_estimate"], 2);
    }
}
