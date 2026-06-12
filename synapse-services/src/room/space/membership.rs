//! Space membership operations: invite, join, leave, member listing.

use serde_json::json;
use synapse_common::ApiError;
use synapse_storage::space::*;
use tracing::{info, instrument, warn};

use super::SpaceService;

impl SpaceService {
    #[instrument(skip(self))]
    pub async fn get_space_members(&self, space_id: &str) -> Result<Vec<SpaceMember>, ApiError> {
        self.space_storage
            .get_space_members(space_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get space members", &e))
    }

    #[instrument(skip(self))]
    pub async fn invite_user(&self, space_id: &str, user_id: &str, inviter: &str) -> Result<SpaceMember, ApiError> {
        info!(space_id = %space_id, user_id = %user_id, inviter = %inviter, "Inviting user to space");

        self.ensure_space_creator_access(space_id, inviter).await?;

        let member = self
            .space_storage
            .add_space_member(space_id, user_id, "invite", Some(inviter))
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to invite user", &e))?;

        let event_id = format!("${}:{}", uuid::Uuid::new_v4(), self.server_name);
        let content = json!({
            "membership": "invite",
        });

        self.space_storage
            .add_space_event(&event_id, space_id, "m.space.member", inviter, content, Some(user_id))
            .await
            .map_err(|e| {
                warn!(
                    error = %e,
                    space_id = %space_id,
                    user_id = %user_id,
                    inviter = %inviter,
                    event_id = %event_id,
                    membership = %"invite",
                    "Failed to add space member event"
                );
                ApiError::internal_with_log("Failed to add space event", &e)
            })?;

        info!(space_id = %space_id, user_id = %user_id, inviter = %inviter, "Invited user to space");
        Ok(member)
    }

    #[instrument(skip(self))]
    pub async fn join_space(&self, space_id: &str, user_id: &str) -> Result<SpaceMember, ApiError> {
        info!(space_id = %space_id, user_id = %user_id, "Joining space");

        let space = self
            .space_storage
            .get_space(space_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get space", &e))?
            .ok_or_else(|| ApiError::not_found("Space not found"))?;

        if space.join_rule == "invite" {
            let existing = self
                .space_storage
                .get_space_member(space_id, user_id)
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to get space member", &e))?;

            let is_invited = existing.as_ref().is_some_and(|member| member.membership == "invite");
            if !is_invited {
                return Err(ApiError::forbidden("Space is invite-only"));
            }
        }

        let member = self
            .space_storage
            .add_space_member(space_id, user_id, "join", None)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to join space", &e))?;

        self.space_storage.update_space_summary(space_id).await.map_err(|e| {
            warn!(error = %e, space_id = %space_id, user_id = %user_id, membership = %"join", "Failed to update space summary");
            ApiError::internal_with_log("Failed to update space summary", &e)
        })?;

        let event_id = format!("${}:{}", uuid::Uuid::new_v4(), self.server_name);
        let content = json!({
            "membership": "join",
        });

        self.space_storage
            .add_space_event(&event_id, space_id, "m.space.member", user_id, content, Some(user_id))
            .await
            .map_err(|e| {
                warn!(
                    error = %e,
                    space_id = %space_id,
                    user_id = %user_id,
                    event_id = %event_id,
                    membership = %"join",
                    "Failed to add space member event"
                );
                ApiError::internal_with_log("Failed to add space event", &e)
            })?;

        info!(space_id = %space_id, user_id = %user_id, "Joined space");
        Ok(member)
    }

    #[instrument(skip(self))]
    pub async fn leave_space(&self, space_id: &str, user_id: &str) -> Result<(), ApiError> {
        info!(space_id = %space_id, user_id = %user_id, "Leaving space");

        self.space_storage
            .remove_space_member(space_id, user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to leave space", &e))?;

        self.space_storage.update_space_summary(space_id).await.map_err(|e| {
            warn!(error = %e, space_id = %space_id, user_id = %user_id, membership = %"leave", "Failed to update space summary");
            ApiError::internal_with_log("Failed to update space summary", &e)
        })?;

        let event_id = format!("${}:{}", uuid::Uuid::new_v4(), self.server_name);
        let content = json!({
            "membership": "leave",
        });

        self.space_storage
            .add_space_event(&event_id, space_id, "m.space.member", user_id, content, Some(user_id))
            .await
            .map_err(|e| {
                warn!(
                    error = %e,
                    space_id = %space_id,
                    user_id = %user_id,
                    event_id = %event_id,
                    membership = %"leave",
                    "Failed to add space member event"
                );
                ApiError::internal_with_log("Failed to add space event", &e)
            })?;

        info!(space_id = %space_id, user_id = %user_id, "Left space");
        Ok(())
    }
}
