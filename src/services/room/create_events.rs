//! Room creation event helpers extracted from `create.rs`.
//!
//! Contains private helper methods for creating room events during room creation.

use crate::common::error::{ApiError, ApiResult};
use crate::common::generate_event_id;
use crate::storage::CreateEventParams;
use serde_json::json;

use super::service::RoomService;

pub(crate) struct PendingAppserviceDispatch {
    pub event_id: String,
    pub room_id: String,
    pub event_type: String,
    pub sender: String,
    pub content: serde_json::Value,
    pub state_key: Option<String>,
}

impl PendingAppserviceDispatch {
    pub(super) fn from_params(params: &CreateEventParams) -> Self {
        Self {
            event_id: params.event_id.clone(),
            room_id: params.room_id.clone(),
            event_type: params.event_type.clone(),
            sender: params.user_id.clone(),
            content: params.content.clone(),
            state_key: params.state_key.clone(),
        }
    }
}

impl RoomService {
    pub(crate) async fn create_room_in_db(
        &self,
        room_id: &str,
        user_id: &str,
        join_rule: &str,
        is_public: bool,
        room_version: &str,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> ApiResult<()> {
        let result = if let Some(tx) = tx {
            self.room_storage.create_room_in_tx(tx, room_id, user_id, join_rule, room_version, is_public).await
        } else {
            self.room_storage.create_room(room_id, user_id, join_rule, room_version, is_public).await
        };

        result.map(|_| ()).map_err(|e| ApiError::internal_with_log("Failed to create room", &e))
    }

    pub(crate) async fn add_creator_to_room(
        &self,
        room_id: &str,
        user_id: &str,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> ApiResult<()> {
        self.member_storage
            .add_member(room_id, user_id, "join", None, None, None, tx)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to add room member", &e))?;

        Ok(())
    }

    #[allow(clippy::needless_option_as_deref, clippy::too_many_arguments)]
    pub(crate) async fn set_room_metadata(
        &self,
        room_id: &str,
        user_id: &str,
        name: Option<&str>,
        topic: Option<&str>,
        base_ts: i64,
        mut tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
        pending_dispatches: &mut Vec<PendingAppserviceDispatch>,
    ) -> ApiResult<()> {
        if let Some(room_name) = name {
            if let Some(ref mut tx) = tx {
                self.room_storage
                    .update_room_name_in_tx(tx, room_id, room_name)
                    .await
                    .map_err(|e| ApiError::internal_with_log("Failed to update room name", &e))?;
            } else {
                self.room_storage
                    .update_room_name(room_id, room_name)
                    .await
                    .map_err(|e| ApiError::internal_with_log("Failed to update room name", &e))?;
            }
            let name_event = CreateEventParams {
                event_id: generate_event_id(&self.server_name),
                room_id: room_id.to_string(),
                user_id: user_id.to_string(),
                event_type: "m.room.name".to_string(),
                content: json!({ "name": room_name }),
                state_key: Some("".to_string()),
                origin_server_ts: base_ts,
            };
            if let Some(tx) = tx.as_deref_mut() {
                let pending_dispatch = PendingAppserviceDispatch::from_params(&name_event);
                self.event_storage
                    .create_event(name_event, Some(tx))
                    .await
                    .map_err(|e| ApiError::internal_with_log("Failed to create m.room.name event", &e))?;
                pending_dispatches.push(pending_dispatch);
            } else {
                self.create_event(name_event, None)
                    .await
                    .map_err(|e| ApiError::internal_with_log("Failed to create m.room.name event", &e))?;
            }
        }

        if let Some(room_topic) = topic {
            if let Some(ref mut tx) = tx {
                self.room_storage
                    .update_room_topic_in_tx(tx, room_id, room_topic)
                    .await
                    .map_err(|e| ApiError::internal_with_log("Failed to update room topic", &e))?;
            } else {
                self.room_storage
                    .update_room_topic(room_id, room_topic)
                    .await
                    .map_err(|e| ApiError::internal_with_log("Failed to update room topic", &e))?;
            }
            let topic_event = CreateEventParams {
                event_id: generate_event_id(&self.server_name),
                room_id: room_id.to_string(),
                user_id: user_id.to_string(),
                event_type: "m.room.topic".to_string(),
                content: json!({ "topic": room_topic }),
                state_key: Some("".to_string()),
                origin_server_ts: base_ts + 1,
            };
            if let Some(tx) = tx.as_deref_mut() {
                let pending_dispatch = PendingAppserviceDispatch::from_params(&topic_event);
                self.event_storage
                    .create_event(topic_event, Some(tx))
                    .await
                    .map_err(|e| ApiError::internal_with_log("Failed to create m.room.topic event", &e))?;
                pending_dispatches.push(pending_dispatch);
            } else {
                self.create_event(topic_event, None)
                    .await
                    .map_err(|e| ApiError::internal_with_log("Failed to create m.room.topic event", &e))?;
            }
        }

        Ok(())
    }

    pub(crate) async fn process_invites(
        &self,
        room_id: &str,
        invite_list: Option<&Vec<String>>,
        sender_user_id: &str,
        base_ts: i64,
        mut tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
        pending_dispatches: &mut Vec<PendingAppserviceDispatch>,
    ) -> ApiResult<()> {
        if let Some(invites) = invite_list {
            let existing_users = self
                .user_storage
                .filter_existing_users(invites)
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to check users existence", &e))?;

            if let Some(ref mut t) = tx {
                let mut offset: i64 = 0;
                for invitee in invites {
                    if !existing_users.contains(invitee) {
                        ::tracing::warn!(
                            room_id = %room_id,
                            invitee = %invitee,
                            sender_user_id = %sender_user_id,
                            "Skipping invite for non-existent user"
                        );
                        continue;
                    }
                    self.member_storage
                        .add_member(room_id, invitee, "invite", None, None, Some(sender_user_id), Some(&mut **t))
                        .await
                        .map_err(|e| ApiError::internal_with_log("Failed to invite user", &e))?;
                    let invite_event = CreateEventParams {
                        event_id: generate_event_id(&self.server_name),
                        room_id: room_id.to_string(),
                        user_id: sender_user_id.to_string(),
                        event_type: "m.room.member".to_string(),
                        content: json!({
                            "membership": "invite",
                            "displayname": invitee.trim_start_matches('@').split(':').next().unwrap_or(invitee),
                        }),
                        state_key: Some(invitee.to_string()),
                        origin_server_ts: base_ts + offset,
                    };
                    let pending_dispatch = PendingAppserviceDispatch::from_params(&invite_event);
                    self.event_storage
                        .create_event(invite_event, Some(&mut **t))
                        .await
                        .map_err(|e| ApiError::internal_with_log("Failed to record m.room.member invite event", &e))?;
                    pending_dispatches.push(pending_dispatch);
                    offset += 1;
                }
            } else {
                let mut offset: i64 = 0;
                for invitee in invites {
                    if !existing_users.contains(invitee) {
                        continue;
                    }
                    self.member_storage
                        .add_member(room_id, invitee, "invite", None, None, Some(sender_user_id), None)
                        .await
                        .map_err(|e| ApiError::internal_with_log("Failed to invite user", &e))?;
                    self.create_event(
                        CreateEventParams {
                            event_id: generate_event_id(&self.server_name),
                            room_id: room_id.to_string(),
                            user_id: sender_user_id.to_string(),
                            event_type: "m.room.member".to_string(),
                            content: json!({
                                "membership": "invite",
                                "displayname": invitee.trim_start_matches('@').split(':').next().unwrap_or(invitee),
                            }),
                            state_key: Some(invitee.to_string()),
                            origin_server_ts: base_ts + offset,
                        },
                        None,
                    )
                    .await
                    .map_err(|e| ApiError::internal_with_log("Failed to record m.room.member invite event", &e))?;
                    offset += 1;
                }
            }
        }
        Ok(())
    }
}
