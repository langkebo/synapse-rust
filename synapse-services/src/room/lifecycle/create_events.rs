//! Room creation event helpers extracted from `create.rs`.
//!
//! Contains private helper methods for creating room events during room creation.

use super::service::LifecycleService;
use serde_json::json;
use std::collections::HashMap;
use synapse_common::generate_event_id;
use synapse_common::{ApiError, ApiResult};
use synapse_storage::CreateEventParams;

impl LifecycleService {
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

        result.map(|_| ()).map_err(|e| ApiError::internal_with_context("Failed to create room", &e))
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
            .map_err(|e| ApiError::internal_with_context("Failed to add room member", &e))?;

        Ok(())
    }

    #[allow(clippy::needless_option_as_deref)]
    pub(crate) async fn set_room_metadata(
        &self,
        room_id: &str,
        user_id: &str,
        name: Option<&str>,
        topic: Option<&str>,
        base_ts: i64,
        mut tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> ApiResult<()> {
        if let Some(room_name) = name {
            if let Some(ref mut tx) = tx {
                self.room_storage
                    .update_room_name_in_tx(tx, room_id, room_name)
                    .await
                    .map_err(|e| ApiError::internal_with_context("Failed to update room name", &e))?;
            } else {
                self.room_storage
                    .update_room_name(room_id, room_name)
                    .await
                    .map_err(|e| ApiError::internal_with_context("Failed to update room name", &e))?;
            }
            self.event_writer
                .create_event(
                    CreateEventParams {
                        event_id: generate_event_id(&self.server_name),
                        room_id: room_id.to_string(),
                        user_id: user_id.to_string(),
                        event_type: "m.room.name".to_string(),
                        content: json!({ "name": room_name }),
                        state_key: Some("".to_string()),
                        origin_server_ts: base_ts,
                        redacts: None,
                    },
                    tx.as_deref_mut(),
                )
                .await
                .map_err(|e| ApiError::internal_with_context("Failed to create m.room.name event", &e))?;
        }

        if let Some(room_topic) = topic {
            if let Some(ref mut tx) = tx {
                self.room_storage
                    .update_room_topic_in_tx(tx, room_id, room_topic)
                    .await
                    .map_err(|e| ApiError::internal_with_context("Failed to update room topic", &e))?;
            } else {
                self.room_storage
                    .update_room_topic(room_id, room_topic)
                    .await
                    .map_err(|e| ApiError::internal_with_context("Failed to update room topic", &e))?;
            }
            self.event_writer
                .create_event(
                    CreateEventParams {
                        event_id: generate_event_id(&self.server_name),
                        room_id: room_id.to_string(),
                        user_id: user_id.to_string(),
                        event_type: "m.room.topic".to_string(),
                        content: json!({ "topic": room_topic }),
                        state_key: Some("".to_string()),
                        origin_server_ts: base_ts + 1,
                        redacts: None,
                    },
                    tx.as_deref_mut(),
                )
                .await
                .map_err(|e| ApiError::internal_with_context("Failed to create m.room.topic event", &e))?;
        }

        Ok(())
    }

    /// Invite a list of users to a room by writing their `m.room.member` (invite)
    /// events into the same transaction as the caller.
    ///
    /// Requires `&mut tx` — callers must open the transaction so this method
    /// can participate in a larger atomic unit (e.g. room creation).
    /// DB-03-b: the previous `Option<&mut tx>` signature was removed; callers
    /// that passed `None` triggered two independent auto-committed writes with
    /// no atomicity, and had zero test or production coverage.
    pub(crate) async fn process_invites(
        &self,
        room_id: &str,
        invite_list: Option<&Vec<String>>,
        invite_reasons: Option<&HashMap<String, String>>,
        sender_user_id: &str,
        base_ts: i64,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> ApiResult<()> {
        if let Some(invites) = invite_list {
            let existing_users = self
                .user_storage
                .filter_existing_users(invites)
                .await
                .map_err(|e| ApiError::internal_with_context("Failed to check users existence", &e))?;

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
                let reason = invite_reasons.and_then(|m| m.get(invitee)).map(String::as_str);
                self.member_storage
                    .add_member(room_id, invitee, "invite", None, reason, Some(sender_user_id), Some(&mut *tx))
                    .await
                    .map_err(|e| ApiError::internal_with_context("Failed to invite user", &e))?;
                self.event_writer
                    .create_event(
                        CreateEventParams {
                            event_id: generate_event_id(&self.server_name),
                            room_id: room_id.to_string(),
                            user_id: sender_user_id.to_string(),
                            event_type: "m.room.member".to_string(),
                            content: build_invite_event_content(invitee, reason),
                            state_key: Some(invitee.to_string()),
                            origin_server_ts: base_ts + offset,
                            redacts: None,
                        },
                        Some(&mut *tx),
                    )
                    .await
                    .map_err(|e| {
                        ApiError::internal_with_context("Failed to record m.room.member invite event", &e)
                    })?;
                offset += 1;
            }
        }
        Ok(())
    }
}

/// Build the `m.room.member` invite event content, optionally including a
/// `reason` field per MSC4491.
fn build_invite_event_content(invitee: &str, reason: Option<&str>) -> serde_json::Value {
    let displayname = invitee.trim_start_matches('@').split(':').next().unwrap_or(invitee);
    match reason {
        Some(r) => json!({
            "membership": "invite",
            "displayname": displayname,
            "reason": r,
        }),
        None => json!({
            "membership": "invite",
            "displayname": displayname,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_invite_event_content_without_reason() {
        let content = build_invite_event_content("@alice:example.com", None);
        assert_eq!(content["membership"], "invite");
        assert_eq!(content["displayname"], "alice");
        assert!(content.get("reason").is_none());
    }

    #[test]
    fn build_invite_event_content_with_reason() {
        let content = build_invite_event_content("@alice:example.com", Some("Welcome!"));
        assert_eq!(content["membership"], "invite");
        assert_eq!(content["displayname"], "alice");
        assert_eq!(content["reason"], "Welcome!");
    }
}
