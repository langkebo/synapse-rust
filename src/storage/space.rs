use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, Row};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Space {
    pub space_id: String,
    pub room_id: String,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub avatar_url: Option<String>,
    pub creator: String,
    pub join_rule: String,
    pub visibility: String,
    pub creation_ts: i64,
    pub updated_ts: Option<i64>,
    pub is_public: bool,
    pub parent_space_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SpaceChild {
    pub id: i64,
    pub space_id: String,
    pub room_id: String,
    #[sqlx(json)]
    pub via_servers: Vec<String>,
    pub order: Option<String>,
    pub suggested: bool,
    pub added_by: String,
    pub added_ts: i64,
    pub removed_ts: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SpaceMember {
    pub space_id: String,
    pub user_id: String,
    pub membership: String,
    pub joined_ts: i64,
    pub updated_ts: Option<i64>,
    pub left_ts: Option<i64>,
    pub inviter: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SpaceSummary {
    pub space_id: String,
    pub summary: serde_json::Value,
    pub children_count: i64,
    pub member_count: i64,
    pub updated_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SpaceEvent {
    pub event_id: String,
    pub space_id: String,
    pub event_type: String,
    pub sender: String,
    pub content: serde_json::Value,
    pub state_key: Option<String>,
    pub origin_server_ts: i64,
    pub processed_ts: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSpaceRequest {
    pub room_id: String,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub avatar_url: Option<String>,
    pub creator: String,
    pub join_rule: Option<String>,
    pub visibility: Option<String>,
    pub is_public: Option<bool>,
    pub parent_space_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddChildRequest {
    pub space_id: String,
    pub room_id: String,
    pub via_servers: Vec<String>,
    pub order: Option<String>,
    pub suggested: Option<bool>,
    pub added_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateSpaceRequest {
    pub name: Option<String>,
    pub topic: Option<String>,
    pub avatar_url: Option<String>,
    pub join_rule: Option<String>,
    pub visibility: Option<String>,
    pub is_public: Option<bool>,
}

impl UpdateSpaceRequest {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn topic(mut self, topic: impl Into<String>) -> Self {
        self.topic = Some(topic.into());
        self
    }

    pub fn avatar_url(mut self, avatar_url: impl Into<String>) -> Self {
        self.avatar_url = Some(avatar_url.into());
        self
    }

    pub fn join_rule(mut self, join_rule: impl Into<String>) -> Self {
        self.join_rule = Some(join_rule.into());
        self
    }

    pub fn visibility(mut self, visibility: impl Into<String>) -> Self {
        self.visibility = Some(visibility.into());
        self
    }

    pub fn is_public(mut self, is_public: bool) -> Self {
        self.is_public = Some(is_public);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceHierarchy {
    pub space: Space,
    pub children: Vec<SpaceChild>,
    pub members: Vec<SpaceMember>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceHierarchyNode {
    pub space: Space,
    pub children: Vec<SpaceHierarchyNode>,
    pub depth: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceHierarchyRequest {
    pub space_id: String,
    pub max_depth: i32,
    pub suggested_only: bool,
    pub limit: Option<i32>,
    pub from: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceHierarchyResponse {
    pub rooms: Vec<SpaceHierarchyRoom>,
    pub next_batch: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SpaceHierarchyRoom {
    pub room_id: String,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub avatar_url: Option<String>,
    pub join_rule: String,
    pub world_readable: bool,
    pub guest_can_join: bool,
    pub num_joined_members: i64,
    pub room_type: Option<String>,
    pub children_state: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceChildInfo {
    pub space_id: String,
    pub room_id: String,
    pub via_servers: Vec<String>,
    pub order: Option<String>,
    pub suggested: bool,
    pub is_space: bool,
    pub depth: i32,
}

#[derive(Clone)]
pub struct SpaceStorage {
    pool: Arc<PgPool>,
}

impl SpaceStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create_space(&self, request: CreateSpaceRequest) -> Result<Space, sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        let space_id = format!(
            "!space_{}:{}",
            uuid::Uuid::new_v4(),
            request
                .room_id
                .split(':')
                .next_back()
                .unwrap_or("localhost")
        );

        let space = sqlx::query_as::<_, Space>(
            r#"
            INSERT INTO spaces (
                space_id, room_id, name, topic, avatar_url, creator,
                join_rule, visibility, creation_ts, is_public, parent_space_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            RETURNING space_id, room_id, name, topic, avatar_url, creator, join_rule, visibility, creation_ts, updated_ts, is_public, parent_space_id
            "#,
        )
        .bind(&space_id)
        .bind(&request.room_id)
        .bind(&request.name)
        .bind(&request.topic)
        .bind(&request.avatar_url)
        .bind(&request.creator)
        .bind(request.join_rule.unwrap_or_else(|| "invite".to_string()))
        .bind(request.visibility.unwrap_or_else(|| "private".to_string()))
        .bind(now)
        .bind(request.is_public.unwrap_or(false))
        .bind(&request.parent_space_id)
        .fetch_one(&*self.pool)
        .await?;

        self.add_space_member(&space.space_id, &request.creator, "join", None)
            .await?;

        Ok(space)
    }

    pub async fn get_space(&self, space_id: &str) -> Result<Option<Space>, sqlx::Error> {
        sqlx::query_as::<_, Space>(r#"SELECT space_id, room_id, name, topic, avatar_url, creator, join_rule, visibility, creation_ts, updated_ts, is_public, parent_space_id FROM spaces WHERE space_id = $1"#)
            .bind(space_id)
            .fetch_optional(&*self.pool)
            .await
    }

    pub async fn get_space_by_room(&self, room_id: &str) -> Result<Option<Space>, sqlx::Error> {
        sqlx::query_as::<_, Space>(r#"SELECT space_id, room_id, name, topic, avatar_url, creator, join_rule, visibility, creation_ts, updated_ts, is_public, parent_space_id FROM spaces WHERE room_id = $1"#)
            .bind(room_id)
            .fetch_optional(&*self.pool)
            .await
    }

    pub async fn update_space(
        &self,
        space_id: &str,
        request: &UpdateSpaceRequest,
    ) -> Result<Space, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query_as::<_, Space>(
            r#"
            UPDATE spaces SET
                name = COALESCE($2, name),
                topic = COALESCE($3, topic),
                avatar_url = COALESCE($4, avatar_url),
                join_rule = COALESCE($5, join_rule),
                visibility = COALESCE($6, visibility),
                is_public = COALESCE($7, is_public),
                updated_ts = $8
            WHERE space_id = $1
            RETURNING space_id, room_id, name, topic, avatar_url, creator, join_rule, visibility, creation_ts, updated_ts, is_public, parent_space_id
            "#,
        )
        .bind(space_id)
        .bind(&request.name)
        .bind(&request.topic)
        .bind(&request.avatar_url)
        .bind(&request.join_rule)
        .bind(&request.visibility)
        .bind(request.is_public)
        .bind(now)
        .fetch_one(&*self.pool)
        .await
    }

    pub async fn delete_space(&self, space_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(r#"DELETE FROM spaces WHERE space_id = $1"#)
            .bind(space_id)
            .execute(&*self.pool)
            .await?;
        Ok(())
    }

    pub async fn add_child(&self, request: AddChildRequest) -> Result<SpaceChild, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query_as::<_, SpaceChild>(
            r#"
            INSERT INTO space_children (
                space_id, room_id, via_servers, "order", suggested, added_by, added_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (space_id, room_id) DO UPDATE SET
                via_servers = EXCLUDED.via_servers,
                "order" = EXCLUDED."order",
                suggested = EXCLUDED.suggested,
                added_by = EXCLUDED.added_by,
                added_ts = EXCLUDED.added_ts,
                removed_ts = NULL
            RETURNING *
            "#,
        )
        .bind(&request.space_id)
        .bind(&request.room_id)
        .bind(&request.via_servers)
        .bind(&request.order)
        .bind(request.suggested.unwrap_or(false))
        .bind(&request.added_by)
        .bind(now)
        .fetch_one(&*self.pool)
        .await
    }

    pub async fn remove_child(&self, space_id: &str, room_id: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query(
            r#"UPDATE space_children SET removed_ts = $3 WHERE space_id = $1 AND room_id = $2"#,
        )
        .bind(space_id)
        .bind(room_id)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_space_children(&self, space_id: &str) -> Result<Vec<SpaceChild>, sqlx::Error> {
        sqlx::query_as::<_, SpaceChild>(
            r#"SELECT id, space_id, room_id, via_servers, "order", suggested, added_by, added_ts, removed_ts FROM space_children WHERE space_id = $1 AND removed_ts IS NULL ORDER BY "order""#
        )
        .bind(space_id)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn get_child_spaces(&self, room_id: &str) -> Result<Vec<SpaceChild>, sqlx::Error> {
        sqlx::query_as::<_, SpaceChild>(
            r#"SELECT id, space_id, room_id, via_servers, "order", suggested, added_by, added_ts, removed_ts FROM space_children WHERE room_id = $1 AND removed_ts IS NULL"#,
        )
        .bind(room_id)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn add_space_member(
        &self,
        space_id: &str,
        user_id: &str,
        membership: &str,
        inviter: Option<&str>,
    ) -> Result<SpaceMember, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query_as::<_, SpaceMember>(
            r#"
            INSERT INTO space_members (space_id, user_id, membership, joined_ts, inviter)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (space_id, user_id) DO UPDATE SET
                membership = EXCLUDED.membership,
                joined_ts = EXCLUDED.joined_ts,
                inviter = EXCLUDED.inviter,
                left_ts = NULL,
                updated_ts = $4
            RETURNING space_id, user_id, membership, joined_ts, updated_ts, left_ts, inviter
            "#,
        )
        .bind(space_id)
        .bind(user_id)
        .bind(membership)
        .bind(now)
        .bind(inviter)
        .fetch_one(&*self.pool)
        .await
    }

    pub async fn remove_space_member(
        &self,
        space_id: &str,
        user_id: &str,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query(
            r#"UPDATE space_members SET membership = 'leave', left_ts = $3, updated_ts = $3 WHERE space_id = $1 AND user_id = $2"#
        )
        .bind(space_id)
        .bind(user_id)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_space_members(&self, space_id: &str) -> Result<Vec<SpaceMember>, sqlx::Error> {
        sqlx::query_as::<_, SpaceMember>(
            r#"SELECT * FROM space_members WHERE space_id = $1 AND membership = 'join'"#,
        )
        .bind(space_id)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn get_user_spaces(&self, user_id: &str) -> Result<Vec<Space>, sqlx::Error> {
        sqlx::query_as::<_, Space>(
            r#"
            SELECT s.* FROM spaces s
            JOIN space_members sm ON s.space_id = sm.space_id
            WHERE sm.user_id = $1 AND sm.membership = 'join'
            ORDER BY s.creation_ts DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn get_public_spaces(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Space>, sqlx::Error> {
        sqlx::query_as::<_, Space>(
            r#"SELECT * FROM spaces WHERE is_public = TRUE ORDER BY creation_ts DESC LIMIT $1 OFFSET $2"#
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn get_space_hierarchy(
        &self,
        space_id: &str,
        _max_depth: i32,
    ) -> Result<SpaceHierarchy, sqlx::Error> {
        let space = self
            .get_space(space_id)
            .await?
            .ok_or_else(|| sqlx::Error::RowNotFound)?;

        let children = self.get_space_children(space_id).await?;
        let members = self.get_space_members(space_id).await?;

        Ok(SpaceHierarchy {
            space,
            children,
            members,
        })
    }

    pub async fn get_space_summary(
        &self,
        space_id: &str,
    ) -> Result<Option<SpaceSummary>, sqlx::Error> {
        sqlx::query_as::<_, SpaceSummary>(r#"SELECT * FROM space_summaries WHERE space_id = $1"#)
            .bind(space_id)
            .fetch_optional(&*self.pool)
            .await
    }

    pub async fn update_space_summary(&self, space_id: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let children_count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM space_children WHERE space_id = $1 AND removed_ts IS NULL"#,
        )
        .bind(space_id)
        .fetch_one(&*self.pool)
        .await?;

        let member_count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM space_members WHERE space_id = $1 AND membership = 'join'"#,
        )
        .bind(space_id)
        .fetch_one(&*self.pool)
        .await?;

        let summary = serde_json::json!({
            "children_count": children_count,
            "member_count": member_count,
        });

        sqlx::query(
            r#"
            INSERT INTO space_summaries (space_id, summary, children_count, member_count, updated_ts)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (space_id) DO UPDATE SET
                summary = EXCLUDED.summary,
                children_count = EXCLUDED.children_count,
                member_count = EXCLUDED.member_count,
                updated_ts = EXCLUDED.updated_ts
            "#
        )
        .bind(space_id)
        .bind(&summary)
        .bind(children_count)
        .bind(member_count)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn add_space_event(
        &self,
        event_id: &str,
        space_id: &str,
        event_type: &str,
        sender: &str,
        content: serde_json::Value,
        state_key: Option<&str>,
    ) -> Result<SpaceEvent, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query_as::<_, SpaceEvent>(
            r#"
            INSERT INTO space_events (event_id, space_id, event_type, sender, content, state_key, origin_server_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING event_id, space_id, event_type, sender, content, state_key, origin_server_ts, processed_ts
            "#,
        )
        .bind(event_id)
        .bind(space_id)
        .bind(event_type)
        .bind(sender)
        .bind(&content)
        .bind(state_key)
        .bind(now)
        .fetch_one(&*self.pool)
        .await
    }

    pub async fn get_space_events(
        &self,
        space_id: &str,
        event_type: Option<&str>,
        limit: i64,
    ) -> Result<Vec<SpaceEvent>, sqlx::Error> {
        match event_type {
            Some(et) => {
                sqlx::query_as::<_, SpaceEvent>(
                    r#"SELECT event_id, space_id, event_type, sender, content, state_key, origin_server_ts, processed_ts FROM space_events WHERE space_id = $1 AND event_type = $2 ORDER BY origin_server_ts DESC LIMIT $3"#
                )
                .bind(space_id)
                .bind(et)
                .bind(limit)
                .fetch_all(&*self.pool)
                .await
            }
            None => {
                sqlx::query_as::<_, SpaceEvent>(
                    r#"SELECT * FROM space_events WHERE space_id = $1 ORDER BY origin_server_ts DESC LIMIT $2"#
                )
                .bind(space_id)
                .bind(limit)
                .fetch_all(&*self.pool)
                .await
            }
        }
    }

    pub async fn search_spaces(&self, query: &str, limit: i64) -> Result<Vec<Space>, sqlx::Error> {
        let pattern = format!("%{}%", query);

        sqlx::query_as::<_, Space>(
            r#"
            SELECT space_id, room_id, name, topic, avatar_url, creator, join_rule, visibility, creation_ts, updated_ts, is_public, parent_space_id
            FROM spaces 
            WHERE is_public = TRUE AND (name ILIKE $1 OR topic ILIKE $1)
            ORDER BY creation_ts DESC 
            LIMIT $2
            "#,
        )
        .bind(&pattern)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn is_space_member(
        &self,
        space_id: &str,
        user_id: &str,
    ) -> Result<bool, sqlx::Error> {
        let count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM space_members WHERE space_id = $1 AND user_id = $2 AND membership = 'join'"#
        )
        .bind(space_id)
        .bind(user_id)
        .fetch_one(&*self.pool)
        .await?;

        Ok(count > 0)
    }

    pub async fn get_space_statistics(&self) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        sqlx::query(r#"SELECT * FROM space_statistics ORDER BY member_count DESC"#)
            .fetch_all(&*self.pool)
            .await
            .map(|rows| {
                rows.into_iter()
                    .map(|row| {
                        serde_json::json!({
                            "space_id": row.get::<String, _>("space_id"),
                            "name": row.get::<Option<String>, _>("name"),
                            "is_public": row.get::<bool, _>("is_public"),
                            "child_room_count": row.get::<i64, _>("child_room_count"),
                            "member_count": row.get::<i64, _>("member_count"),
                            "creation_ts": row.get::<i64, _>("creation_ts"),
                            "updated_ts": row.get::<Option<i64>, _>("updated_ts"),
                        })
                    })
                    .collect()
            })
    }

    pub async fn get_recursive_hierarchy(
        &self,
        space_id: &str,
        max_depth: i32,
        suggested_only: bool,
    ) -> Result<Vec<SpaceChildInfo>, sqlx::Error> {
        let mut all_children = Vec::new();
        let mut visited = std::collections::HashSet::new();
        self.collect_hierarchy_recursive(
            space_id,
            0,
            max_depth,
            suggested_only,
            &mut all_children,
            &mut visited,
        )
        .await?;
        Ok(all_children)
    }

    async fn collect_hierarchy_recursive(
        &self,
        space_id: &str,
        current_depth: i32,
        max_depth: i32,
        suggested_only: bool,
        all_children: &mut Vec<SpaceChildInfo>,
        visited: &mut std::collections::HashSet<String>,
    ) -> Result<(), sqlx::Error> {
        if current_depth >= max_depth || visited.contains(space_id) {
            return Ok(());
        }
        visited.insert(space_id.to_string());

        let children = if suggested_only {
            sqlx::query_as::<_, SpaceChild>(
                r#"SELECT id, space_id, room_id, via_servers, "order", suggested, added_by, added_ts, removed_ts FROM space_children WHERE space_id = $1 AND suggested = TRUE AND removed_ts IS NULL ORDER BY "order""#
            )
            .bind(space_id)
            .fetch_all(&*self.pool)
            .await?
        } else {
            self.get_space_children(space_id).await?
        };

        for child in children {
            let is_space = self.get_space_by_room(&child.room_id).await?.is_some();

            all_children.push(SpaceChildInfo {
                space_id: child.space_id.clone(),
                room_id: child.room_id.clone(),
                via_servers: child.via_servers.clone(),
                order: child.order.clone(),
                suggested: child.suggested,
                is_space,
                depth: current_depth,
            });

            if is_space {
                if let Some(child_space) = self.get_space_by_room(&child.room_id).await? {
                    Box::pin(self.collect_hierarchy_recursive(
                        &child_space.space_id,
                        current_depth + 1,
                        max_depth,
                        suggested_only,
                        all_children,
                        visited,
                    ))
                    .await?;
                }
            }
        }

        Ok(())
    }

    pub async fn get_space_hierarchy_paginated(
        &self,
        space_id: &str,
        max_depth: i32,
        suggested_only: bool,
        limit: Option<i32>,
        from: Option<&str>,
    ) -> Result<SpaceHierarchyResponse, sqlx::Error> {
        let limit = limit.unwrap_or(100);
        let all_children = self
            .get_recursive_hierarchy(space_id, max_depth, suggested_only)
            .await?;

        let start_index = if let Some(from_token) = from {
            all_children
                .iter()
                .position(|c| c.room_id == from_token)
                .unwrap_or(0)
        } else {
            0
        };

        let end_index = std::cmp::min(start_index + limit as usize, all_children.len());
        let paginated_children: Vec<SpaceChildInfo> = all_children[start_index..end_index].to_vec();

        let mut rooms = Vec::new();
        for child in paginated_children {
            if let Some(room) = self.build_hierarchy_room(&child).await? {
                rooms.push(room);
            }
        }

        let next_batch = if end_index < all_children.len() {
            all_children.get(end_index).map(|c| c.room_id.clone())
        } else {
            None
        };

        Ok(SpaceHierarchyResponse { rooms, next_batch })
    }

    async fn build_hierarchy_room(
        &self,
        child: &SpaceChildInfo,
    ) -> Result<Option<SpaceHierarchyRoom>, sqlx::Error> {
        let children_state = self.get_children_state_events(&child.room_id).await?;

        let room = if child.is_space {
            if let Some(space) = self.get_space_by_room(&child.room_id).await? {
                let join_rule = space.join_rule.clone();
                let visibility = space.visibility.clone();
                let guest_can_join = join_rule == "public";
                Some(SpaceHierarchyRoom {
                    room_id: child.room_id.clone(),
                    name: space.name,
                    topic: space.topic,
                    avatar_url: space.avatar_url,
                    join_rule,
                    world_readable: visibility == "public",
                    guest_can_join,
                    num_joined_members: self.get_space_member_count(&space.space_id).await?,
                    room_type: Some("m.space".to_string()),
                    children_state,
                })
            } else {
                None
            }
        } else {
            Some(SpaceHierarchyRoom {
                room_id: child.room_id.clone(),
                name: None,
                topic: None,
                avatar_url: None,
                join_rule: "invite".to_string(),
                world_readable: false,
                guest_can_join: false,
                num_joined_members: 0,
                room_type: None,
                children_state,
            })
        };

        Ok(room)
    }

    async fn get_children_state_events(
        &self,
        room_id: &str,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let children = self.get_child_spaces(room_id).await?;

        Ok(children
            .into_iter()
            .map(|child| {
                serde_json::json!({
                    "type": "m.space.child",
                    "state_key": child.room_id,
                    "content": {
                        "via": child.via_servers,
                        "order": child.order,
                        "suggested": child.suggested,
                    },
                    "sender": child.added_by,
                    "origin_server_ts": child.added_ts,
                })
            })
            .collect())
    }

    async fn get_space_member_count(&self, space_id: &str) -> Result<i64, sqlx::Error> {
        let count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM space_members WHERE space_id = $1 AND membership = 'join'"#,
        )
        .bind(space_id)
        .fetch_one(&*self.pool)
        .await?;

        Ok(count)
    }

    pub async fn check_user_can_see_space(
        &self,
        space_id: &str,
        user_id: &str,
    ) -> Result<bool, sqlx::Error> {
        let space = self.get_space(space_id).await?;

        match space {
            Some(space) if space.is_public => Ok(true),
            Some(space) => {
                let is_member = self.is_space_member(&space.space_id, user_id).await?;
                Ok(is_member)
            }
            None => Ok(false),
        }
    }

    pub async fn get_parent_spaces(&self, room_id: &str) -> Result<Vec<Space>, sqlx::Error> {
        let children = self.get_child_spaces(room_id).await?;
        let mut parent_spaces = Vec::new();

        for child in children {
            if let Some(space) = self.get_space(&child.space_id).await? {
                parent_spaces.push(space);
            }
        }

        Ok(parent_spaces)
    }

    pub async fn get_space_tree_path(&self, space_id: &str) -> Result<Vec<Space>, sqlx::Error> {
        let mut path = Vec::new();
        let mut current_id = Some(space_id.to_string());

        while let Some(id) = current_id {
            if let Some(space) = self.get_space(&id).await? {
                current_id = space.parent_space_id.clone();
                path.push(space);
            } else {
                break;
            }
        }

        path.reverse();
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_space() -> Space {
        Space {
            space_id: "!test_space:localhost".to_string(),
            room_id: "!test_room:localhost".to_string(),
            name: Some("Test Space".to_string()),
            topic: Some("A test space".to_string()),
            avatar_url: None,
            creator: "@test:localhost".to_string(),
            join_rule: "invite".to_string(),
            visibility: "private".to_string(),
            creation_ts: 1234567890,
            updated_ts: None,
            is_public: false,
            parent_space_id: None,
        }
    }

    fn create_test_space_child() -> SpaceChild {
        SpaceChild {
            id: 1,
            space_id: "!test_space:localhost".to_string(),
            room_id: "!child_room:localhost".to_string(),
            via_servers: vec!["localhost".to_string()],
            order: Some("1".to_string()),
            suggested: false,
            added_by: "@test:localhost".to_string(),
            added_ts: 1234567890,
            removed_ts: None,
        }
    }

    fn create_test_space_member() -> SpaceMember {
        SpaceMember {
            space_id: "!test_space:localhost".to_string(),
            user_id: "@test:localhost".to_string(),
            membership: "join".to_string(),
            joined_ts: 1234567890,
            updated_ts: None,
            left_ts: None,
            inviter: None,
        }
    }

    #[test]
    fn test_space_serialization() {
        let space = create_test_space();
        let json = serde_json::to_string(&space).unwrap();
        let deserialized: Space = serde_json::from_str(&json).unwrap();

        assert_eq!(space.space_id, deserialized.space_id);
        assert_eq!(space.room_id, deserialized.room_id);
        assert_eq!(space.name, deserialized.name);
        assert_eq!(space.topic, deserialized.topic);
        assert_eq!(space.creator, deserialized.creator);
        assert_eq!(space.join_rule, deserialized.join_rule);
        assert_eq!(space.visibility, deserialized.visibility);
        assert_eq!(space.is_public, deserialized.is_public);
    }

    #[test]
    fn test_space_child_serialization() {
        let child = create_test_space_child();
        let json = serde_json::to_string(&child).unwrap();
        let deserialized: SpaceChild = serde_json::from_str(&json).unwrap();

        assert_eq!(child.space_id, deserialized.space_id);
        assert_eq!(child.room_id, deserialized.room_id);
        assert_eq!(child.via_servers, deserialized.via_servers);
        assert_eq!(child.order, deserialized.order);
        assert_eq!(child.suggested, deserialized.suggested);
    }

    #[test]
    fn test_space_member_serialization() {
        let member = create_test_space_member();
        let json = serde_json::to_string(&member).unwrap();
        let deserialized: SpaceMember = serde_json::from_str(&json).unwrap();

        assert_eq!(member.space_id, deserialized.space_id);
        assert_eq!(member.user_id, deserialized.user_id);
        assert_eq!(member.membership, deserialized.membership);
        assert_eq!(member.joined_ts, deserialized.joined_ts);
    }

    #[test]
    fn test_create_space_request() {
        let request = CreateSpaceRequest {
            room_id: "!room:localhost".to_string(),
            name: Some("New Space".to_string()),
            topic: Some("Description".to_string()),
            avatar_url: None,
            creator: "@user:localhost".to_string(),
            join_rule: Some("public".to_string()),
            visibility: Some("public".to_string()),
            is_public: Some(true),
            parent_space_id: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: CreateSpaceRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(request.room_id, deserialized.room_id);
        assert_eq!(request.name, deserialized.name);
        assert_eq!(request.creator, deserialized.creator);
        assert_eq!(request.join_rule, deserialized.join_rule);
    }

    #[test]
    fn test_add_child_request() {
        let request = AddChildRequest {
            space_id: "!space:localhost".to_string(),
            room_id: "!child:localhost".to_string(),
            via_servers: vec!["server1.com".to_string(), "server2.com".to_string()],
            order: Some("001".to_string()),
            suggested: Some(true),
            added_by: "@user:localhost".to_string(),
        };

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: AddChildRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(request.space_id, deserialized.space_id);
        assert_eq!(request.room_id, deserialized.room_id);
        assert_eq!(request.via_servers.len(), deserialized.via_servers.len());
        assert_eq!(request.suggested, deserialized.suggested);
    }

    #[test]
    fn test_space_hierarchy() {
        let space = create_test_space();
        let child = create_test_space_child();
        let member = create_test_space_member();

        let hierarchy = SpaceHierarchy {
            space: space.clone(),
            children: vec![child],
            members: vec![member],
        };

        let json = serde_json::to_string(&hierarchy).unwrap();
        let deserialized: SpaceHierarchy = serde_json::from_str(&json).unwrap();

        assert_eq!(hierarchy.space.space_id, deserialized.space.space_id);
        assert_eq!(hierarchy.children.len(), deserialized.children.len());
        assert_eq!(hierarchy.members.len(), deserialized.members.len());
    }

    #[test]
    fn test_space_summary() {
        let summary = SpaceSummary {
            space_id: "!space:localhost".to_string(),
            summary: serde_json::json!({"key": "value"}),
            children_count: 5,
            member_count: 10,
            updated_ts: 1234567890,
        };

        let json = serde_json::to_string(&summary).unwrap();
        let deserialized: SpaceSummary = serde_json::from_str(&json).unwrap();

        assert_eq!(summary.space_id, deserialized.space_id);
        assert_eq!(summary.children_count, deserialized.children_count);
        assert_eq!(summary.member_count, deserialized.member_count);
    }

    #[test]
    fn test_space_event() {
        let event = SpaceEvent {
            event_id: "$event:localhost".to_string(),
            space_id: "!space:localhost".to_string(),
            event_type: "m.space.child".to_string(),
            sender: "@user:localhost".to_string(),
            content: serde_json::json!({"room_id": "!child:localhost"}),
            state_key: Some("!child:localhost".to_string()),
            origin_server_ts: 1234567890,
            processed_ts: None,
        };

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: SpaceEvent = serde_json::from_str(&json).unwrap();

        assert_eq!(event.event_id, deserialized.event_id);
        assert_eq!(event.event_type, deserialized.event_type);
        assert_eq!(event.sender, deserialized.sender);
    }
}
