use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, Row};
use std::collections::HashMap;
use std::sync::Arc;

use crate::trigram_ranking::TrigramRanking;

fn escape_like_pattern(input: &str) -> String {
    input.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Space {
    pub space_id: String,
    pub room_id: String,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub avatar_url: Option<String>,
    pub creator: String,
    pub join_rule: String,
    pub visibility: Option<String>,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
    pub is_public: bool,
    pub parent_space_id: Option<String>,
    pub room_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SpaceChild {
    pub id: i64,
    pub space_id: String,
    pub room_id: String,
    pub sender: String,
    pub is_suggested: bool,
    pub via_servers: Vec<String>,
    pub added_ts: i64,
    pub order: Option<String>,
    pub suggested: Option<bool>,
    pub added_by: Option<String>,
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
    pub id: i64,
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
    pub sender: String,
    pub is_suggested: bool,
    pub via_servers: Vec<String>,
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
    pub children: Vec<Self>,
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
    pub is_suggested: bool,
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
            request.room_id.split(':').next_back().unwrap_or("localhost")
        );

        let space = sqlx::query_as::<_, Space>(
            r"
            INSERT INTO spaces (
                space_id, room_id, name, topic, avatar_url, creator,
                join_rule, visibility, is_public, created_ts, parent_space_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            RETURNING space_id, room_id, name, topic, avatar_url, creator, join_rule, visibility, created_ts, updated_ts, is_public, parent_space_id, room_type
            ",
        )
        .bind(&space_id)
        .bind(&request.room_id)
        .bind(&request.name)
        .bind(&request.topic)
        .bind(&request.avatar_url)
        .bind(&request.creator)
        .bind(request.join_rule.unwrap_or_else(|| "invite".to_string()))
        .bind(request.visibility.unwrap_or_else(|| "private".to_string()))
        .bind(request.is_public.unwrap_or(false))
        .bind(now)
        .bind(&request.parent_space_id)
        .fetch_one(&*self.pool)
        .await?;

        self.add_space_member(&space.space_id, &request.creator, "join", None).await?;

        Ok(space)
    }

    pub async fn get_space(&self, space_id: &str) -> Result<Option<Space>, sqlx::Error> {
        sqlx::query_as::<_, Space>(r"SELECT space_id, room_id, name, topic, avatar_url, creator, join_rule, visibility, created_ts, updated_ts, is_public, parent_space_id, room_type FROM spaces WHERE space_id = $1")
            .bind(space_id)
            .fetch_optional(&*self.pool)
            .await
    }

    pub async fn get_space_by_room(&self, room_id: &str) -> Result<Option<Space>, sqlx::Error> {
        sqlx::query_as::<_, Space>(r"SELECT space_id, room_id, name, topic, avatar_url, creator, join_rule, visibility, created_ts, updated_ts, is_public, parent_space_id, room_type FROM spaces WHERE room_id = $1")
            .bind(room_id)
            .fetch_optional(&*self.pool)
            .await
    }

    pub async fn get_spaces_by_rooms_batch(&self, room_ids: &[String]) -> Result<HashMap<String, Space>, sqlx::Error> {
        if room_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let spaces = sqlx::query_as::<_, Space>(
            r"SELECT space_id, room_id, name, topic, avatar_url, creator, join_rule, visibility, created_ts, updated_ts, is_public, parent_space_id, room_type FROM spaces WHERE room_id = ANY($1)",
        )
        .bind(room_ids)
        .fetch_all(&*self.pool)
        .await?;
        let mut map = HashMap::with_capacity(spaces.len());
        for space in spaces {
            map.insert(space.room_id.clone(), space);
        }
        Ok(map)
    }

    pub async fn update_space(&self, space_id: &str, request: &UpdateSpaceRequest) -> Result<Space, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query_as::<_, Space>(
            r"
            UPDATE spaces SET
                name = COALESCE($2, name),
                topic = COALESCE($3, topic),
                avatar_url = COALESCE($4, avatar_url),
                join_rule = COALESCE($5, join_rule),
                visibility = COALESCE($6, visibility),
                is_public = COALESCE($7, is_public),
                updated_ts = $8
            WHERE space_id = $1
            RETURNING space_id, room_id, name, topic, avatar_url, creator, join_rule, visibility, created_ts, updated_ts, is_public, parent_space_id, room_type
            ",
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
        sqlx::query(r"DELETE FROM spaces WHERE space_id = $1").bind(space_id).execute(&*self.pool).await?;
        Ok(())
    }

    pub async fn add_child(&self, request: AddChildRequest) -> Result<SpaceChild, sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        let via_servers =
            serde_json::Value::Array(request.via_servers.iter().cloned().map(serde_json::Value::String).collect());

        sqlx::query_as::<_, SpaceChild>(
            r#"
            INSERT INTO space_children (
                space_id, room_id, sender, is_suggested, via_servers, added_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (space_id, room_id) DO UPDATE SET
                via_servers = EXCLUDED.via_servers,
                is_suggested = EXCLUDED.is_suggested,
                sender = EXCLUDED.sender,
                added_ts = EXCLUDED.added_ts
            RETURNING
                id,
                space_id,
                room_id,
                sender,
                is_suggested,
                ARRAY(SELECT jsonb_array_elements_text(via_servers)) as via_servers,
                added_ts,
                NULL::TEXT as "order",
                NULL::BOOLEAN as suggested,
                NULL::TEXT as added_by,
                NULL::BIGINT as removed_ts
            "#,
        )
        .bind(&request.space_id)
        .bind(&request.room_id)
        .bind(&request.sender)
        .bind(request.is_suggested)
        .bind(&via_servers)
        .bind(now)
        .fetch_one(&*self.pool)
        .await
    }

    pub async fn remove_child(&self, space_id: &str, room_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(r"DELETE FROM space_children WHERE space_id = $1 AND room_id = $2")
            .bind(space_id)
            .bind(room_id)
            .execute(&*self.pool)
            .await?;

        Ok(())
    }

    pub async fn get_space_children(&self, space_id: &str) -> Result<Vec<SpaceChild>, sqlx::Error> {
        sqlx::query_as::<_, SpaceChild>(
            r#"
            SELECT
                id,
                space_id,
                room_id,
                sender,
                is_suggested,
                ARRAY(SELECT jsonb_array_elements_text(via_servers)) as via_servers,
                added_ts,
                NULL::TEXT as "order",
                NULL::BOOLEAN as suggested,
                NULL::TEXT as added_by,
                NULL::BIGINT as removed_ts
            FROM space_children
            WHERE space_id = $1
            ORDER BY added_ts
            "#,
        )
        .bind(space_id)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn get_child_spaces(&self, room_id: &str) -> Result<Vec<SpaceChild>, sqlx::Error> {
        sqlx::query_as::<_, SpaceChild>(
            r#"
            SELECT
                id,
                space_id,
                room_id,
                sender,
                is_suggested,
                ARRAY(SELECT jsonb_array_elements_text(via_servers)) as via_servers,
                added_ts,
                NULL::TEXT as "order",
                NULL::BOOLEAN as suggested,
                NULL::TEXT as added_by,
                NULL::BIGINT as removed_ts
            FROM space_children
            WHERE room_id = $1
            "#,
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
            r"
            INSERT INTO space_members (space_id, user_id, membership, joined_ts, inviter)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (space_id, user_id) DO UPDATE SET
                membership = EXCLUDED.membership,
                joined_ts = EXCLUDED.joined_ts,
                inviter = EXCLUDED.inviter,
                left_ts = NULL,
                updated_ts = $4
            RETURNING space_id, user_id, membership, joined_ts, updated_ts, left_ts, inviter
            ",
        )
        .bind(space_id)
        .bind(user_id)
        .bind(membership)
        .bind(now)
        .bind(inviter)
        .fetch_one(&*self.pool)
        .await
    }

    pub async fn remove_space_member(&self, space_id: &str, user_id: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query(
            r"UPDATE space_members SET membership = 'leave', left_ts = $3, updated_ts = $3 WHERE space_id = $1 AND user_id = $2"
        )
        .bind(space_id)
        .bind(user_id)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_space_members(&self, space_id: &str) -> Result<Vec<SpaceMember>, sqlx::Error> {
        sqlx::query_as::<_, SpaceMember>(r"SELECT space_id, user_id, membership, joined_ts, updated_ts, left_ts, inviter FROM space_members WHERE space_id = $1 AND membership = 'join'")
            .bind(space_id)
            .fetch_all(&*self.pool)
            .await
    }

    pub async fn get_space_member(&self, space_id: &str, user_id: &str) -> Result<Option<SpaceMember>, sqlx::Error> {
        sqlx::query_as::<_, SpaceMember>(r"SELECT space_id, user_id, membership, joined_ts, updated_ts, left_ts, inviter FROM space_members WHERE space_id = $1 AND user_id = $2")
            .bind(space_id)
            .bind(user_id)
            .fetch_optional(&*self.pool)
            .await
    }

    pub async fn get_user_spaces(&self, user_id: &str) -> Result<Vec<Space>, sqlx::Error> {
        sqlx::query_as::<_, Space>(
            r"
            SELECT s.space_id, s.room_id, s.name, s.topic, s.avatar_url, s.creator, s.join_rule, s.visibility, s.created_ts, s.updated_ts, s.is_public, s.parent_space_id, s.room_type FROM spaces s
            JOIN space_members sm ON s.space_id = sm.space_id
            WHERE sm.user_id = $1 AND sm.membership = 'join'
            ORDER BY s.created_ts DESC
            ",
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn get_public_spaces(
        &self,
        limit: i64,
        cursor_created_ts: Option<i64>,
        cursor_space_id: Option<&str>,
    ) -> Result<Vec<Space>, sqlx::Error> {
        sqlx::query_as::<_, Space>(r"SELECT space_id, room_id, name, topic, avatar_url, creator, join_rule, visibility, created_ts, updated_ts, is_public, parent_space_id, room_type
            FROM spaces
            WHERE is_public = TRUE
              AND (($2::BIGINT IS NULL AND $3::TEXT IS NULL)
                OR created_ts < $2
                OR (created_ts = $2 AND space_id < $3))
            ORDER BY created_ts DESC, space_id DESC
            LIMIT $1")
        .bind(limit)
        .bind(cursor_created_ts)
        .bind(cursor_space_id)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn get_space_hierarchy(&self, space_id: &str, _max_depth: i32) -> Result<SpaceHierarchy, sqlx::Error> {
        let space = self.get_space(space_id).await?.ok_or_else(|| sqlx::Error::RowNotFound)?;

        let children = self.get_space_children(space_id).await?;
        let members = self.get_space_members(space_id).await?;

        Ok(SpaceHierarchy { space, children, members })
    }

    pub async fn get_space_summary(&self, space_id: &str) -> Result<Option<SpaceSummary>, sqlx::Error> {
        sqlx::query_as::<_, SpaceSummary>(r"SELECT id, space_id, summary, children_count, member_count, updated_ts FROM space_summaries WHERE space_id = $1")
            .bind(space_id)
            .fetch_optional(&*self.pool)
            .await
    }

    pub async fn update_space_summary(&self, space_id: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let children_count: i64 = sqlx::query_scalar(r"SELECT COUNT(*) FROM space_children WHERE space_id = $1")
            .bind(space_id)
            .fetch_one(&*self.pool)
            .await?;

        let member_count: i64 =
            sqlx::query_scalar(r"SELECT COUNT(*) FROM space_members WHERE space_id = $1 AND membership = 'join'")
                .bind(space_id)
                .fetch_one(&*self.pool)
                .await?;

        let summary = serde_json::json!({
            "children_count": children_count,
            "member_count": member_count,
        });

        sqlx::query(
            r"
            INSERT INTO space_summaries (space_id, summary, children_count, member_count, updated_ts)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (space_id) DO UPDATE SET
                summary = EXCLUDED.summary,
                children_count = EXCLUDED.children_count,
                member_count = EXCLUDED.member_count,
                updated_ts = EXCLUDED.updated_ts
            ",
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
            r"
            INSERT INTO space_events (event_id, space_id, event_type, sender, content, state_key, origin_server_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING event_id, space_id, event_type, sender, content, state_key, origin_server_ts, processed_ts
            ",
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
                    r"SELECT event_id, space_id, event_type, sender, content, state_key, origin_server_ts, processed_ts FROM space_events WHERE space_id = $1 AND event_type = $2 ORDER BY origin_server_ts DESC LIMIT $3"
                )
                .bind(space_id)
                .bind(et)
                .bind(limit)
                .fetch_all(&*self.pool)
                .await
            }
            None => {
                sqlx::query_as::<_, SpaceEvent>(
                    r"SELECT event_id, space_id, event_type, sender, content, state_key, origin_server_ts, processed_ts FROM space_events WHERE space_id = $1 ORDER BY origin_server_ts DESC LIMIT $2"
                )
                .bind(space_id)
                .bind(limit)
                .fetch_all(&*self.pool)
                .await
            }
        }
    }

    pub async fn search_spaces(
        &self,
        query: &str,
        limit: i64,
        user_id: Option<&str>,
    ) -> Result<Vec<Space>, sqlx::Error> {
        let normalized = query.trim();
        if normalized.is_empty() {
            return Ok(Vec::new());
        }

        let escaped = escape_like_pattern(normalized);
        let exact_pattern = escaped.clone();
        let prefix_pattern = format!("{escaped}%");
        let contains_pattern = format!("%{escaped}%");

        match user_id {
            Some(uid) => {
                let name_rank = TrigramRanking::new("s.name", "spaces s");
                let topic_rank = TrigramRanking::new("s.topic", "spaces s");

                let sql = format!(
                    r"
                    WITH visible_spaces AS (
                        SELECT DISTINCT s.space_id
                        FROM spaces s
                        LEFT JOIN space_members sm
                            ON sm.space_id = s.space_id AND sm.user_id = $5 AND sm.membership = 'join'
                        WHERE s.is_public = TRUE OR s.creator = $5 OR sm.user_id IS NOT NULL
                    ),
                    candidate_matches AS (
                        SELECT
                            space_id,
                            MIN(match_priority) AS match_priority,
                            MAX(match_similarity) AS match_similarity
                        FROM (
                            SELECT s.space_id,
                                {},
                                COALESCE({}, 0.0) AS match_similarity
                            FROM spaces s
                            JOIN visible_spaces vs ON vs.space_id = s.space_id
                            WHERE s.name IS NOT NULL
                              AND ({})

                            UNION ALL

                            SELECT s.space_id,
                                {},
                                COALESCE({}, 0.0) AS match_similarity
                            FROM spaces s
                            JOIN visible_spaces vs ON vs.space_id = s.space_id
                            WHERE s.topic IS NOT NULL
                              AND ({})
                        ) AS matches
                        GROUP BY space_id
                    )
                    SELECT
                        s.space_id,
                        s.room_id,
                        s.name,
                        s.topic,
                        s.avatar_url,
                        s.creator,
                        s.join_rule,
                        s.visibility,
                        s.created_ts,
                        s.updated_ts,
                        s.is_public,
                        s.parent_space_id,
                        s.room_type
                    FROM candidate_matches cm
                    JOIN spaces s ON s.space_id = cm.space_id
                    ORDER BY
                        cm.match_priority ASC,
                        cm.match_similarity DESC,
                        s.created_ts DESC
                    LIMIT $6
                    ",
                    name_rank.match_priority_case(),
                    name_rank.similarity_expr(),
                    name_rank.where_clause(),
                    topic_rank.match_priority_case(),
                    topic_rank.similarity_expr(),
                    topic_rank.where_clause(),
                );

                sqlx::query_as::<_, Space>(&sql)
                    .bind(&exact_pattern)
                    .bind(&prefix_pattern)
                    .bind(&contains_pattern)
                    .bind(normalized)
                    .bind(uid)
                    .bind(limit)
                    .fetch_all(&*self.pool)
                    .await
            }
            None => {
                let name_rank = TrigramRanking::new("name", "spaces");
                let topic_rank = TrigramRanking::new("topic", "spaces");

                let sql = format!(
                    r"
                    WITH candidate_matches AS (
                        SELECT
                            space_id,
                            MIN(match_priority) AS match_priority,
                            MAX(match_similarity) AS match_similarity
                        FROM (
                            {}
                            UNION ALL
                            {}
                        ) AS matches
                        GROUP BY space_id
                    )
                    SELECT
                        s.space_id,
                        s.room_id,
                        s.name,
                        s.topic,
                        s.avatar_url,
                        s.creator,
                        s.join_rule,
                        s.visibility,
                        s.created_ts,
                        s.updated_ts,
                        s.is_public,
                        s.parent_space_id,
                        s.room_type
                    FROM candidate_matches cm
                    JOIN spaces s ON s.space_id = cm.space_id
                    ORDER BY
                        cm.match_priority ASC,
                        cm.match_similarity DESC,
                        s.created_ts DESC
                    LIMIT $5
                    ",
                    name_rank.column_match_subquery("space_id", Some("is_public = TRUE"), true),
                    topic_rank.column_match_subquery("space_id", Some("is_public = TRUE"), true),
                );

                sqlx::query_as::<_, Space>(&sql)
                    .bind(&exact_pattern)
                    .bind(&prefix_pattern)
                    .bind(&contains_pattern)
                    .bind(normalized)
                    .bind(limit)
                    .fetch_all(&*self.pool)
                    .await
            }
        }
    }

    pub async fn is_space_member(&self, space_id: &str, user_id: &str) -> Result<bool, sqlx::Error> {
        let count: i64 = sqlx::query_scalar(
            r"SELECT COUNT(*) FROM space_members WHERE space_id = $1 AND user_id = $2 AND membership = 'join'",
        )
        .bind(space_id)
        .bind(user_id)
        .fetch_one(&*self.pool)
        .await?;

        Ok(count > 0)
    }

    pub async fn get_space_statistics(&self, limit: i64) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        sqlx::query(r"SELECT space_id, name, is_public, child_room_count, member_count, created_ts, updated_ts FROM space_statistics ORDER BY member_count DESC LIMIT $1")
            .bind(limit)
            .fetch_all(&*self.pool)
            .await
            .map(
                |rows| {
                    rows.into_iter()
                        .map(|row| {
                            serde_json::json!({
                                "space_id": row.get::<String, _>("space_id"),
                                "name": row.get::<Option<String>, _>("name"),
                                "is_public": row.get::<bool, _>("is_public"),
                                "child_room_count": row.get::<i64, _>("child_room_count"),
                                "member_count": row.get::<i64, _>("member_count"),
                                "created_ts": row.get::<i64, _>("created_ts"),
                                "updated_ts": row.get::<Option<i64>, _>("updated_ts"),
                            })
                        })
                        .collect()
                },
            )
    }

    pub async fn get_recursive_hierarchy(
        &self,
        space_id: &str,
        max_depth: i32,
        suggested_only: bool,
    ) -> Result<Vec<SpaceChildInfo>, sqlx::Error> {
        let mut all_children = Vec::new();
        let mut visited = std::collections::HashSet::new();
        self.collect_hierarchy_recursive(space_id, 0, max_depth, suggested_only, &mut all_children, &mut visited)
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
                r#"
                SELECT
                    id,
                    space_id,
                    room_id,
                    sender,
                    is_suggested,
                    via_servers,
                    added_ts,
                    NULL::TEXT as "order",
                    NULL::BOOLEAN as suggested,
                    NULL::TEXT as added_by,
                    NULL::BIGINT as removed_ts
                FROM space_children
                WHERE space_id = $1 AND is_suggested = TRUE
                ORDER BY added_ts
                "#,
            )
            .bind(space_id)
            .fetch_all(&*self.pool)
            .await?
        } else {
            self.get_space_children(space_id).await?
        };

        let child_room_ids: Vec<String> = children.iter().map(|c| c.room_id.clone()).collect();
        let spaces_map = self.get_spaces_by_rooms_batch(&child_room_ids).await?;

        for child in children {
            let child_space = spaces_map.get(&child.room_id);
            let is_space = child_space.is_some();

            all_children.push(SpaceChildInfo {
                space_id: child.space_id.clone(),
                room_id: child.room_id.clone(),
                via_servers: child.via_servers.clone(),
                is_suggested: child.is_suggested,
                is_space,
                depth: current_depth,
            });

            if is_space {
                if let Some(child_space) = child_space {
                    Box::pin(self.collect_hierarchy_recursive(
                        &child_space.space_id.clone(),
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
        let all_children = self.get_recursive_hierarchy(space_id, max_depth, suggested_only).await?;

        let start_index = if let Some(from_token) = from {
            all_children.iter().position(|c| c.room_id == from_token).unwrap_or(0)
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

        let next_batch =
            if end_index < all_children.len() { all_children.get(end_index).map(|c| c.room_id.clone()) } else { None };

        Ok(SpaceHierarchyResponse { rooms, next_batch })
    }

    async fn build_hierarchy_room(&self, child: &SpaceChildInfo) -> Result<Option<SpaceHierarchyRoom>, sqlx::Error> {
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
                    world_readable: visibility.as_deref() == Some("public"),
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

    async fn get_children_state_events(&self, room_id: &str) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let children = if let Some(space) = self.get_space_by_room(room_id).await? {
            self.get_space_children(&space.space_id).await?
        } else {
            Vec::new()
        };

        Ok(children
            .into_iter()
            .map(|child| {
                serde_json::json!({
                    "type": "m.space.child",
                    "state_key": child.room_id,
                    "content": {
                        "via": child.via_servers,
                        "suggested": child.is_suggested,
                    },
                    "sender": child.sender,
                    "origin_server_ts": child.added_ts,
                })
            })
            .collect())
    }

    async fn get_space_member_count(&self, space_id: &str) -> Result<i64, sqlx::Error> {
        let count: i64 =
            sqlx::query_scalar(r"SELECT COUNT(*) FROM space_members WHERE space_id = $1 AND membership = 'join'")
                .bind(space_id)
                .fetch_one(&*self.pool)
                .await?;

        Ok(count)
    }

    pub async fn check_user_can_see_space(&self, space_id: &str, user_id: &str) -> Result<bool, sqlx::Error> {
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

    pub async fn resolve_space_id(&self, identifier: &str) -> Result<Option<String>, sqlx::Error> {
        let result: Option<(String,)> = sqlx::query_as(
            r"SELECT space_id FROM spaces WHERE space_id = $1 OR room_id = $1 ORDER BY CASE WHEN space_id = $1 THEN 0 ELSE 1 END LIMIT 1",
        )
        .bind(identifier)
        .fetch_optional(&*self.pool)
        .await?;
        Ok(result.map(|r| r.0))
    }

    pub async fn get_all_spaces_for_admin(&self) -> Result<Vec<Space>, sqlx::Error> {
        sqlx::query_as::<_, Space>(
            r"SELECT space_id, room_id, name, topic, avatar_url, creator, join_rule, visibility, created_ts, updated_ts, is_public, parent_space_id, room_type FROM spaces ORDER BY created_ts DESC",
        )
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn get_space_by_identifier(&self, identifier: &str) -> Result<Option<Space>, sqlx::Error> {
        sqlx::query_as::<_, Space>(
            r"SELECT space_id, room_id, name, topic, avatar_url, creator, join_rule, visibility, created_ts, updated_ts, is_public, parent_space_id, room_type FROM spaces WHERE space_id = $1 OR room_id = $1 ORDER BY CASE WHEN space_id = $1 THEN 0 ELSE 1 END LIMIT 1",
        )
        .bind(identifier)
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn get_space_user_ids(&self, space_id: &str) -> Result<Vec<String>, sqlx::Error> {
        let rows: Vec<(String,)> =
            sqlx::query_as(r"SELECT user_id FROM space_members WHERE space_id = $1 AND membership = 'join'")
                .bind(space_id)
                .fetch_all(&*self.pool)
                .await?;
        Ok(rows.into_iter().map(|r| r.0).collect())
    }

    pub async fn get_space_room_ids(&self, space_id: &str) -> Result<Vec<String>, sqlx::Error> {
        let rows: Vec<(String,)> = sqlx::query_as(r"SELECT room_id FROM space_children WHERE space_id = $1")
            .bind(space_id)
            .fetch_all(&*self.pool)
            .await?;
        Ok(rows.into_iter().map(|r| r.0).collect())
    }

    pub async fn get_space_member_and_child_count(&self, space_id: &str) -> Result<(i64, i64), sqlx::Error> {
        let member_count: i64 =
            sqlx::query_scalar(r"SELECT COUNT(*) FROM space_members WHERE space_id = $1 AND membership = 'join'")
                .bind(space_id)
                .fetch_one(&*self.pool)
                .await?;

        let child_count: i64 = sqlx::query_scalar(r"SELECT COUNT(*) FROM space_children WHERE space_id = $1")
            .bind(space_id)
            .fetch_one(&*self.pool)
            .await?;

        Ok((member_count, child_count))
    }

    pub async fn delete_space_returning_count(&self, space_id: &str) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(r"DELETE FROM spaces WHERE space_id = $1").bind(space_id).execute(&*self.pool).await?;
        Ok(result.rows_affected())
    }

    pub async fn get_space_children_paginated(
        &self,
        space_id: &str,
        limit: i64,
        from_added_ts: Option<i64>,
        from_id: Option<i64>,
    ) -> Result<Vec<SpaceChild>, sqlx::Error> {
        if let (Some(ts), Some(id)) = (from_added_ts, from_id) {
            sqlx::query_as::<_, SpaceChild>(
                r#"
                SELECT
                    id,
                    space_id,
                    room_id,
                    sender,
                    is_suggested,
                    COALESCE(ARRAY(SELECT jsonb_array_elements_text(via_servers)), '{}') AS via_servers,
                    added_ts,
                    NULL::TEXT AS "order",
                    NULL::BOOLEAN AS suggested,
                    NULL::TEXT AS added_by,
                    NULL::BIGINT AS removed_ts
                FROM space_children
                WHERE space_id = $1 AND (added_ts > $2 OR (added_ts = $2 AND id > $3))
                ORDER BY added_ts ASC, id ASC
                LIMIT $4
                "#,
            )
            .bind(space_id)
            .bind(ts)
            .bind(id)
            .bind(limit)
            .fetch_all(&*self.pool)
            .await
        } else {
            sqlx::query_as::<_, SpaceChild>(
                r#"
                SELECT
                    id,
                    space_id,
                    room_id,
                    sender,
                    is_suggested,
                    COALESCE(ARRAY(SELECT jsonb_array_elements_text(via_servers)), '{}') AS via_servers,
                    added_ts,
                    NULL::TEXT AS "order",
                    NULL::BOOLEAN AS suggested,
                    NULL::TEXT AS added_by,
                    NULL::BIGINT AS removed_ts
                FROM space_children
                WHERE space_id = $1
                ORDER BY added_ts ASC, id ASC
                LIMIT $2
                "#,
            )
            .bind(space_id)
            .bind(limit)
            .fetch_all(&*self.pool)
            .await
        }
    }

    pub async fn get_space_members_paginated(
        &self,
        space_id: &str,
        limit: i64,
        from_joined_ts: Option<i64>,
        from_user_id: Option<&str>,
    ) -> Result<Vec<SpaceMember>, sqlx::Error> {
        if let (Some(ts), Some(user_id)) = (from_joined_ts, from_user_id) {
            sqlx::query_as::<_, SpaceMember>(
                r#"
                SELECT space_id, user_id, membership, joined_ts, updated_ts, left_ts, inviter
                FROM space_members
                WHERE space_id = $1 AND membership = 'join' AND (joined_ts > $2 OR (joined_ts = $2 AND user_id > $3))
                ORDER BY joined_ts ASC, user_id ASC
                LIMIT $4
                "#,
            )
            .bind(space_id)
            .bind(ts)
            .bind(user_id)
            .bind(limit)
            .fetch_all(&*self.pool)
            .await
        } else {
            sqlx::query_as::<_, SpaceMember>(
                r#"
                SELECT space_id, user_id, membership, joined_ts, updated_ts, left_ts, inviter
                FROM space_members
                WHERE space_id = $1 AND membership = 'join'
                ORDER BY joined_ts ASC, user_id ASC
                LIMIT $2
                "#,
            )
            .bind(space_id)
            .bind(limit)
            .fetch_all(&*self.pool)
            .await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_space() -> Space {
        Space {
            space_id: "!test_space:localhost".to_string(),
            room_id: "!test_space:localhost".to_string(),
            name: Some("Test Space".to_string()),
            topic: Some("A test space".to_string()),
            avatar_url: None,
            creator: "@test:localhost".to_string(),
            join_rule: "invite".to_string(),
            visibility: Some("private".to_string()),
            created_ts: 1234567890,
            updated_ts: None,
            is_public: false,
            parent_space_id: None,
            room_type: None,
        }
    }

    fn create_test_space_child() -> SpaceChild {
        SpaceChild {
            id: 1,
            space_id: "!test_space:localhost".to_string(),
            room_id: "!child_room:localhost".to_string(),
            sender: "@test:localhost".to_string(),
            is_suggested: false,
            via_servers: vec!["localhost".to_string()],
            added_ts: 1234567890,
            order: None,
            suggested: None,
            added_by: None,
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
        assert_eq!(child.sender, deserialized.sender);
        assert_eq!(child.is_suggested, deserialized.is_suggested);
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
            sender: "@user:localhost".to_string(),
            is_suggested: true,
            via_servers: vec!["server1.com".to_string(), "server2.com".to_string()],
        };

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: AddChildRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(request.space_id, deserialized.space_id);
        assert_eq!(request.room_id, deserialized.room_id);
        assert_eq!(request.via_servers.len(), deserialized.via_servers.len());
        assert_eq!(request.is_suggested, deserialized.is_suggested);
    }

    #[test]
    fn test_space_hierarchy() {
        let space = create_test_space();
        let child = create_test_space_child();
        let member = create_test_space_member();

        let hierarchy = SpaceHierarchy { space, children: vec![child], members: vec![member] };

        let json = serde_json::to_string(&hierarchy).unwrap();
        let deserialized: SpaceHierarchy = serde_json::from_str(&json).unwrap();

        assert_eq!(hierarchy.space.space_id, deserialized.space.space_id);
        assert_eq!(hierarchy.children.len(), deserialized.children.len());
        assert_eq!(hierarchy.members.len(), deserialized.members.len());
    }

    #[test]
    fn test_space_summary() {
        let summary = SpaceSummary {
            id: 1,
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

#[cfg(test)]
mod db_tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;

    async fn test_pool() -> Arc<PgPool> {
        let db_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
        let pool =
            PgPoolOptions::new().max_connections(2).connect(&db_url).await.expect("Failed to connect to test database");
        Arc::new(pool)
    }

    /// Clean up any leftover data from previous test runs.
    async fn cleanup(pool: &Arc<PgPool>, space_id: &str) {
        let _ = sqlx::query(r"DELETE FROM space_events WHERE space_id = $1").bind(space_id).execute(&**pool).await;
        let _ = sqlx::query(r"DELETE FROM space_children WHERE space_id = $1").bind(space_id).execute(&**pool).await;
        let _ = sqlx::query(r"DELETE FROM space_members WHERE space_id = $1").bind(space_id).execute(&**pool).await;
        let _ = sqlx::query(r"DELETE FROM space_summaries WHERE space_id = $1").bind(space_id).execute(&**pool).await;
        let _ = sqlx::query(r"DELETE FROM spaces WHERE space_id = $1").bind(space_id).execute(&**pool).await;
    }

    // === Test 1: create_space ===
    #[tokio::test]
    async fn test_create_space_returns_valid_space() {
        let pool = test_pool().await;
        let storage = SpaceStorage::new(&pool);
        let room_id = format!("!space_create_{}:example.com", uuid::Uuid::new_v4());

        let request = CreateSpaceRequest {
            room_id: room_id.clone(),
            name: Some("Test Space".to_string()),
            topic: Some("A test space".to_string()),
            avatar_url: None,
            creator: "@creator:example.com".to_string(),
            join_rule: None,
            visibility: None,
            is_public: Some(true),
            parent_space_id: None,
        };

        let space = storage.create_space(request).await.expect("create_space should succeed");
        assert!(!space.space_id.is_empty());
        assert_eq!(space.name, Some("Test Space".to_string()));
        assert_eq!(space.topic, Some("A test space".to_string()));
        assert!(space.is_public);

        cleanup(&pool, &space.space_id).await;
    }

    // === Test 2: get_space (found) ===
    #[tokio::test]
    async fn test_get_space_found() {
        let pool = test_pool().await;
        let storage = SpaceStorage::new(&pool);
        let room_id = format!("!space_get_{}:example.com", uuid::Uuid::new_v4());

        let request = CreateSpaceRequest {
            room_id: room_id.clone(),
            name: Some("Get Test".to_string()),
            topic: None,
            avatar_url: None,
            creator: "@g:example.com".to_string(),
            join_rule: None,
            visibility: None,
            is_public: None,
            parent_space_id: None,
        };
        let created = storage.create_space(request).await.unwrap();

        let found = storage.get_space(&created.space_id).await.expect("get_space should succeed");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, Some("Get Test".to_string()));

        cleanup(&pool, &created.space_id).await;
    }

    // === Test 3: get_space (not found) ===
    #[tokio::test]
    async fn test_get_space_not_found() {
        let pool = test_pool().await;
        let storage = SpaceStorage::new(&pool);
        let result = storage.get_space("!nonexistent_space:example.com").await.expect("get_space should succeed");
        assert!(result.is_none());
    }

    // === Test 4: get_space_by_room ===
    #[tokio::test]
    async fn test_get_space_by_room() {
        let pool = test_pool().await;
        let storage = SpaceStorage::new(&pool);
        let room_id = format!("!space_by_room_{}:example.com", uuid::Uuid::new_v4());

        let request = CreateSpaceRequest {
            room_id: room_id.clone(),
            name: Some("Room Lookup".to_string()),
            topic: None,
            avatar_url: None,
            creator: "@r:example.com".to_string(),
            join_rule: None,
            visibility: None,
            is_public: None,
            parent_space_id: None,
        };
        let created = storage.create_space(request).await.unwrap();

        let found = storage.get_space_by_room(&room_id).await.expect("get_space_by_room should succeed");
        assert!(found.is_some());
        assert_eq!(found.unwrap().space_id, created.space_id);

        cleanup(&pool, &created.space_id).await;
    }

    // === Test 5: update_space ===
    #[tokio::test]
    async fn test_update_space() {
        let pool = test_pool().await;
        let storage = SpaceStorage::new(&pool);
        let room_id = format!("!space_update_{}:example.com", uuid::Uuid::new_v4());

        let request = CreateSpaceRequest {
            room_id: room_id.clone(),
            name: Some("Original".to_string()),
            topic: None,
            avatar_url: None,
            creator: "@u:example.com".to_string(),
            join_rule: None,
            visibility: None,
            is_public: None,
            parent_space_id: None,
        };
        let created = storage.create_space(request).await.unwrap();

        let update = UpdateSpaceRequest::new().name("Updated Name").topic("Updated Topic");
        let updated = storage.update_space(&created.space_id, &update).await.expect("update_space should succeed");
        assert_eq!(updated.name, Some("Updated Name".to_string()));
        assert_eq!(updated.topic, Some("Updated Topic".to_string()));

        cleanup(&pool, &created.space_id).await;
    }

    // === Test 6: delete_space ===
    #[tokio::test]
    async fn test_delete_space() {
        let pool = test_pool().await;
        let storage = SpaceStorage::new(&pool);
        let room_id = format!("!space_del_{}:example.com", uuid::Uuid::new_v4());

        let request = CreateSpaceRequest {
            room_id: room_id.clone(),
            name: Some("Delete Me".to_string()),
            topic: None,
            avatar_url: None,
            creator: "@d:example.com".to_string(),
            join_rule: None,
            visibility: None,
            is_public: None,
            parent_space_id: None,
        };
        let created = storage.create_space(request).await.unwrap();

        storage.delete_space(&created.space_id).await.expect("delete_space should succeed");
        let found = storage.get_space(&created.space_id).await.unwrap();
        assert!(found.is_none());

        // Clean up related records (delete_space only removes from spaces table)
        cleanup(&pool, &created.space_id).await;
    }

    // === Test 7: space children CRUD ===
    #[tokio::test]
    async fn test_space_children_crud() {
        let pool = test_pool().await;
        let storage = SpaceStorage::new(&pool);
        let room_id = format!("!sp_child_{}:example.com", uuid::Uuid::new_v4());
        let child_room_id = format!("!childroom_{}:example.com", uuid::Uuid::new_v4());

        let request = CreateSpaceRequest {
            room_id: room_id.clone(),
            name: Some("Parent Space".to_string()),
            topic: None,
            avatar_url: None,
            creator: "@p:example.com".to_string(),
            join_rule: None,
            visibility: None,
            is_public: None,
            parent_space_id: None,
        };
        let space = storage.create_space(request).await.unwrap();

        let child = storage
            .add_child(AddChildRequest {
                space_id: space.space_id.clone(),
                room_id: child_room_id.clone(),
                sender: "@sender:example.com".to_string(),
                is_suggested: true,
                via_servers: vec!["example.com".to_string()],
            })
            .await
            .expect("add_child should succeed");
        assert_eq!(child.room_id, child_room_id);

        let children = storage.get_space_children(&space.space_id).await.expect("get_space_children should succeed");
        assert_eq!(children.len(), 1);

        storage.remove_child(&space.space_id, &child_room_id).await.expect("remove_child should succeed");
        let after = storage.get_space_children(&space.space_id).await.unwrap();
        assert!(after.is_empty());

        cleanup(&pool, &space.space_id).await;
    }

    // === Test 8: space members CRUD ===
    #[tokio::test]
    async fn test_space_members_crud() {
        let pool = test_pool().await;
        let storage = SpaceStorage::new(&pool);
        let room_id = format!("!sp_member_{}:example.com", uuid::Uuid::new_v4());

        let request = CreateSpaceRequest {
            room_id: room_id.clone(),
            name: Some("Member Space".to_string()),
            topic: None,
            avatar_url: None,
            creator: "@m:example.com".to_string(),
            join_rule: None,
            visibility: None,
            is_public: None,
            parent_space_id: None,
        };
        let space = storage.create_space(request).await.unwrap();

        // Creator is auto-added as a member
        let members = storage.get_space_members(&space.space_id).await.expect("get_space_members should succeed");
        assert!(!members.is_empty()); // creator is always a member

        let member =
            storage.get_space_member(&space.space_id, "@m:example.com").await.expect("get_space_member should succeed");
        assert!(member.is_some());

        // Add another member
        storage
            .add_space_member(&space.space_id, "@other:example.com", "join", None)
            .await
            .expect("add_space_member should succeed");
        assert!(storage.is_space_member(&space.space_id, "@other:example.com").await.unwrap());

        // Remove the member
        storage
            .remove_space_member(&space.space_id, "@other:example.com")
            .await
            .expect("remove_space_member should succeed");
        assert!(!storage.is_space_member(&space.space_id, "@other:example.com").await.unwrap());

        cleanup(&pool, &space.space_id).await;
    }

    // === Test 9: get_user_spaces ===
    #[tokio::test]
    async fn test_get_user_spaces() {
        let pool = test_pool().await;
        let storage = SpaceStorage::new(&pool);
        let room_id = format!("!sp_user_{}:example.com", uuid::Uuid::new_v4());

        let request = CreateSpaceRequest {
            room_id: room_id.clone(),
            name: Some("User Space".to_string()),
            topic: None,
            avatar_url: None,
            creator: "@us:example.com".to_string(),
            join_rule: None,
            visibility: None,
            is_public: None,
            parent_space_id: None,
        };
        let space = storage.create_space(request).await.unwrap();

        let user_spaces = storage.get_user_spaces("@us:example.com").await.expect("get_user_spaces should succeed");
        assert!(!user_spaces.is_empty());
        assert!(user_spaces.iter().any(|s| s.space_id == space.space_id));

        cleanup(&pool, &space.space_id).await;
    }

    // === Test 10: get_public_spaces ===
    #[tokio::test]
    async fn test_get_public_spaces() {
        let pool = test_pool().await;
        let storage = SpaceStorage::new(&pool);
        let spaces = storage.get_public_spaces(10, None, None).await.expect("get_public_spaces should succeed");
        // All returned spaces should be public
        for space in &spaces {
            assert!(space.is_public);
        }
    }

    // === Test 11: get_spaces_by_rooms_batch ===
    #[tokio::test]
    async fn test_get_spaces_by_rooms_batch() {
        let pool = test_pool().await;
        let storage = SpaceStorage::new(&pool);
        let room_id = format!("!sp_batch_{}:example.com", uuid::Uuid::new_v4());

        let request = CreateSpaceRequest {
            room_id: room_id.clone(),
            name: Some("Batch Space".to_string()),
            topic: None,
            avatar_url: None,
            creator: "@b:example.com".to_string(),
            join_rule: None,
            visibility: None,
            is_public: None,
            parent_space_id: None,
        };
        let space = storage.create_space(request).await.unwrap();

        let map = storage
            .get_spaces_by_rooms_batch(&[room_id.clone()])
            .await
            .expect("get_spaces_by_rooms_batch should succeed");
        assert!(map.contains_key(&room_id));
        assert_eq!(map[&room_id].space_id, space.space_id);

        // Empty batch should return empty map
        let empty_map = storage.get_spaces_by_rooms_batch(&[]).await.expect("empty batch should succeed");
        assert!(empty_map.is_empty());

        cleanup(&pool, &space.space_id).await;
    }

    // === Test 12: get_space_summary and update_space_summary ===
    #[tokio::test]
    async fn test_get_space_summary_and_update() {
        let pool = test_pool().await;
        let storage = SpaceStorage::new(&pool);
        let room_id = format!("!sp_sum_{}:example.com", uuid::Uuid::new_v4());

        let request = CreateSpaceRequest {
            room_id: room_id.clone(),
            name: Some("Summary Space".to_string()),
            topic: None,
            avatar_url: None,
            creator: "@s:example.com".to_string(),
            join_rule: None,
            visibility: None,
            is_public: None,
            parent_space_id: None,
        };
        let space = storage.create_space(request).await.unwrap();

        // New space has no summary until update is called
        let summary = storage.get_space_summary(&space.space_id).await.expect("get_space_summary should succeed");
        assert!(summary.is_none(), "new space should not have a summary yet");

        // After update, summary should exist
        storage.update_space_summary(&space.space_id).await.expect("update_space_summary should succeed");
        let after = storage.get_space_summary(&space.space_id).await.unwrap();
        assert!(after.is_some());
        let s = after.unwrap();
        assert!(s.member_count >= 1, "should have at least the creator as member");

        cleanup(&pool, &space.space_id).await;
    }

    // === Test 13: get_child_spaces (reverse lookup) ===
    #[tokio::test]
    async fn test_get_child_spaces() {
        let pool = test_pool().await;
        let storage = SpaceStorage::new(&pool);
        let room_id = format!("!sp_child_of_{}:example.com", uuid::Uuid::new_v4());
        let child_room_id = format!("!child_of_{}:example.com", uuid::Uuid::new_v4());

        let request = CreateSpaceRequest {
            room_id: room_id.clone(),
            name: Some("Child Of".to_string()),
            topic: None,
            avatar_url: None,
            creator: "@c:example.com".to_string(),
            join_rule: None,
            visibility: None,
            is_public: None,
            parent_space_id: None,
        };
        let space = storage.create_space(request).await.unwrap();

        // Add a child room to this space
        storage
            .add_child(AddChildRequest {
                space_id: space.space_id.clone(),
                room_id: child_room_id.clone(),
                sender: "@sender:example.com".to_string(),
                is_suggested: false,
                via_servers: vec!["example.com".to_string()],
            })
            .await
            .unwrap();

        // Reverse lookup: find spaces that have this room as a child
        let child_spaces = storage.get_child_spaces(&child_room_id).await.expect("get_child_spaces should succeed");
        assert!(!child_spaces.is_empty());
        assert_eq!(child_spaces[0].space_id, space.space_id);

        cleanup(&pool, &space.space_id).await;
    }

    // === Test 14: get_space_hierarchy ===
    #[tokio::test]
    async fn test_get_space_hierarchy() {
        let pool = test_pool().await;
        let storage = SpaceStorage::new(&pool);
        let room_id = format!("!sp_hier_{}:example.com", uuid::Uuid::new_v4());

        let request = CreateSpaceRequest {
            room_id: room_id.clone(),
            name: Some("Hierarchy Space".to_string()),
            topic: None,
            avatar_url: None,
            creator: "@h:example.com".to_string(),
            join_rule: None,
            visibility: None,
            is_public: None,
            parent_space_id: None,
        };
        let space = storage.create_space(request).await.unwrap();

        let hierarchy =
            storage.get_space_hierarchy(&space.space_id, 2).await.expect("get_space_hierarchy should succeed");
        // SpaceHierarchy has: space, children, members
        assert_eq!(hierarchy.space.space_id, space.space_id);
        assert!(!hierarchy.members.is_empty(), "creator should be a member");

        // Test with non-existent space_id
        let result = storage.get_space_hierarchy("!nonexistent:example.com", 2).await;
        assert!(result.is_err(), "should error for non-existent space");

        cleanup(&pool, &space.space_id).await;
    }

    // === Test 15: add_space_event and get_space_events ===
    #[tokio::test]
    async fn test_add_and_get_space_events() {
        let pool = test_pool().await;
        let storage = SpaceStorage::new(&pool);
        let room_id = format!("!sp_evt_{}:example.com", uuid::Uuid::new_v4());
        let event_id = format!("$evt_{}:example.com", uuid::Uuid::new_v4());

        let request = CreateSpaceRequest {
            room_id: room_id.clone(),
            name: Some("Event Space".to_string()),
            topic: None,
            avatar_url: None,
            creator: "@e:example.com".to_string(),
            join_rule: None,
            visibility: None,
            is_public: None,
            parent_space_id: None,
        };
        let space = storage.create_space(request).await.unwrap();

        // Add a space event
        let added = storage
            .add_space_event(
                &event_id,
                &space.space_id,
                "m.space.child",
                "@sender:example.com",
                serde_json::json!({"via": ["example.com"]}),
                Some("!child:example.com"),
            )
            .await
            .expect("add_space_event should succeed");
        assert_eq!(added.event_id, event_id);
        assert_eq!(added.event_type, "m.space.child");

        // Get all events for the space
        let events =
            storage.get_space_events(&space.space_id, None, 10).await.expect("get_space_events should succeed");
        assert!(!events.is_empty());
        assert_eq!(events[0].event_id, event_id);

        // Get events filtered by type
        let filtered = storage
            .get_space_events(&space.space_id, Some("m.space.child"), 10)
            .await
            .expect("get_space_events filtered should succeed");
        assert!(!filtered.is_empty());

        // Get events filtered by non-matching type
        let no_match = storage
            .get_space_events(&space.space_id, Some("m.room.create"), 10)
            .await
            .expect("get_space_events should succeed");
        assert!(no_match.is_empty());

        cleanup(&pool, &space.space_id).await;
    }

    // === Test 16: get_space_member_and_child_count ===
    #[tokio::test]
    async fn test_get_space_member_and_child_count() {
        let pool = test_pool().await;
        let storage = SpaceStorage::new(&pool);
        let room_id = format!("!sp_count_{}:example.com", uuid::Uuid::new_v4());
        let child_room_id = format!("!child_count_{}:example.com", uuid::Uuid::new_v4());

        let request = CreateSpaceRequest {
            room_id: room_id.clone(),
            name: Some("Count Space".to_string()),
            topic: None,
            avatar_url: None,
            creator: "@cnt:example.com".to_string(),
            join_rule: None,
            visibility: None,
            is_public: None,
            parent_space_id: None,
        };
        let space = storage.create_space(request).await.unwrap();

        // Initially: 1 member (creator), 0 children
        let (member_count, child_count) = storage
            .get_space_member_and_child_count(&space.space_id)
            .await
            .expect("get_space_member_and_child_count should succeed");
        assert_eq!(member_count, 1);
        assert_eq!(child_count, 0);

        // Add a child
        storage
            .add_child(AddChildRequest {
                space_id: space.space_id.clone(),
                room_id: child_room_id.clone(),
                sender: "@sender:example.com".to_string(),
                is_suggested: false,
                via_servers: vec!["example.com".to_string()],
            })
            .await
            .unwrap();

        // Add another member
        storage.add_space_member(&space.space_id, "@other:example.com", "join", None).await.unwrap();

        let (member_count2, child_count2) =
            storage.get_space_member_and_child_count(&space.space_id).await.expect("count should succeed after adding");
        assert_eq!(member_count2, 2);
        assert_eq!(child_count2, 1);

        cleanup(&pool, &space.space_id).await;
    }
}
