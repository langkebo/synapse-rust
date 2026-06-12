//! Space children and hierarchy operations.

use crate::common::ApiError;
use crate::storage::space::*;
use serde_json::json;
use tracing::{info, instrument, warn};

use super::SpaceService;

impl SpaceService {
    #[instrument(skip(self, request))]
    pub async fn add_child(&self, request: AddChildRequest) -> Result<SpaceChild, ApiError> {
        info!(
            space_id = %request.space_id,
            room_id = %request.room_id,
            sender = %request.sender,
            "Adding child to space"
        );

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
            warn!(error = %e, space_id = %child.space_id, room_id = %child.room_id, "Failed to update space summary");
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
                warn!(
                    error = %e,
                    space_id = %child.space_id,
                    room_id = %child.room_id,
                    sender = %child.sender,
                    event_id = %event_id,
                    "Failed to add space child event"
                );
                ApiError::internal_with_log("Failed to add space event", &e)
            })?;

        info!(
            space_id = %child.space_id,
            room_id = %child.room_id,
            sender = %child.sender,
            "Added child to space"
        );
        Ok(child)
    }

    #[instrument(skip(self))]
    pub async fn remove_child(&self, space_id: &str, room_id: &str, user_id: &str) -> Result<(), ApiError> {
        info!(space_id = %space_id, room_id = %room_id, user_id = %user_id, "Removing child from space");

        self.ensure_space_creator_access(space_id, user_id).await?;

        self.space_storage
            .remove_child(space_id, room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to remove child", &e))?;

        self.space_storage.update_space_summary(space_id).await.map_err(|e| {
            warn!(error = %e, space_id = %space_id, room_id = %room_id, user_id = %user_id, "Failed to update space summary");
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
                warn!(
                    error = %e,
                    space_id = %space_id,
                    room_id = %room_id,
                    user_id = %user_id,
                    event_id = %event_id,
                    "Failed to add space child removal event"
                );
                ApiError::internal_with_log("Failed to add space event", &e)
            })?;

        info!(space_id = %space_id, room_id = %room_id, user_id = %user_id, "Removed child from space");
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
    pub async fn get_space_children_paginated(
        &self,
        space_id: &str,
        limit: i64,
        from_added_ts: Option<i64>,
        from_id: Option<i64>,
    ) -> Result<Vec<SpaceChild>, ApiError> {
        self.space_storage
            .get_space_children_paginated(space_id, limit, from_added_ts, from_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get space children", &e))
    }

    // ── Hierarchy ──

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
    ) -> Vec<crate::storage::space::SpaceHierarchyRoom> {
        use crate::storage::space::SpaceHierarchyRoom;

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
            space_id = %space_id,
            max_depth,
            suggested_only,
            "Getting space hierarchy v1"
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
    ) -> Result<Vec<crate::storage::space::SpaceChildInfo>, ApiError> {
        info!(
            space_id = %space_id,
            max_depth,
            suggested_only,
            "Getting recursive hierarchy"
        );

        self.space_storage
            .get_recursive_hierarchy(space_id, max_depth, suggested_only)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get recursive hierarchy", &e))
    }

    #[instrument(skip(self))]
    pub async fn get_space_summary_with_children(
        &self,
        space_id: &str,
        user_id: Option<&str>,
    ) -> Result<serde_json::Value, ApiError> {
        info!(space_id = %space_id, user_authenticated = user_id.is_some(), "Getting space summary with children");

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

        let children_res = self.space_storage.get_space_children(space_id).await;

        let children = match children_res {
            Ok(c) => c,
            Err(e) => return Err(ApiError::internal_with_log("Failed to get space children", &e)),
        };

        let members_res = self.space_storage.get_space_members(space_id).await;

        let members = match members_res {
            Ok(m) => m,
            Err(e) => return Err(ApiError::internal_with_log("Failed to get space members", &e)),
        };

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
