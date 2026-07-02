use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use std::sync::Arc;
use tracing;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RoomSummary {
    pub id: Option<i64>,
    pub room_id: String,
    pub room_type: Option<String>,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub avatar_url: Option<String>,
    pub canonical_alias: Option<String>,
    #[sqlx(rename = "join_rules")]
    pub join_rule: String,
    pub history_visibility: String,
    pub guest_access: String,
    pub is_direct: bool,
    pub is_space: bool,
    pub is_encrypted: bool,
    pub member_count: i64,
    pub joined_member_count: i64,
    pub invited_member_count: i64,
    pub hero_users: serde_json::Value,
    pub last_event_id: Option<String>,
    pub last_event_ts: Option<i64>,
    pub last_message_ts: Option<i64>,
    pub unread_notifications: i64,
    pub unread_highlight: i64,
    pub updated_ts: Option<i64>,
    pub created_ts: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RoomSummaryMember {
    pub id: i64,
    pub room_id: String,
    pub user_id: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub membership: String,
    pub is_hero: bool,
    pub last_active_ts: Option<i64>,
    pub updated_ts: i64,
    pub created_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RoomSummaryState {
    pub id: i64,
    pub room_id: String,
    pub event_type: String,
    pub state_key: String,
    pub event_id: Option<String>,
    pub content: serde_json::Value,
    pub updated_ts: i64,
}

/// Input entry for batch upserts via [`RoomSummaryStorage::set_states_batch`].
#[derive(Debug, Clone)]
pub struct RoomSummaryStateEntry {
    pub event_type: String,
    pub state_key: String,
    pub event_id: Option<String>,
    pub content: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RoomSummaryStats {
    pub id: i64,
    pub room_id: String,
    pub total_events: i64,
    pub total_state_events: i64,
    pub total_messages: i64,
    pub total_media: i64,
    pub storage_size: i64,
    pub last_updated_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRoomSummaryRequest {
    pub room_id: String,
    pub room_type: Option<String>,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub avatar_url: Option<String>,
    pub canonical_alias: Option<String>,
    pub join_rule: Option<String>,
    pub history_visibility: Option<String>,
    pub guest_access: Option<String>,
    pub is_direct: Option<bool>,
    pub is_space: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateRoomSummaryRequest {
    pub name: Option<String>,
    pub topic: Option<String>,
    pub avatar_url: Option<String>,
    pub canonical_alias: Option<String>,
    pub join_rule: Option<String>,
    pub history_visibility: Option<String>,
    pub guest_access: Option<String>,
    pub is_direct: Option<bool>,
    pub is_space: Option<bool>,
    pub is_encrypted: Option<bool>,
    pub last_event_id: Option<String>,
    pub last_event_ts: Option<i64>,
    pub last_message_ts: Option<i64>,
    pub hero_users: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSummaryMemberRequest {
    pub room_id: String,
    pub user_id: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub membership: String,
    pub is_hero: Option<bool>,
    pub last_active_ts: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSummaryMemberRequest {
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub membership: Option<String>,
    pub is_hero: Option<bool>,
    pub last_active_ts: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomSummaryResponse {
    pub room_id: String,
    pub room_type: Option<String>,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub avatar_url: Option<String>,
    pub canonical_alias: Option<String>,
    pub join_rule: String,
    pub history_visibility: String,
    pub guest_access: String,
    pub is_direct: bool,
    pub is_space: bool,
    pub is_encrypted: bool,
    pub member_count: i64,
    pub joined_member_count: i64,
    pub invited_member_count: i64,
    pub heroes: Vec<RoomSummaryHero>,
    pub last_event_ts: Option<i64>,
    pub last_message_ts: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomSummaryHero {
    pub user_id: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Clone)]
pub struct RoomSummaryStorage {
    pool: Arc<PgPool>,
}

impl RoomSummaryStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create_summary(&self, request: CreateRoomSummaryRequest) -> Result<RoomSummary, sqlx::Error> {
        tracing::info!(room_id = %request.room_id, "Creating room summary");
        let now = Utc::now().timestamp_millis();

        let row = sqlx::query_as::<_, RoomSummary>(
            r"
            INSERT INTO room_summaries (
                room_id, room_type, name, topic, avatar_url, canonical_alias,
                join_rules, history_visibility, guest_access, is_direct, is_space,
                is_encrypted, member_count, joined_member_count, invited_member_count, hero_users,
                unread_notifications, unread_highlight, updated_ts, created_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, FALSE, 0, 0, 0, '[]'::jsonb, 0, 0, $12, $12)
            RETURNING id, room_id, room_type, name, topic, avatar_url, canonical_alias, join_rules, history_visibility, guest_access, is_direct, is_space, is_encrypted, member_count, joined_member_count, invited_member_count, hero_users, last_event_id, last_event_ts, last_message_ts, unread_notifications, unread_highlight, updated_ts, created_ts
            ",
        )
        .bind(&request.room_id)
        .bind(&request.room_type)
        .bind(&request.name)
        .bind(&request.topic)
        .bind(&request.avatar_url)
        .bind(&request.canonical_alias)
        .bind(request.join_rule.unwrap_or_else(|| "invite".to_string()))
        .bind(
            request
                .history_visibility
                .unwrap_or_else(|| "shared".to_string()),
        )
        .bind(
            request
                .guest_access
                .unwrap_or_else(|| "forbidden".to_string()),
        )
        .bind(request.is_direct.unwrap_or(false))
        .bind(request.is_space.unwrap_or(false))
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_summary(&self, room_id: &str) -> Result<Option<RoomSummary>, sqlx::Error> {
        tracing::debug!(room_id = %room_id, "Querying room summary");
        let row =
            sqlx::query_as::<_, RoomSummary>("SELECT id, room_id, room_type, name, topic, avatar_url, canonical_alias, join_rules, history_visibility, guest_access, is_direct, is_space, is_encrypted, member_count, joined_member_count, invited_member_count, hero_users, last_event_id, last_event_ts, last_message_ts, unread_notifications, unread_highlight, updated_ts, created_ts FROM room_summaries WHERE room_id = $1")
                .bind(room_id)
                .fetch_optional(&*self.pool)
                .await?;

        if row.is_none() {
            tracing::warn!(room_id = %room_id, "Room summary not found");
        }

        Ok(row)
    }

    pub async fn update_summary(
        &self,
        room_id: &str,
        request: UpdateRoomSummaryRequest,
    ) -> Result<RoomSummary, sqlx::Error> {
        tracing::info!(room_id = %room_id, "Updating room summary");
        let now = Utc::now().timestamp_millis();
        let row = sqlx::query_as::<_, RoomSummary>(
            r"
            UPDATE room_summaries SET
                name = COALESCE($2, name),
                topic = COALESCE($3, topic),
                avatar_url = COALESCE($4, avatar_url),
                canonical_alias = COALESCE($5, canonical_alias),
                join_rules = COALESCE($6, join_rules),
                history_visibility = COALESCE($7, history_visibility),
                guest_access = COALESCE($8, guest_access),
                is_direct = COALESCE($9, is_direct),
                is_space = COALESCE($10, is_space),
                is_encrypted = COALESCE($11, is_encrypted),
                last_event_id = COALESCE($12, last_event_id),
                last_event_ts = COALESCE($13, last_event_ts),
                last_message_ts = COALESCE($14, last_message_ts),
                hero_users = COALESCE($15, hero_users),
                updated_ts = $16
            WHERE room_id = $1
            RETURNING id, room_id, room_type, name, topic, avatar_url, canonical_alias, join_rules, history_visibility, guest_access, is_direct, is_space, is_encrypted, member_count, joined_member_count, invited_member_count, hero_users, last_event_id, last_event_ts, last_message_ts, unread_notifications, unread_highlight, updated_ts, created_ts
            ",
        )
        .bind(room_id)
        .bind(&request.name)
        .bind(&request.topic)
        .bind(&request.avatar_url)
        .bind(&request.canonical_alias)
        .bind(&request.join_rule)
        .bind(&request.history_visibility)
        .bind(&request.guest_access)
        .bind(request.is_direct)
        .bind(request.is_space)
        .bind(request.is_encrypted)
        .bind(&request.last_event_id)
        .bind(request.last_event_ts)
        .bind(request.last_message_ts)
        .bind(&request.hero_users)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn set_canonical_alias(
        &self,
        room_id: &str,
        canonical_alias: Option<&str>,
    ) -> Result<RoomSummary, sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        sqlx::query_as::<_, RoomSummary>(
            r"
            UPDATE room_summaries
            SET canonical_alias = $2,
                updated_ts = $3
            WHERE room_id = $1
            RETURNING id, room_id, room_type, name, topic, avatar_url, canonical_alias, join_rules, history_visibility, guest_access, is_direct, is_space, is_encrypted, member_count, joined_member_count, invited_member_count, hero_users, last_event_id, last_event_ts, last_message_ts, unread_notifications, unread_highlight, updated_ts, created_ts
            ",
        )
        .bind(room_id)
        .bind(canonical_alias)
        .bind(now)
        .fetch_one(&*self.pool)
        .await
    }

    pub async fn delete_summary(&self, room_id: &str) -> Result<(), sqlx::Error> {
        tracing::info!(room_id = %room_id, "Deleting room summary");
        sqlx::query("DELETE FROM room_summaries WHERE room_id = $1").bind(room_id).execute(&*self.pool).await?;

        Ok(())
    }

    pub async fn get_summaries_by_ids(&self, room_ids: &[String]) -> Result<Vec<RoomSummary>, sqlx::Error> {
        let rows = sqlx::query_as::<_, RoomSummary>(
            "SELECT id, room_id, room_type, name, topic, avatar_url, canonical_alias, join_rules, history_visibility, guest_access, is_direct, is_space, is_encrypted, member_count, joined_member_count, invited_member_count, hero_users, last_event_id, last_event_ts, last_message_ts, unread_notifications, unread_highlight, updated_ts, created_ts FROM room_summaries WHERE room_id = ANY($1)",
        )
        .bind(room_ids)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn get_summaries_for_user(&self, user_id: &str) -> Result<Vec<RoomSummary>, sqlx::Error> {
        tracing::debug!(user_id = %user_id, "Querying room summaries for user");
        let rows = sqlx::query_as::<_, RoomSummary>(
            r"
            SELECT rs.id, rs.room_id, rs.room_type, rs.name, rs.topic, rs.avatar_url, rs.canonical_alias, rs.join_rules, rs.history_visibility, rs.guest_access, rs.is_direct, rs.is_space, rs.is_encrypted, rs.member_count, rs.joined_member_count, rs.invited_member_count, rs.hero_users, rs.last_event_id, rs.last_event_ts, rs.last_message_ts, rs.unread_notifications, rs.unread_highlight, rs.updated_ts, rs.created_ts FROM room_summaries rs
            INNER JOIN room_summary_members rsm ON rs.room_id = rsm.room_id
            WHERE rsm.user_id = $1 AND rsm.membership IN ('join', 'invite')
            ORDER BY rs.last_event_ts DESC NULLS LAST
            ",
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn add_member(&self, request: CreateSummaryMemberRequest) -> Result<RoomSummaryMember, sqlx::Error> {
        tracing::info!(room_id = %request.room_id, user_id = %request.user_id, membership = %request.membership, "Adding member to room summary");
        let now = Utc::now().timestamp_millis();

        let row = sqlx::query_as::<_, RoomSummaryMember>(
            r"
            INSERT INTO room_summary_members (
                room_id, user_id, display_name, avatar_url, membership, is_hero, last_active_ts, updated_ts, created_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8)
            ON CONFLICT (room_id, user_id) DO UPDATE SET
                display_name = COALESCE(EXCLUDED.display_name, room_summary_members.display_name),
                avatar_url = COALESCE(EXCLUDED.avatar_url, room_summary_members.avatar_url),
                membership = EXCLUDED.membership,
                is_hero = COALESCE(EXCLUDED.is_hero, room_summary_members.is_hero),
                last_active_ts = COALESCE(EXCLUDED.last_active_ts, room_summary_members.last_active_ts),
                updated_ts = EXCLUDED.updated_ts
            RETURNING id, room_id, user_id, display_name, avatar_url, membership, is_hero, last_active_ts, updated_ts, created_ts
            ",
        )
        .bind(&request.room_id)
        .bind(&request.user_id)
        .bind(&request.display_name)
        .bind(&request.avatar_url)
        .bind(&request.membership)
        .bind(request.is_hero.unwrap_or(false))
        .bind(request.last_active_ts)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        self.refresh_member_counts(&request.room_id).await?;

        Ok(row)
    }

    pub async fn add_members_batch(
        &self,
        room_id: &str,
        members: Vec<CreateSummaryMemberRequest>,
    ) -> Result<usize, sqlx::Error> {
        if members.is_empty() {
            return Ok(0);
        }

        tracing::info!(room_id = %room_id, count = members.len(), "Batch adding members to room summary");

        let now = Utc::now().timestamp_millis();
        let mut user_ids: Vec<String> = Vec::with_capacity(members.len());
        let mut display_names: Vec<Option<String>> = Vec::with_capacity(members.len());
        let mut avatar_urls: Vec<Option<String>> = Vec::with_capacity(members.len());
        let mut memberships: Vec<String> = Vec::with_capacity(members.len());
        let mut is_heroes: Vec<bool> = Vec::with_capacity(members.len());
        let mut last_active_tss: Vec<Option<i64>> = Vec::with_capacity(members.len());
        let mut nows: Vec<i64> = Vec::with_capacity(members.len());

        for m in &members {
            user_ids.push(m.user_id.clone());
            display_names.push(m.display_name.clone());
            avatar_urls.push(m.avatar_url.clone());
            memberships.push(m.membership.clone());
            is_heroes.push(m.is_hero.unwrap_or(false));
            last_active_tss.push(m.last_active_ts);
            nows.push(now);
        }

        let result = sqlx::query(
            r"
            INSERT INTO room_summary_members (
                room_id, user_id, display_name, avatar_url, membership, is_hero, last_active_ts, updated_ts, created_ts
            )
            SELECT $1, u, d, a, m, h, l, n, n
            FROM UNNEST($2::TEXT[], $3::TEXT[], $4::TEXT[], $5::TEXT[], $6::BOOLEAN[], $7::BIGINT[], $8::BIGINT[])
            AS t(u, d, a, m, h, l, n)
            ON CONFLICT (room_id, user_id) DO UPDATE SET
                display_name = COALESCE(EXCLUDED.display_name, room_summary_members.display_name),
                avatar_url = COALESCE(EXCLUDED.avatar_url, room_summary_members.avatar_url),
                membership = EXCLUDED.membership,
                is_hero = COALESCE(EXCLUDED.is_hero, room_summary_members.is_hero),
                last_active_ts = COALESCE(EXCLUDED.last_active_ts, room_summary_members.last_active_ts),
                updated_ts = EXCLUDED.updated_ts
            ",
        )
        .bind(room_id)
        .bind(&user_ids)
        .bind(&display_names)
        .bind(&avatar_urls)
        .bind(&memberships)
        .bind(&is_heroes)
        .bind(&last_active_tss)
        .bind(&nows)
        .execute(&*self.pool)
        .await?;

        self.refresh_member_counts(room_id).await?;

        Ok(result.rows_affected() as usize)
    }

    pub async fn update_member(
        &self,
        room_id: &str,
        user_id: &str,
        request: UpdateSummaryMemberRequest,
    ) -> Result<RoomSummaryMember, sqlx::Error> {
        tracing::info!(room_id = %room_id, user_id = %user_id, "Updating room summary member");
        let now = Utc::now().timestamp_millis();
        let row = sqlx::query_as::<_, RoomSummaryMember>(
            r"
            UPDATE room_summary_members SET
                display_name = COALESCE($3, display_name),
                avatar_url = COALESCE($4, avatar_url),
                membership = COALESCE($5, membership),
                is_hero = COALESCE($6, is_hero),
                last_active_ts = COALESCE($7, last_active_ts),
                updated_ts = $8
            WHERE room_id = $1 AND user_id = $2
            RETURNING id, room_id, user_id, display_name, avatar_url, membership, is_hero, last_active_ts, updated_ts, created_ts
            ",
        )
        .bind(room_id)
        .bind(user_id)
        .bind(&request.display_name)
        .bind(&request.avatar_url)
        .bind(&request.membership)
        .bind(request.is_hero)
        .bind(request.last_active_ts)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        self.refresh_member_counts(room_id).await?;

        Ok(row)
    }

    pub async fn remove_member(&self, room_id: &str, user_id: &str) -> Result<(), sqlx::Error> {
        tracing::info!(room_id = %room_id, user_id = %user_id, "Removing member from room summary");
        sqlx::query("DELETE FROM room_summary_members WHERE room_id = $1 AND user_id = $2")
            .bind(room_id)
            .bind(user_id)
            .execute(&*self.pool)
            .await?;

        self.refresh_member_counts(room_id).await?;

        Ok(())
    }

    pub async fn get_members(&self, room_id: &str) -> Result<Vec<RoomSummaryMember>, sqlx::Error> {
        let rows = sqlx::query_as::<_, RoomSummaryMember>(
            "SELECT id, room_id, user_id, display_name, avatar_url, membership, is_hero, last_active_ts, updated_ts, created_ts FROM room_summary_members WHERE room_id = $1 ORDER BY is_hero DESC, user_id",
        )
        .bind(room_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn get_heroes(&self, room_id: &str, limit: i64) -> Result<Vec<RoomSummaryMember>, sqlx::Error> {
        let rows = sqlx::query_as::<_, RoomSummaryMember>(
            r"
            SELECT id, room_id, user_id, display_name, avatar_url, membership, is_hero, last_active_ts, updated_ts, created_ts FROM room_summary_members
            WHERE room_id = $1 AND membership = 'join'
            ORDER BY is_hero DESC, last_active_ts DESC NULLS LAST
            LIMIT $2
            ",
        )
        .bind(room_id)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    /// Batch variant of [`get_heroes`] that fetches heroes for multiple rooms
    /// in a single query, respecting the same per-room limit via a window
    /// function. Returns heroes keyed by `room_id`.
    pub async fn get_heroes_batch(
        &self,
        room_ids: &[String],
        limit: i64,
    ) -> Result<std::collections::HashMap<String, Vec<RoomSummaryMember>>, sqlx::Error> {
        if room_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let rows = sqlx::query_as::<_, RoomSummaryMember>(
            r"
            SELECT id, room_id, user_id, display_name, avatar_url, membership, is_hero, last_active_ts, updated_ts, created_ts
            FROM (
                SELECT id, room_id, user_id, display_name, avatar_url, membership, is_hero, last_active_ts, updated_ts, created_ts,
                       ROW_NUMBER() OVER (PARTITION BY room_id ORDER BY is_hero DESC, last_active_ts DESC NULLS LAST) AS rn
                FROM room_summary_members
                WHERE room_id = ANY($1) AND membership = 'join'
            ) t
            WHERE rn <= $2
            ",
        )
        .bind(room_ids)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;

        let mut result: std::collections::HashMap<String, Vec<RoomSummaryMember>> =
            room_ids.iter().map(|id| (id.clone(), Vec::new())).collect();

        for member in rows {
            if let Some(heroes) = result.get_mut(&member.room_id) {
                heroes.push(member);
            }
        }

        Ok(result)
    }

    pub async fn get_hero_candidates(&self, room_id: &str, limit: i64) -> Result<Vec<RoomSummaryMember>, sqlx::Error> {
        let rows = sqlx::query_as::<_, RoomSummaryMember>(
            r"
            SELECT id, room_id, user_id, display_name, avatar_url, membership, is_hero, last_active_ts, updated_ts, created_ts
            FROM room_summary_members
            WHERE room_id = $1 AND membership = 'join'
            ORDER BY last_active_ts DESC NULLS LAST, updated_ts DESC, user_id ASC
            LIMIT $2
            ",
        )
        .bind(room_id)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn set_hero_members(&self, room_id: &str, hero_user_ids: &[String]) -> Result<(), sqlx::Error> {
        sqlx::query(
            r"
            UPDATE room_summary_members
            SET is_hero = user_id = ANY($2::text[])
            WHERE room_id = $1
            ",
        )
        .bind(room_id)
        .bind(hero_user_ids)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn set_state(
        &self,
        room_id: &str,
        event_type: &str,
        state_key: &str,
        event_id: Option<&str>,
        content: serde_json::Value,
    ) -> Result<RoomSummaryState, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let row = sqlx::query_as::<_, RoomSummaryState>(
            r"
            INSERT INTO room_summary_state (room_id, event_type, state_key, event_id, content, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (room_id, event_type, state_key) DO UPDATE SET
                event_id = EXCLUDED.event_id,
                content = EXCLUDED.content,
                updated_ts = EXCLUDED.updated_ts
            RETURNING *
            ",
        )
        .bind(room_id)
        .bind(event_type)
        .bind(state_key)
        .bind(event_id)
        .bind(&content)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    /// Batch upsert for room summary state events, avoiding N+1 round trips when
    /// syncing many state events for a room.
    pub async fn set_states_batch(&self, room_id: &str, entries: &[RoomSummaryStateEntry]) -> Result<u64, sqlx::Error> {
        if entries.is_empty() {
            return Ok(0);
        }

        let now = Utc::now().timestamp_millis();
        let event_types: Vec<String> = entries.iter().map(|e| e.event_type.clone()).collect();
        let state_keys: Vec<String> = entries.iter().map(|e| e.state_key.clone()).collect();
        let event_ids: Vec<Option<String>> = entries.iter().map(|e| e.event_id.clone()).collect();
        let contents: Vec<serde_json::Value> = entries.iter().map(|e| e.content.clone()).collect();

        let result = sqlx::query(
            r"
            INSERT INTO room_summary_state (room_id, event_type, state_key, event_id, content, updated_ts)
            SELECT $1, et, sk, ei, co, $2
            FROM UNNEST($3::TEXT[], $4::TEXT[], $5::TEXT[], $6::JSONB[])
            AS t(et, sk, ei, co)
            ON CONFLICT (room_id, event_type, state_key) DO UPDATE SET
                event_id = EXCLUDED.event_id,
                content = EXCLUDED.content,
                updated_ts = EXCLUDED.updated_ts
            ",
        )
        .bind(room_id)
        .bind(now)
        .bind(&event_types)
        .bind(&state_keys)
        .bind(&event_ids)
        .bind(&contents)
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn get_state(
        &self,
        room_id: &str,
        event_type: &str,
        state_key: &str,
    ) -> Result<Option<RoomSummaryState>, sqlx::Error> {
        let row = sqlx::query_as::<_, RoomSummaryState>(
            "SELECT id, room_id, event_type, state_key, event_id, content, updated_ts FROM room_summary_state WHERE room_id = $1 AND event_type = $2 AND state_key = $3",
        )
        .bind(room_id)
        .bind(event_type)
        .bind(state_key)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_all_state(&self, room_id: &str) -> Result<Vec<RoomSummaryState>, sqlx::Error> {
        let rows = sqlx::query_as::<_, RoomSummaryState>(
            "SELECT id, room_id, event_type, state_key, event_id, content, updated_ts FROM room_summary_state WHERE room_id = $1",
        )
        .bind(room_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn get_stats(&self, room_id: &str) -> Result<Option<RoomSummaryStats>, sqlx::Error> {
        let row = sqlx::query_as::<_, RoomSummaryStats>(
            "SELECT id::BIGINT AS id, room_id, total_events::BIGINT AS total_events, total_state_events::BIGINT AS total_state_events, total_messages::BIGINT AS total_messages, total_media::BIGINT AS total_media, storage_size::BIGINT AS storage_size, last_updated_ts::BIGINT AS last_updated_ts FROM room_summary_stats WHERE room_id = $1",
        )
        .bind(room_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn update_stats(
        &self,
        room_id: &str,
        total_events: i64,
        total_state_events: i64,
        total_messages: i64,
        total_media: i64,
        storage_size: i64,
    ) -> Result<RoomSummaryStats, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let row = sqlx::query_as::<_, RoomSummaryStats>(
            r"
            INSERT INTO room_summary_stats (room_id, total_events, total_state_events, total_messages, total_media, storage_size, last_updated_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (room_id) DO UPDATE SET
                total_events = EXCLUDED.total_events,
                total_state_events = EXCLUDED.total_state_events,
                total_messages = EXCLUDED.total_messages,
                total_media = EXCLUDED.total_media,
                storage_size = EXCLUDED.storage_size,
                last_updated_ts = EXCLUDED.last_updated_ts
            RETURNING id::BIGINT AS id, room_id, total_events::BIGINT AS total_events, total_state_events::BIGINT AS total_state_events, total_messages::BIGINT AS total_messages, total_media::BIGINT AS total_media, storage_size::BIGINT AS storage_size, last_updated_ts::BIGINT AS last_updated_ts
            ",
        )
        .bind(room_id)
        .bind(total_events)
        .bind(total_state_events)
        .bind(total_messages)
        .bind(total_media)
        .bind(storage_size)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn queue_update(
        &self,
        room_id: &str,
        event_id: &str,
        event_type: &str,
        state_key: Option<&str>,
        priority: i32,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query(
            r"
            INSERT INTO room_summary_update_queue (room_id, event_id, event_type, state_key, priority, created_ts)
            VALUES ($1, $2, $3, $4, $5, $6)
            ",
        )
        .bind(room_id)
        .bind(event_id)
        .bind(event_type)
        .bind(state_key)
        .bind(priority)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_pending_updates(&self, limit: i64) -> Result<Vec<RoomSummaryUpdateQueueItem>, sqlx::Error> {
        let rows = sqlx::query_as::<_, RoomSummaryUpdateQueueItem>(
            r"
            SELECT id, room_id, event_id, event_type, state_key, priority, status, created_ts, processed_ts, error_message, retry_count
            FROM room_summary_update_queue
            WHERE status = 'pending'
            ORDER BY priority DESC, created_ts ASC, id ASC
            LIMIT $1
            FOR UPDATE SKIP LOCKED
            ",
        )
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn mark_update_processed(&self, id: i64) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query("UPDATE room_summary_update_queue SET status = 'processed', processed_ts = $2 WHERE id = $1")
            .bind(id)
            .bind(now)
            .execute(&*self.pool)
            .await?;

        Ok(())
    }

    pub async fn mark_update_failed(&self, id: i64, error: &str) -> Result<(), sqlx::Error> {
        tracing::warn!(id = id, error = %error, "Marking room summary update as failed");
        sqlx::query(
            r"
            UPDATE room_summary_update_queue SET
                status = 'failed',
                error_message = $2,
                retry_count = retry_count + 1
            WHERE id = $1
            ",
        )
        .bind(id)
        .bind(error)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn increment_unread_notifications(&self, room_id: &str, highlight: bool) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        if highlight {
            sqlx::query(
                "UPDATE room_summaries SET unread_notifications = unread_notifications + 1, unread_highlight = unread_highlight + 1, updated_ts = $2 WHERE room_id = $1",
            )
            .bind(room_id)
            .bind(now)
            .execute(&*self.pool)
            .await?;
        } else {
            sqlx::query(
                "UPDATE room_summaries SET unread_notifications = unread_notifications + 1, updated_ts = $2 WHERE room_id = $1",
            )
            .bind(room_id)
            .bind(now)
            .execute(&*self.pool)
            .await?;
        }

        Ok(())
    }

    pub async fn clear_unread_notifications(&self, room_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE room_summaries SET unread_notifications = 0, unread_highlight = 0, updated_ts = $2 WHERE room_id = $1",
        )
        .bind(room_id)
        .bind(Utc::now().timestamp_millis())
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    async fn refresh_member_counts(&self, room_id: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        sqlx::query(
            r"
            UPDATE room_summaries
            SET
                member_count = counts.member_count,
                joined_member_count = counts.joined_member_count,
                invited_member_count = counts.invited_member_count,
                updated_ts = $2
            FROM (
                SELECT
                    COUNT(*)::BIGINT AS member_count,
                    COUNT(*) FILTER (WHERE membership = 'join')::BIGINT AS joined_member_count,
                    COUNT(*) FILTER (WHERE membership = 'invite')::BIGINT AS invited_member_count
                FROM room_summary_members
                WHERE room_id = $1
            ) AS counts
            WHERE room_summaries.room_id = $1
            ",
        )
        .bind(room_id)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct RoomSummaryUpdateQueueItem {
    pub id: i64,
    pub room_id: String,
    pub event_id: String,
    pub event_type: String,
    pub state_key: Option<String>,
    pub priority: i32,
    pub status: String,
    pub created_ts: i64,
    pub processed_ts: Option<i64>,
    pub error_message: Option<String>,
    pub retry_count: i32,
}

impl RoomSummary {
    pub fn to_response(&self, heroes: Vec<RoomSummaryHero>) -> RoomSummaryResponse {
        RoomSummaryResponse {
            room_id: self.room_id.clone(),
            room_type: self.room_type.clone(),
            name: self.name.clone(),
            topic: self.topic.clone(),
            avatar_url: self.avatar_url.clone(),
            canonical_alias: self.canonical_alias.clone(),
            join_rule: self.join_rule.clone(),
            history_visibility: self.history_visibility.clone(),
            guest_access: self.guest_access.clone(),
            is_direct: self.is_direct,
            is_space: self.is_space,
            is_encrypted: self.is_encrypted,
            member_count: self.member_count,
            joined_member_count: self.joined_member_count,
            invited_member_count: self.invited_member_count,
            heroes,
            last_event_ts: self.last_event_ts,
            last_message_ts: self.last_message_ts,
        }
    }
}

impl From<RoomSummaryMember> for RoomSummaryHero {
    fn from(member: RoomSummaryMember) -> Self {
        Self { user_id: member.user_id, display_name: member.display_name, avatar_url: member.avatar_url }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_room_summary_creation() {
        let summary = RoomSummary {
            id: Some(1),
            room_id: "!room:example.com".to_string(),
            room_type: None,
            name: Some("Test Room".to_string()),
            topic: Some("A test room".to_string()),
            avatar_url: Some("mxc://avatar".to_string()),
            canonical_alias: None,
            join_rule: "public".to_string(),
            history_visibility: "shared".to_string(),
            guest_access: "forbidden".to_string(),
            is_direct: false,
            is_space: false,
            is_encrypted: false,
            member_count: 10,
            joined_member_count: 8,
            invited_member_count: 2,
            hero_users: serde_json::json!([]),
            last_event_id: None,
            last_event_ts: None,
            last_message_ts: None,
            unread_notifications: 0,
            unread_highlight: 0,
            updated_ts: Some(1234567890),
            created_ts: Some(1234567800),
        };
        assert_eq!(summary.room_id, "!room:example.com");
        assert!(summary.name.is_some());
    }

    #[test]
    fn test_room_summary_member_creation() {
        let member = RoomSummaryMember {
            id: 1,
            room_id: "!room:example.com".to_string(),
            user_id: "@alice:example.com".to_string(),
            display_name: Some("Alice".to_string()),
            avatar_url: Some("mxc://alice".to_string()),
            membership: "join".to_string(),
            is_hero: true,
            last_active_ts: Some(1234567890),
            updated_ts: 1234567890,
            created_ts: 1234567800,
        };
        assert_eq!(member.user_id, "@alice:example.com");
        assert!(member.is_hero);
    }

    #[test]
    fn test_room_summary_state_creation() {
        let state = RoomSummaryState {
            id: 1,
            room_id: "!room:example.com".to_string(),
            event_type: "m.room.create".to_string(),
            state_key: "".to_string(),
            event_id: None,
            content: serde_json::json!({"creator": "@admin:example.com"}),
            updated_ts: 1234567890,
        };
        assert_eq!(state.room_id, "!room:example.com");
    }

    #[test]
    fn test_room_summary_stats_creation() {
        let stats = RoomSummaryStats {
            id: 1,
            room_id: "!room:example.com".to_string(),
            total_events: 100,
            total_state_events: 20,
            total_messages: 100,
            total_media: 10,
            storage_size: 1048576,
            last_updated_ts: 1234567890,
        };
        assert_eq!(stats.total_messages, 100);
    }

    #[test]
    fn test_create_room_summary_request() {
        let request = CreateRoomSummaryRequest {
            room_id: "!room:example.com".to_string(),
            room_type: None,
            name: Some("New Room".to_string()),
            topic: Some("Topic".to_string()),
            avatar_url: None,
            canonical_alias: None,
            join_rule: Some("public".to_string()),
            history_visibility: Some("shared".to_string()),
            guest_access: Some("forbidden".to_string()),
            is_direct: Some(false),
            is_space: Some(false),
        };
        assert_eq!(request.room_id, "!room:example.com");
    }

    #[test]
    fn test_update_room_summary_request() {
        let request = UpdateRoomSummaryRequest {
            name: Some("Updated Name".to_string()),
            topic: None,
            avatar_url: Some("mxc://new".to_string()),
            canonical_alias: None,
            join_rule: None,
            history_visibility: None,
            guest_access: None,
            is_direct: None,
            is_space: None,
            is_encrypted: None,
            last_event_id: None,
            last_event_ts: None,
            last_message_ts: None,
            hero_users: None,
        };
        assert!(request.name.is_some());
    }

    #[test]
    fn test_hero_users_json() {
        let heroes = vec![
            RoomSummaryHero {
                user_id: "@alice:example.com".to_string(),
                display_name: Some("Alice".to_string()),
                avatar_url: None,
            },
            RoomSummaryHero {
                user_id: "@bob:example.com".to_string(),
                display_name: Some("Bob".to_string()),
                avatar_url: Some("mxc://bob".to_string()),
            },
        ];
        let json = serde_json::to_string(&heroes).unwrap();
        assert!(json.contains("@alice:example.com"));
    }

    #[test]
    fn test_room_summary_optional_fields() {
        let summary = RoomSummary {
            id: Some(2),
            room_id: "!room2:example.com".to_string(),
            room_type: None,
            name: None,
            topic: None,
            avatar_url: None,
            canonical_alias: None,
            join_rule: "public".to_string(),
            history_visibility: "shared".to_string(),
            guest_access: "forbidden".to_string(),
            is_direct: false,
            is_space: false,
            is_encrypted: false,
            member_count: 0,
            joined_member_count: 0,
            invited_member_count: 0,
            hero_users: serde_json::json!([]),
            last_event_id: None,
            last_event_ts: None,
            last_message_ts: None,
            unread_notifications: 0,
            unread_highlight: 0,
            updated_ts: Some(0),
            created_ts: Some(0),
        };
        assert!(summary.name.is_none());
        assert!(summary.topic.is_none());
    }

    // ── to_response tests ──

    #[test]
    fn test_to_response_empty_heroes() {
        let summary = RoomSummary {
            id: Some(1),
            room_id: "!room:example.com".to_string(),
            room_type: None,
            name: Some("Test Room".to_string()),
            topic: Some("A test room".to_string()),
            avatar_url: Some("mxc://avatar".to_string()),
            canonical_alias: None,
            join_rule: "public".to_string(),
            history_visibility: "shared".to_string(),
            guest_access: "forbidden".to_string(),
            is_direct: false,
            is_space: false,
            is_encrypted: true,
            member_count: 5,
            joined_member_count: 3,
            invited_member_count: 2,
            hero_users: serde_json::json!([]),
            last_event_id: None,
            last_event_ts: Some(1_700_000_000_000i64),
            last_message_ts: Some(1_700_000_000_001i64),
            unread_notifications: 0,
            unread_highlight: 0,
            updated_ts: Some(1_700_000_000_000i64),
            created_ts: Some(1_700_000_000_000i64),
        };

        let response = summary.to_response(vec![]);

        assert_eq!(response.room_id, "!room:example.com");
        assert_eq!(response.name, Some("Test Room".to_string()));
        assert_eq!(response.is_encrypted, true);
        assert_eq!(response.member_count, 5);
        assert!(response.heroes.is_empty());
        assert_eq!(response.last_event_ts, Some(1_700_000_000_000i64));
        assert_eq!(response.last_message_ts, Some(1_700_000_000_001i64));
        assert!(response.room_type.is_none());
    }

    #[test]
    fn test_to_response_with_heroes() {
        let summary = RoomSummary {
            id: Some(1),
            room_id: "!room:example.com".to_string(),
            room_type: None,
            name: Some("Test Room".to_string()),
            topic: None,
            avatar_url: None,
            canonical_alias: None,
            join_rule: "public".to_string(),
            history_visibility: "shared".to_string(),
            guest_access: "forbidden".to_string(),
            is_direct: true,
            is_space: false,
            is_encrypted: false,
            member_count: 3,
            joined_member_count: 3,
            invited_member_count: 0,
            hero_users: serde_json::json!([]),
            last_event_id: None,
            last_event_ts: None,
            last_message_ts: None,
            unread_notifications: 0,
            unread_highlight: 0,
            updated_ts: None,
            created_ts: None,
        };

        let heroes = vec![
            RoomSummaryHero {
                user_id: "@alice:example.com".to_string(),
                display_name: Some("Alice".to_string()),
                avatar_url: Some("mxc://alice".to_string()),
            },
            RoomSummaryHero {
                user_id: "@bob:example.com".to_string(),
                display_name: Some("Bob".to_string()),
                avatar_url: None,
            },
        ];

        let response = summary.to_response(heroes);

        assert_eq!(response.room_id, "!room:example.com");
        assert_eq!(response.is_direct, true);
        assert_eq!(response.heroes.len(), 2);
        assert_eq!(response.heroes[0].user_id, "@alice:example.com");
        assert_eq!(response.heroes[0].display_name, Some("Alice".to_string()));
        assert_eq!(response.heroes[0].avatar_url, Some("mxc://alice".to_string()));
        assert_eq!(response.heroes[1].user_id, "@bob:example.com");
        assert_eq!(response.heroes[1].display_name, Some("Bob".to_string()));
        assert!(response.heroes[1].avatar_url.is_none());
        assert!(response.last_event_ts.is_none());
        assert!(response.topic.is_none());
    }

    #[test]
    fn test_to_response_preserves_all_fields() {
        let summary = RoomSummary {
            id: Some(1),
            room_id: "!room:example.com".to_string(),
            room_type: Some("m.space".to_string()),
            name: Some("Space".to_string()),
            topic: Some("A space room".to_string()),
            avatar_url: Some("mxc://space_avatar".to_string()),
            canonical_alias: Some("#space:example.com".to_string()),
            join_rule: "knock".to_string(),
            history_visibility: "world_readable".to_string(),
            guest_access: "can_join".to_string(),
            is_direct: false,
            is_space: true,
            is_encrypted: false,
            member_count: 10,
            joined_member_count: 8,
            invited_member_count: 2,
            hero_users: serde_json::json!([]),
            last_event_id: None,
            last_event_ts: Some(1_700_000_000_000i64),
            last_message_ts: Some(1_700_000_000_001i64),
            unread_notifications: 0,
            unread_highlight: 0,
            updated_ts: Some(1_700_000_000_000i64),
            created_ts: Some(1_700_000_000_000i64),
        };

        let hero = RoomSummaryHero {
            user_id: "@admin:example.com".to_string(),
            display_name: Some("Admin".to_string()),
            avatar_url: Some("mxc://admin".to_string()),
        };

        let response = summary.to_response(vec![hero]);

        assert_eq!(response.room_id, "!room:example.com");
        assert_eq!(response.room_type, Some("m.space".to_string()));
        assert_eq!(response.name, Some("Space".to_string()));
        assert_eq!(response.topic, Some("A space room".to_string()));
        assert_eq!(response.avatar_url, Some("mxc://space_avatar".to_string()));
        assert_eq!(response.canonical_alias, Some("#space:example.com".to_string()));
        assert_eq!(response.join_rule, "knock");
        assert_eq!(response.history_visibility, "world_readable");
        assert_eq!(response.guest_access, "can_join");
        assert_eq!(response.is_space, true);
        assert_eq!(response.is_direct, false);
        assert_eq!(response.is_encrypted, false);
        assert_eq!(response.member_count, 10);
        assert_eq!(response.joined_member_count, 8);
        assert_eq!(response.invited_member_count, 2);
        assert_eq!(response.heroes.len(), 1);
        assert_eq!(response.heroes[0].user_id, "@admin:example.com");
        assert_eq!(response.last_event_ts, Some(1_700_000_000_000i64));
        assert_eq!(response.last_message_ts, Some(1_700_000_000_001i64));
    }

    // ── RoomSummaryMember → RoomSummaryHero conversion tests ──

    #[test]
    fn test_member_to_hero_conversion_all_fields() {
        let member = RoomSummaryMember {
            id: 1,
            room_id: "!room:example.com".to_string(),
            user_id: "@alice:example.com".to_string(),
            display_name: Some("Alice".to_string()),
            avatar_url: Some("mxc://alice".to_string()),
            membership: "join".to_string(),
            is_hero: true,
            last_active_ts: Some(1_700_000_000_000i64),
            updated_ts: 1_700_000_000_000i64,
            created_ts: 1_700_000_000_000i64,
        };

        let hero: RoomSummaryHero = member.into();

        assert_eq!(hero.user_id, "@alice:example.com");
        assert_eq!(hero.display_name, Some("Alice".to_string()));
        assert_eq!(hero.avatar_url, Some("mxc://alice".to_string()));
    }

    #[test]
    fn test_member_to_hero_conversion_optional_none() {
        let member = RoomSummaryMember {
            id: 2,
            room_id: "!room:example.com".to_string(),
            user_id: "@bob:example.com".to_string(),
            display_name: None,
            avatar_url: None,
            membership: "leave".to_string(),
            is_hero: false,
            last_active_ts: None,
            updated_ts: 1_700_000_000_000i64,
            created_ts: 1_700_000_000_000i64,
        };

        let hero: RoomSummaryHero = RoomSummaryHero::from(member);

        assert_eq!(hero.user_id, "@bob:example.com");
        assert!(hero.display_name.is_none());
        assert!(hero.avatar_url.is_none());
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod db_tests {
    use super::*;
    use serde_json::json;
    use sqlx::postgres::PgPoolOptions;
    use sqlx::PgPool;
    use std::env;
    use std::sync::Arc;

    async fn test_pool() -> Arc<PgPool> {
        let db_url = env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
        let pool =
            PgPoolOptions::new().max_connections(2).connect(&db_url).await.expect("Failed to connect to test database");
        Arc::new(pool)
    }

    async fn ensure_test_user(pool: &PgPool, user_id: &str) {
        let username = user_id.strip_prefix('@').and_then(|u| u.split(':').next()).unwrap_or("testuser");
        sqlx::query(
            "INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, EXTRACT(EPOCH FROM NOW()) * 1000) ON CONFLICT (user_id) DO NOTHING",
        )
        .bind(user_id)
        .bind(username)
        .execute(pool)
        .await
        .ok();
    }

    async fn ensure_test_room(pool: &PgPool, room_id: &str) {
        sqlx::query(
            "INSERT INTO rooms (room_id, room_version, is_public, creator, created_ts) VALUES ($1, '1', false, '@test:localhost', EXTRACT(EPOCH FROM NOW()) * 1000) ON CONFLICT (room_id) DO NOTHING",
        )
        .bind(room_id)
        .execute(pool)
        .await
        .ok();
    }

    /// Delete test data from all room-summary tables in FK-safe order.
    async fn cleanup_summary_data(pool: &PgPool, suffix: &str) {
        let pattern = format!("%{suffix}");
        // state depends on rooms
        let _ = sqlx::query("DELETE FROM room_summary_state WHERE room_id LIKE $1").bind(&pattern).execute(pool).await;
        // members depends on rooms + users
        let _ =
            sqlx::query("DELETE FROM room_summary_members WHERE room_id LIKE $1").bind(&pattern).execute(pool).await;
        // stats depends on rooms
        let _ = sqlx::query("DELETE FROM room_summary_stats WHERE room_id LIKE $1").bind(&pattern).execute(pool).await;
        // queue depends on rooms
        let _ = sqlx::query("DELETE FROM room_summary_update_queue WHERE room_id LIKE $1")
            .bind(&pattern)
            .execute(pool)
            .await;
        // summaries depends on rooms
        let _ = sqlx::query("DELETE FROM room_summaries WHERE room_id LIKE $1").bind(&pattern).execute(pool).await;
        // FK-parents: clean test rooms and users last
        let _ = sqlx::query("DELETE FROM rooms WHERE room_id LIKE $1").bind(&pattern).execute(pool).await;
        let _ = sqlx::query("DELETE FROM users WHERE user_id LIKE $1").bind(&pattern).execute(pool).await;
    }

    fn make_suffix() -> String {
        uuid::Uuid::new_v4().to_string().replace('-', "")
    }

    // ── create_summary ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_create_summary_with_all_fields() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        let room_id = format!("!rs_cs_{suffix}:localhost");
        cleanup_summary_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_id).await;

        let storage = RoomSummaryStorage::new(&pool);
        let request = CreateRoomSummaryRequest {
            room_id: room_id.clone(),
            room_type: Some("m.space".to_string()),
            name: Some("My Test Room".to_string()),
            topic: Some("Testing room summary".to_string()),
            avatar_url: Some("mxc://test.org/avatar".to_string()),
            canonical_alias: Some("#myroom:localhost".to_string()),
            join_rule: Some("invite".to_string()),
            history_visibility: Some("joined".to_string()),
            guest_access: Some("can_join".to_string()),
            is_direct: Some(false),
            is_space: Some(true),
        };

        let summary = storage.create_summary(request).await.unwrap();

        assert!(summary.id.is_some());
        assert_eq!(summary.room_id, room_id);
        assert_eq!(summary.room_type.as_deref(), Some("m.space"));
        assert_eq!(summary.name.as_deref(), Some("My Test Room"));
        assert_eq!(summary.topic.as_deref(), Some("Testing room summary"));
        assert_eq!(summary.avatar_url.as_deref(), Some("mxc://test.org/avatar"));
        assert_eq!(summary.canonical_alias.as_deref(), Some("#myroom:localhost"));
        assert_eq!(summary.join_rule, "invite");
        assert_eq!(summary.history_visibility, "joined");
        assert_eq!(summary.guest_access, "can_join");
        assert!(!summary.is_direct);
        assert!(summary.is_space);
        assert!(!summary.is_encrypted);
        assert_eq!(summary.member_count, 0);
        assert_eq!(summary.joined_member_count, 0);
        assert_eq!(summary.invited_member_count, 0);
        assert!(summary.created_ts.is_some());
        assert_eq!(summary.updated_ts, summary.created_ts);

        cleanup_summary_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_create_summary_default_values() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        let room_id = format!("!rs_csd_{suffix}:localhost");
        cleanup_summary_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_id).await;

        let storage = RoomSummaryStorage::new(&pool);
        let request = CreateRoomSummaryRequest {
            room_id: room_id.clone(),
            room_type: None,
            name: None,
            topic: None,
            avatar_url: None,
            canonical_alias: None,
            join_rule: None,
            history_visibility: None,
            guest_access: None,
            is_direct: None,
            is_space: None,
        };

        let summary = storage.create_summary(request).await.unwrap();

        assert_eq!(summary.room_id, room_id);
        assert_eq!(summary.join_rule, "invite");
        assert_eq!(summary.history_visibility, "shared");
        assert_eq!(summary.guest_access, "forbidden");
        assert!(!summary.is_direct);
        assert!(!summary.is_space);
        assert_eq!(summary.unread_notifications, 0);
        assert_eq!(summary.unread_highlight, 0);

        cleanup_summary_data(&pool, &suffix).await;
    }

    // ── get_summary ─────────────────────────────────────────────────

    #[tokio::test]
    async fn test_get_summary_found() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        let room_id = format!("!rs_gs_{suffix}:localhost");
        cleanup_summary_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_id).await;

        let storage = RoomSummaryStorage::new(&pool);
        storage
            .create_summary(CreateRoomSummaryRequest {
                room_id: room_id.clone(),
                room_type: None,
                name: Some("Found Me".to_string()),
                topic: None,
                avatar_url: None,
                canonical_alias: None,
                join_rule: None,
                history_visibility: None,
                guest_access: None,
                is_direct: None,
                is_space: None,
            })
            .await
            .unwrap();

        let found = storage.get_summary(&room_id).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name.as_deref(), Some("Found Me"));

        cleanup_summary_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_summary_not_found() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        let nonexistent = format!("!nonexistent_{suffix}:localhost");
        cleanup_summary_data(&pool, &suffix).await;

        let storage = RoomSummaryStorage::new(&pool);
        let result = storage.get_summary(&nonexistent).await.unwrap();
        assert!(result.is_none());

        cleanup_summary_data(&pool, &suffix).await;
    }

    // ── update_summary ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_update_summary_updates_fields() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        let room_id = format!("!rs_us_{suffix}:localhost");
        cleanup_summary_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_id).await;

        let storage = RoomSummaryStorage::new(&pool);
        storage
            .create_summary(CreateRoomSummaryRequest {
                room_id: room_id.clone(),
                room_type: None,
                name: Some("Original".to_string()),
                topic: None,
                avatar_url: None,
                canonical_alias: None,
                join_rule: None,
                history_visibility: None,
                guest_access: None,
                is_direct: None,
                is_space: None,
            })
            .await
            .unwrap();

        let updated = storage
            .update_summary(
                &room_id,
                UpdateRoomSummaryRequest {
                    name: Some("Updated Name".to_string()),
                    topic: Some("Updated Topic".to_string()),
                    is_encrypted: Some(true),
                    last_event_ts: Some(1_700_000_000_000i64),
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        assert_eq!(updated.name.as_deref(), Some("Updated Name"));
        assert_eq!(updated.topic.as_deref(), Some("Updated Topic"));
        assert!(updated.is_encrypted);
        assert_eq!(updated.last_event_ts, Some(1_700_000_000_000i64));

        cleanup_summary_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_update_summary_keeps_unchanged_fields() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        let room_id = format!("!rs_usk_{suffix}:localhost");
        cleanup_summary_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_id).await;

        let storage = RoomSummaryStorage::new(&pool);
        storage
            .create_summary(CreateRoomSummaryRequest {
                room_id: room_id.clone(),
                room_type: Some("m.room".to_string()),
                name: Some("Keep".to_string()),
                topic: Some("Keep topic".to_string()),
                avatar_url: None,
                canonical_alias: None,
                join_rule: None,
                history_visibility: None,
                guest_access: None,
                is_direct: None,
                is_space: None,
            })
            .await
            .unwrap();

        let updated = storage
            .update_summary(
                &room_id,
                UpdateRoomSummaryRequest { name: Some("New Name".to_string()), ..Default::default() },
            )
            .await
            .unwrap();

        assert_eq!(updated.name.as_deref(), Some("New Name"));
        // unchanged fields should stay
        assert_eq!(updated.topic.as_deref(), Some("Keep topic"));
        assert_eq!(updated.room_type.as_deref(), Some("m.room"));

        cleanup_summary_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_update_summary_not_found() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        let room_id = format!("!rs_usnf_{suffix}:localhost");
        cleanup_summary_data(&pool, &suffix).await;

        let storage = RoomSummaryStorage::new(&pool);
        let result = storage
            .update_summary(
                &room_id,
                UpdateRoomSummaryRequest { name: Some("Ghost".to_string()), ..Default::default() },
            )
            .await;

        assert!(result.is_err());

        cleanup_summary_data(&pool, &suffix).await;
    }

    // ── set_canonical_alias ─────────────────────────────────────────

    #[tokio::test]
    async fn test_set_canonical_alias_sets_and_clears() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        let room_id = format!("!rs_sca_{suffix}:localhost");
        cleanup_summary_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_id).await;

        let storage = RoomSummaryStorage::new(&pool);
        storage
            .create_summary(CreateRoomSummaryRequest {
                room_id: room_id.clone(),
                room_type: None,
                name: None,
                topic: None,
                avatar_url: None,
                canonical_alias: None,
                join_rule: None,
                history_visibility: None,
                guest_access: None,
                is_direct: None,
                is_space: None,
            })
            .await
            .unwrap();

        // Set alias
        let summary = storage.set_canonical_alias(&room_id, Some("#testalias:localhost")).await.unwrap();
        assert_eq!(summary.canonical_alias.as_deref(), Some("#testalias:localhost"));

        // Clear alias
        let summary = storage.set_canonical_alias(&room_id, None).await.unwrap();
        assert!(summary.canonical_alias.is_none());

        cleanup_summary_data(&pool, &suffix).await;
    }

    // ── delete_summary ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_delete_summary_removes_record() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        let room_id = format!("!rs_ds_{suffix}:localhost");
        cleanup_summary_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_id).await;

        let storage = RoomSummaryStorage::new(&pool);
        storage
            .create_summary(CreateRoomSummaryRequest {
                room_id: room_id.clone(),
                room_type: None,
                name: None,
                topic: None,
                avatar_url: None,
                canonical_alias: None,
                join_rule: None,
                history_visibility: None,
                guest_access: None,
                is_direct: None,
                is_space: None,
            })
            .await
            .unwrap();

        storage.delete_summary(&room_id).await.unwrap();
        let result = storage.get_summary(&room_id).await.unwrap();
        assert!(result.is_none());

        // Idempotent — second delete should not error
        storage.delete_summary(&room_id).await.unwrap();

        cleanup_summary_data(&pool, &suffix).await;
    }

    // ── get_summaries_by_ids ────────────────────────────────────────

    #[tokio::test]
    async fn test_get_summaries_by_ids_multiple() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        let room_a = format!("!rs_gsi_a_{suffix}:localhost");
        let room_b = format!("!rs_gsi_b_{suffix}:localhost");
        let room_c = format!("!rs_gsi_c_{suffix}:localhost");
        cleanup_summary_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_a).await;
        ensure_test_room(&pool, &room_b).await;
        ensure_test_room(&pool, &room_c).await;

        let storage = RoomSummaryStorage::new(&pool);
        let req = |id: &str| CreateRoomSummaryRequest {
            room_id: id.to_string(),
            room_type: None,
            name: None,
            topic: None,
            avatar_url: None,
            canonical_alias: None,
            join_rule: None,
            history_visibility: None,
            guest_access: None,
            is_direct: None,
            is_space: None,
        };
        storage.create_summary(req(&room_a)).await.unwrap();
        storage.create_summary(req(&room_b)).await.unwrap();
        storage.create_summary(req(&room_c)).await.unwrap();

        let ids = vec![room_a.clone(), room_b.clone(), room_c.clone()];
        let results = storage.get_summaries_by_ids(&ids).await.unwrap();
        assert_eq!(results.len(), 3);

        // Partial — one matching
        let partial = storage.get_summaries_by_ids(&[room_a.clone()]).await.unwrap();
        assert_eq!(partial.len(), 1);
        assert_eq!(partial[0].room_id, room_a);

        // Empty input
        let empty: Vec<String> = vec![];
        let results = storage.get_summaries_by_ids(&empty).await.unwrap();
        assert!(results.is_empty());

        cleanup_summary_data(&pool, &suffix).await;
    }

    // ── get_summaries_for_user ──────────────────────────────────────

    #[tokio::test]
    async fn test_get_summaries_for_user_returns_joined_rooms() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        let room_a = format!("!rs_gsu_a_{suffix}:localhost");
        let room_b = format!("!rs_gsu_b_{suffix}:localhost");
        let user_id = format!("@rs_gsu_{suffix}:localhost");
        cleanup_summary_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_a).await;
        ensure_test_room(&pool, &room_b).await;
        ensure_test_user(&pool, &user_id).await;

        let storage = RoomSummaryStorage::new(&pool);
        let req = |id: &str| CreateRoomSummaryRequest {
            room_id: id.to_string(),
            room_type: None,
            name: None,
            topic: None,
            avatar_url: None,
            canonical_alias: None,
            join_rule: None,
            history_visibility: None,
            guest_access: None,
            is_direct: None,
            is_space: None,
        };
        storage.create_summary(req(&room_a)).await.unwrap();
        storage.create_summary(req(&room_b)).await.unwrap();

        // Add user to room_a only
        storage
            .add_member(CreateSummaryMemberRequest {
                room_id: room_a.clone(),
                user_id: user_id.clone(),
                display_name: Some("TestUser".to_string()),
                avatar_url: None,
                membership: "join".to_string(),
                is_hero: None,
                last_active_ts: None,
            })
            .await
            .unwrap();

        let summaries = storage.get_summaries_for_user(&user_id).await.unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].room_id, room_a);

        cleanup_summary_data(&pool, &suffix).await;
    }

    // ── add_member ──────────────────────────────────────────────────

    #[tokio::test]
    async fn test_add_member_creates_record() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        let room_id = format!("!rs_am_{suffix}:localhost");
        let user_id = format!("@rs_am_{suffix}:localhost");
        cleanup_summary_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_id).await;
        ensure_test_user(&pool, &user_id).await;

        let storage = RoomSummaryStorage::new(&pool);
        storage
            .create_summary(CreateRoomSummaryRequest {
                room_id: room_id.clone(),
                room_type: None,
                name: None,
                topic: None,
                avatar_url: None,
                canonical_alias: None,
                join_rule: None,
                history_visibility: None,
                guest_access: None,
                is_direct: None,
                is_space: None,
            })
            .await
            .unwrap();

        let member = storage
            .add_member(CreateSummaryMemberRequest {
                room_id: room_id.clone(),
                user_id: user_id.clone(),
                display_name: Some("Alice".to_string()),
                avatar_url: Some("mxc://alice".to_string()),
                membership: "join".to_string(),
                is_hero: Some(true),
                last_active_ts: Some(1_700_000_000_000i64),
            })
            .await
            .unwrap();

        assert_eq!(member.room_id, room_id);
        assert_eq!(member.user_id, user_id);
        assert_eq!(member.display_name.as_deref(), Some("Alice"));
        assert_eq!(member.avatar_url.as_deref(), Some("mxc://alice"));
        assert_eq!(member.membership, "join");
        assert!(member.is_hero);

        // Verify member counts were refreshed
        let summary = storage.get_summary(&room_id).await.unwrap().unwrap();
        assert_eq!(summary.member_count, 1);
        assert_eq!(summary.joined_member_count, 1);
        assert_eq!(summary.invited_member_count, 0);

        cleanup_summary_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_add_member_duplicate_upserts() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        let room_id = format!("!rs_amd_{suffix}:localhost");
        let user_id = format!("@rs_amd_{suffix}:localhost");
        cleanup_summary_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_id).await;
        ensure_test_user(&pool, &user_id).await;

        let storage = RoomSummaryStorage::new(&pool);
        storage
            .create_summary(CreateRoomSummaryRequest {
                room_id: room_id.clone(),
                room_type: None,
                name: None,
                topic: None,
                avatar_url: None,
                canonical_alias: None,
                join_rule: None,
                history_visibility: None,
                guest_access: None,
                is_direct: None,
                is_space: None,
            })
            .await
            .unwrap();

        // First add (creates the record)
        let _m1 = storage
            .add_member(CreateSummaryMemberRequest {
                room_id: room_id.clone(),
                user_id: user_id.clone(),
                display_name: Some("First".to_string()),
                avatar_url: None,
                membership: "join".to_string(),
                is_hero: Some(false),
                last_active_ts: None,
            })
            .await
            .unwrap();

        // Duplicate with different membership
        let m2 = storage
            .add_member(CreateSummaryMemberRequest {
                room_id: room_id.clone(),
                user_id: user_id.clone(),
                display_name: Some("Second".to_string()),
                avatar_url: Some("mxc://second".to_string()),
                membership: "leave".to_string(),
                is_hero: None,
                last_active_ts: None,
            })
            .await
            .unwrap();

        // Should update membership and overwrite display_name (COALESCE prefers EXCLUDED when non-null)
        assert_eq!(m2.membership, "leave");
        assert_eq!(m2.display_name.as_deref(), Some("Second"));
        assert_eq!(m2.avatar_url.as_deref(), Some("mxc://second"));

        cleanup_summary_data(&pool, &suffix).await;
    }

    // ── add_members_batch ───────────────────────────────────────────

    #[tokio::test]
    async fn test_add_members_batch_inserts_multiple() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        let room_id = format!("!rs_amb_{suffix}:localhost");
        let u1 = format!("@rs_amb1_{suffix}:localhost");
        let u2 = format!("@rs_amb2_{suffix}:localhost");
        let u3 = format!("@rs_amb3_{suffix}:localhost");
        cleanup_summary_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_id).await;
        ensure_test_user(&pool, &u1).await;
        ensure_test_user(&pool, &u2).await;
        ensure_test_user(&pool, &u3).await;

        let storage = RoomSummaryStorage::new(&pool);
        storage
            .create_summary(CreateRoomSummaryRequest {
                room_id: room_id.clone(),
                room_type: None,
                name: None,
                topic: None,
                avatar_url: None,
                canonical_alias: None,
                join_rule: None,
                history_visibility: None,
                guest_access: None,
                is_direct: None,
                is_space: None,
            })
            .await
            .unwrap();

        let affected = storage
            .add_members_batch(
                &room_id,
                vec![
                    CreateSummaryMemberRequest {
                        room_id: room_id.clone(),
                        user_id: u1.clone(),
                        display_name: Some("User1".to_string()),
                        avatar_url: None,
                        membership: "join".to_string(),
                        is_hero: Some(true),
                        last_active_ts: None,
                    },
                    CreateSummaryMemberRequest {
                        room_id: room_id.clone(),
                        user_id: u2.clone(),
                        display_name: Some("User2".to_string()),
                        avatar_url: None,
                        membership: "join".to_string(),
                        is_hero: Some(false),
                        last_active_ts: None,
                    },
                    CreateSummaryMemberRequest {
                        room_id: room_id.clone(),
                        user_id: u3.clone(),
                        display_name: Some("User3".to_string()),
                        avatar_url: None,
                        membership: "invite".to_string(),
                        is_hero: None,
                        last_active_ts: None,
                    },
                ],
            )
            .await
            .unwrap();

        assert_eq!(affected, 3);

        let members = storage.get_members(&room_id).await.unwrap();
        assert_eq!(members.len(), 3);

        // Member counts should be refreshed
        let summary = storage.get_summary(&room_id).await.unwrap().unwrap();
        assert_eq!(summary.member_count, 3);
        assert_eq!(summary.joined_member_count, 2);
        assert_eq!(summary.invited_member_count, 1);

        cleanup_summary_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_add_members_batch_empty_returns_zero() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        let room_id = format!("!rs_ambe_{suffix}:localhost");
        cleanup_summary_data(&pool, &suffix).await;

        let storage = RoomSummaryStorage::new(&pool);
        let affected = storage.add_members_batch(&room_id, vec![]).await.unwrap();
        assert_eq!(affected, 0);

        cleanup_summary_data(&pool, &suffix).await;
    }

    // ── update_member ───────────────────────────────────────────────

    #[tokio::test]
    async fn test_update_member_changes_fields() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        let room_id = format!("!rs_um_{suffix}:localhost");
        let user_id = format!("@rs_um_{suffix}:localhost");
        cleanup_summary_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_id).await;
        ensure_test_user(&pool, &user_id).await;

        let storage = RoomSummaryStorage::new(&pool);
        storage
            .create_summary(CreateRoomSummaryRequest {
                room_id: room_id.clone(),
                room_type: None,
                name: None,
                topic: None,
                avatar_url: None,
                canonical_alias: None,
                join_rule: None,
                history_visibility: None,
                guest_access: None,
                is_direct: None,
                is_space: None,
            })
            .await
            .unwrap();
        storage
            .add_member(CreateSummaryMemberRequest {
                room_id: room_id.clone(),
                user_id: user_id.clone(),
                display_name: Some("Original".to_string()),
                avatar_url: None,
                membership: "join".to_string(),
                is_hero: Some(false),
                last_active_ts: None,
            })
            .await
            .unwrap();

        let updated = storage
            .update_member(
                &room_id,
                &user_id,
                UpdateSummaryMemberRequest {
                    display_name: Some("Updated".to_string()),
                    avatar_url: Some("mxc://new_avatar".to_string()),
                    membership: Some("leave".to_string()),
                    is_hero: Some(true),
                    last_active_ts: Some(1_700_000_000_000i64),
                },
            )
            .await
            .unwrap();

        assert_eq!(updated.display_name.as_deref(), Some("Updated"));
        assert_eq!(updated.avatar_url.as_deref(), Some("mxc://new_avatar"));
        assert_eq!(updated.membership, "leave");
        assert!(updated.is_hero);

        cleanup_summary_data(&pool, &suffix).await;
    }

    // ── remove_member ───────────────────────────────────────────────

    #[tokio::test]
    async fn test_remove_member_deletes_and_updates_counts() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        let room_id = format!("!rs_rm_{suffix}:localhost");
        let user_id = format!("@rs_rm_{suffix}:localhost");
        cleanup_summary_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_id).await;
        ensure_test_user(&pool, &user_id).await;

        let storage = RoomSummaryStorage::new(&pool);
        storage
            .create_summary(CreateRoomSummaryRequest {
                room_id: room_id.clone(),
                room_type: None,
                name: None,
                topic: None,
                avatar_url: None,
                canonical_alias: None,
                join_rule: None,
                history_visibility: None,
                guest_access: None,
                is_direct: None,
                is_space: None,
            })
            .await
            .unwrap();
        storage
            .add_member(CreateSummaryMemberRequest {
                room_id: room_id.clone(),
                user_id: user_id.clone(),
                display_name: None,
                avatar_url: None,
                membership: "join".to_string(),
                is_hero: None,
                last_active_ts: None,
            })
            .await
            .unwrap();

        storage.remove_member(&room_id, &user_id).await.unwrap();

        let members = storage.get_members(&room_id).await.unwrap();
        assert!(members.is_empty());

        // Counts should be zero
        let summary = storage.get_summary(&room_id).await.unwrap().unwrap();
        assert_eq!(summary.member_count, 0);

        // Idempotent — removing again should not error
        storage.remove_member(&room_id, &user_id).await.unwrap();

        cleanup_summary_data(&pool, &suffix).await;
    }

    // ── get_members ─────────────────────────────────────────────────

    #[tokio::test]
    async fn test_get_members_returns_ordered_list() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        let room_id = format!("!rs_gm_{suffix}:localhost");
        let u1 = format!("@rs_gma_{suffix}:localhost");
        let u2 = format!("@rs_gmb_{suffix}:localhost");
        cleanup_summary_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_id).await;
        ensure_test_user(&pool, &u1).await;
        ensure_test_user(&pool, &u2).await;

        let storage = RoomSummaryStorage::new(&pool);
        storage
            .create_summary(CreateRoomSummaryRequest {
                room_id: room_id.clone(),
                room_type: None,
                name: None,
                topic: None,
                avatar_url: None,
                canonical_alias: None,
                join_rule: None,
                history_visibility: None,
                guest_access: None,
                is_direct: None,
                is_space: None,
            })
            .await
            .unwrap();

        storage
            .add_members_batch(
                &room_id,
                vec![
                    CreateSummaryMemberRequest {
                        room_id: room_id.clone(),
                        user_id: u2.clone(),
                        display_name: None,
                        avatar_url: None,
                        membership: "join".to_string(),
                        is_hero: Some(false),
                        last_active_ts: None,
                    },
                    CreateSummaryMemberRequest {
                        room_id: room_id.clone(),
                        user_id: u1.clone(),
                        display_name: None,
                        avatar_url: None,
                        membership: "join".to_string(),
                        is_hero: Some(true),
                        last_active_ts: None,
                    },
                ],
            )
            .await
            .unwrap();

        let members = storage.get_members(&room_id).await.unwrap();
        assert_eq!(members.len(), 2);
        // Heroes first, then by user_id
        assert!(members[0].is_hero);
        assert_eq!(members[0].user_id, u1);

        cleanup_summary_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_members_empty_room() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        let room_id = format!("!rs_gme_{suffix}:localhost");
        cleanup_summary_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_id).await;

        let storage = RoomSummaryStorage::new(&pool);
        storage
            .create_summary(CreateRoomSummaryRequest {
                room_id: room_id.clone(),
                room_type: None,
                name: None,
                topic: None,
                avatar_url: None,
                canonical_alias: None,
                join_rule: None,
                history_visibility: None,
                guest_access: None,
                is_direct: None,
                is_space: None,
            })
            .await
            .unwrap();

        let members = storage.get_members(&room_id).await.unwrap();
        assert!(members.is_empty());

        cleanup_summary_data(&pool, &suffix).await;
    }

    // ── get_heroes ──────────────────────────────────────────────────

    #[tokio::test]
    async fn test_get_heroes_ordered_and_limited() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        let room_id = format!("!rs_gh_{suffix}:localhost");
        let u1 = format!("@rs_gh1_{suffix}:localhost");
        let u2 = format!("@rs_gh2_{suffix}:localhost");
        let u3 = format!("@rs_gh3_{suffix}:localhost");
        cleanup_summary_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_id).await;
        ensure_test_user(&pool, &u1).await;
        ensure_test_user(&pool, &u2).await;
        ensure_test_user(&pool, &u3).await;

        let storage = RoomSummaryStorage::new(&pool);
        storage
            .create_summary(CreateRoomSummaryRequest {
                room_id: room_id.clone(),
                room_type: None,
                name: None,
                topic: None,
                avatar_url: None,
                canonical_alias: None,
                join_rule: None,
                history_visibility: None,
                guest_access: None,
                is_direct: None,
                is_space: None,
            })
            .await
            .unwrap();

        storage
            .add_members_batch(
                &room_id,
                vec![
                    CreateSummaryMemberRequest {
                        room_id: room_id.clone(),
                        user_id: u1.clone(),
                        display_name: None,
                        avatar_url: None,
                        membership: "join".to_string(),
                        is_hero: Some(false),
                        last_active_ts: Some(1_700_000_000_001i64),
                    },
                    CreateSummaryMemberRequest {
                        room_id: room_id.clone(),
                        user_id: u2.clone(),
                        display_name: None,
                        avatar_url: None,
                        membership: "join".to_string(),
                        is_hero: Some(true),
                        last_active_ts: Some(1_700_000_000_000i64),
                    },
                    CreateSummaryMemberRequest {
                        room_id: room_id.clone(),
                        user_id: u3.clone(),
                        display_name: None,
                        avatar_url: None,
                        membership: "invite".to_string(),
                        is_hero: Some(false),
                        last_active_ts: None,
                    },
                ],
            )
            .await
            .unwrap();

        // u3 is invited, so should be excluded from heroes (only 'join')
        let heroes = storage.get_heroes(&room_id, 10).await.unwrap();
        assert_eq!(heroes.len(), 2);
        // Heroes (is_hero=true) first
        assert!(heroes[0].is_hero);
        assert_eq!(heroes[0].user_id, u2);

        // Respect limit
        let limited = storage.get_heroes(&room_id, 1).await.unwrap();
        assert_eq!(limited.len(), 1);
        assert_eq!(limited[0].user_id, u2);

        cleanup_summary_data(&pool, &suffix).await;
    }

    // ── get_heroes_batch ────────────────────────────────────────────

    #[tokio::test]
    async fn test_get_heroes_batch_multiple_rooms() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        let room_a = format!("!rs_ghb_a_{suffix}:localhost");
        let room_b = format!("!rs_ghb_b_{suffix}:localhost");
        let u1 = format!("@rs_ghb1_{suffix}:localhost");
        let u2 = format!("@rs_ghb2_{suffix}:localhost");
        cleanup_summary_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_a).await;
        ensure_test_room(&pool, &room_b).await;
        ensure_test_user(&pool, &u1).await;
        ensure_test_user(&pool, &u2).await;

        let storage = RoomSummaryStorage::new(&pool);
        let req = |id: &str| CreateRoomSummaryRequest {
            room_id: id.to_string(),
            room_type: None,
            name: None,
            topic: None,
            avatar_url: None,
            canonical_alias: None,
            join_rule: None,
            history_visibility: None,
            guest_access: None,
            is_direct: None,
            is_space: None,
        };
        storage.create_summary(req(&room_a)).await.unwrap();
        storage.create_summary(req(&room_b)).await.unwrap();

        // Add member to room_a only
        storage
            .add_member(CreateSummaryMemberRequest {
                room_id: room_a.clone(),
                user_id: u1.clone(),
                display_name: None,
                avatar_url: None,
                membership: "join".to_string(),
                is_hero: Some(true),
                last_active_ts: None,
            })
            .await
            .unwrap();

        let result = storage.get_heroes_batch(&[room_a.clone(), room_b.clone()], 10).await.unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[&room_a].len(), 1);
        assert_eq!(result[&room_a][0].user_id, u1);
        assert!(result[&room_b].is_empty());

        // Empty input
        let empty_result = storage.get_heroes_batch(&[], 10).await.unwrap();
        assert!(empty_result.is_empty());

        cleanup_summary_data(&pool, &suffix).await;
    }

    // ── get_hero_candidates ─────────────────────────────────────────

    #[tokio::test]
    async fn test_get_hero_candidates_returns_joined_sorted() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        let room_id = format!("!rs_ghc_{suffix}:localhost");
        let u1 = format!("@rs_ghc1_{suffix}:localhost");
        let u2 = format!("@rs_ghc2_{suffix}:localhost");
        let u3 = format!("@rs_ghc3_{suffix}:localhost");
        cleanup_summary_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_id).await;
        ensure_test_user(&pool, &u1).await;
        ensure_test_user(&pool, &u2).await;
        ensure_test_user(&pool, &u3).await;

        let storage = RoomSummaryStorage::new(&pool);
        storage
            .create_summary(CreateRoomSummaryRequest {
                room_id: room_id.clone(),
                room_type: None,
                name: None,
                topic: None,
                avatar_url: None,
                canonical_alias: None,
                join_rule: None,
                history_visibility: None,
                guest_access: None,
                is_direct: None,
                is_space: None,
            })
            .await
            .unwrap();

        storage
            .add_members_batch(
                &room_id,
                vec![
                    CreateSummaryMemberRequest {
                        room_id: room_id.clone(),
                        user_id: u1.clone(),
                        display_name: None,
                        avatar_url: None,
                        membership: "join".to_string(),
                        is_hero: Some(false),
                        last_active_ts: Some(1_700_000_000_003i64),
                    },
                    CreateSummaryMemberRequest {
                        room_id: room_id.clone(),
                        user_id: u2.clone(),
                        display_name: None,
                        avatar_url: None,
                        membership: "join".to_string(),
                        is_hero: Some(true),
                        last_active_ts: Some(1_700_000_000_001i64),
                    },
                    CreateSummaryMemberRequest {
                        room_id: room_id.clone(),
                        user_id: u3.clone(),
                        display_name: None,
                        avatar_url: None,
                        membership: "leave".to_string(),
                        is_hero: Some(false),
                        last_active_ts: Some(1_700_000_000_002i64),
                    },
                ],
            )
            .await
            .unwrap();

        // Candidates include all joined members, sorted by last_active_ts DESC
        let candidates = storage.get_hero_candidates(&room_id, 10).await.unwrap();
        // u3 is 'leave', so excluded; u1 and u2 are 'join'
        assert_eq!(candidates.len(), 2);
        // Most recently active first
        assert_eq!(candidates[0].user_id, u1);
        assert_eq!(candidates[1].user_id, u2);

        // Respect limit
        let limited = storage.get_hero_candidates(&room_id, 1).await.unwrap();
        assert_eq!(limited.len(), 1);

        cleanup_summary_data(&pool, &suffix).await;
    }

    // ── set_hero_members ────────────────────────────────────────────

    #[tokio::test]
    async fn test_set_hero_members_updates_flags() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        let room_id = format!("!rs_shm_{suffix}:localhost");
        let u1 = format!("@rs_shm1_{suffix}:localhost");
        let u2 = format!("@rs_shm2_{suffix}:localhost");
        let u3 = format!("@rs_shm3_{suffix}:localhost");
        cleanup_summary_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_id).await;
        ensure_test_user(&pool, &u1).await;
        ensure_test_user(&pool, &u2).await;
        ensure_test_user(&pool, &u3).await;

        let storage = RoomSummaryStorage::new(&pool);
        storage
            .create_summary(CreateRoomSummaryRequest {
                room_id: room_id.clone(),
                room_type: None,
                name: None,
                topic: None,
                avatar_url: None,
                canonical_alias: None,
                join_rule: None,
                history_visibility: None,
                guest_access: None,
                is_direct: None,
                is_space: None,
            })
            .await
            .unwrap();

        storage
            .add_members_batch(
                &room_id,
                vec![
                    CreateSummaryMemberRequest {
                        room_id: room_id.clone(),
                        user_id: u1.clone(),
                        display_name: None,
                        avatar_url: None,
                        membership: "join".to_string(),
                        is_hero: Some(true),
                        last_active_ts: None,
                    },
                    CreateSummaryMemberRequest {
                        room_id: room_id.clone(),
                        user_id: u2.clone(),
                        display_name: None,
                        avatar_url: None,
                        membership: "join".to_string(),
                        is_hero: Some(false),
                        last_active_ts: None,
                    },
                    CreateSummaryMemberRequest {
                        room_id: room_id.clone(),
                        user_id: u3.clone(),
                        display_name: None,
                        avatar_url: None,
                        membership: "join".to_string(),
                        is_hero: Some(false),
                        last_active_ts: None,
                    },
                ],
            )
            .await
            .unwrap();

        // Set only u2 and u3 as heroes (u1 gets removed)
        storage.set_hero_members(&room_id, &[u2.clone(), u3.clone()]).await.unwrap();

        let members = storage.get_members(&room_id).await.unwrap();
        let hero_count = members.iter().filter(|m| m.is_hero).count();
        assert_eq!(hero_count, 2);
        let u1_member = members.iter().find(|m| m.user_id == u1).unwrap();
        assert!(!u1_member.is_hero);
        let u2_member = members.iter().find(|m| m.user_id == u2).unwrap();
        assert!(u2_member.is_hero);
        let u3_member = members.iter().find(|m| m.user_id == u3).unwrap();
        assert!(u3_member.is_hero);

        cleanup_summary_data(&pool, &suffix).await;
    }

    // ── set_state / get_state / get_all_state ───────────────────────

    #[tokio::test]
    async fn test_state_crud_single() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        let room_id = format!("!rs_scs_{suffix}:localhost");
        cleanup_summary_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_id).await;

        let storage = RoomSummaryStorage::new(&pool);
        let content = json!({"creator": "@admin:localhost", "room_version": "1"});

        // Create state
        let state = storage.set_state(&room_id, "m.room.create", "", None, content.clone()).await.unwrap();
        assert_eq!(state.room_id, room_id);
        assert_eq!(state.event_type, "m.room.create");
        assert_eq!(state.state_key, "");
        assert_eq!(state.content, content);

        // Fetch it back
        let fetched = storage.get_state(&room_id, "m.room.create", "").await.unwrap();
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().content, content);

        // Fetch nonexistent
        let nonexistent = storage.get_state(&room_id, "m.room.name", "").await.unwrap();
        assert!(nonexistent.is_none());

        cleanup_summary_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_state_upsert_overwrites_existing() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        let room_id = format!("!rs_suo_{suffix}:localhost");
        cleanup_summary_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_id).await;

        let storage = RoomSummaryStorage::new(&pool);

        let s1 = storage
            .set_state(&room_id, "m.room.name", "", Some("$ev1:localhost"), json!({"name": "First"}))
            .await
            .unwrap();
        assert_eq!(s1.event_id.as_deref(), Some("$ev1:localhost"));
        assert_eq!(s1.content, json!({"name": "First"}));

        // Upsert same type+key with different data
        let s2 = storage
            .set_state(&room_id, "m.room.name", "", Some("$ev2:localhost"), json!({"name": "Second"}))
            .await
            .unwrap();
        assert_eq!(s2.event_id.as_deref(), Some("$ev2:localhost"));
        assert_eq!(s2.content, json!({"name": "Second"}));

        cleanup_summary_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_set_states_batch_inserts_all() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        let room_id = format!("!rs_ssb_{suffix}:localhost");
        cleanup_summary_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_id).await;

        let storage = RoomSummaryStorage::new(&pool);

        let affected = storage
            .set_states_batch(
                &room_id,
                &[
                    RoomSummaryStateEntry {
                        event_type: "m.room.create".to_string(),
                        state_key: "".to_string(),
                        event_id: None,
                        content: json!({"creator": "@a:localhost"}),
                    },
                    RoomSummaryStateEntry {
                        event_type: "m.room.name".to_string(),
                        state_key: "".to_string(),
                        event_id: Some("$ev_n:localhost".to_string()),
                        content: json!({"name": "Batch Room"}),
                    },
                    RoomSummaryStateEntry {
                        event_type: "m.room.join_rules".to_string(),
                        state_key: "".to_string(),
                        event_id: None,
                        content: json!({"join_rule": "public"}),
                    },
                ],
            )
            .await
            .unwrap();
        assert_eq!(affected, 3);

        let all = storage.get_all_state(&room_id).await.unwrap();
        assert_eq!(all.len(), 3);

        // Empty batch
        let zero = storage.set_states_batch(&room_id, &[]).await.unwrap();
        assert_eq!(zero, 0);

        cleanup_summary_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_all_state_returns_all_entries() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        let room_id = format!("!rs_gas_{suffix}:localhost");
        cleanup_summary_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_id).await;

        let storage = RoomSummaryStorage::new(&pool);
        storage
            .set_states_batch(
                &room_id,
                &[
                    RoomSummaryStateEntry {
                        event_type: "m.room.create".to_string(),
                        state_key: "".to_string(),
                        event_id: None,
                        content: json!({}),
                    },
                    RoomSummaryStateEntry {
                        event_type: "m.room.member".to_string(),
                        state_key: "@alice:localhost".to_string(),
                        event_id: None,
                        content: json!({"membership": "join"}),
                    },
                ],
            )
            .await
            .unwrap();

        let all = storage.get_all_state(&room_id).await.unwrap();
        assert_eq!(all.len(), 2);

        // Verify types
        let types: Vec<&str> = all.iter().map(|s| s.event_type.as_str()).collect();
        assert!(types.contains(&"m.room.create"));
        assert!(types.contains(&"m.room.member"));

        cleanup_summary_data(&pool, &suffix).await;
    }

    // ── get_stats / update_stats ────────────────────────────────────

    #[tokio::test]
    async fn test_stats_crud() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        let room_id = format!("!rs_st_{suffix}:localhost");
        cleanup_summary_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_id).await;

        let storage = RoomSummaryStorage::new(&pool);

        // Initially no stats
        let initial = storage.get_stats(&room_id).await.unwrap();
        assert!(initial.is_none());

        // Insert stats
        let stats = storage.update_stats(&room_id, 100, 20, 80, 5, 1048576).await.unwrap();
        assert_eq!(stats.total_events, 100);
        assert_eq!(stats.total_state_events, 20);
        assert_eq!(stats.total_messages, 80);
        assert_eq!(stats.total_media, 5);
        assert_eq!(stats.storage_size, 1048576);

        // Fetch back
        let fetched = storage.get_stats(&room_id).await.unwrap().unwrap();
        assert_eq!(fetched.total_events, 100);

        // Update — upsert
        let updated = storage.update_stats(&room_id, 200, 40, 160, 10, 2097152).await.unwrap();
        assert_eq!(updated.total_events, 200);
        assert_eq!(updated.storage_size, 2097152);

        cleanup_summary_data(&pool, &suffix).await;
    }

    // ── queue_update / get_pending_updates / mark_processed / mark_failed ─

    #[tokio::test]
    async fn test_queue_lifecycle() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        let room_id = format!("!rs_ql_{suffix}:localhost");
        cleanup_summary_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_id).await;

        let storage = RoomSummaryStorage::new(&pool);

        // Queue some updates
        storage.queue_update(&room_id, "$ev1", "m.room.message", None, 10).await.unwrap();
        storage.queue_update(&room_id, "$ev2", "m.room.member", Some("@alice:localhost"), 5).await.unwrap();
        storage.queue_update(&room_id, "$ev3", "m.room.name", Some(""), 1).await.unwrap();

        // Get pending (ordered by priority DESC) — may contain entries from other tests
        let pending = storage.get_pending_updates(100).await.unwrap();
        let ours_before: Vec<_> = pending.iter().filter(|p| p.room_id == room_id).collect();
        assert_eq!(ours_before.len(), 3, "should have 3 pending updates for our room");
        // Highest priority first
        assert_eq!(ours_before[0].event_id, "$ev1");
        assert_eq!(ours_before[0].priority, 10);
        assert_eq!(ours_before[1].event_id, "$ev2");
        assert_eq!(ours_before[2].event_id, "$ev3");

        // Respect limit — count of our items in limited result
        let limited = storage.get_pending_updates(1).await.unwrap();
        let ours_limited: Vec<_> = limited.iter().filter(|p| p.room_id == room_id).collect();
        // May be 0 or 1 depending on whether higher-priority items from other tests exist
        assert!(ours_limited.len() <= 1);

        // Mark one of ours as processed
        let target = ours_before[0];
        storage.mark_update_processed(target.id).await.unwrap();
        let after = storage.get_pending_updates(100).await.unwrap();
        let ours_after: Vec<_> = after.iter().filter(|p| p.room_id == room_id).collect();
        assert_eq!(ours_after.len(), 2, "should have 2 pending after marking one processed");

        // Mark another of ours as failed
        storage.mark_update_failed(ours_after[0].id, "test error").await.unwrap();

        cleanup_summary_data(&pool, &suffix).await;
    }

    // ── unread notifications ────────────────────────────────────────

    #[tokio::test]
    async fn test_unread_notifications_increment_and_clear() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        let room_id = format!("!rs_un_{suffix}:localhost");
        cleanup_summary_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_id).await;

        let storage = RoomSummaryStorage::new(&pool);
        storage
            .create_summary(CreateRoomSummaryRequest {
                room_id: room_id.clone(),
                room_type: None,
                name: None,
                topic: None,
                avatar_url: None,
                canonical_alias: None,
                join_rule: None,
                history_visibility: None,
                guest_access: None,
                is_direct: None,
                is_space: None,
            })
            .await
            .unwrap();

        // Increment regular
        storage.increment_unread_notifications(&room_id, false).await.unwrap();
        let s = storage.get_summary(&room_id).await.unwrap().unwrap();
        assert_eq!(s.unread_notifications, 1);
        assert_eq!(s.unread_highlight, 0);

        // Increment highlight
        storage.increment_unread_notifications(&room_id, true).await.unwrap();
        let s = storage.get_summary(&room_id).await.unwrap().unwrap();
        assert_eq!(s.unread_notifications, 2);
        assert_eq!(s.unread_highlight, 1);

        // Clear
        storage.clear_unread_notifications(&room_id).await.unwrap();
        let s = storage.get_summary(&room_id).await.unwrap().unwrap();
        assert_eq!(s.unread_notifications, 0);
        assert_eq!(s.unread_highlight, 0);

        cleanup_summary_data(&pool, &suffix).await;
    }
}
