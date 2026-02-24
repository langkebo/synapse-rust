use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RoomSummary {
    pub id: i64,
    pub room_id: String,
    pub room_type: Option<String>,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub avatar_url: Option<String>,
    pub canonical_alias: Option<String>,
    pub join_rules: String,
    pub history_visibility: String,
    pub guest_access: String,
    pub is_direct: bool,
    pub is_space: bool,
    pub is_encrypted: bool,
    pub member_count: i32,
    pub joined_member_count: i32,
    pub invited_member_count: i32,
    pub hero_users: serde_json::Value,
    pub last_event_id: Option<String>,
    pub last_event_ts: Option<i64>,
    pub last_message_ts: Option<i64>,
    pub unread_notifications: i32,
    pub unread_highlight: i32,
    pub updated_ts: i64,
    pub created_ts: i64,
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
    pub join_rules: Option<String>,
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
    pub join_rules: Option<String>,
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
    pub join_rules: String,
    pub history_visibility: String,
    pub guest_access: String,
    pub is_direct: bool,
    pub is_space: bool,
    pub is_encrypted: bool,
    pub member_count: i32,
    pub joined_member_count: i32,
    pub invited_member_count: i32,
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

    pub async fn create_summary(
        &self,
        request: CreateRoomSummaryRequest,
    ) -> Result<RoomSummary, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let row = sqlx::query_as::<_, RoomSummary>(
            r#"
            INSERT INTO room_summaries (
                room_id, room_type, name, topic, avatar_url, canonical_alias,
                join_rules, history_visibility, guest_access, is_direct, is_space,
                member_count, joined_member_count, invited_member_count, hero_users,
                updated_ts, created_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, 0, 0, 0, '[]'::jsonb, $12, $13)
            RETURNING *
            "#,
        )
        .bind(&request.room_id)
        .bind(&request.room_type)
        .bind(&request.name)
        .bind(&request.topic)
        .bind(&request.avatar_url)
        .bind(&request.canonical_alias)
        .bind(request.join_rules.unwrap_or_else(|| "invite".to_string()))
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
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_summary(&self, room_id: &str) -> Result<Option<RoomSummary>, sqlx::Error> {
        let row =
            sqlx::query_as::<_, RoomSummary>("SELECT * FROM room_summaries WHERE room_id = $1")
                .bind(room_id)
                .fetch_optional(&*self.pool)
                .await?;

        Ok(row)
    }

    pub async fn update_summary(
        &self,
        room_id: &str,
        request: UpdateRoomSummaryRequest,
    ) -> Result<RoomSummary, sqlx::Error> {
        let row = sqlx::query_as::<_, RoomSummary>(
            r#"
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
                hero_users = COALESCE($15, hero_users)
            WHERE room_id = $1
            RETURNING *
            "#,
        )
        .bind(room_id)
        .bind(&request.name)
        .bind(&request.topic)
        .bind(&request.avatar_url)
        .bind(&request.canonical_alias)
        .bind(&request.join_rules)
        .bind(&request.history_visibility)
        .bind(&request.guest_access)
        .bind(request.is_direct)
        .bind(request.is_space)
        .bind(request.is_encrypted)
        .bind(&request.last_event_id)
        .bind(request.last_event_ts)
        .bind(request.last_message_ts)
        .bind(&request.hero_users)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn delete_summary(&self, room_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM room_summaries WHERE room_id = $1")
            .bind(room_id)
            .execute(&*self.pool)
            .await?;

        Ok(())
    }

    pub async fn get_summaries_by_ids(
        &self,
        room_ids: &[String],
    ) -> Result<Vec<RoomSummary>, sqlx::Error> {
        let rows = sqlx::query_as::<_, RoomSummary>(
            "SELECT * FROM room_summaries WHERE room_id = ANY($1)",
        )
        .bind(room_ids)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn get_summaries_for_user(
        &self,
        user_id: &str,
    ) -> Result<Vec<RoomSummary>, sqlx::Error> {
        let rows = sqlx::query_as::<_, RoomSummary>(
            r#"
            SELECT rs.* FROM room_summaries rs
            INNER JOIN room_summary_members rsm ON rs.room_id = rsm.room_id
            WHERE rsm.user_id = $1 AND rsm.membership IN ('join', 'invite')
            ORDER BY rs.last_event_ts DESC NULLS LAST
            "#,
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn add_member(
        &self,
        request: CreateSummaryMemberRequest,
    ) -> Result<RoomSummaryMember, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let row = sqlx::query_as::<_, RoomSummaryMember>(
            r#"
            INSERT INTO room_summary_members (
                room_id, user_id, display_name, avatar_url, membership, is_hero, last_active_ts, updated_ts, created_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8)
            ON CONFLICT (room_id, user_id) DO UPDATE SET
                display_name = COALESCE(EXCLUDED.display_name, room_summary_members.display_name),
                avatar_url = COALESCE(EXCLUDED.avatar_url, room_summary_members.avatar_url),
                membership = EXCLUDED.membership,
                last_active_ts = COALESCE(EXCLUDED.last_active_ts, room_summary_members.last_active_ts),
                updated_ts = EXCLUDED.updated_ts
            RETURNING *
            "#,
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

        Ok(row)
    }

    pub async fn update_member(
        &self,
        room_id: &str,
        user_id: &str,
        request: UpdateSummaryMemberRequest,
    ) -> Result<RoomSummaryMember, sqlx::Error> {
        let row = sqlx::query_as::<_, RoomSummaryMember>(
            r#"
            UPDATE room_summary_members SET
                display_name = COALESCE($3, display_name),
                avatar_url = COALESCE($4, avatar_url),
                membership = COALESCE($5, membership),
                is_hero = COALESCE($6, is_hero),
                last_active_ts = COALESCE($7, last_active_ts)
            WHERE room_id = $1 AND user_id = $2
            RETURNING *
            "#,
        )
        .bind(room_id)
        .bind(user_id)
        .bind(&request.display_name)
        .bind(&request.avatar_url)
        .bind(&request.membership)
        .bind(request.is_hero)
        .bind(request.last_active_ts)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn remove_member(&self, room_id: &str, user_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM room_summary_members WHERE room_id = $1 AND user_id = $2")
            .bind(room_id)
            .bind(user_id)
            .execute(&*self.pool)
            .await?;

        Ok(())
    }

    pub async fn get_members(&self, room_id: &str) -> Result<Vec<RoomSummaryMember>, sqlx::Error> {
        let rows = sqlx::query_as::<_, RoomSummaryMember>(
            "SELECT * FROM room_summary_members WHERE room_id = $1 ORDER BY is_hero DESC, user_id",
        )
        .bind(room_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn get_heroes(
        &self,
        room_id: &str,
        limit: i64,
    ) -> Result<Vec<RoomSummaryMember>, sqlx::Error> {
        let rows = sqlx::query_as::<_, RoomSummaryMember>(
            r#"
            SELECT * FROM room_summary_members 
            WHERE room_id = $1 AND membership = 'join'
            ORDER BY is_hero DESC, last_active_ts DESC NULLS LAST
            LIMIT $2
            "#,
        )
        .bind(room_id)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
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
            r#"
            INSERT INTO room_summary_state (room_id, event_type, state_key, event_id, content, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (room_id, event_type, state_key) DO UPDATE SET
                event_id = EXCLUDED.event_id,
                content = EXCLUDED.content,
                updated_ts = EXCLUDED.updated_ts
            RETURNING *
            "#,
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

    pub async fn get_state(
        &self,
        room_id: &str,
        event_type: &str,
        state_key: &str,
    ) -> Result<Option<RoomSummaryState>, sqlx::Error> {
        let row = sqlx::query_as::<_, RoomSummaryState>(
            "SELECT * FROM room_summary_state WHERE room_id = $1 AND event_type = $2 AND state_key = $3",
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
            "SELECT * FROM room_summary_state WHERE room_id = $1",
        )
        .bind(room_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn get_stats(&self, room_id: &str) -> Result<Option<RoomSummaryStats>, sqlx::Error> {
        let row = sqlx::query_as::<_, RoomSummaryStats>(
            "SELECT * FROM room_summary_stats WHERE room_id = $1",
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
            r#"
            INSERT INTO room_summary_stats (room_id, total_events, total_state_events, total_messages, total_media, storage_size, last_updated_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (room_id) DO UPDATE SET
                total_events = EXCLUDED.total_events,
                total_state_events = EXCLUDED.total_state_events,
                total_messages = EXCLUDED.total_messages,
                total_media = EXCLUDED.total_media,
                storage_size = EXCLUDED.storage_size,
                last_updated_ts = EXCLUDED.last_updated_ts
            RETURNING *
            "#,
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
            r#"
            INSERT INTO room_summary_update_queue (room_id, event_id, event_type, state_key, priority, created_ts)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
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

    pub async fn get_pending_updates(
        &self,
        limit: i64,
    ) -> Result<Vec<RoomSummaryUpdateQueueItem>, sqlx::Error> {
        let rows = sqlx::query_as::<_, RoomSummaryUpdateQueueItem>(
            r#"
            SELECT * FROM room_summary_update_queue
            WHERE status = 'pending'
            ORDER BY priority DESC, created_ts ASC
            LIMIT $1
            FOR UPDATE SKIP LOCKED
            "#,
        )
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn mark_update_processed(&self, id: i64) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query(
            "UPDATE room_summary_update_queue SET status = 'processed', processed_ts = $2 WHERE id = $1",
        )
        .bind(id)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn mark_update_failed(&self, id: i64, error: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE room_summary_update_queue SET
                status = 'failed',
                error_message = $2,
                retry_count = retry_count + 1
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(error)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn increment_unread_notifications(
        &self,
        room_id: &str,
        highlight: bool,
    ) -> Result<(), sqlx::Error> {
        if highlight {
            sqlx::query(
                "UPDATE room_summaries SET unread_notifications = unread_notifications + 1, unread_highlight = unread_highlight + 1 WHERE room_id = $1",
            )
            .bind(room_id)
            .execute(&*self.pool)
            .await?;
        } else {
            sqlx::query(
                "UPDATE room_summaries SET unread_notifications = unread_notifications + 1 WHERE room_id = $1",
            )
            .bind(room_id)
            .execute(&*self.pool)
            .await?;
        }

        Ok(())
    }

    pub async fn clear_unread_notifications(&self, room_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE room_summaries SET unread_notifications = 0, unread_highlight = 0 WHERE room_id = $1",
        )
        .bind(room_id)
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
            join_rules: self.join_rules.clone(),
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
        Self {
            user_id: member.user_id,
            display_name: member.display_name,
            avatar_url: member.avatar_url,
        }
    }
}
