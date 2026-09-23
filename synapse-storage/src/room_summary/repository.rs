use super::models::*;
use sqlx::PgPool;
use std::sync::Arc;
use synapse_common::current_timestamp_millis;

/// The `RoomSummaryStorage` struct.
#[derive(Clone)]
pub struct RoomSummaryStorage {
    pool: Arc<PgPool>,
}

impl RoomSummaryStorage {
    /// See [`new`].
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    /// See [`create_summary`].
    pub async fn create_summary(&self, request: CreateRoomSummaryRequest) -> Result<RoomSummary, sqlx::Error> {
        tracing::info!(room_id = %request.room_id, "Creating room summary");
        let now = current_timestamp_millis();

        let row = sqlx::query_as!(
            RoomSummary,
            r"
            INSERT INTO room_summaries (
                room_id, room_type, name, topic, avatar_url, canonical_alias,
                join_rules, history_visibility, guest_access, is_direct, is_space,
                is_encrypted, member_count, joined_member_count, invited_member_count, hero_users,
                unread_notifications, unread_highlight, updated_ts, created_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, FALSE, 0, 0, 0, '[]'::jsonb, 0, 0, $12, $12)
            RETURNING id, room_id, room_type, name, topic, avatar_url, canonical_alias, join_rules AS join_rule, history_visibility, guest_access, is_direct, is_space, is_encrypted, member_count, joined_member_count, invited_member_count, hero_users, last_event_id, last_event_ts, last_message_ts, unread_notifications, unread_highlight, updated_ts, created_ts
            ",
            &request.room_id,
            request.room_type.as_deref(),
            request.name.as_deref(),
            request.topic.as_deref(),
            request.avatar_url.as_deref(),
            request.canonical_alias.as_deref(),
            request.join_rule.unwrap_or_else(|| "invite".to_string()),
            request.history_visibility.unwrap_or_else(|| "shared".to_string()),
            request.guest_access.unwrap_or_else(|| "forbidden".to_string()),
            request.is_direct.unwrap_or(false),
            request.is_space.unwrap_or(false),
            now
        )
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    /// See [`get_summary`].
    pub async fn get_summary(&self, room_id: &str) -> Result<Option<RoomSummary>, sqlx::Error> {
        tracing::debug!(room_id = %room_id, "Querying room summary");
        let row = sqlx::query_as!(
            RoomSummary,
            r"SELECT id, room_id, room_type, name, topic, avatar_url, canonical_alias, join_rules AS join_rule, history_visibility, guest_access, is_direct, is_space, is_encrypted, member_count, joined_member_count, invited_member_count, hero_users, last_event_id, last_event_ts, last_message_ts, unread_notifications, unread_highlight, updated_ts, created_ts FROM room_summaries WHERE room_id = $1",
            room_id
        )
        .fetch_optional(&*self.pool)
        .await?;

        if row.is_none() {
            tracing::warn!(room_id = %room_id, "Room summary not found");
        }

        Ok(row)
    }

    /// See [`update_summary`].
    pub async fn update_summary(
        &self,
        room_id: &str,
        request: UpdateRoomSummaryRequest,
    ) -> Result<RoomSummary, sqlx::Error> {
        tracing::info!(room_id = %room_id, "Updating room summary");
        let now = current_timestamp_millis();
        let row = sqlx::query_as!(
            RoomSummary,
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
            RETURNING id, room_id, room_type, name, topic, avatar_url, canonical_alias, join_rules AS join_rule, history_visibility, guest_access, is_direct, is_space, is_encrypted, member_count, joined_member_count, invited_member_count, hero_users, last_event_id, last_event_ts, last_message_ts, unread_notifications, unread_highlight, updated_ts, created_ts
            ",
            room_id,
            request.name.as_deref(),
            request.topic.as_deref(),
            request.avatar_url.as_deref(),
            request.canonical_alias.as_deref(),
            request.join_rule.as_deref(),
            request.history_visibility.as_deref(),
            request.guest_access.as_deref(),
            request.is_direct,
            request.is_space,
            request.is_encrypted,
            request.last_event_id.as_deref(),
            request.last_event_ts,
            request.last_message_ts,
            request.hero_users.as_ref(),
            now
        )
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    /// See [`set_canonical_alias`].
    pub async fn set_canonical_alias(
        &self,
        room_id: &str,
        canonical_alias: Option<&str>,
    ) -> Result<RoomSummary, sqlx::Error> {
        let now = current_timestamp_millis();
        sqlx::query_as!(
            RoomSummary,
            r"
            UPDATE room_summaries
            SET canonical_alias = $2,
                updated_ts = $3
            WHERE room_id = $1
            RETURNING id, room_id, room_type, name, topic, avatar_url, canonical_alias, join_rules AS join_rule, history_visibility, guest_access, is_direct, is_space, is_encrypted, member_count, joined_member_count, invited_member_count, hero_users, last_event_id, last_event_ts, last_message_ts, unread_notifications, unread_highlight, updated_ts, created_ts
            ",
            room_id,
            canonical_alias,
            now
        )
        .fetch_one(&*self.pool)
        .await
    }

    /// See [`delete_summary`].
    pub async fn delete_summary(&self, room_id: &str) -> Result<(), sqlx::Error> {
        tracing::info!(room_id = %room_id, "Deleting room summary");
        sqlx::query!("DELETE FROM room_summaries WHERE room_id = $1", room_id).execute(&*self.pool).await?;

        Ok(())
    }

    /// See [`get_summaries_by_ids`].
    pub async fn get_summaries_by_ids(&self, room_ids: &[String]) -> Result<Vec<RoomSummary>, sqlx::Error> {
        let rows = sqlx::query_as!(
            RoomSummary,
            r"SELECT id, room_id, room_type, name, topic, avatar_url, canonical_alias, join_rules AS join_rule, history_visibility, guest_access, is_direct, is_space, is_encrypted, member_count, joined_member_count, invited_member_count, hero_users, last_event_id, last_event_ts, last_message_ts, unread_notifications, unread_highlight, updated_ts, created_ts FROM room_summaries WHERE room_id = ANY($1)",
            room_ids
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    /// See [`get_summaries_for_user`].
    pub async fn get_summaries_for_user(&self, user_id: &str) -> Result<Vec<RoomSummary>, sqlx::Error> {
        tracing::debug!(user_id = %user_id, "Querying room summaries for user");
        let rows = sqlx::query_as!(
            RoomSummary,
            r"
            SELECT rs.id, rs.room_id, rs.room_type, rs.name, rs.topic, rs.avatar_url, rs.canonical_alias, rs.join_rules AS join_rule, rs.history_visibility, rs.guest_access, rs.is_direct, rs.is_space, rs.is_encrypted, rs.member_count, rs.joined_member_count, rs.invited_member_count, rs.hero_users, rs.last_event_id, rs.last_event_ts, rs.last_message_ts, rs.unread_notifications, rs.unread_highlight, rs.updated_ts, rs.created_ts FROM room_summaries rs
            INNER JOIN room_summary_members rsm ON rs.room_id = rsm.room_id
            WHERE rsm.user_id = $1 AND rsm.membership IN ('join', 'invite')
            ORDER BY rs.last_event_ts DESC NULLS LAST
            ",
            user_id
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    /// See [`add_member`].
    pub async fn add_member(&self, request: CreateSummaryMemberRequest) -> Result<RoomSummaryMember, sqlx::Error> {
        tracing::info!(room_id = %request.room_id, user_id = %request.user_id, membership = %request.membership, "Adding member to room summary");
        let now = current_timestamp_millis();

        let row = sqlx::query_as!(
            RoomSummaryMember,
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
            &request.room_id,
            &request.user_id,
            request.display_name.as_deref(),
            request.avatar_url.as_deref(),
            &request.membership,
            request.is_hero.unwrap_or(false),
            request.last_active_ts,
            now
        )
        .fetch_one(&*self.pool)
        .await?;

        self.refresh_member_counts(&request.room_id).await?;

        Ok(row)
    }

    /// DB-06 / P0-2: transactional variant of `add_member`. Writes the
    /// `room_summary_members` row inside the caller's transaction, then
    /// runs the member-count refresh in the same transaction so both
    /// tables stay consistent.
    pub async fn add_member_in_tx(
        &self,
        request: CreateSummaryMemberRequest,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> Result<RoomSummaryMember, sqlx::Error> {
        tracing::info!(room_id = %request.room_id, user_id = %request.user_id, membership = %request.membership, "Adding member to room summary (in tx)");
        let now = current_timestamp_millis();

        let row = sqlx::query_as!(
            RoomSummaryMember,
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
            &request.room_id,
            &request.user_id,
            request.display_name.as_deref(),
            request.avatar_url.as_deref(),
            &request.membership,
            request.is_hero.unwrap_or(false),
            request.last_active_ts,
            now
        )
        .fetch_one(&mut **tx)
        .await?;

        // refresh_member_counts has no tx variant; do it inline.
        sqlx::query!(
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
            &request.room_id,
            now
        )
        .execute(&mut **tx)
        .await?;

        Ok(row)
    }

    /// See [`add_members_batch`].
    pub async fn add_members_batch(
        &self,
        room_id: &str,
        members: Vec<CreateSummaryMemberRequest>,
    ) -> Result<usize, sqlx::Error> {
        if members.is_empty() {
            return Ok(0);
        }

        tracing::info!(room_id = %room_id, count = members.len(), "Batch adding members to room summary");

        let now = current_timestamp_millis();
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

        // NOTE(C9): intentionally dynamic. `query!` cannot type-check this call because the
        // per-element-nullable arrays below (`Vec<Option<String>>` / `Vec<Option<i64>>`) have no
        // mapping in sqlx-postgres' `param_type_for_id` (only non-null element arrays such as
        // `Vec<String> | &[String]` are registered), so the macro rejects `&[Option<String>]`
        // with E0308. Tightening direction: pass one `jsonb_to_recordset($2)` argument instead of
        // seven parallel arrays, or wait for sqlx to register `Vec<Option<T>>` params.
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

    /// See [`update_member`].
    pub async fn update_member(
        &self,
        room_id: &str,
        user_id: &str,
        request: UpdateSummaryMemberRequest,
    ) -> Result<RoomSummaryMember, sqlx::Error> {
        tracing::info!(room_id = %room_id, user_id = %user_id, "Updating room summary member");
        let now = current_timestamp_millis();
        let row = sqlx::query_as!(
            RoomSummaryMember,
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
            room_id,
            user_id,
            request.display_name.as_deref(),
            request.avatar_url.as_deref(),
            request.membership.as_deref(),
            request.is_hero,
            request.last_active_ts,
            now
        )
        .fetch_one(&*self.pool)
        .await?;

        self.refresh_member_counts(room_id).await?;

        Ok(row)
    }

    /// See [`remove_member`].
    pub async fn remove_member(&self, room_id: &str, user_id: &str) -> Result<(), sqlx::Error> {
        tracing::info!(room_id = %room_id, user_id = %user_id, "Removing member from room summary");
        sqlx::query!("DELETE FROM room_summary_members WHERE room_id = $1 AND user_id = $2", room_id, user_id)
            .execute(&*self.pool)
            .await?;

        self.refresh_member_counts(room_id).await?;

        Ok(())
    }

    /// See [`get_members`].
    pub async fn get_members(&self, room_id: &str) -> Result<Vec<RoomSummaryMember>, sqlx::Error> {
        let rows = sqlx::query_as!(
            RoomSummaryMember,
            r"SELECT id, room_id, user_id, display_name, avatar_url, membership, is_hero, last_active_ts, updated_ts, created_ts FROM room_summary_members WHERE room_id = $1 ORDER BY is_hero DESC, user_id",
            room_id
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    /// See [`get_heroes`].
    pub async fn get_heroes(&self, room_id: &str, limit: i64) -> Result<Vec<RoomSummaryMember>, sqlx::Error> {
        let rows = sqlx::query_as!(
            RoomSummaryMember,
            r"
            SELECT id, room_id, user_id, display_name, avatar_url, membership, is_hero, last_active_ts, updated_ts, created_ts FROM room_summary_members
            WHERE room_id = $1 AND membership = 'join'
            ORDER BY is_hero DESC, last_active_ts DESC NULLS LAST
            LIMIT $2
            ",
            room_id,
            limit
        )
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

        let rows = sqlx::query_as!(
            RoomSummaryMember,
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
            room_ids,
            limit
        )
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

    /// See [`get_hero_candidates`].
    pub async fn get_hero_candidates(&self, room_id: &str, limit: i64) -> Result<Vec<RoomSummaryMember>, sqlx::Error> {
        let rows = sqlx::query_as!(
            RoomSummaryMember,
            r"
            SELECT id, room_id, user_id, display_name, avatar_url, membership, is_hero, last_active_ts, updated_ts, created_ts
            FROM room_summary_members
            WHERE room_id = $1 AND membership = 'join'
            ORDER BY last_active_ts DESC NULLS LAST, updated_ts DESC, user_id ASC
            LIMIT $2
            ",
            room_id,
            limit
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    /// See [`set_hero_members`].
    pub async fn set_hero_members(&self, room_id: &str, hero_user_ids: &[String]) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r"
            UPDATE room_summary_members
            SET is_hero = user_id = ANY($2::text[])
            WHERE room_id = $1
            ",
            room_id,
            hero_user_ids
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// See [`set_state`].
    pub async fn set_state(
        &self,
        room_id: &str,
        event_type: &str,
        state_key: &str,
        event_id: Option<&str>,
        content: serde_json::Value,
    ) -> Result<RoomSummaryState, sqlx::Error> {
        let now = current_timestamp_millis();

        let row = sqlx::query_as!(
            RoomSummaryState,
            r"
            INSERT INTO room_summary_state (room_id, event_type, state_key, event_id, content, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (room_id, event_type, state_key) DO UPDATE SET
                event_id = EXCLUDED.event_id,
                content = EXCLUDED.content,
                updated_ts = EXCLUDED.updated_ts
            RETURNING id, room_id, event_type, state_key, event_id, content, updated_ts
            ",
            room_id,
            event_type,
            state_key,
            event_id,
            &content,
            now
        )
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

        let now = current_timestamp_millis();
        let event_types: Vec<String> = entries.iter().map(|e| e.event_type.clone()).collect();
        let state_keys: Vec<String> = entries.iter().map(|e| e.state_key.clone()).collect();
        let event_ids: Vec<Option<String>> = entries.iter().map(|e| e.event_id.clone()).collect();
        let contents: Vec<serde_json::Value> = entries.iter().map(|e| e.content.clone()).collect();

        // NOTE(C9): intentionally dynamic, same sqlx limitation as `add_members_batch` above:
        // `event_ids` is `Vec<Option<String>>` and sqlx-postgres registers no `Vec<Option<T>>`
        // param mapping, so `query!` rejects it with E0308. Tightening direction:
        // `jsonb_to_recordset($3)` single-argument form.
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

    /// See [`get_state`].
    pub async fn get_state(
        &self,
        room_id: &str,
        event_type: &str,
        state_key: &str,
    ) -> Result<Option<RoomSummaryState>, sqlx::Error> {
        let row = sqlx::query_as!(
            RoomSummaryState,
            r"SELECT id, room_id, event_type, state_key, event_id, content, updated_ts FROM room_summary_state WHERE room_id = $1 AND event_type = $2 AND state_key = $3",
            room_id,
            event_type,
            state_key
        )
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    /// See [`get_all_state`].
    pub async fn get_all_state(&self, room_id: &str) -> Result<Vec<RoomSummaryState>, sqlx::Error> {
        let rows = sqlx::query_as!(
            RoomSummaryState,
            r"SELECT id, room_id, event_type, state_key, event_id, content, updated_ts FROM room_summary_state WHERE room_id = $1",
            room_id
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    /// See [`get_stats`].
    pub async fn get_stats(&self, room_id: &str) -> Result<Option<RoomSummaryStats>, sqlx::Error> {
        let row = sqlx::query_as!(
            RoomSummaryStats,
            r"SELECT id::BIGINT AS id, room_id, total_events::BIGINT AS total_events, total_state_events::BIGINT AS total_state_events, total_messages::BIGINT AS total_messages, total_media::BIGINT AS total_media, storage_size::BIGINT AS storage_size, last_updated_ts::BIGINT AS last_updated_ts FROM room_summary_stats WHERE room_id = $1",
            room_id
        )
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    /// See [`update_stats`].
    pub async fn update_stats(
        &self,
        room_id: &str,
        total_events: i64,
        total_state_events: i64,
        total_messages: i64,
        total_media: i64,
        storage_size: i64,
    ) -> Result<RoomSummaryStats, sqlx::Error> {
        let now = current_timestamp_millis();

        let row = sqlx::query_as!(
            RoomSummaryStats,
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
            room_id,
            total_events,
            total_state_events,
            total_messages,
            total_media,
            storage_size,
            now
        )
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    /// See [`queue_update`].
    pub async fn queue_update(
        &self,
        room_id: &str,
        event_id: &str,
        event_type: &str,
        state_key: Option<&str>,
        priority: i32,
    ) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();

        sqlx::query!(
            r"
            INSERT INTO room_summary_update_queue (room_id, event_id, event_type, state_key, priority, created_ts)
            VALUES ($1, $2, $3, $4, $5, $6)
            ",
            room_id,
            event_id,
            event_type,
            state_key,
            priority,
            now
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// See [`get_pending_updates`].
    pub async fn get_pending_updates(&self, limit: i64) -> Result<Vec<RoomSummaryUpdateQueueItem>, sqlx::Error> {
        let rows = sqlx::query_as!(
            RoomSummaryUpdateQueueItem,
            r"
            SELECT id, room_id, event_id, event_type, state_key, priority, status, created_ts, processed_ts, error_message, retry_count
            FROM room_summary_update_queue
            WHERE status = 'pending'
            ORDER BY priority DESC, created_ts ASC, id ASC
            LIMIT $1
            FOR UPDATE SKIP LOCKED
            ",
            limit
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    /// See [`mark_update_processed`].
    pub async fn mark_update_processed(&self, id: i64) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();

        sqlx::query!(
            "UPDATE room_summary_update_queue SET status = 'processed', processed_ts = $2 WHERE id = $1",
            id,
            now
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// See [`mark_update_failed`].
    pub async fn mark_update_failed(&self, id: i64, error: &str) -> Result<(), sqlx::Error> {
        tracing::warn!(id = id, error = %error, "Marking room summary update as failed");
        sqlx::query!(
            r"
            UPDATE room_summary_update_queue SET
                status = 'failed',
                error_message = $2,
                retry_count = retry_count + 1
            WHERE id = $1
            ",
            id,
            error
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// See [`increment_unread_notifications`].
    pub async fn increment_unread_notifications(&self, room_id: &str, highlight: bool) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();
        if highlight {
            sqlx::query!(
                "UPDATE room_summaries SET unread_notifications = unread_notifications + 1, unread_highlight = unread_highlight + 1, updated_ts = $2 WHERE room_id = $1",
                room_id,
                now
            )
            .execute(&*self.pool)
            .await?;
        } else {
            sqlx::query!(
                "UPDATE room_summaries SET unread_notifications = unread_notifications + 1, updated_ts = $2 WHERE room_id = $1",
                room_id,
                now
            )
            .execute(&*self.pool)
            .await?;
        }

        Ok(())
    }

    /// See [`clear_unread_notifications`].
    pub async fn clear_unread_notifications(&self, room_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE room_summaries SET unread_notifications = 0, unread_highlight = 0, updated_ts = $2 WHERE room_id = $1",
            room_id,
            current_timestamp_millis()
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    async fn refresh_member_counts(&self, room_id: &str) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();
        sqlx::query!(
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
            room_id,
            now
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }
}
