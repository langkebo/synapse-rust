//! Room membership queries and shared utilities.
//!
//! Action methods live in [`membership_actions`].
//! Moderation methods live in [`membership_moderation`].

use crate::common::error::{ApiError, ApiResult};
use crate::*;
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

        let members_with_profiles = self
            .member_storage
            .get_room_members_with_profiles(room_id, "join")
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get members", &e))?;

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

    pub async fn get_joined_rooms(&self, user_id: &str) -> ApiResult<Vec<String>> {
        self.member_storage
            .get_joined_rooms(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get joined rooms", &e))
    }

    pub async fn get_shared_room_users(&self, user_id: &str) -> ApiResult<Vec<String>> {
        self.member_storage
            .get_shared_room_users(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get shared room users", &e))
    }

    pub async fn share_common_room(&self, user_id: &str, other_user_id: &str) -> ApiResult<bool> {
        self.member_storage
            .share_common_room(user_id, other_user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check shared room membership", &e))
    }

    pub async fn share_common_rooms_batch(
        &self,
        user_id: &str,
        other_user_ids: &[String],
    ) -> ApiResult<Vec<String>> {
        self.member_storage
            .share_common_rooms_batch(user_id, other_user_ids)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check shared room membership batch", &e))
    }

    pub async fn get_joined_members_with_profiles(&self, room_id: &str) -> ApiResult<Vec<storage::RoomMember>> {
        self.member_storage
            .get_joined_members(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get joined members", &e))
    }

    pub async fn get_membership_history(&self, room_id: &str, limit: i64) -> ApiResult<Vec<storage::RoomMember>> {
        self.member_storage
            .get_membership_history(room_id, limit)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get membership history", &e))
    }

    pub async fn get_room_members_by_membership(
        &self,
        room_id: &str,
        membership: &str,
    ) -> ApiResult<Vec<storage::RoomMember>> {
        self.member_storage
            .get_room_members(room_id, membership)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get room members", &e))
    }

    pub async fn has_any_non_banned_member_from_server(&self, room_id: &str, server_name: &str) -> ApiResult<bool> {
        self.member_storage
            .has_any_non_banned_member_from_server(room_id, server_name)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check server room membership", &e))
    }

    /// Check whether a user shares any joined room with a member from the
    /// given server domain. Single-query replacement for the previous
    /// `get_joined_rooms` + per-room `get_room_members` N+1 pattern used in
    /// federation origin validation.
    pub async fn user_shares_room_with_server(&self, user_id: &str, server_name: &str) -> ApiResult<bool> {
        self.member_storage
            .user_shares_room_with_server(user_id, server_name)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check user shares room with server", &e))
    }

    /// Batch version of `user_shares_room_with_server`: returns the subset of
    /// `user_ids` that share at least one joined room with a member from the
    /// given server domain. Used by federation `keys_claim` / `keys_query` to
    /// validate multiple users in a single query instead of M × (1 + N).
    pub async fn filter_users_sharing_room_with_server(
        &self,
        user_ids: &[String],
        server_name: &str,
    ) -> ApiResult<std::collections::HashSet<String>> {
        self.member_storage
            .filter_users_sharing_room_with_server(user_ids, server_name)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to batch check users sharing room with server", &e))
    }

    pub async fn get_room_membership(&self, room_id: &str, user_id: &str) -> ApiResult<Option<String>> {
        self.member_storage
            .get_membership_state(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check room membership", &e))
    }

    pub async fn get_room_member_record(&self, room_id: &str, user_id: &str) -> ApiResult<Option<storage::RoomMember>> {
        self.member_storage
            .get_room_member(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to load room member", &e))
    }

    pub async fn remove_member_record(&self, room_id: &str, user_id: &str) -> ApiResult<()> {
        self.member_storage
            .remove_member(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to remove room member", &e))
    }

    pub async fn get_room_members_paginated_admin(
        &self,
        room_id: &str,
        membership: &str,
        limit: i64,
        from: Option<&str>,
    ) -> ApiResult<Vec<storage::RoomMember>> {
        self.member_storage
            .get_room_members_paginated(room_id, membership, limit, from)
            .await
            .map_err(|e| ApiError::database_with_log("Failed to get room members", &e))
    }

    pub async fn get_room_member_count_admin(&self, room_id: &str) -> ApiResult<i64> {
        self.member_storage
            .get_room_member_count(room_id)
            .await
            .map_err(|e| ApiError::database_with_log("Failed to count room members", &e))
    }

    pub async fn admin_ban_user_membership(&self, room_id: &str, user_id: &str, banned_by: &str) -> ApiResult<()> {
        self.member_storage
            .ban_member(room_id, user_id, banned_by)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to ban user", &e))
    }

    pub async fn admin_unban_user_membership(&self, room_id: &str, user_id: &str) -> ApiResult<()> {
        self.member_storage
            .unban_member(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to unban user", &e))
    }

    pub async fn set_ban_reason(&self, room_id: &str, user_id: &str, reason: &str) -> ApiResult<()> {
        self.member_storage
            .set_ban_reason(room_id, user_id, reason)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to set ban reason", &e))
    }

    pub async fn force_leave_membership(&self, room_id: &str, user_id: &str, now: i64) -> ApiResult<()> {
        self.member_storage
            .force_leave_membership(room_id, user_id, now)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to force leave membership", &e))
    }

    pub async fn decrement_member_count(&self, room_id: &str) -> ApiResult<()> {
        self.room_storage
            .decrement_member_count(room_id)
            .await
            .map(|_| ())
            .map_err(|e| ApiError::internal_with_log("Failed to update member count", &e))
    }

    pub async fn get_invited_members_count(&self, room_id: &str) -> ApiResult<i64> {
        let summary = self.room_summary_service.get_summary(room_id).await?;
        Ok(summary.map(|summary| summary.invited_member_count).unwrap_or(0))
    }

    pub async fn add_member(
        &self,
        room_id: &str,
        user_id: &str,
        membership: &str,
        display_name: Option<&str>,
        join_reason: Option<&str>,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> ApiResult<storage::RoomMember> {
        let should_update_summary = tx.is_none();
        let member = self
            .member_storage
            .add_member(room_id, user_id, membership, display_name, join_reason, None, tx)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to add member", &e))?;

        if should_update_summary {
            let request = storage::room_summary::CreateSummaryMemberRequest {
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
