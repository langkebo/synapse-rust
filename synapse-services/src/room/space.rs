use synapse_common::ApiError;
use synapse_storage::room::RoomStorage;
use synapse_storage::space::*;
use serde_json::json;
use std::sync::Arc;
use tracing::{error, info, instrument, warn};

pub struct SpaceService {
    space_storage: Arc<SpaceStorage>,
    room_storage: Arc<RoomStorage>,
    server_name: String,
}

impl SpaceService {
    pub fn new(space_storage: Arc<SpaceStorage>, room_storage: Arc<RoomStorage>, server_name: String) -> Self {
        Self { space_storage, room_storage, server_name }
    }

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
        info!("Deleting space: space_id={}, user={}", space_id, user_id);

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

        info!("Space deleted successfully: space_id={}", space_id);
        Ok(())
    }

    #[instrument(skip(self, request))]
    pub async fn add_child(&self, request: AddChildRequest) -> Result<SpaceChild, ApiError> {
        info!("Adding child to space: space_id={}, room_id={}", request.space_id, request.room_id);

        self.ensure_space_creator_access(&request.space_id, &request.sender).await?;

        let _room = self
            .room_storage
            .get_room(&request.room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get room", &e))?
            .ok_or_else(|| ApiError::not_found("Room not found"))?;

        let child = self
            .space_storage
            .add_child(request)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to add child", &e))?;

        self.space_storage.update_space_summary(&child.space_id).await.map_err(|e| {
            warn!("Failed to update space summary: {}", e);
            ApiError::internal_with_log("Failed to update space summary", &e)
        })?;

        let event_id = format!("${}:{}", uuid::Uuid::new_v4(), self.server_name);
        let content = json!({
            "room_id": child.room_id,
            "via": child.via_servers,
            "suggested": child.is_suggested,
        });

        self.space_storage
            .add_space_event(&event_id, &child.space_id, "m.space.child", &child.sender, content, Some(&child.room_id))
            .await
            .map_err(|e| {
                warn!("Failed to add space child event: {}", e);
                ApiError::internal_with_log("Failed to add space event", &e)
            })?;

        info!("Child added successfully: space_id={}, room_id={}", child.space_id, child.room_id);
        Ok(child)
    }

    #[instrument(skip(self))]
    pub async fn remove_child(&self, space_id: &str, room_id: &str, user_id: &str) -> Result<(), ApiError> {
        info!("Removing child from space: space_id={}, room_id={}", space_id, room_id);

        self.ensure_space_creator_access(space_id, user_id).await?;

        self.space_storage
            .remove_child(space_id, room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to remove child", &e))?;

        self.space_storage.update_space_summary(space_id).await.map_err(|e| {
            warn!("Failed to update space summary: {}", e);
            ApiError::internal_with_log("Failed to update space summary", &e)
        })?;

        let event_id = format!("${}:{}", uuid::Uuid::new_v4(), self.server_name);
        let content = json!({
            "room_id": room_id,
        });

        self.space_storage
            .add_space_event(&event_id, space_id, "m.space.child", user_id, content, Some(room_id))
            .await
            .map_err(|e| {
                warn!("Failed to add space child removal event: {}", e);
                ApiError::internal_with_log("Failed to add space event", &e)
            })?;

        info!("Child removed successfully: space_id={}, room_id={}", space_id, room_id);
        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn get_space_children(&self, space_id: &str) -> Result<Vec<SpaceChild>, ApiError> {
        self.space_storage
            .get_space_children(space_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get space children", &e))
    }

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

    #[instrument(skip(self))]
    pub async fn get_space_members(&self, space_id: &str) -> Result<Vec<SpaceMember>, ApiError> {
        self.space_storage
            .get_space_members(space_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get space members", &e))
    }

    #[instrument(skip(self))]
    pub async fn invite_user(&self, space_id: &str, user_id: &str, inviter: &str) -> Result<SpaceMember, ApiError> {
        info!("Inviting user to space: space_id={}, user={}", space_id, user_id);

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
                warn!("Failed to add space member event: {}", e);
                ApiError::internal_with_log("Failed to add space event", &e)
            })?;

        info!("User invited successfully: space_id={}, user={}", space_id, user_id);
        Ok(member)
    }

    #[instrument(skip(self))]
    pub async fn join_space(&self, space_id: &str, user_id: &str) -> Result<SpaceMember, ApiError> {
        info!("User joining space: space_id={}, user={}", space_id, user_id);

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
            warn!("Failed to update space summary: {}", e);
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
                warn!("Failed to add space member event: {}", e);
                ApiError::internal_with_log("Failed to add space event", &e)
            })?;

        info!("User joined space successfully: space_id={}, user={}", space_id, user_id);
        Ok(member)
    }

    #[instrument(skip(self))]
    pub async fn leave_space(&self, space_id: &str, user_id: &str) -> Result<(), ApiError> {
        info!("User leaving space: space_id={}, user={}", space_id, user_id);

        self.space_storage
            .remove_space_member(space_id, user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to leave space", &e))?;

        self.space_storage.update_space_summary(space_id).await.map_err(|e| {
            warn!("Failed to update space summary: {}", e);
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
                warn!("Failed to add space member event: {}", e);
                ApiError::internal_with_log("Failed to add space event", &e)
            })?;

        info!("User left space successfully: space_id={}, user={}", space_id, user_id);
        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn get_user_spaces(&self, user_id: &str) -> Result<Vec<Space>, ApiError> {
        self.space_storage
            .get_user_spaces(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get user spaces", &e))
    }

    async fn ensure_room_creator_access(&self, room_id: &str, user_id: &str) -> Result<(), ApiError> {
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

    async fn ensure_space_creator_access(&self, space_id: &str, user_id: &str) -> Result<(), ApiError> {
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
    pub async fn get_space_hierarchy(&self, space_id: &str, max_depth: i32) -> Result<SpaceHierarchy, ApiError> {
        self.space_storage.get_space_hierarchy(space_id, max_depth).await.map_err(|e| {
            if matches!(e, sqlx::Error::RowNotFound) {
                ApiError::not_found("Space not found".to_string())
            } else {
                ApiError::internal_with_log("Failed to get space hierarchy", &e)
            }
        })
    }

    pub async fn build_hierarchy_rooms(
        &self,
        children: &[SpaceChild],
    ) -> Vec<synapse_storage::space::SpaceHierarchyRoom> {
        use synapse_storage::space::SpaceHierarchyRoom;

        futures::future::join_all(children.iter().map(|child| async move {
            let (name, topic, avatar_url, join_rule, world_readable, guest_can_join, room_type) =
                if let Ok(Some(space)) = self.space_storage.get_space_by_room(&child.room_id).await {
                    let world_readable = space.visibility.as_deref() == Some("public");
                    let guest_can_join = space.join_rule == "public";
                    let join_rule = space.join_rule;
                    (
                        space.name,
                        space.topic,
                        space.avatar_url,
                        join_rule,
                        world_readable,
                        guest_can_join,
                        Some("m.space".to_string()),
                    )
                } else {
                    (None, None, None, "invite".to_string(), false, false, Some("m.room".to_string()))
                };

            SpaceHierarchyRoom {
                room_id: child.room_id.clone(),
                name,
                topic,
                avatar_url,
                join_rule,
                world_readable,
                guest_can_join,
                num_joined_members: 0,
                room_type,
                children_state: vec![serde_json::json!({
                    "type": "m.space.child",
                    "state_key": &child.room_id,
                    "content": {
                        "via": &child.via_servers,
                        "suggested": child.is_suggested,
                    },
                    "sender": &child.sender,
                })],
            }
        }))
        .await
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
    pub async fn get_space_statistics(&self) -> Result<Vec<serde_json::Value>, ApiError> {
        self.space_storage
            .get_space_statistics()
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get space statistics", &e))
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
    ) -> Result<synapse_storage::space::SpaceHierarchyResponse, ApiError> {
        info!(
            "Getting space hierarchy v1: space_id={}, max_depth={}, suggested_only={}",
            space_id, max_depth, suggested_only
        );

        if let Some(uid) = user_id {
            let can_see = self
                .space_storage
                .check_user_can_see_space(space_id, uid)
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to check space visibility", &e))?;

            if !can_see {
                return Err(ApiError::forbidden("User cannot access this space"));
            }
        }

        self.space_storage
            .get_space_hierarchy_paginated(space_id, max_depth, suggested_only, limit, from)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get space hierarchy", &e))
    }

    #[instrument(skip(self))]
    pub async fn get_recursive_hierarchy(
        &self,
        space_id: &str,
        max_depth: i32,
        suggested_only: bool,
    ) -> Result<Vec<synapse_storage::space::SpaceChildInfo>, ApiError> {
        info!("Getting recursive hierarchy: space_id={}, max_depth={}", space_id, max_depth);

        self.space_storage
            .get_recursive_hierarchy(space_id, max_depth, suggested_only)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get recursive hierarchy", &e))
    }

    #[instrument(skip(self))]
    pub async fn get_parent_spaces(&self, room_id: &str) -> Result<Vec<Space>, ApiError> {
        info!("Getting parent spaces for room: room_id={}", room_id);

        self.space_storage
            .get_parent_spaces(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get parent spaces", &e))
    }

    #[instrument(skip(self))]
    pub async fn get_space_tree_path(&self, space_id: &str) -> Result<Vec<Space>, ApiError> {
        info!("Getting space tree path: space_id={}", space_id);

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
            .map_err(|e| ApiError::internal_with_log("Failed to get space", &e))?
            .ok_or_else(|| ApiError::not_found("Space not found"))?;

        if let Some(uid) = user_id {
            let can_see = self
                .space_storage
                .check_user_can_see_space(space_id, uid)
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

        let child_rooms = futures::future::join_all(children.iter().map(|child| async {
            if let Some(child_space) = self.space_storage.get_space_by_room(&child.room_id).await.ok().flatten() {
                Some(serde_json::json!({
                    "room_id": child.room_id,
                    "name": child_space.name,
                    "topic": child_space.topic,
                    "avatar_url": child_space.avatar_url,
                    "join_rule": child_space.join_rule,
                    "room_type": "m.space",
                    "via_servers": child.via_servers,
                    "suggested": child.is_suggested,
                }))
            } else {
                Some(serde_json::json!({
                    "room_id": child.room_id,
                    "room_type": "m.room",
                    "via_servers": child.via_servers,
                    "suggested": child.is_suggested,
                }))
            }
        }))
        .await;

        Ok(serde_json::json!({
            "room_id": space.space_id,
            "name": space.name,
            "topic": space.topic,
            "avatar_url": space.avatar_url,
            "join_rule": &space.join_rule,
            "world_readable": space.visibility.as_deref() == Some("public"),
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
                        "suggested": child.is_suggested,
                    },
                    "sender": &child.sender,
                })
            }).collect::<Vec<_>>(),
        }))
    }
}
