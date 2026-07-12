//! Space service — core CRUD, state, and query operations.
//!
//! Domain-specific operations live in sibling modules:
//! - [`membership`] — invite, join, leave, member listing
//! - [`children`] — child management, hierarchy, summary-with-children

pub mod children;
pub mod membership;

use serde_json::json;
use std::sync::Arc;
use synapse_common::ApiError;
use synapse_storage::space::*;
use synapse_storage::RoomStoreApi;
use tracing::{error, info, instrument, warn};

pub struct SpaceService {
    pub(crate) space_storage: Arc<dyn SpaceStoreApi>,
    pub(crate) room_storage: Arc<dyn RoomStoreApi>,
    pub(crate) server_name: String,
}

impl SpaceService {
    pub fn new(
        space_storage: Arc<dyn SpaceStoreApi>,
        room_storage: Arc<dyn RoomStoreApi>,
        server_name: String,
    ) -> Self {
        Self { space_storage, room_storage, server_name }
    }

    // ── Core CRUD ──

    #[instrument(skip(self, request), fields(room_id = %request.room_id, creator = %request.creator))]
    pub async fn create_space(&self, request: CreateSpaceRequest) -> Result<Space, ApiError> {
        info!("Creating space");

        self.ensure_room_creator_access(&request.room_id, &request.creator).await?;

        let space = self.space_storage.create_space(request).await.map_err(|e| {
            error!(error = %e, "Failed to persist space to storage");
            ApiError::internal("Failed to create space")
        })?;

        let event_id = format!("${}:{}", uuid::Uuid::new_v4(), self.server_name);
        let content = json!({
            "type": "m.space",
            "room_id": space.space_id,
        });

        if let Err(e) = self
            .space_storage
            .add_space_event(&event_id, &space.space_id, "m.space.creation", &space.creator, content, None)
            .await
        {
            error!(error = %e, space_id = %space.space_id, "Failed to add space creation event");
            return Err(ApiError::internal("Failed to add space event"));
        }

        if let Err(e) = self.space_storage.update_space_summary(&space.space_id).await {
            warn!(error = %e, space_id = %space.space_id, "Failed to update space summary");
        }

        info!(space_id = %space.space_id, "Space created successfully");
        Ok(space)
    }

    #[instrument(skip(self), fields(space_id = %space_id))]
    pub async fn get_space(&self, space_id: &str) -> Result<Option<Space>, ApiError> {
        self.space_storage.get_space(space_id).await.map_err(|e| {
            error!(error = %e, "Failed to load space from storage");
            ApiError::internal("Failed to get space")
        })
    }

    #[instrument(skip(self), fields(room_id = %room_id))]
    pub async fn get_space_by_room(&self, room_id: &str) -> Result<Option<Space>, ApiError> {
        self.space_storage.get_space_by_room(room_id).await.map_err(|e| {
            error!(error = %e, "Failed to load space by room from storage");
            ApiError::internal("Failed to get space by room")
        })
    }

    #[instrument(skip(self, request), fields(space_id = %space_id, user_id = %user_id))]
    pub async fn update_space(
        &self,
        space_id: &str,
        request: &UpdateSpaceRequest,
        user_id: &str,
    ) -> Result<Space, ApiError> {
        info!("Updating space");

        self.ensure_space_creator_access(space_id, user_id).await?;

        self.space_storage.update_space(space_id, request).await.map_err(|e| {
            error!(error = %e, "Failed to update space in storage");
            ApiError::internal("Failed to update space")
        })
    }

    #[instrument(skip(self))]
    pub async fn delete_space(&self, space_id: &str, user_id: &str) -> Result<(), ApiError> {
        info!(space_id = %space_id, user_id = %user_id, "Deleting space");

        let space = self
            .space_storage
            .get_space(space_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get space", &e))?
            .ok_or_else(|| ApiError::not_found("Space not found"))?;

        if space.creator != user_id {
            return Err(ApiError::forbidden("Only the space creator can delete it"));
        }

        self.space_storage
            .delete_space(space_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to delete space", &e))?;

        info!(space_id = %space_id, user_id = %user_id, "Deleted space");
        Ok(())
    }

    // ── State ──

    #[instrument(skip(self))]
    pub async fn get_space_state(
        &self,
        space_id: &str,
        user_id: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, ApiError> {
        let space = self
            .space_storage
            .get_space(space_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get space", &e))?
            .ok_or_else(|| ApiError::not_found("Space not found"))?;

        if !space.is_public {
            let user_id = user_id.ok_or_else(|| ApiError::unauthorized("Authentication required"))?;
            let can_see = self
                .space_storage
                .check_user_can_see_space(space_id, user_id)
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to check space visibility", &e))?;
            if !can_see {
                return Err(ApiError::forbidden("User cannot access this space"));
            }
        }

        let children = self
            .space_storage
            .get_space_children(space_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get space children", &e))?;
        let members = self
            .space_storage
            .get_space_members(space_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get space members", &e))?;

        let mut state = vec![json!({
            "type": "m.room.create",
            "state_key": "",
            "content": {
                "creator": space.creator,
                "room_type": "m.space",
            }
        })];

        if let Some(name) = space.name {
            state.push(json!({
                "type": "m.room.name",
                "state_key": "",
                "content": { "name": name }
            }));
        }

        if let Some(topic) = space.topic {
            state.push(json!({
                "type": "m.room.topic",
                "state_key": "",
                "content": { "topic": topic }
            }));
        }

        if let Some(avatar_url) = space.avatar_url {
            state.push(json!({
                "type": "m.room.avatar",
                "state_key": "",
                "content": { "url": avatar_url }
            }));
        }

        state.push(json!({
            "type": "m.room.join_rules",
            "state_key": "",
            "content": { "join_rule": space.join_rule }
        }));

        for child in children {
            state.push(json!({
                "type": "m.space.child",
                "state_key": child.room_id,
                "content": {
                    "via": child.via_servers,
                    "suggested": child.is_suggested,
                },
                "sender": child.sender,
            }));
        }

        for member in members {
            state.push(json!({
                "type": "m.space.member",
                "state_key": member.user_id,
                "content": {
                    "membership": member.membership,
                },
                "sender": member.inviter.unwrap_or_default(),
            }));
        }

        Ok(state)
    }

    // ── Queries ──

    #[instrument(skip(self))]
    pub async fn get_user_spaces(&self, user_id: &str) -> Result<Vec<Space>, ApiError> {
        self.space_storage
            .get_user_spaces(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get user spaces", &e))
    }

    #[instrument(skip(self))]
    pub async fn get_public_spaces(
        &self,
        limit: i64,
        cursor_created_ts: Option<i64>,
        cursor_space_id: Option<&str>,
    ) -> Result<Vec<Space>, ApiError> {
        self.space_storage
            .get_public_spaces(limit, cursor_created_ts, cursor_space_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get public spaces", &e))
    }

    #[instrument(skip(self))]
    pub async fn resolve_space_id(&self, identifier: &str) -> Result<Option<String>, ApiError> {
        self.space_storage
            .resolve_space_id(identifier)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to resolve space", &e))
    }

    #[instrument(skip(self))]
    pub async fn get_all_spaces_for_admin(&self) -> Result<Vec<Space>, ApiError> {
        self.space_storage
            .get_all_spaces_for_admin()
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get spaces", &e))
    }

    #[instrument(skip(self))]
    pub async fn get_space_by_identifier(&self, identifier: &str) -> Result<Option<Space>, ApiError> {
        self.space_storage
            .get_space_by_identifier(identifier)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get space", &e))
    }

    #[instrument(skip(self))]
    pub async fn delete_space_returning_count(&self, space_id: &str) -> Result<u64, ApiError> {
        self.space_storage
            .delete_space_returning_count(space_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to delete space", &e))
    }

    #[instrument(skip(self))]
    pub async fn get_space_user_ids(&self, space_id: &str) -> Result<Vec<String>, ApiError> {
        self.space_storage
            .get_space_user_ids(space_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get space users", &e))
    }

    #[instrument(skip(self))]
    pub async fn get_space_room_ids(&self, space_id: &str) -> Result<Vec<String>, ApiError> {
        self.space_storage
            .get_space_room_ids(space_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get space rooms", &e))
    }

    #[instrument(skip(self))]
    pub async fn get_space_member_and_child_count(&self, space_id: &str) -> Result<(i64, i64), ApiError> {
        self.space_storage
            .get_space_member_and_child_count(space_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get space statistics", &e))
    }

    #[instrument(skip(self))]
    pub async fn get_space_summary(&self, space_id: &str) -> Result<Option<SpaceSummary>, ApiError> {
        self.space_storage
            .get_space_summary(space_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get space summary", &e))
    }

    #[instrument(skip(self))]
    pub async fn search_spaces(&self, query: &str, limit: i64, user_id: Option<&str>) -> Result<Vec<Space>, ApiError> {
        self.space_storage
            .search_spaces(query, limit, user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to search spaces", &e))
    }

    #[instrument(skip(self))]
    pub async fn get_space_statistics(&self, limit: i64) -> Result<Vec<serde_json::Value>, ApiError> {
        self.space_storage
            .get_space_statistics(limit)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get space statistics", &e))
    }

    #[instrument(skip(self))]
    pub async fn get_parent_spaces(&self, room_id: &str) -> Result<Vec<Space>, ApiError> {
        info!(room_id = %room_id, "Getting parent spaces for room");

        self.space_storage
            .get_parent_spaces(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get parent spaces", &e))
    }

    #[instrument(skip(self))]
    pub async fn get_space_tree_path(&self, space_id: &str) -> Result<Vec<Space>, ApiError> {
        info!(space_id = %space_id, "Getting space tree path");

        self.space_storage
            .get_space_tree_path(space_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get space tree path", &e))
    }

    #[instrument(skip(self))]
    pub async fn check_user_can_see_space(&self, space_id: &str, user_id: &str) -> Result<bool, ApiError> {
        self.space_storage
            .check_user_can_see_space(space_id, user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check space visibility", &e))
    }

    // ── Access helpers ──

    pub(crate) async fn ensure_room_creator_access(&self, room_id: &str, user_id: &str) -> Result<(), ApiError> {
        let room = self
            .room_storage
            .get_room(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get room", &e))?
            .ok_or_else(|| ApiError::not_found("Room not found"))?;

        if room.creator_user_id.as_deref() != Some(user_id) {
            return Err(ApiError::forbidden("Only the room creator can create a space for this room"));
        }

        Ok(())
    }

    pub(crate) async fn ensure_space_creator_access(&self, space_id: &str, user_id: &str) -> Result<(), ApiError> {
        let space = self
            .space_storage
            .get_space(space_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get space", &e))?
            .ok_or_else(|| ApiError::not_found("Space not found"))?;

        if space.creator != user_id {
            return Err(ApiError::forbidden("Only the space creator can modify this space"));
        }

        Ok(())
    }
}
