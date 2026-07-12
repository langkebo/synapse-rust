use super::models::*;
use super::SpaceStorage;
use async_trait::async_trait;
use std::collections::HashMap;

#[async_trait]
pub trait SpaceStoreApi: Send + Sync {
    async fn create_space(&self, request: CreateSpaceRequest) -> Result<Space, sqlx::Error>;
    async fn get_space(&self, space_id: &str) -> Result<Option<Space>, sqlx::Error>;
    async fn get_space_by_room(&self, room_id: &str) -> Result<Option<Space>, sqlx::Error>;
    async fn get_spaces_by_rooms_batch(&self, room_ids: &[String]) -> Result<HashMap<String, Space>, sqlx::Error>;
    async fn update_space(&self, space_id: &str, request: &UpdateSpaceRequest) -> Result<Space, sqlx::Error>;
    async fn delete_space(&self, space_id: &str) -> Result<(), sqlx::Error>;
    async fn add_child(&self, request: AddChildRequest) -> Result<SpaceChild, sqlx::Error>;
    async fn remove_child(&self, space_id: &str, room_id: &str) -> Result<(), sqlx::Error>;
    async fn get_space_children(&self, space_id: &str) -> Result<Vec<SpaceChild>, sqlx::Error>;
    async fn get_child_spaces(&self, room_id: &str) -> Result<Vec<SpaceChild>, sqlx::Error>;
    async fn add_space_member(
        &self,
        space_id: &str,
        user_id: &str,
        membership: &str,
        inviter: Option<&str>,
    ) -> Result<SpaceMember, sqlx::Error>;
    async fn remove_space_member(&self, space_id: &str, user_id: &str) -> Result<(), sqlx::Error>;
    async fn get_space_members(&self, space_id: &str) -> Result<Vec<SpaceMember>, sqlx::Error>;
    async fn get_space_member(&self, space_id: &str, user_id: &str) -> Result<Option<SpaceMember>, sqlx::Error>;
    async fn get_user_spaces(&self, user_id: &str) -> Result<Vec<Space>, sqlx::Error>;
    async fn get_public_spaces(
        &self,
        limit: i64,
        cursor_created_ts: Option<i64>,
        cursor_space_id: Option<&str>,
    ) -> Result<Vec<Space>, sqlx::Error>;
    async fn get_space_hierarchy(&self, space_id: &str, max_depth: i32) -> Result<SpaceHierarchy, sqlx::Error>;
    async fn get_space_summary(&self, space_id: &str) -> Result<Option<SpaceSummary>, sqlx::Error>;
    async fn update_space_summary(&self, space_id: &str) -> Result<(), sqlx::Error>;
    async fn add_space_event(
        &self,
        event_id: &str,
        space_id: &str,
        event_type: &str,
        sender: &str,
        content: serde_json::Value,
        state_key: Option<&str>,
    ) -> Result<SpaceEvent, sqlx::Error>;
    async fn get_space_events(
        &self,
        space_id: &str,
        event_type: Option<&str>,
        limit: i64,
    ) -> Result<Vec<SpaceEvent>, sqlx::Error>;
    async fn search_spaces(&self, query: &str, limit: i64, user_id: Option<&str>) -> Result<Vec<Space>, sqlx::Error>;
    async fn is_space_member(&self, space_id: &str, user_id: &str) -> Result<bool, sqlx::Error>;
    async fn get_space_statistics(&self, limit: i64) -> Result<Vec<serde_json::Value>, sqlx::Error>;
    async fn get_recursive_hierarchy(
        &self,
        space_id: &str,
        max_depth: i32,
        suggested_only: bool,
    ) -> Result<Vec<SpaceChildInfo>, sqlx::Error>;
    async fn get_space_hierarchy_paginated(
        &self,
        space_id: &str,
        max_depth: i32,
        suggested_only: bool,
        limit: Option<i32>,
        from: Option<&str>,
    ) -> Result<SpaceHierarchyResponse, sqlx::Error>;
    async fn check_user_can_see_space(&self, space_id: &str, user_id: &str) -> Result<bool, sqlx::Error>;
    async fn get_parent_spaces(&self, room_id: &str) -> Result<Vec<Space>, sqlx::Error>;
    async fn get_space_tree_path(&self, space_id: &str) -> Result<Vec<Space>, sqlx::Error>;
    async fn resolve_space_id(&self, identifier: &str) -> Result<Option<String>, sqlx::Error>;
    async fn get_all_spaces_for_admin(&self) -> Result<Vec<Space>, sqlx::Error>;
    async fn get_space_by_identifier(&self, identifier: &str) -> Result<Option<Space>, sqlx::Error>;
    async fn get_space_user_ids(&self, space_id: &str) -> Result<Vec<String>, sqlx::Error>;
    async fn get_space_room_ids(&self, space_id: &str) -> Result<Vec<String>, sqlx::Error>;
    async fn get_space_member_and_child_count(&self, space_id: &str) -> Result<(i64, i64), sqlx::Error>;
    async fn delete_space_returning_count(&self, space_id: &str) -> Result<u64, sqlx::Error>;
    async fn get_space_children_paginated(
        &self,
        space_id: &str,
        limit: i64,
        from_added_ts: Option<i64>,
        from_id: Option<i64>,
    ) -> Result<Vec<SpaceChild>, sqlx::Error>;
    async fn get_space_members_paginated(
        &self,
        space_id: &str,
        limit: i64,
        from_joined_ts: Option<i64>,
        from_user_id: Option<&str>,
    ) -> Result<Vec<SpaceMember>, sqlx::Error>;
}

#[async_trait]
impl SpaceStoreApi for SpaceStorage {
    async fn create_space(&self, request: CreateSpaceRequest) -> Result<Space, sqlx::Error> {
        self.create_space(request).await
    }
    async fn get_space(&self, space_id: &str) -> Result<Option<Space>, sqlx::Error> {
        self.get_space(space_id).await
    }
    async fn get_space_by_room(&self, room_id: &str) -> Result<Option<Space>, sqlx::Error> {
        self.get_space_by_room(room_id).await
    }
    async fn get_spaces_by_rooms_batch(&self, room_ids: &[String]) -> Result<HashMap<String, Space>, sqlx::Error> {
        self.get_spaces_by_rooms_batch(room_ids).await
    }
    async fn update_space(&self, space_id: &str, request: &UpdateSpaceRequest) -> Result<Space, sqlx::Error> {
        self.update_space(space_id, request).await
    }
    async fn delete_space(&self, space_id: &str) -> Result<(), sqlx::Error> {
        self.delete_space(space_id).await
    }
    async fn add_child(&self, request: AddChildRequest) -> Result<SpaceChild, sqlx::Error> {
        self.add_child(request).await
    }
    async fn remove_child(&self, space_id: &str, room_id: &str) -> Result<(), sqlx::Error> {
        self.remove_child(space_id, room_id).await
    }
    async fn get_space_children(&self, space_id: &str) -> Result<Vec<SpaceChild>, sqlx::Error> {
        self.get_space_children(space_id).await
    }
    async fn get_child_spaces(&self, room_id: &str) -> Result<Vec<SpaceChild>, sqlx::Error> {
        self.get_child_spaces(room_id).await
    }
    async fn add_space_member(
        &self,
        space_id: &str,
        user_id: &str,
        membership: &str,
        inviter: Option<&str>,
    ) -> Result<SpaceMember, sqlx::Error> {
        self.add_space_member(space_id, user_id, membership, inviter).await
    }
    async fn remove_space_member(&self, space_id: &str, user_id: &str) -> Result<(), sqlx::Error> {
        self.remove_space_member(space_id, user_id).await
    }
    async fn get_space_members(&self, space_id: &str) -> Result<Vec<SpaceMember>, sqlx::Error> {
        self.get_space_members(space_id).await
    }
    async fn get_space_member(&self, space_id: &str, user_id: &str) -> Result<Option<SpaceMember>, sqlx::Error> {
        self.get_space_member(space_id, user_id).await
    }
    async fn get_user_spaces(&self, user_id: &str) -> Result<Vec<Space>, sqlx::Error> {
        self.get_user_spaces(user_id).await
    }
    async fn get_public_spaces(
        &self,
        limit: i64,
        cursor_created_ts: Option<i64>,
        cursor_space_id: Option<&str>,
    ) -> Result<Vec<Space>, sqlx::Error> {
        self.get_public_spaces(limit, cursor_created_ts, cursor_space_id).await
    }
    async fn get_space_hierarchy(&self, space_id: &str, max_depth: i32) -> Result<SpaceHierarchy, sqlx::Error> {
        self.get_space_hierarchy(space_id, max_depth).await
    }
    async fn get_space_summary(&self, space_id: &str) -> Result<Option<SpaceSummary>, sqlx::Error> {
        self.get_space_summary(space_id).await
    }
    async fn update_space_summary(&self, space_id: &str) -> Result<(), sqlx::Error> {
        self.update_space_summary(space_id).await
    }
    async fn add_space_event(
        &self,
        event_id: &str,
        space_id: &str,
        event_type: &str,
        sender: &str,
        content: serde_json::Value,
        state_key: Option<&str>,
    ) -> Result<SpaceEvent, sqlx::Error> {
        self.add_space_event(event_id, space_id, event_type, sender, content, state_key).await
    }
    async fn get_space_events(
        &self,
        space_id: &str,
        event_type: Option<&str>,
        limit: i64,
    ) -> Result<Vec<SpaceEvent>, sqlx::Error> {
        self.get_space_events(space_id, event_type, limit).await
    }
    async fn search_spaces(&self, query: &str, limit: i64, user_id: Option<&str>) -> Result<Vec<Space>, sqlx::Error> {
        self.search_spaces(query, limit, user_id).await
    }
    async fn is_space_member(&self, space_id: &str, user_id: &str) -> Result<bool, sqlx::Error> {
        self.is_space_member(space_id, user_id).await
    }
    async fn get_space_statistics(&self, limit: i64) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        self.get_space_statistics(limit).await
    }
    async fn get_recursive_hierarchy(
        &self,
        space_id: &str,
        max_depth: i32,
        suggested_only: bool,
    ) -> Result<Vec<SpaceChildInfo>, sqlx::Error> {
        self.get_recursive_hierarchy(space_id, max_depth, suggested_only).await
    }
    async fn get_space_hierarchy_paginated(
        &self,
        space_id: &str,
        max_depth: i32,
        suggested_only: bool,
        limit: Option<i32>,
        from: Option<&str>,
    ) -> Result<SpaceHierarchyResponse, sqlx::Error> {
        self.get_space_hierarchy_paginated(space_id, max_depth, suggested_only, limit, from).await
    }
    async fn check_user_can_see_space(&self, space_id: &str, user_id: &str) -> Result<bool, sqlx::Error> {
        self.check_user_can_see_space(space_id, user_id).await
    }
    async fn get_parent_spaces(&self, room_id: &str) -> Result<Vec<Space>, sqlx::Error> {
        self.get_parent_spaces(room_id).await
    }
    async fn get_space_tree_path(&self, space_id: &str) -> Result<Vec<Space>, sqlx::Error> {
        self.get_space_tree_path(space_id).await
    }
    async fn resolve_space_id(&self, identifier: &str) -> Result<Option<String>, sqlx::Error> {
        self.resolve_space_id(identifier).await
    }
    async fn get_all_spaces_for_admin(&self) -> Result<Vec<Space>, sqlx::Error> {
        self.get_all_spaces_for_admin().await
    }
    async fn get_space_by_identifier(&self, identifier: &str) -> Result<Option<Space>, sqlx::Error> {
        self.get_space_by_identifier(identifier).await
    }
    async fn get_space_user_ids(&self, space_id: &str) -> Result<Vec<String>, sqlx::Error> {
        self.get_space_user_ids(space_id).await
    }
    async fn get_space_room_ids(&self, space_id: &str) -> Result<Vec<String>, sqlx::Error> {
        self.get_space_room_ids(space_id).await
    }
    async fn get_space_member_and_child_count(&self, space_id: &str) -> Result<(i64, i64), sqlx::Error> {
        self.get_space_member_and_child_count(space_id).await
    }
    async fn delete_space_returning_count(&self, space_id: &str) -> Result<u64, sqlx::Error> {
        self.delete_space_returning_count(space_id).await
    }
    async fn get_space_children_paginated(
        &self,
        space_id: &str,
        limit: i64,
        from_added_ts: Option<i64>,
        from_id: Option<i64>,
    ) -> Result<Vec<SpaceChild>, sqlx::Error> {
        self.get_space_children_paginated(space_id, limit, from_added_ts, from_id).await
    }
    async fn get_space_members_paginated(
        &self,
        space_id: &str,
        limit: i64,
        from_joined_ts: Option<i64>,
        from_user_id: Option<&str>,
    ) -> Result<Vec<SpaceMember>, sqlx::Error> {
        self.get_space_members_paginated(space_id, limit, from_joined_ts, from_user_id).await
    }
}
