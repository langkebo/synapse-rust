use crate::common::ApiError;
use crate::storage::event::EventStorage;
use crate::storage::room::RoomStorage;
use crate::storage::space::*;
use serde_json::json;
use std::sync::Arc;
use tracing::{error, info, instrument, warn};

pub struct SpaceService {
    space_storage: Arc<SpaceStorage>,
    room_storage: Arc<RoomStorage>,
    #[allow(dead_code)]
    event_storage: Arc<EventStorage>,
    server_name: String,
}

impl SpaceService {
    pub fn new(
        space_storage: Arc<SpaceStorage>,
        room_storage: Arc<RoomStorage>,
        event_storage: Arc<EventStorage>,
        server_name: String,
    ) -> Self {
        Self {
            space_storage,
            room_storage,
            event_storage,
            server_name,
        }
    }

    #[instrument(skip(self, request))]
    pub async fn create_space(&self, request: CreateSpaceRequest) -> Result<Space, ApiError> {
        info!("Creating space: room_id={}", request.room_id);

        let _room = self
            .room_storage
            .get_room(&request.room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get room: {}", e)))?
            .ok_or_else(|| ApiError::not_found("Room not found"))?;

        let space = self
            .space_storage
            .create_space(request)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to create space: {}", e)))?;

        let event_id = format!("${}:{}", uuid::Uuid::new_v4(), self.server_name);
        let content = json!({
            "type": "m.space",
            "room_id": space.room_id,
        });

        self.space_storage
            .add_space_event(
                &event_id,
                &space.space_id,
                "m.space.creation",
                &space.creator,
                content,
                None,
            )
            .await
            .map_err(|e| {
                error!("Failed to add space creation event: {}", e);
                ApiError::internal(format!("Failed to add space event: {}", e))
            })?;

        self.space_storage
            .update_space_summary(&space.space_id)
            .await
            .map_err(|e| {
                warn!("Failed to update space summary: {}", e);
                ApiError::internal(format!("Failed to update space summary: {}", e))
            })?;

        info!("Space created successfully: space_id={}", space.space_id);
        Ok(space)
    }

    #[instrument(skip(self))]
    pub async fn get_space(&self, space_id: &str) -> Result<Option<Space>, ApiError> {
        self.space_storage
            .get_space(space_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get space: {}", e)))
    }

    #[instrument(skip(self))]
    pub async fn get_space_by_room(&self, room_id: &str) -> Result<Option<Space>, ApiError> {
        self.space_storage
            .get_space_by_room(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get space by room: {}", e)))
    }

    #[instrument(skip(self))]
    pub async fn update_space(
        &self,
        space_id: &str,
        request: &UpdateSpaceRequest,
        user_id: &str,
    ) -> Result<Space, ApiError> {
        info!("Updating space: space_id={}, user={}", space_id, user_id);

        let is_member = self
            .space_storage
            .is_space_member(space_id, user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to check membership: {}", e)))?;

        if !is_member {
            return Err(ApiError::forbidden("User is not a member of this space"));
        }

        let space = self
            .space_storage
            .update_space(space_id, request)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to update space: {}", e)))?;

        self.space_storage
            .update_space_summary(space_id)
            .await
            .map_err(|e| {
                warn!("Failed to update space summary: {}", e);
                ApiError::internal(format!("Failed to update space summary: {}", e))
            })?;

        info!("Space updated successfully: space_id={}", space_id);
        Ok(space)
    }

    #[instrument(skip(self))]
    pub async fn delete_space(&self, space_id: &str, user_id: &str) -> Result<(), ApiError> {
        info!("Deleting space: space_id={}, user={}", space_id, user_id);

        let space = self
            .space_storage
            .get_space(space_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get space: {}", e)))?
            .ok_or_else(|| ApiError::not_found("Space not found"))?;

        if space.creator != user_id {
            return Err(ApiError::forbidden("Only the space creator can delete it"));
        }

        self.space_storage
            .delete_space(space_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to delete space: {}", e)))?;

        info!("Space deleted successfully: space_id={}", space_id);
        Ok(())
    }

    #[instrument(skip(self, request))]
    pub async fn add_child(&self, request: AddChildRequest) -> Result<SpaceChild, ApiError> {
        info!(
            "Adding child to space: space_id={}, room_id={}",
            request.space_id, request.room_id
        );

        let is_member = self
            .space_storage
            .is_space_member(&request.space_id, &request.added_by)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to check membership: {}", e)))?;

        if !is_member {
            return Err(ApiError::forbidden("User is not a member of this space"));
        }

        let _room = self
            .room_storage
            .get_room(&request.room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get room: {}", e)))?
            .ok_or_else(|| ApiError::not_found("Room not found"))?;

        let child = self
            .space_storage
            .add_child(request)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to add child: {}", e)))?;

        self.space_storage
            .update_space_summary(&child.space_id)
            .await
            .map_err(|e| {
                warn!("Failed to update space summary: {}", e);
                ApiError::internal(format!("Failed to update space summary: {}", e))
            })?;

        let event_id = format!("${}:{}", uuid::Uuid::new_v4(), self.server_name);
        let content = json!({
            "room_id": child.room_id,
            "via": child.via_servers,
            "order": child.order,
            "suggested": child.suggested,
        });

        self.space_storage
            .add_space_event(
                &event_id,
                &child.space_id,
                "m.space.child",
                &child.added_by,
                content,
                Some(&child.room_id),
            )
            .await
            .map_err(|e| {
                warn!("Failed to add space child event: {}", e);
                ApiError::internal(format!("Failed to add space event: {}", e))
            })?;

        info!(
            "Child added successfully: space_id={}, room_id={}",
            child.space_id, child.room_id
        );
        Ok(child)
    }

    #[instrument(skip(self))]
    pub async fn remove_child(
        &self,
        space_id: &str,
        room_id: &str,
        user_id: &str,
    ) -> Result<(), ApiError> {
        info!(
            "Removing child from space: space_id={}, room_id={}",
            space_id, room_id
        );

        let is_member = self
            .space_storage
            .is_space_member(space_id, user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to check membership: {}", e)))?;

        if !is_member {
            return Err(ApiError::forbidden("User is not a member of this space"));
        }

        self.space_storage
            .remove_child(space_id, room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to remove child: {}", e)))?;

        self.space_storage
            .update_space_summary(space_id)
            .await
            .map_err(|e| {
                warn!("Failed to update space summary: {}", e);
                ApiError::internal(format!("Failed to update space summary: {}", e))
            })?;

        let event_id = format!("${}:{}", uuid::Uuid::new_v4(), self.server_name);
        let content = json!({
            "room_id": room_id,
        });

        self.space_storage
            .add_space_event(
                &event_id,
                space_id,
                "m.space.child",
                user_id,
                content,
                Some(room_id),
            )
            .await
            .map_err(|e| {
                warn!("Failed to add space child removal event: {}", e);
                ApiError::internal(format!("Failed to add space event: {}", e))
            })?;

        info!(
            "Child removed successfully: space_id={}, room_id={}",
            space_id, room_id
        );
        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn get_space_children(&self, space_id: &str) -> Result<Vec<SpaceChild>, ApiError> {
        self.space_storage
            .get_space_children(space_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get space children: {}", e)))
    }

    #[instrument(skip(self))]
    pub async fn get_space_members(&self, space_id: &str) -> Result<Vec<SpaceMember>, ApiError> {
        self.space_storage
            .get_space_members(space_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get space members: {}", e)))
    }

    #[instrument(skip(self))]
    pub async fn invite_user(
        &self,
        space_id: &str,
        user_id: &str,
        inviter: &str,
    ) -> Result<SpaceMember, ApiError> {
        info!(
            "Inviting user to space: space_id={}, user={}",
            space_id, user_id
        );

        let is_member = self
            .space_storage
            .is_space_member(space_id, inviter)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to check membership: {}", e)))?;

        if !is_member {
            return Err(ApiError::forbidden("User is not a member of this space"));
        }

        let member = self
            .space_storage
            .add_space_member(space_id, user_id, "invite", Some(inviter))
            .await
            .map_err(|e| ApiError::internal(format!("Failed to invite user: {}", e)))?;

        let event_id = format!("${}:{}", uuid::Uuid::new_v4(), self.server_name);
        let content = json!({
            "membership": "invite",
        });

        self.space_storage
            .add_space_event(
                &event_id,
                space_id,
                "m.space.member",
                inviter,
                content,
                Some(user_id),
            )
            .await
            .map_err(|e| {
                warn!("Failed to add space member event: {}", e);
                ApiError::internal(format!("Failed to add space event: {}", e))
            })?;

        info!(
            "User invited successfully: space_id={}, user={}",
            space_id, user_id
        );
        Ok(member)
    }

    #[instrument(skip(self))]
    pub async fn join_space(&self, space_id: &str, user_id: &str) -> Result<SpaceMember, ApiError> {
        info!(
            "User joining space: space_id={}, user={}",
            space_id, user_id
        );

        let space = self
            .space_storage
            .get_space(space_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get space: {}", e)))?
            .ok_or_else(|| ApiError::not_found("Space not found"))?;

        if space.join_rule == "invite" {
            let existing = self
                .space_storage
                .get_space_members(space_id)
                .await
                .map_err(|e| ApiError::internal(format!("Failed to get space members: {}", e)))?;

            let is_invited = existing
                .iter()
                .any(|m| m.user_id == user_id && m.membership == "invite");
            if !is_invited {
                return Err(ApiError::forbidden("Space is invite-only"));
            }
        }

        let member = self
            .space_storage
            .add_space_member(space_id, user_id, "join", None)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to join space: {}", e)))?;

        self.space_storage
            .update_space_summary(space_id)
            .await
            .map_err(|e| {
                warn!("Failed to update space summary: {}", e);
                ApiError::internal(format!("Failed to update space summary: {}", e))
            })?;

        let event_id = format!("${}:{}", uuid::Uuid::new_v4(), self.server_name);
        let content = json!({
            "membership": "join",
        });

        self.space_storage
            .add_space_event(
                &event_id,
                space_id,
                "m.space.member",
                user_id,
                content,
                Some(user_id),
            )
            .await
            .map_err(|e| {
                warn!("Failed to add space member event: {}", e);
                ApiError::internal(format!("Failed to add space event: {}", e))
            })?;

        info!(
            "User joined space successfully: space_id={}, user={}",
            space_id, user_id
        );
        Ok(member)
    }

    #[instrument(skip(self))]
    pub async fn leave_space(&self, space_id: &str, user_id: &str) -> Result<(), ApiError> {
        info!(
            "User leaving space: space_id={}, user={}",
            space_id, user_id
        );

        self.space_storage
            .remove_space_member(space_id, user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to leave space: {}", e)))?;

        self.space_storage
            .update_space_summary(space_id)
            .await
            .map_err(|e| {
                warn!("Failed to update space summary: {}", e);
                ApiError::internal(format!("Failed to update space summary: {}", e))
            })?;

        let event_id = format!("${}:{}", uuid::Uuid::new_v4(), self.server_name);
        let content = json!({
            "membership": "leave",
        });

        self.space_storage
            .add_space_event(
                &event_id,
                space_id,
                "m.space.member",
                user_id,
                content,
                Some(user_id),
            )
            .await
            .map_err(|e| {
                warn!("Failed to add space member event: {}", e);
                ApiError::internal(format!("Failed to add space event: {}", e))
            })?;

        info!(
            "User left space successfully: space_id={}, user={}",
            space_id, user_id
        );
        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn get_user_spaces(&self, user_id: &str) -> Result<Vec<Space>, ApiError> {
        self.space_storage
            .get_user_spaces(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get user spaces: {}", e)))
    }

    #[instrument(skip(self))]
    pub async fn get_public_spaces(&self, limit: i64, offset: i64) -> Result<Vec<Space>, ApiError> {
        self.space_storage
            .get_public_spaces(limit, offset)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get public spaces: {}", e)))
    }

    #[instrument(skip(self))]
    pub async fn get_space_hierarchy(
        &self,
        space_id: &str,
        max_depth: i32,
    ) -> Result<SpaceHierarchy, ApiError> {
        self.space_storage
            .get_space_hierarchy(space_id, max_depth)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get space hierarchy: {}", e)))
    }

    #[instrument(skip(self))]
    pub async fn get_space_summary(
        &self,
        space_id: &str,
    ) -> Result<Option<SpaceSummary>, ApiError> {
        self.space_storage
            .get_space_summary(space_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get space summary: {}", e)))
    }

    #[instrument(skip(self))]
    pub async fn search_spaces(&self, query: &str, limit: i64) -> Result<Vec<Space>, ApiError> {
        self.space_storage
            .search_spaces(query, limit)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to search spaces: {}", e)))
    }

    #[instrument(skip(self))]
    pub async fn get_space_statistics(&self) -> Result<Vec<serde_json::Value>, ApiError> {
        self.space_storage
            .get_space_statistics()
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get space statistics: {}", e)))
    }

    #[instrument(skip(self))]
    pub async fn get_space_hierarchy_v1(
        &self,
        space_id: &str,
        max_depth: i32,
        suggested_only: bool,
        limit: Option<i32>,
        from: Option<&str>,
        user_id: Option<&str>,
    ) -> Result<crate::storage::space::SpaceHierarchyResponse, ApiError> {
        info!(
            "Getting space hierarchy v1: space_id={}, max_depth={}, suggested_only={}",
            space_id, max_depth, suggested_only
        );

        if let Some(uid) = user_id {
            let can_see = self
                .space_storage
                .check_user_can_see_space(space_id, uid)
                .await
                .map_err(|e| {
                    ApiError::internal(format!("Failed to check space visibility: {}", e))
                })?;

            if !can_see {
                return Err(ApiError::forbidden("User cannot access this space"));
            }
        }

        self.space_storage
            .get_space_hierarchy_paginated(space_id, max_depth, suggested_only, limit, from)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get space hierarchy: {}", e)))
    }

    #[instrument(skip(self))]
    pub async fn get_recursive_hierarchy(
        &self,
        space_id: &str,
        max_depth: i32,
        suggested_only: bool,
    ) -> Result<Vec<crate::storage::space::SpaceChildInfo>, ApiError> {
        info!(
            "Getting recursive hierarchy: space_id={}, max_depth={}",
            space_id, max_depth
        );

        self.space_storage
            .get_recursive_hierarchy(space_id, max_depth, suggested_only)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get recursive hierarchy: {}", e)))
    }

    #[instrument(skip(self))]
    pub async fn get_parent_spaces(&self, room_id: &str) -> Result<Vec<Space>, ApiError> {
        info!("Getting parent spaces for room: room_id={}", room_id);

        self.space_storage
            .get_parent_spaces(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get parent spaces: {}", e)))
    }

    #[instrument(skip(self))]
    pub async fn get_space_tree_path(&self, space_id: &str) -> Result<Vec<Space>, ApiError> {
        info!("Getting space tree path: space_id={}", space_id);

        self.space_storage
            .get_space_tree_path(space_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get space tree path: {}", e)))
    }

    #[instrument(skip(self))]
    pub async fn check_user_can_see_space(
        &self,
        space_id: &str,
        user_id: &str,
    ) -> Result<bool, ApiError> {
        self.space_storage
            .check_user_can_see_space(space_id, user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to check space visibility: {}", e)))
    }

    #[instrument(skip(self))]
    pub async fn get_space_summary_with_children(
        &self,
        space_id: &str,
        user_id: Option<&str>,
    ) -> Result<serde_json::Value, ApiError> {
        info!("Getting space summary with children: space_id={}", space_id);

        let space = self
            .space_storage
            .get_space(space_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get space: {}", e)))?
            .ok_or_else(|| ApiError::not_found("Space not found"))?;

        if let Some(uid) = user_id {
            let can_see = self
                .space_storage
                .check_user_can_see_space(space_id, uid)
                .await
                .map_err(|e| {
                    ApiError::internal(format!("Failed to check space visibility: {}", e))
                })?;

            if !can_see {
                return Err(ApiError::forbidden("User cannot access this space"));
            }
        }

        let children = self
            .space_storage
            .get_space_children(space_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get space children: {}", e)))?;

        let members = self
            .space_storage
            .get_space_members(space_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get space members: {}", e)))?;

        let child_rooms = futures::future::join_all(children.iter().map(|child| async {
            if let Some(child_space) = self
                .space_storage
                .get_space_by_room(&child.room_id)
                .await
                .ok()
                .flatten()
            {
                Some(serde_json::json!({
                    "room_id": child.room_id,
                    "name": child_space.name,
                    "topic": child_space.topic,
                    "avatar_url": child_space.avatar_url,
                    "join_rule": child_space.join_rule,
                    "room_type": "m.space",
                    "via_servers": child.via_servers,
                    "order": child.order,
                    "suggested": child.suggested,
                }))
            } else {
                Some(serde_json::json!({
                    "room_id": child.room_id,
                    "room_type": "m.room",
                    "via_servers": child.via_servers,
                    "order": child.order,
                    "suggested": child.suggested,
                }))
            }
        }))
        .await;

        Ok(serde_json::json!({
            "room_id": space.room_id,
            "name": space.name,
            "topic": space.topic,
            "avatar_url": space.avatar_url,
            "join_rule": &space.join_rule,
            "world_readable": space.visibility == "public",
            "guest_can_join": space.join_rule == "public",
            "room_type": "m.space",
            "num_joined_members": members.len(),
            "children": child_rooms.into_iter().flatten().collect::<Vec<_>>(),
            "children_state": children.iter().map(|child| {
                serde_json::json!({
                    "type": "m.space.child",
                    "state_key": &child.room_id,
                    "content": {
                        "via": &child.via_servers,
                        "order": &child.order,
                        "suggested": child.suggested,
                    },
                    "sender": &child.added_by,
                })
            }).collect::<Vec<_>>(),
        }))
    }
}
