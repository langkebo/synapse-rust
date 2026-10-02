//! Room creation event helpers extracted from `create.rs`.
//!
//! Contains private helper methods for creating room events during room creation.

use super::creation_graph::CreationGraph;
use super::service::LifecycleService;
use serde_json::json;
use std::collections::HashMap;
use synapse_common::generate_event_id;
use synapse_common::MatrixErrorCode;
use synapse_common::{ApiError, ApiResult};
use synapse_storage::{CreateEventParams, PduGraphFields};

/// `true` 表示这次 create 派生的 `room_id` 已经存在（并发 createRoom 的派生 id 撞车）。
///
/// 判据同时覆盖两种形态，避免"换一种写法就漏判"：
/// * [`sqlx::Error::RowNotFound`] —— `rooms` 插入走 `ON CONFLICT (room_id) DO NOTHING`，
///   0 行受影响时 `create_room_with_executor` 就返回它（C88 起的唯一形态）；
/// * SQLSTATE `23505`（unique violation）—— 保留给仍用裸 INSERT 的调用点（例如
///   `RoomStoreApi` 的实现替身或未来新增的旁路），语义相同。
fn is_room_id_already_taken(err: &sqlx::Error) -> bool {
    matches!(err, sqlx::Error::RowNotFound) || err.as_database_error().map(|e| e.is_unique_violation()).unwrap_or(false)
}

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
    ///
    /// `allow_modification` is forwarded to
    /// [`crate::module_service::consult_event_admission`]: `true` for events
    /// whose bytes this server authored and persists verbatim, `false` for the
    /// create event itself — its content hash *is* the room id (MSC4291), so a
    /// rewrite would desync the identity the caller already derived and pinned
    /// in `graph`.
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
        allow_modification: bool,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> ApiResult<String> {
        let placeholder_id = event_id.map(str::to_string).unwrap_or_else(|| generate_event_id(&self.server_name));

        let mut params = CreateEventParams {
            event_id: placeholder_id,
            room_id: room_id.to_string(),
            user_id: sender.to_string(),
            event_type: event_type.to_string(),
            content,
            state_key: state_key.map(str::to_string),
            origin_server_ts,
            redacts: None,
        };

        // Consult the admission gate *before* the DAG tip advances or the row is
        // written. A refusal propagates as `403`; because the caller rolls the
        // surrounding transaction back, no partial room survives. The rewrite
        // (when `allow_modification`) lands before `graph.next`, so the recorded
        // DAG metadata describes the bytes actually persisted.
        crate::module_service::consult_event_admission(
            self.event_admission_gate.as_ref(),
            self.event_reader.as_ref(),
            &mut params,
            allow_modification,
        )
        .await?;

        let metadata = graph.next(&params.event_id, event_type, state_key, sender, &params.content);

        let written = self
            .event_writer
            .create_event_with_pdu(
                params,
                PduGraphFields {
                    depth: Some(metadata.depth),
                    prev_events: Some(metadata.prev_events),
                    auth_events: Some(metadata.auth_events),
                },
                tx,
            )
            .await
            .map_err(|e| ApiError::internal_with_cause(&format!("Failed to create {event_type} event"), e))?;

        // The row is persisted under the *finalized* ID, which the graph must
        // record before the next creation event is emitted. Writing through
        // `create_event_with_graph` instead would take the byte-faithful inbound
        // pass-through (which deliberately never finalizes) and persist the
        // placeholder in every room version.
        graph.rekey_last(&written.event_id, event_type, state_key);

        Ok(written.event_id)
    }

    /// See [`create_room_in_db`].
    ///
    /// ⚠️ C88：**派生 id 已被占用**是一种可预期的业务冲突，不能降级成 500 "Failed to create room"。
    /// v12 的 room_id 由 create 事件的 reference hash 决定（MSC4291）⇒ 同一毫秒 + 同一内容的
    /// 并发 createRoom（负载测试的常态）会派生同一个 id。进程内已由
    /// `current_timestamp_millis_monotonic()`（严格递增时钟）避免；跨进程撞车时这里返回
    /// **409 `M_ROOM_IN_USE`**（客户端可重试），而不是把它伪装成内部错误。
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

        result.map(|_| ()).map_err(|e| {
            if is_room_id_already_taken(&e) {
                tracing::warn!(
                    room_id = %room_id,
                    user_id = %user_id,
                    error = %e,
                    "派生的 room_id 已被占用（并发 createRoom 的派生 id 撞车）⇒ 409 M_ROOM_IN_USE"
                );
                ApiError::conflict_with(
                    MatrixErrorCode::RoomInUse,
                    "The room id derived from this create event is already in use; retry the request".to_string(),
                )
            } else {
                ApiError::internal_with_cause("Failed to create room", e)
            }
        })
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
                true,
                tx.as_deref_mut(),
            )
            .await?;
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
                true,
                tx.as_deref_mut(),
            )
            .await?;
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
                    true,
                    Some(&mut *tx),
                )
                .await?;
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

#[cfg(test)]
mod room_id_conflict_tests {
    use super::is_room_id_already_taken;

    /// C88：分类器必须认两种形态 —— `ON CONFLICT DO NOTHING` 下的 `RowNotFound`（0 行受影响）
    /// 与裸 INSERT 的 23505（unique violation）—— 否则"换一种写法就漏判"，
    /// 撞车又会被降级成 500。
    #[test]
    fn room_id_conflict_is_recognised_in_both_shapes() {
        assert!(is_room_id_already_taken(&sqlx::Error::RowNotFound), "0 行受影响必须识别为撞车");

        let unique = sqlx::Error::Database(Box::new(FakeUniqueViolation));
        assert!(is_room_id_already_taken(&unique), "23505 必须识别为撞车");

        let other = sqlx::Error::Protocol("something else".to_string());
        assert!(!is_room_id_already_taken(&other), "其它错误不得被误判成撞车（否则真错误会被吞成 409）");
    }

    /// 只实现 `is_unique_violation() == true` 的最小 database error（`sqlx::DatabaseError` 有
    /// 必填方法，逐条给默认实现即可）。
    #[derive(Debug)]
    struct FakeUniqueViolation;

    impl std::fmt::Display for FakeUniqueViolation {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "duplicate key value violates unique constraint \"rooms_pkey\"")
        }
    }

    impl std::error::Error for FakeUniqueViolation {}

    impl sqlx::error::DatabaseError for FakeUniqueViolation {
        fn message(&self) -> &str {
            "duplicate key value violates unique constraint \"rooms_pkey\""
        }

        fn kind(&self) -> sqlx::error::ErrorKind {
            sqlx::error::ErrorKind::UniqueViolation
        }

        fn as_error(&self) -> &(dyn std::error::Error + Send + Sync + 'static) {
            self
        }

        fn as_error_mut(&mut self) -> &mut (dyn std::error::Error + Send + Sync + 'static) {
            self
        }

        fn into_error(self: Box<Self>) -> Box<dyn std::error::Error + Send + Sync + 'static> {
            self
        }
    }
}
