//! Room creation event helpers extracted from `create.rs`.
//!
//! Contains private helper methods for creating room events during room creation.

use super::creation_graph::CreationGraph;
use super::service::LifecycleService;
use serde_json::json;
use std::collections::HashMap;
use synapse_common::generate_event_id;
use synapse_common::{ApiError, ApiResult};
use synapse_storage::{CreateEventParams, PduGraphFields};

impl LifecycleService {
    /// Write one event of the room-creation sequence together with its DAG
    /// metadata.
    ///
    /// The event ID is generated here and recorded in `graph` **before** the
    /// write, so the tracker's view of the DAG tip and of the room state matches
    /// the rows this transaction is about to persist. The write-path decorator
    /// cannot do this: it reads committed state, which cannot see the rows the
    /// caller's transaction has not committed yet (see `graph_metadata`).
    ///
    /// The generated ID is only a placeholder for v3+ rooms, where identity is
    /// the reference hash and cannot be known before the PDU's graph fields are
    /// (decision §4.1). `create_event_with_pdu` finalizes it — reading the room
    /// version through this transaction — so the write returns the ID the row
    /// was actually persisted under, and `graph` is re-pointed at it before the
    /// next event is emitted. For v1/v2 the placeholder *is* final and
    /// finalization leaves it untouched.
    ///
    /// Returns the id the row was persisted under, so a caller that must know the
    /// event's identity (e.g. deriving a v12 room id from the create event) never
    /// has to re-implement finalization.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn write_creation_event(
        &self,
        graph: &mut CreationGraph,
        // `event_id`: the event's identity when the caller already knows it
        // (room v12+ derives the room id from the create event's id, so it must
        // be able to pin it); `None` mints a fresh placeholder. A finalizing
        // writer recomputes the id from the PDU and must land on the same value.
        event_id: Option<&str>,
        room_id: &str,
        sender: &str,
        event_type: &str,
        state_key: Option<&str>,
        content: serde_json::Value,
        origin_server_ts: i64,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> Result<String, sqlx::Error> {
        let placeholder_id = event_id.map(str::to_string).unwrap_or_else(|| generate_event_id(&self.server_name));
        let metadata = graph.next(&placeholder_id, event_type, state_key, sender, &content);

        let written = self
            .event_writer
            .create_event_with_pdu(
                CreateEventParams {
                    event_id: placeholder_id,
                    room_id: room_id.to_string(),
                    user_id: sender.to_string(),
                    event_type: event_type.to_string(),
                    content,
                    state_key: state_key.map(str::to_string),
                    origin_server_ts,
                    redacts: None,
                },
                PduGraphFields {
                    depth: Some(metadata.depth),
                    prev_events: Some(metadata.prev_events),
                    auth_events: Some(metadata.auth_events),
                },
                tx,
            )
            .await?;

        // The row is persisted under the *finalized* ID, which the graph must
        // record before the next creation event is emitted. Writing through
        // `create_event_with_graph` instead would take the byte-faithful inbound
        // pass-through (which deliberately never finalizes) and persist the
        // placeholder in every room version.
        graph.rekey_last(&written.event_id, event_type, state_key);

        Ok(written.event_id)
    }

    /// See [`create_room_in_db`].
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

        result.map(|_| ()).map_err(|e| ApiError::internal_with_cause("Failed to create room", e))
    }

    /// See [`add_creator_to_room`].
    pub(crate) async fn add_creator_to_room(
        &self,
        room_id: &str,
        user_id: &str,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> ApiResult<()> {
        self.member_storage
            .add_member(room_id, user_id, "join", None, None, None, tx)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to add room member", e))?;

        Ok(())
    }

    /// See [`set_room_metadata`].
    #[allow(clippy::needless_option_as_deref, clippy::too_many_arguments)]
    pub(crate) async fn set_room_metadata(
        &self,
        room_id: &str,
        user_id: &str,
        name: Option<&str>,
        topic: Option<&str>,
        base_ts: i64,
        graph: &mut CreationGraph,
        mut tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> ApiResult<()> {
        if let Some(room_name) = name {
            if let Some(ref mut tx) = tx {
                self.room_storage
                    .update_room_name_in_tx(tx, room_id, room_name)
                    .await
                    .map_err(|e| ApiError::internal_with_cause("Failed to update room name", e))?;
            } else {
                self.room_storage
                    .update_room_name(room_id, room_name)
                    .await
                    .map_err(|e| ApiError::internal_with_cause("Failed to update room name", e))?;
            }
            self.write_creation_event(
                graph,
                None,
                room_id,
                user_id,
                "m.room.name",
                Some(""),
                json!({ "name": room_name }),
                base_ts,
                tx.as_deref_mut(),
            )
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to create m.room.name event", e))?;
        }

        if let Some(room_topic) = topic {
            if let Some(ref mut tx) = tx {
                self.room_storage
                    .update_room_topic_in_tx(tx, room_id, room_topic)
                    .await
                    .map_err(|e| ApiError::internal_with_cause("Failed to update room topic", e))?;
            } else {
                self.room_storage
                    .update_room_topic(room_id, room_topic)
                    .await
                    .map_err(|e| ApiError::internal_with_cause("Failed to update room topic", e))?;
            }
            self.write_creation_event(
                graph,
                None,
                room_id,
                user_id,
                "m.room.topic",
                Some(""),
                json!({ "topic": room_topic }),
                base_ts + 1,
                tx.as_deref_mut(),
            )
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to create m.room.topic event", e))?;
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
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn process_invites(
        &self,
        room_id: &str,
        invite_list: Option<&Vec<String>>,
        invite_reasons: Option<&HashMap<String, String>>,
        sender_user_id: &str,
        base_ts: i64,
        graph: &mut CreationGraph,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> ApiResult<()> {
        if let Some(invites) = invite_list {
            let existing_users = self
                .user_storage
                .filter_existing_users(invites)
                .await
                .map_err(|e| ApiError::internal_with_cause("Failed to check users existence", e))?;

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
                    .map_err(|e| ApiError::internal_with_cause("Failed to invite user", e))?;
                self.write_creation_event(
                    graph,
                    None,
                    room_id,
                    sender_user_id,
                    "m.room.member",
                    Some(invitee),
                    build_invite_event_content(invitee, reason),
                    base_ts + offset,
                    Some(&mut *tx),
                )
                .await
                .map_err(|e| ApiError::internal_with_cause("Failed to record m.room.member invite event", e))?;
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
