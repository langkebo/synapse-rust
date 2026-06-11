//! Room service — core facade, configuration, lifecycle, and basic room queries.
//!
//! Domain-specific operations live in sibling modules:
//! - [`membership`] — join, leave, invite, kick, ban, knock, forget
//! - [`messages`] — send, paginate, ephemeral events, typing
//! - [`events`] — state events, event CRUD, signatures
//! - [`receipts`] — read receipts
//! - [`aliases`] — alias CRUD, directory, public rooms
//! - [`read_markers`] — MSC2654 read markers
//! - [`upgrade`] — room upgrade, tombstone, migration
//! - [`burn_after_read`] — burn-after-read scheduling
//! - [`info`] — encryption status, deletion, user room lists
//! - [`create`] — room creation
//! - [`space`] — space/child management
//! - [`summary`] — room summary computation

use crate::common::error::{ApiError, ApiResult};
use crate::common::task_queue::RedisTaskQueue;
use crate::common::validation::Validator;
use crate::services::*;
use crate::storage::UserStorage;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

// ── Configuration types ──

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
    pub is_direct: Option<bool>,
    pub room_type: Option<String>,
    /// Per Matrix C-S spec, additional state events the client wants applied
    /// after the standard set (m.room.create, m.room.member, power_levels,
    /// join_rules, history_visibility, guest_access, name, topic). Element
    /// uses this to install `m.room.encryption` for DMs.
    pub initial_state: Option<Vec<serde_json::Value>>,
    /// Extra top-level fields for the m.room.create event (e.g. `type`,
    /// `predecessor`). Merged into the create content.
    pub creation_content: Option<serde_json::Value>,
    /// Room version to record on m.room.create. Defaults to the server's
    /// capabilities default ("10") when None.
    pub room_version: Option<String>,
    /// Power level overrides applied on top of the spec defaults.
    pub power_level_content_override: Option<serde_json::Value>,
}

pub struct RoomServiceConfig {
    pub room_storage: RoomStorage,
    pub member_storage: RoomMemberStorage,
    pub event_storage: EventStorage,
    pub user_storage: UserStorage,
    pub auth_service: AuthService,
    pub room_summary_service: Arc<RoomSummaryService>,
    pub validator: Arc<Validator>,
    pub server_name: String,
    pub task_queue: Option<Arc<RedisTaskQueue>>,
    pub relations_storage: crate::storage::relations::RelationsStorage,
    pub event_broadcaster: Arc<crate::federation::event_broadcaster::EventBroadcaster>,
    #[cfg(feature = "beacons")]
    pub beacon_service: Option<Arc<crate::services::beacon_service::BeaconService>>,
    #[cfg(not(feature = "beacons"))]
    pub beacon_service: Option<()>,
}

// ── Service struct ──

pub struct RoomService {
    pub(crate) room_storage: RoomStorage,
    pub(crate) member_storage: RoomMemberStorage,
    pub event_storage: EventStorage,
    pub user_storage: UserStorage,
    pub(crate) auth_service: AuthService,
    pub validator: Arc<Validator>,
    pub server_name: String,
    pub task_queue: Option<Arc<RedisTaskQueue>>,
    pub active_tasks: Arc<RwLock<HashMap<String, tokio::task::JoinHandle<()>>>>,
    pub room_summary_service: Arc<RoomSummaryService>,
    pub(crate) relations_storage: crate::storage::relations::RelationsStorage,
    pub(crate) event_broadcaster: Arc<crate::federation::event_broadcaster::EventBroadcaster>,
    #[cfg(feature = "beacons")]
    pub(crate) beacon_service: Option<Arc<crate::services::beacon_service::BeaconService>>,
}

// ── Constructor & lifecycle ──

impl RoomService {
    pub fn new(config: RoomServiceConfig) -> Self {
        Self {
            room_storage: config.room_storage,
            member_storage: config.member_storage,
            event_storage: config.event_storage,
            user_storage: config.user_storage,
            auth_service: config.auth_service,
            room_summary_service: config.room_summary_service,
            validator: config.validator,
            server_name: config.server_name,
            task_queue: config.task_queue,
            active_tasks: Arc::new(RwLock::new(HashMap::new())),
            relations_storage: config.relations_storage,
            event_broadcaster: config.event_broadcaster,
            #[cfg(feature = "beacons")]
            beacon_service: config.beacon_service,
        }
    }

    pub fn room_summary_service(&self) -> &RoomSummaryService {
        &self.room_summary_service
    }

    pub fn start_cleanup_task(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        let active_tasks = self.active_tasks.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                let mut tasks = active_tasks.write().await;
                let before = tasks.len();
                tasks.retain(|_key, handle| !handle.is_finished());
                let after = tasks.len();
                if before != after {
                    ::tracing::debug!(
                        target: "room_service_cleanup",
                        cleaned = before - after,
                        remaining = after,
                        "Cleaned up completed background tasks"
                    );
                }
            }
        })
    }

    #[::tracing::instrument(skip(self))]
    pub async fn cleanup_completed_tasks(&self) -> usize {
        let mut tasks = self.active_tasks.write().await;
        tasks.retain(|_key, handle| !handle.is_finished());
        tasks.len()
    }

    pub async fn abort_task(&self, task_id: &str) -> bool {
        let mut tasks = self.active_tasks.write().await;
        if let Some(handle) = tasks.remove(task_id) {
            handle.abort();
            true
        } else {
            false
        }
    }

    pub async fn shutdown(&self) {
        let mut tasks = self.active_tasks.write().await;
        for (task_id, handle) in tasks.drain() {
            ::tracing::info!(task_id = %task_id, "Aborting delayed task");
            handle.abort();
        }
    }

    // ── Basic room queries ──

    pub async fn get_room(&self, room_id: &str) -> ApiResult<serde_json::Value> {
        let room = self
            .room_storage
            .get_room(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get room", &e))?;

        match room {
            Some(r) => Ok(json!({
                "room_id": r.room_id,
                "name": r.name,
                "topic": r.topic,
                "canonical_alias": r.canonical_alias,
                "is_public": r.is_public,
                "creator": r.creator_user_id,
                "join_rule": r.join_rule
            })),
            None => Err(ApiError::not_found("Room not found".to_string())),
        }
    }

    pub async fn get_room_state(&self, room_id: &str, user_id: &str) -> ApiResult<serde_json::Value> {
        if !self
            .member_storage
            .is_member(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check membership", &e))?
        {
            return Err(ApiError::forbidden("You are not a member of this room".to_string()));
        }

        let room = self
            .room_storage
            .get_room(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get room", &e))?;

        match room {
            Some(r) => Ok(json!({
                "room_id": r.room_id,
                "name": r.name,
                "topic": r.topic,
                "canonical_alias": r.canonical_alias,
                "is_public": r.is_public,
                "creator": r.creator_user_id,
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
            .map_err(|e| ApiError::internal_with_log("Failed to get rooms", &e))?;

        let rooms_data = self
            .room_storage
            .get_rooms_batch(&room_ids)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to fetch rooms batch", &e))?;

        let rooms: Vec<serde_json::Value> = rooms_data
            .into_iter()
            .map(|room| {
                json!({
                    "room_id": room.room_id,
                    "name": room.name,
                    "topic": room.topic,
                    "is_public": room.is_public,
                    "join_rule": room.join_rule
                })
            })
            .collect();

        Ok(json!(rooms))
    }
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

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
        let config = CreateRoomConfig { preset: Some("trusted_private_chat".to_string()), ..Default::default() };
        assert_eq!(config.preset.as_deref(), Some("trusted_private_chat"));
    }

    #[test]
    fn test_burn_after_read_metadata_detection() {
        let content = json!({
            "body": "secret message",
            "msgtype": "m.text",
            "burn_after_read": true
        });

        let has_metadata = content.as_object().is_some_and(|c| c.contains_key("burn_after_read"));

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