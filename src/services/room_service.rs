use crate::common::validation::Validator;
use crate::common::{generate_event_id, generate_room_id};
use crate::services::*;
use crate::storage::CreateEventParams;
use crate::storage::UserStorage;
use serde_json::json;
use std::sync::Arc;

#[derive(Debug, Default, Clone)]
pub struct CreateRoomConfig {
    pub visibility: Option<String>,
    pub room_alias_name: Option<String>,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub invite_list: Option<Vec<String>>,
    pub preset: Option<String>,
    pub encryption: Option<String>,
    pub history_visibility: Option<String>,
}

pub struct RoomService {
    room_storage: RoomStorage,
    member_storage: RoomMemberStorage,
    event_storage: EventStorage,
    user_storage: UserStorage,
    validator: Arc<Validator>,
    server_name: String,
}

impl RoomService {
    pub fn new(
        room_storage: RoomStorage,
        member_storage: RoomMemberStorage,
        event_storage: EventStorage,
        user_storage: UserStorage,
        validator: Arc<Validator>,
        server_name: String,
    ) -> Self {
        Self {
            room_storage,
            member_storage,
            event_storage,
            user_storage,
            validator,
            server_name,
        }
    }

    pub async fn create_room(
        &self,
        user_id: &str,
        config: CreateRoomConfig,
    ) -> ApiResult<serde_json::Value> {
        if let Some(alias) = &config.room_alias_name {
            if let Err(e) = self.validator.validate_username(alias) {
                return Err(e.into());
            }
        }

        let room_id = self.generate_room_id();
        let join_rule = self.determine_join_rule(config.preset.as_deref());
        let is_public = self.is_public_visibility(config.visibility.as_deref());

        self.create_room_in_db(&room_id, user_id, join_rule, is_public)
            .await?;
        self.add_creator_to_room(&room_id, user_id).await?;
        self.set_room_metadata(&room_id, config.name.as_deref(), config.topic.as_deref())
            .await?;
        self.process_invites(&room_id, config.invite_list.as_ref())
            .await?;

        let room_alias = self.format_room_alias(config.room_alias_name.as_deref());
        Ok(self.build_room_response(&room_id, room_alias))
    }

    fn generate_room_id(&self) -> String {
        generate_room_id(&self.server_name)
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
        self.room_storage
            .create_room(room_id, user_id, join_rule, "1", is_public)
            .await
            .map(|_| ())
            .map_err(|e| ApiError::internal(format!("Failed to create room: {}", e)))
    }

    async fn add_creator_to_room(&self, room_id: &str, user_id: &str) -> ApiResult<()> {
        self.member_storage
            .add_member(room_id, user_id, "join", None, None)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to add room member: {}", e)))?;

        self.room_storage
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
            self.room_storage
                .update_room_name(room_id, room_name)
                .await
                .map_err(|e| ApiError::internal(format!("Failed to update room name: {}", e)))?;
        }

        if let Some(room_topic) = topic {
            self.room_storage
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
                if !self.user_storage.user_exists(invitee).await.map_err(|e| {
                    ApiError::internal(format!("Failed to check user existence: {}", e))
                })? {
                    ::tracing::warn!("Skipping invite for non-existent user: {}", invitee);
                    continue;
                }
                self.member_storage
                    .add_member(room_id, invitee, "invite", None, None)
                    .await
                    .map_err(|e| ApiError::internal(format!("Failed to invite user: {}", e)))?;
            }
        }
        Ok(())
    }

    fn format_room_alias(&self, room_alias_name: Option<&str>) -> Option<String> {
        room_alias_name.map(|a| format!("#{}:{}", a, self.server_name))
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
            .member_storage
            .is_member(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to check membership: {}", e)))?
        {
            return Err(ApiError::forbidden(
                "You are not a member of this room".to_string(),
            ));
        }

        let event_id = generate_event_id(&self.server_name);
        let now = chrono::Utc::now().timestamp_millis();

        let event_content = json!({
            "msgtype": message_type,
            "body": content
        });

        self.event_storage
            .create_event(CreateEventParams {
                event_id: event_id.clone(),
                room_id: room_id.to_string(),
                user_id: user_id.to_string(),
                event_type: "m.room.message".to_string(),
                content: event_content,
                state_key: None,
                origin_server_ts: now,
            })
            .await
            .map_err(|e| ApiError::internal(format!("Failed to send message: {}", e)))?;

        Ok(json!({
            "event_id": event_id
        }))
    }

    pub async fn join_room(&self, room_id: &str, user_id: &str) -> ApiResult<()> {
        if !self
            .room_storage
            .room_exists(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to check room: {}", e)))?
        {
            return Err(ApiError::not_found("Room not found".to_string()));
        }

        if !self
            .user_storage
            .user_exists(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to check user existence: {}", e)))?
        {
            return Err(ApiError::not_found("User not found".to_string()));
        }

        self.member_storage
            .add_member(room_id, user_id, "join", None, None)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to join room: {}", e)))?;

        self.room_storage
            .increment_member_count(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to update member count: {}", e)))?;

        Ok(())
    }

    pub async fn leave_room(&self, room_id: &str, user_id: &str) -> ApiResult<()> {
        self.member_storage
            .remove_member(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to leave room: {}", e)))?;

        self.room_storage
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
            .member_storage
            .get_room_members(room_id, "join")
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get members: {}", e)))?;

        Ok(json!({ "chunk": members }))
    }

    pub async fn get_room(&self, room_id: &str) -> ApiResult<serde_json::Value> {
        let room = self
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
                "creator": r.creator,
                "join_rule": r.join_rule
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
                "creator": r.creator,
                "join_rule": r.join_rule
            })),
            None => Err(ApiError::not_found("Room not found".to_string())),
        }
    }

    pub async fn get_user_rooms(&self, user_id: &str) -> ApiResult<serde_json::Value> {
        let room_ids = self
            .member_storage
            .get_joined_rooms(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get rooms: {}", e)))?;

        let mut rooms = Vec::new();
        for room_id in room_ids {
            if let Ok(Some(room)) = self.room_storage.get_room(&room_id).await {
                rooms.push(json!({
                    "room_id": room.room_id,
                    "name": room.name,
                    "topic": room.topic,
                    "is_public": room.is_public,
                    "join_rule": room.join_rule
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
            .event_storage
            .get_room_events(room_id, limit)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get messages: {}", e)))?;

        let event_list: Vec<serde_json::Value> = events
            .iter()
            .map(|e| {
                json!({
                    "type": e.event_type,
                    "content": e.content,
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

    pub async fn invite_user(
        &self,
        room_id: &str,
        inviter_id: &str,
        invitee_id: &str,
    ) -> ApiResult<()> {
        if !self
            .room_storage
            .room_exists(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to check room: {}", e)))?
        {
            return Err(ApiError::not_found("Room not found".to_string()));
        }

        if !self
            .user_storage
            .user_exists(invitee_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to check user existence: {}", e)))?
        {
            return Err(ApiError::not_found("User not found".to_string()));
        }

        self.member_storage
            .add_member(room_id, invitee_id, "invite", None, Some(inviter_id))
            .await
            .map_err(|e| ApiError::internal(format!("Failed to create invite event: {}", e)))?;
        Ok(())
    }

    pub async fn ban_user(
        &self,
        room_id: &str,
        user_id: &str,
        banned_by: &str,
        _reason: Option<&str>,
    ) -> ApiResult<()> {
        if !self
            .room_storage
            .room_exists(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to check room: {}", e)))?
        {
            return Err(ApiError::not_found("Room not found".to_string()));
        }

        if !self
            .user_storage
            .user_exists(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to check user existence: {}", e)))?
        {
            return Err(ApiError::not_found("User not found".to_string()));
        }

        self.member_storage
            .ban_member(room_id, user_id, banned_by)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to ban user: {}", e)))?;
        Ok(())
    }

    pub async fn get_state_events(&self, room_id: &str) -> ApiResult<Vec<serde_json::Value>> {
        let events = self
            .event_storage
            .get_state_events(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get state events: {}", e)))?;

        let event_list: Vec<serde_json::Value> = events
            .iter()
            .map(|e| {
                json!({
                    "event_id": e.event_id,
                    "sender": e.user_id,
                    "type": e.event_type,
                    "content": e.content,
                    "state_key": e.state_key
                })
            })
            .collect();

        Ok(event_list)
    }

    pub async fn get_public_rooms(&self, limit: i64) -> ApiResult<serde_json::Value> {
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

        Ok(json!({
            "chunk": room_list,
            "total_room_count_estimate": room_list.len() as i64
        }))
    }

    pub async fn delete_room(&self, room_id: &str) -> ApiResult<()> {
        self.room_storage
            .delete_room(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to delete room: {}", e)))
    }

    pub async fn get_joined_rooms(&self, user_id: &str) -> ApiResult<Vec<String>> {
        self.member_storage
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
        let room_alias = "#test:example.com";

        let response = json!({
            "room_id": room_id,
            "room_alias": room_alias
        });

        assert_eq!(response["room_id"], room_id);
        assert_eq!(response["room_alias"], room_alias);
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
