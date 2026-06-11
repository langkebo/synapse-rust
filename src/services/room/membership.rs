//! Room membership queries and shared utilities.
//!
//! Action methods (join, leave, forget) live in [`membership_actions`].
//! Moderation methods (invite, knock, ban, unban, kick) live in [`membership_moderation`].

use crate::common::error::{ApiError, ApiResult};
use crate::services::*;
use serde_json::json;

use super::service::RoomService;

impl RoomService {
    pub async fn get_room_members(&self, room_id: &str, user_id: &str) -> ApiResult<serde_json::Value> {
        if !self
            .room_storage
            .room_exists(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check room existence", &e))?
        {
            return Err(ApiError::not_found("Room not found".to_string()));
        }

        if !self
            .member_storage
            .is_member(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check membership", &e))?
        {
            return Err(ApiError::forbidden("You are not a member of this room".to_string()));
        }

        let members_res = self
            .member_storage
            .get_room_members_with_profiles(room_id, "join")
            .await;
        
        let members_with_profiles = match members_res {
            Ok(m) => m,
            Err(e) => return Err(ApiError::internal_with_log("Failed to get members", &e)),
        };

        let chunk: Vec<serde_json::Value> = members_with_profiles
            .iter()
            .map(|(m, user_displayname, user_avatar_url)| {
                let mut content = serde_json::Map::new();
                content.insert("membership".to_string(), json!(m.membership));
                let effective_displayname = m.display_name.as_deref().or(user_displayname.as_deref());
                if let Some(dn) = effective_displayname {
                    content.insert("displayname".to_string(), json!(dn));
                }
                let effective_avatar_url = m.avatar_url.as_deref().or(user_avatar_url.as_deref());
                if let Some(au) = effective_avatar_url {
                    content.insert("avatar_url".to_string(), json!(au));
                }
                if let Some(reason) = &m.reason {
                    content.insert("reason".to_string(), json!(reason));
                }
                json!({
                    "type": "m.room.member",
                    "state_key": m.user_id,
                    "content": content,
                    "event_id": m.event_id,
                    "origin_server_ts": m.joined_ts.unwrap_or(m.updated_ts.unwrap_or(0)),
                    "room_id": m.room_id,
                    "sender": m.sender.as_deref().unwrap_or(&m.user_id),
                })
            })
            .collect();

        Ok(json!({ "chunk": chunk }))
    }

    pub async fn get_joined_members_with_profiles(&self, room_id: &str) -> ApiResult<Vec<RoomMember>> {
        self.member_storage
            .get_joined_members(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get joined members", &e))
    }

    pub async fn get_membership_history(&self, room_id: &str, limit: i64) -> ApiResult<Vec<RoomMember>> {
        self.member_storage
            .get_membership_history(room_id, limit)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get membership history", &e))
    }

    pub async fn get_room_members_by_membership(
        &self,
        room_id: &str,
        membership: &str,
    ) -> ApiResult<Vec<RoomMember>> {
        self.member_storage
            .get_room_members(room_id, membership)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get room members", &e))
    }

    pub async fn get_joined_rooms(&self, user_id: &str) -> ApiResult<Vec<String>> {
        self.member_storage
            .get_joined_rooms(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get joined rooms", &e))
    }

    pub async fn get_room_membership(&self, room_id: &str, user_id: &str) -> ApiResult<Option<String>> {
        self.member_storage
            .get_membership_state(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check room membership", &e))
    }

    pub async fn get_invited_members_count(&self, room_id: &str) -> ApiResult<i64> {
        let count = sqlx::query_scalar::<_, i64>(
            r#"SELECT COALESCE(COUNT(*), 0) FROM room_memberships WHERE room_id = $1 AND membership = 'invite'"#,
        )
        .bind(room_id)
        .fetch_one(&*self.room_storage.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get invited members count", &e))?;
        Ok(count)
    }

    pub async fn add_member(
        &self,
        room_id: &str,
        user_id: &str,
        membership: &str,
        display_name: Option<&str>,
        join_reason: Option<&str>,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> ApiResult<crate::storage::RoomMember> {
        let should_update_summary = tx.is_none();
        let member = self
            .member_storage
            .add_member(room_id, user_id, membership, display_name, join_reason, None, tx)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to add member", &e))?;

        if should_update_summary {
            let request = crate::storage::room_summary::CreateSummaryMemberRequest {
                room_id: room_id.to_string(),
                user_id: user_id.to_string(),
                display_name: display_name.map(|value| value.to_string()),
                avatar_url: None,
                membership: membership.to_string(),
                is_hero: None,
                last_active_ts: member.joined_ts.or(member.updated_ts),
            };

            if let Err(error) = self.room_summary_service.add_member(request).await {
                ::tracing::warn!(
                    error = %error,
                    room_id = %room_id,
                    user_id = %user_id,
                    membership = %membership,
                    "Failed to update room summary member"
                );
            }

            if let Err(error) = self.room_summary_service.recalculate_heroes(room_id).await {
                ::tracing::warn!(error = %error, room_id = %room_id, "Failed to recalculate room summary heroes");
            }
        }

        Ok(member)
    }
}