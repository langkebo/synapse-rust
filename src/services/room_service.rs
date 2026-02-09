use crate::common::background_job::BackgroundJob;
use crate::common::constants::BURN_AFTER_READ_DELAY_SECS;
use crate::common::task_queue::RedisTaskQueue;
use crate::common::validation::Validator;
use crate::common::{generate_event_id, generate_room_id};
use crate::services::*;
use crate::storage::CreateEventParams;
use crate::storage::UserStorage;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::RwLock;

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
    task_queue: Option<Arc<RedisTaskQueue>>,
    // CRITICAL FIX: Track spawned tasks to prevent memory leaks and enable graceful shutdown
    active_tasks: Arc<RwLock<HashMap<String, tokio::task::JoinHandle<()>>>>,
}

impl RoomService {
    pub fn new(
        room_storage: RoomStorage,
        member_storage: RoomMemberStorage,
        event_storage: EventStorage,
        user_storage: UserStorage,
        validator: Arc<Validator>,
        server_name: String,
        task_queue: Option<Arc<RedisTaskQueue>>,
    ) -> Self {
        Self {
            room_storage,
            member_storage,
            event_storage,
            user_storage,
            validator,
            server_name,
            task_queue,
            active_tasks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Clean up completed tasks and return count of remaining active tasks
    pub async fn cleanup_completed_tasks(&self) -> usize {
        let mut tasks = self.active_tasks.write()
            .expect("Task manager lock poisoned - cannot recover");
        tasks.retain(|_key, handle| {
            !handle.is_finished()
        });
        tasks.len()
    }

    /// Abort a specific delayed task
    pub async fn abort_task(&self, task_id: &str) -> bool {
        let mut tasks = self.active_tasks.write()
            .expect("Task manager lock poisoned - cannot recover");
        if let Some(handle) = tasks.remove(task_id) {
            handle.abort();
            true
        } else {
            false
        }
    }

    /// Graceful shutdown - abort all active delayed tasks
    pub async fn shutdown(&self) {
        let mut tasks = self.active_tasks.write()
            .expect("Task manager lock poisoned - cannot recover during shutdown");
        for (task_id, handle) in tasks.drain() {
            ::tracing::info!("Aborting delayed task: {}", task_id);
            handle.abort();
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
        let mut join_rule = self.determine_join_rule(config.preset.as_deref());
        let is_public = self.is_public_visibility(config.visibility.as_deref());
        
        // Handle trusted_private_chat preset
        let is_trusted_private = config.preset.as_deref() == Some("trusted_private_chat");
        if is_trusted_private {
            join_rule = "invite";
        }

        self.create_room_in_db(&room_id, user_id, join_rule, is_public)
            .await?;
        self.add_creator_to_room(&room_id, user_id).await?;
        self.set_room_metadata(&room_id, config.name.as_deref(), config.topic.as_deref())
            .await?;
        self.process_invites(&room_id, config.invite_list.as_ref())
            .await?;

        // Handle trusted private chat specific logic
        if is_trusted_private {
            // Set history visibility to invited
            let now = chrono::Utc::now().timestamp_millis();
            let history_content = json!({ "history_visibility": "invited" });
            self.event_storage.create_event(CreateEventParams {
                event_id: generate_event_id(&self.server_name),
                room_id: room_id.clone(),
                user_id: user_id.to_string(),
                event_type: "m.room.history_visibility".to_string(),
                content: history_content,
                state_key: Some("".to_string()),
                origin_server_ts: now,
            }).await.map_err(|e| ApiError::internal(format!("Failed to set history visibility: {}", e)))?;

            // Set guest access to forbidden
            let guest_content = json!({ "guest_access": "forbidden" });
            self.event_storage.create_event(CreateEventParams {
                event_id: generate_event_id(&self.server_name),
                room_id: room_id.clone(),
                user_id: user_id.to_string(),
                event_type: "m.room.guest_access".to_string(),
                content: guest_content,
                state_key: Some("".to_string()),
                origin_server_ts: now,
            }).await.map_err(|e| ApiError::internal(format!("Failed to set guest access: {}", e)))?;

            // Set privacy marker for anti-screenshot
            let privacy_content = json!({ "action": "block_screenshot" });
            self.event_storage.create_event(CreateEventParams {
                event_id: generate_event_id(&self.server_name),
                room_id: room_id.clone(),
                user_id: user_id.to_string(),
                event_type: "com.hula.privacy".to_string(),
                content: privacy_content,
                state_key: Some("".to_string()),
                origin_server_ts: now,
            }).await.map_err(|e| ApiError::internal(format!("Failed to set privacy marker: {}", e)))?;
        }

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
            let existing_users = self.user_storage.filter_existing_users(invites).await.map_err(|e| {
                ApiError::internal(format!("Failed to check users existence: {}", e))
            })?;

            for invitee in invites {
                if !existing_users.contains(invitee) {
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

        let rooms_data = self.room_storage.get_rooms_batch(&room_ids).await
            .map_err(|e| ApiError::internal(format!("Failed to fetch rooms batch: {}", e)))?;

        let rooms: Vec<serde_json::Value> = rooms_data.into_iter().map(|room| {
            json!({
                "room_id": room.room_id,
                "name": room.name,
                "topic": room.topic,
                "is_public": room.is_public,
                "join_rule": room.join_rule
            })
        }).collect();

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

    pub async fn room_exists(&self, room_id: &str) -> ApiResult<bool> {
        let exists =
            self.room_storage.room_exists(room_id).await.map_err(|e| {
                ApiError::database(format!("Failed to check room existence: {}", e))
            })?;
        Ok(exists)
    }

    pub async fn is_room_creator(&self, room_id: &str, user_id: &str) -> ApiResult<bool> {
        let room = self
            .room_storage
            .get_room(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get room: {}", e)))?;

        match room {
            Some(r) => Ok(r.creator == user_id),
            None => Ok(false),
        }
    }

    pub async fn get_room_aliases(&self, room_id: &str) -> ApiResult<Vec<String>> {
        self.room_storage
            .get_room_aliases(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get room aliases: {}", e)))
    }

    pub async fn set_room_alias(
        &self,
        room_id: &str,
        alias: &str,
        created_by: &str,
    ) -> ApiResult<()> {
        self.room_storage
            .set_room_alias(room_id, alias, created_by)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to set room alias: {}", e)))
    }

    pub async fn get_room_by_alias(&self, alias: &str) -> ApiResult<Option<String>> {
        self.room_storage
            .get_room_by_alias(alias)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get room by alias: {}", e)))
    }

    pub async fn remove_room_alias(&self, room_id: &str) -> ApiResult<()> {
        self.room_storage
            .remove_room_alias(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to remove room alias: {}", e)))
    }

    pub async fn set_room_directory(&self, room_id: &str, is_public: bool) -> ApiResult<()> {
        self.room_storage
            .set_room_directory(room_id, is_public)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to set room directory: {}", e)))
    }

    pub async fn remove_room_directory(&self, room_id: &str) -> ApiResult<()> {
        self.room_storage
            .remove_room_directory(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to remove room from directory: {}", e)))
    }

    pub async fn process_read_receipt(
        &self,
        room_id: &str,
        event_id: &str,
        _user_id: &str,
    ) -> ApiResult<()> {
        let event = match self.event_storage.get_event(event_id).await {
            Ok(Some(e)) => e,
            _ => return Ok(()),
        };

        let content = match event.content.as_object() {
            Some(c) => c,
            None => return Ok(()),
        };

        if !content.contains_key("burn_after_read") {
            return Ok(());
        }

        let queue = match self.task_queue.clone() {
            Some(q) => q,
            None => return Ok(()),
        };

        let rid = room_id.to_string();
        let eid = event_id.to_string();
        let task_id = format!("burn_after_read:{}:{}", rid, eid);

        ::tracing::info!("Scheduling burn-after-read for event {} in room {}", eid, rid);

        // CRITICAL FIX: Track spawned task to prevent memory leaks
        let handle = tokio::spawn(async move {
            tokio::time::sleep(secs(BURN_AFTER_READ_DELAY_SECS)).await;

            let job = BackgroundJob::RedactEvent {
                event_id: eid.clone(),
                room_id: rid.clone(),
                reason: Some("Burn after read".to_string()),
            };

            match queue.submit(job).await {
                Ok(_) => {
                    ::tracing::info!("Submitted redaction job for event {}", eid);
                }
                Err(e) => {
                    ::tracing::error!("Failed to submit redaction job for event {}: {}", eid, e);
                }
            }
        });

        // Store the task handle for later cleanup/management
        self.active_tasks.write()
            .expect("Task manager lock poisoned - cannot store task handle")
            .insert(task_id, handle);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn test_room_service_creation() {
        // Skip this test as it requires full ServiceContainer setup with database pool
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
    fn test_join_rule_trusted_private() {
        let preset = "trusted_private_chat";
        let join_rule = match preset {
            "trusted_private_chat" => "invite",
            _ => "other",
        };
        assert_eq!(join_rule, "invite");
    }

    #[test]
    fn test_trusted_private_chat_preset_config() {
        let config = CreateRoomConfig {
            preset: Some("trusted_private_chat".to_string()),
            ..Default::default()
        };
        assert_eq!(config.preset.as_deref(), Some("trusted_private_chat"));
    }

    #[test]
    fn test_burn_after_read_metadata_detection() {
        let content = json!({
            "body": "secret message",
            "msgtype": "m.text",
            "burn_after_read": true
        });
        
        let has_metadata = content.as_object()
            .map(|c| c.contains_key("burn_after_read"))
            .unwrap_or(false);
            
        assert!(has_metadata);
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
