//! Thread storage: [`ThreadStorage`] pool methods, the [`ThreadStoreApi`] trait
//! and its implementation.

use async_trait::async_trait;
use sqlx::{Pool, Postgres};
use std::sync::Arc;
use synapse_common::current_timestamp_millis;

use super::models::{
    CreateThreadReplyParams, CreateThreadRootParams, ThreadListParams, ThreadReadReceipt, ThreadRelation, ThreadReply,
    ThreadRoot, ThreadStatistics, ThreadSubscription, ThreadSummary,
};
/// The `ThreadStorage` struct.
#[derive(Clone)]
pub struct ThreadStorage {
    /// The `pool` field.
    pub pool: Arc<Pool<Postgres>>,
}

impl ThreadStorage {
    /// See [`new`].
    pub fn new(pool: &Arc<Pool<Postgres>>) -> Self {
        Self { pool: pool.clone() }
    }

    /// See [`create_thread_root`].
    pub async fn create_thread_root(&self, params: CreateThreadRootParams) -> Result<ThreadRoot, sqlx::Error> {
        let now = current_timestamp_millis();

        sqlx::query_as!(
            ThreadRoot,
            r#"
            INSERT INTO thread_roots (
                room_id, root_event_id, sender, thread_id, participants, created_ts
            )
            VALUES ($1, $2, $3, $4, jsonb_build_array($3::TEXT), $5)
            RETURNING id, room_id, root_event_id, sender, thread_id, reply_count,
                      last_reply_event_id, last_reply_sender, last_reply_ts,
                      participants, is_fetched AS "is_fetched!", created_ts, updated_ts
            "#,
            &params.room_id,
            &params.root_event_id,
            &params.sender,
            params.thread_id.as_deref(),
            now
        )
        .fetch_one(&*self.pool)
        .await
    }

    /// See [`get_thread_root`].
    pub async fn get_thread_root(&self, room_id: &str, thread_id: &str) -> Result<Option<ThreadRoot>, sqlx::Error> {
        sqlx::query_as!(
            ThreadRoot,
            r#"
            SELECT id, room_id, root_event_id, sender, thread_id, reply_count,
                   last_reply_event_id, last_reply_sender, last_reply_ts,
                   participants, is_fetched AS "is_fetched!", created_ts, updated_ts
            FROM thread_roots
            WHERE room_id = $1 AND thread_id = $2
            "#,
            room_id,
            thread_id
        )
        .fetch_optional(&*self.pool)
        .await
    }

    /// See [`get_thread_root_by_event`].
    pub async fn get_thread_root_by_event(
        &self,
        room_id: &str,
        root_event_id: &str,
    ) -> Result<Option<ThreadRoot>, sqlx::Error> {
        sqlx::query_as!(
            ThreadRoot,
            r#"
            SELECT id, room_id, root_event_id, sender, thread_id, reply_count,
                   last_reply_event_id, last_reply_sender, last_reply_ts,
                   participants, is_fetched AS "is_fetched!", created_ts, updated_ts
            FROM thread_roots
            WHERE room_id = $1 AND root_event_id = $2
            "#,
            room_id,
            root_event_id
        )
        .fetch_optional(&*self.pool)
        .await
    }

    /// See [`list_thread_roots`].
    pub async fn list_thread_roots(&self, params: ThreadListParams) -> Result<Vec<ThreadRoot>, sqlx::Error> {
        let limit = i64::from(params.limit.unwrap_or(50));

        if let Some(from) = params.from {
            sqlx::query_as!(
                ThreadRoot,
                r#"
                SELECT id, room_id, root_event_id, sender, thread_id, reply_count,
                       last_reply_event_id, last_reply_sender, last_reply_ts,
                       participants, is_fetched AS "is_fetched!", created_ts, updated_ts
                FROM thread_roots
                WHERE room_id = $1 AND thread_id > $2
                ORDER BY thread_id ASC
                LIMIT $3
                "#,
                &params.room_id,
                from,
                limit
            )
            .fetch_all(&*self.pool)
            .await
        } else {
            // Use thread_id ASC ordering to keep pagination stable: the cursor
            // path above filters by `thread_id > $2`, so the no-cursor branch
            // must use the same ordering for the cursor to be meaningful.
            sqlx::query_as!(
                ThreadRoot,
                r#"
                SELECT id, room_id, root_event_id, sender, thread_id, reply_count,
                       last_reply_event_id, last_reply_sender, last_reply_ts,
                       participants, is_fetched AS "is_fetched!", created_ts, updated_ts
                FROM thread_roots
                WHERE room_id = $1
                ORDER BY thread_id ASC
                LIMIT $2
                "#,
                &params.room_id,
                limit
            )
            .fetch_all(&*self.pool)
            .await
        }
    }

    /// See [`list_all_thread_roots`].
    pub async fn list_all_thread_roots(
        &self,
        limit: Option<i32>,
        from: Option<String>,
    ) -> Result<Vec<ThreadRoot>, sqlx::Error> {
        let limit = i64::from(limit.unwrap_or(50));

        if let Some(from) = from {
            sqlx::query_as!(
                ThreadRoot,
                r#"
                SELECT id, room_id, root_event_id, sender, thread_id, reply_count,
                       last_reply_event_id, last_reply_sender, last_reply_ts,
                       participants, is_fetched AS "is_fetched!", created_ts, updated_ts
                FROM thread_roots
                WHERE thread_id > $1
                ORDER BY thread_id ASC
                LIMIT $2
                "#,
                from,
                limit
            )
            .fetch_all(&*self.pool)
            .await
        } else {
            // Match the cursor path's `ORDER BY thread_id ASC` so the cursor
            // returned by the first page can be reused on subsequent pages.
            sqlx::query_as!(
                ThreadRoot,
                r#"
                SELECT id, room_id, root_event_id, sender, thread_id, reply_count,
                       last_reply_event_id, last_reply_sender, last_reply_ts,
                       participants, is_fetched AS "is_fetched!", created_ts, updated_ts
                FROM thread_roots
                ORDER BY thread_id ASC
                LIMIT $1
                "#,
                limit
            )
            .fetch_all(&*self.pool)
            .await
        }
    }

    /// See [`create_thread_reply`].
    pub async fn create_thread_reply(&self, params: CreateThreadReplyParams) -> Result<ThreadReply, sqlx::Error> {
        let now = current_timestamp_millis();
        let mut tx = self.pool.begin().await?;

        let reply = sqlx::query_as!(
            ThreadReply,
            r#"
            INSERT INTO thread_replies (
                room_id, thread_id, event_id, root_event_id, sender,
                in_reply_to_event_id, content, origin_server_ts, created_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            RETURNING id, room_id, thread_id, event_id, root_event_id, sender,
                      in_reply_to_event_id, content, origin_server_ts, is_edited, is_redacted, created_ts
            "#,
            &params.room_id,
            &params.thread_id,
            &params.event_id,
            &params.root_event_id,
            &params.sender,
            params.in_reply_to_event_id.as_deref(),
            &params.content,
            params.origin_server_ts,
            now
        )
        .fetch_one(&mut *tx)
        .await?;

        sqlx::query!(
            r#"
            UPDATE thread_roots
            SET reply_count = reply_count + 1,
                last_reply_event_id = $3,
                last_reply_sender = $4,
                last_reply_ts = $5,
                participants = (
                    SELECT COALESCE(jsonb_agg(participant ORDER BY participant), '[]'::jsonb)
                    FROM (
                        SELECT DISTINCT participant
                        FROM (
                            SELECT jsonb_array_elements_text(COALESCE(thread_roots.participants, '[]'::jsonb)) AS participant
                            UNION
                            SELECT $4::TEXT AS participant
                        ) AS merged_participants
                    ) AS deduped_participants
                ),
                updated_ts = $6
            WHERE room_id = $1 AND thread_id = $2
            "#,
            &params.room_id,
            &params.thread_id,
            &params.event_id,
            &params.sender,
            params.origin_server_ts,
            now
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(reply)
    }

    /// See [`get_thread_replies`].
    pub async fn get_thread_replies(
        &self,
        room_id: &str,
        thread_id: &str,
        limit: Option<i32>,
        from: Option<String>,
    ) -> Result<Vec<ThreadReply>, sqlx::Error> {
        let limit = i64::from(limit.unwrap_or(50));

        if let Some(from) = from {
            sqlx::query_as!(
                ThreadReply,
                r#"
                SELECT id, room_id, thread_id, event_id, root_event_id, sender,
                       in_reply_to_event_id, content, origin_server_ts, is_edited, is_redacted, created_ts
                FROM thread_replies
                WHERE room_id = $1 AND thread_id = $2 AND event_id > $3
                ORDER BY origin_server_ts ASC
                LIMIT $4
                "#,
                room_id,
                thread_id,
                from,
                limit
            )
            .fetch_all(&*self.pool)
            .await
        } else {
            sqlx::query_as!(
                ThreadReply,
                r#"
                SELECT id, room_id, thread_id, event_id, root_event_id, sender,
                       in_reply_to_event_id, content, origin_server_ts, is_edited, is_redacted, created_ts
                FROM thread_replies
                WHERE room_id = $1 AND thread_id = $2
                ORDER BY origin_server_ts ASC
                LIMIT $3
                "#,
                room_id,
                thread_id,
                limit
            )
            .fetch_all(&*self.pool)
            .await
        }
    }

    /// See [`get_reply_count`].
    pub async fn get_reply_count(&self, room_id: &str, thread_id: &str) -> Result<i32, sqlx::Error> {
        let count = sqlx::query_scalar!(
            r#"
            SELECT COUNT(*) AS "count!" FROM thread_replies
            WHERE room_id = $1 AND thread_id = $2
            "#,
            room_id,
            thread_id
        )
        .fetch_one(&*self.pool)
        .await?;

        Ok(count as i32)
    }

    /// See [`get_thread_participants`].
    pub async fn get_thread_participants(&self, room_id: &str, thread_id: &str) -> Result<Vec<String>, sqlx::Error> {
        let result = sqlx::query_scalar!(
            r#"
            SELECT DISTINCT sender AS "sender!" FROM (
                SELECT sender FROM thread_roots WHERE room_id = $1 AND thread_id = $2
                UNION
                SELECT sender FROM thread_replies WHERE room_id = $1 AND thread_id = $2
            ) AS participants
            "#,
            room_id,
            thread_id
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(result)
    }

    /// See [`subscribe_to_thread`].
    pub async fn subscribe_to_thread(
        &self,
        room_id: &str,
        thread_id: &str,
        user_id: &str,
        notification_level: &str,
    ) -> Result<ThreadSubscription, sqlx::Error> {
        let now = current_timestamp_millis();

        sqlx::query_as!(
            ThreadSubscription,
            r#"
            INSERT INTO thread_subscriptions (
                room_id, thread_id, user_id, notification_level, subscribed_ts, updated_ts
            )
            VALUES ($1, $2, $3, $4, $5, $5)
            ON CONFLICT (room_id, thread_id, user_id) DO UPDATE SET
                notification_level = EXCLUDED.notification_level,
                is_muted = FALSE,
                updated_ts = EXCLUDED.updated_ts
            RETURNING id, room_id, thread_id, user_id, notification_level AS "notification_level!",
                      is_muted AS "is_muted!", is_pinned AS "is_pinned!", subscribed_ts, updated_ts
            "#,
            room_id,
            thread_id,
            user_id,
            notification_level,
            now
        )
        .fetch_one(&*self.pool)
        .await
    }

    /// See [`unsubscribe_from_thread`].
    pub async fn unsubscribe_from_thread(
        &self,
        room_id: &str,
        thread_id: &str,
        user_id: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            DELETE FROM thread_subscriptions
            WHERE room_id = $1 AND thread_id = $2 AND user_id = $3
            "#,
            room_id,
            thread_id,
            user_id
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// See [`mute_thread`].
    pub async fn mute_thread(
        &self,
        room_id: &str,
        thread_id: &str,
        user_id: &str,
    ) -> Result<ThreadSubscription, sqlx::Error> {
        let now = current_timestamp_millis();

        sqlx::query_as!(
            ThreadSubscription,
            r#"
            INSERT INTO thread_subscriptions (
                room_id, thread_id, user_id, notification_level, is_muted, subscribed_ts, updated_ts
            )
            VALUES ($1, $2, $3, 'none', TRUE, $4, $4)
            ON CONFLICT (room_id, thread_id, user_id) DO UPDATE SET
                is_muted = TRUE,
                updated_ts = EXCLUDED.updated_ts
            RETURNING id, room_id, thread_id, user_id, notification_level AS "notification_level!",
                      is_muted AS "is_muted!", is_pinned AS "is_pinned!", subscribed_ts, updated_ts
            "#,
            room_id,
            thread_id,
            user_id,
            now
        )
        .fetch_one(&*self.pool)
        .await
    }

    /// See [`get_thread_subscription`].
    pub async fn get_thread_subscription(
        &self,
        room_id: &str,
        thread_id: &str,
        user_id: &str,
    ) -> Result<Option<ThreadSubscription>, sqlx::Error> {
        sqlx::query_as!(
            ThreadSubscription,
            r#"
            SELECT id, room_id, thread_id, user_id, notification_level AS "notification_level!",
                   is_muted AS "is_muted!", is_pinned AS "is_pinned!", subscribed_ts, updated_ts
            FROM thread_subscriptions
            WHERE room_id = $1 AND thread_id = $2 AND user_id = $3
            "#,
            room_id,
            thread_id,
            user_id
        )
        .fetch_optional(&*self.pool)
        .await
    }

    /// See [`get_user_thread_subscriptions`].
    pub async fn get_user_thread_subscriptions(
        &self,
        user_id: &str,
        limit: Option<i32>,
        from: Option<String>,
    ) -> Result<Vec<ThreadSubscription>, sqlx::Error> {
        let limit = i64::from(limit.unwrap_or(50));
        // `from` is interpreted as a `thread_id` keyset cursor (consistent
        // with `list_threads` / `get_thread_replies`). Pagination uses
        // `thread_id` lexicographic ordering so the cursor is self-consistent
        // even when multiple subscriptions share the same `updated_ts`.
        // NOTE: this differs from the previous `ORDER BY updated_ts DESC` —
        // callers that need newest-first should sort the returned Vec in
        // memory (the in-memory list is bounded by `limit`).
        if let Some(from) = from {
            sqlx::query_as!(
                ThreadSubscription,
                r#"
                SELECT id, room_id, thread_id, user_id, notification_level AS "notification_level!",
                       is_muted AS "is_muted!", is_pinned AS "is_pinned!", subscribed_ts, updated_ts
                FROM thread_subscriptions
                WHERE user_id = $1 AND thread_id > $2
                ORDER BY thread_id ASC
                LIMIT $3
                "#,
                user_id,
                from,
                limit
            )
            .fetch_all(&*self.pool)
            .await
        } else {
            sqlx::query_as!(
                ThreadSubscription,
                r#"
                SELECT id, room_id, thread_id, user_id, notification_level AS "notification_level!",
                       is_muted AS "is_muted!", is_pinned AS "is_pinned!", subscribed_ts, updated_ts
                FROM thread_subscriptions
                WHERE user_id = $1
                ORDER BY thread_id ASC
                LIMIT $2
                "#,
                user_id,
                limit
            )
            .fetch_all(&*self.pool)
            .await
        }
    }

    /// See [`update_read_receipt`].
    pub async fn update_read_receipt(
        &self,
        room_id: &str,
        thread_id: &str,
        user_id: &str,
        event_id: &str,
        origin_server_ts: i64,
    ) -> Result<ThreadReadReceipt, sqlx::Error> {
        let now = current_timestamp_millis();

        sqlx::query_as!(
            ThreadReadReceipt,
            r#"
            INSERT INTO thread_read_receipts (
                room_id, thread_id, user_id, last_read_event_id, last_read_ts, updated_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (room_id, thread_id, user_id) DO UPDATE SET
                last_read_event_id = EXCLUDED.last_read_event_id,
                last_read_ts = EXCLUDED.last_read_ts,
                unread_count = 0,
                updated_ts = EXCLUDED.updated_ts
            RETURNING id, room_id, thread_id, user_id, last_read_event_id, last_read_ts, unread_count, updated_ts
            "#,
            room_id,
            thread_id,
            user_id,
            event_id,
            origin_server_ts,
            now
        )
        .fetch_one(&*self.pool)
        .await
    }

    /// See [`get_read_receipt`].
    pub async fn get_read_receipt(
        &self,
        room_id: &str,
        thread_id: &str,
        user_id: &str,
    ) -> Result<Option<ThreadReadReceipt>, sqlx::Error> {
        sqlx::query_as!(
            ThreadReadReceipt,
            r#"
            SELECT id, room_id, thread_id, user_id, last_read_event_id, last_read_ts, unread_count, updated_ts
            FROM thread_read_receipts
            WHERE room_id = $1 AND thread_id = $2 AND user_id = $3
            "#,
            room_id,
            thread_id,
            user_id
        )
        .fetch_optional(&*self.pool)
        .await
    }

    /// See [`increment_unread_count`].
    pub async fn increment_unread_count(
        &self,
        room_id: &str,
        thread_id: &str,
        user_id: &str,
    ) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();

        sqlx::query!(
            r#"
            INSERT INTO thread_read_receipts (
                room_id, thread_id, user_id, last_read_ts, unread_count, updated_ts
            )
            VALUES ($1, $2, $3, 0, 1, $4)
            ON CONFLICT (room_id, thread_id, user_id) DO UPDATE SET
                unread_count = thread_read_receipts.unread_count + 1,
                updated_ts = EXCLUDED.updated_ts
            "#,
            room_id,
            thread_id,
            user_id,
            now
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// See [`create_thread_relation`].
    pub async fn create_thread_relation(
        &self,
        room_id: &str,
        event_id: &str,
        relates_to_event_id: &str,
        relation_type: &str,
        thread_id: Option<&str>,
        is_falling_back: bool,
    ) -> Result<ThreadRelation, sqlx::Error> {
        let now = current_timestamp_millis();

        sqlx::query_as!(
            ThreadRelation,
            r#"
            INSERT INTO thread_relations (
                room_id, event_id, relates_to_event_id, relation_type,
                thread_id, is_falling_back, created_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id, room_id, event_id, relates_to_event_id, relation_type, thread_id, is_falling_back, created_ts
            "#,
            room_id,
            event_id,
            relates_to_event_id,
            relation_type,
            thread_id,
            is_falling_back,
            now
        )
        .fetch_one(&*self.pool)
        .await
    }

    /// See [`mark_reply_edited`].
    pub async fn mark_reply_edited(&self, room_id: &str, event_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE thread_replies
            SET is_edited = TRUE
            WHERE room_id = $1 AND event_id = $2 AND is_edited = FALSE
            "#,
            room_id,
            event_id
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// See [`mark_reply_redacted`].
    pub async fn mark_reply_redacted(&self, room_id: &str, event_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE thread_replies
            SET is_redacted = TRUE, content = '{}'
            WHERE room_id = $1 AND event_id = $2 AND is_redacted = FALSE
            "#,
            room_id,
            event_id
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// See [`delete_thread`].
    pub async fn delete_thread(&self, room_id: &str, thread_id: &str) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;

        sqlx::query!(r"DELETE FROM thread_relations WHERE room_id = $1 AND thread_id = $2", room_id, thread_id)
            .execute(&mut *tx)
            .await?;

        sqlx::query!(r"DELETE FROM thread_replies WHERE room_id = $1 AND thread_id = $2", room_id, thread_id)
            .execute(&mut *tx)
            .await?;

        sqlx::query!(r"DELETE FROM thread_roots WHERE room_id = $1 AND thread_id = $2", room_id, thread_id)
            .execute(&mut *tx)
            .await?;

        sqlx::query!(r"DELETE FROM thread_subscriptions WHERE room_id = $1 AND thread_id = $2", room_id, thread_id)
            .execute(&mut *tx)
            .await?;

        sqlx::query!(r"DELETE FROM thread_read_receipts WHERE room_id = $1 AND thread_id = $2", room_id, thread_id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(())
    }

    /// See [`get_threads_with_unread`].
    pub async fn get_threads_with_unread(
        &self,
        user_id: &str,
        room_id: Option<&str>,
    ) -> Result<Vec<ThreadReadReceipt>, sqlx::Error> {
        if let Some(room_id) = room_id {
            sqlx::query_as!(
                ThreadReadReceipt,
                r#"
                SELECT id, room_id, thread_id, user_id, last_read_event_id, last_read_ts, unread_count, updated_ts
                FROM thread_read_receipts
                WHERE user_id = $1 AND room_id = $2 AND unread_count > 0
                ORDER BY updated_ts DESC
                "#,
                user_id,
                room_id
            )
            .fetch_all(&*self.pool)
            .await
        } else {
            sqlx::query_as!(
                ThreadReadReceipt,
                r#"
                SELECT id, room_id, thread_id, user_id, last_read_event_id, last_read_ts, unread_count, updated_ts
                FROM thread_read_receipts
                WHERE user_id = $1 AND unread_count > 0
                ORDER BY updated_ts DESC
                "#,
                user_id
            )
            .fetch_all(&*self.pool)
            .await
        }
    }

    /// See [`get_thread_summary`].
    pub async fn get_thread_summary(
        &self,
        room_id: &str,
        thread_id: &str,
    ) -> Result<Option<ThreadSummary>, sqlx::Error> {
        sqlx::query_as!(
            ThreadSummary,
            r#"
            WITH root AS (
                SELECT
                    tr.id,
                    tr.room_id,
                    COALESCE(tr.thread_id, '') AS thread_id,
                    tr.root_event_id,
                    tr.sender AS root_sender,
                    COALESCE(e.content, '{}'::jsonb) AS root_content,
                    COALESCE(e.origin_server_ts, tr.created_ts) AS root_origin_server_ts,
                    tr.is_fetched AS is_frozen,
                    tr.created_ts,
                    COALESCE(tr.updated_ts, tr.created_ts) AS base_updated_ts
                FROM thread_roots tr
                LEFT JOIN events e
                    ON e.event_id = tr.root_event_id
                   AND e.room_id = tr.room_id
                WHERE tr.room_id = $1 AND tr.thread_id = $2
            ),
            latest_reply AS (
                SELECT
                    r.event_id AS latest_event_id,
                    r.sender AS latest_sender,
                    r.content AS latest_content,
                    r.origin_server_ts AS latest_origin_server_ts
                FROM thread_replies r
                WHERE r.room_id = $1 AND r.thread_id = $2
                ORDER BY r.origin_server_ts DESC, r.id DESC
                LIMIT 1
            ),
            reply_stats AS (
                SELECT COUNT(*)::INTEGER AS reply_count
                FROM thread_replies
                WHERE room_id = $1 AND thread_id = $2
            ),
            participants AS (
                SELECT COALESCE(jsonb_agg(sender ORDER BY sender), '[]'::jsonb) AS participants
                FROM (
                    SELECT root_sender AS sender FROM root
                    UNION
                    SELECT DISTINCT sender
                    FROM thread_replies
                    WHERE room_id = $1 AND thread_id = $2
                ) AS deduped_senders
            )
            SELECT
                root.id AS "id!",
                root.room_id AS "room_id!",
                root.thread_id AS "thread_id!",
                root.root_event_id AS "root_event_id!",
                root.root_sender AS "root_sender!",
                root.root_content AS "root_content!",
                root.root_origin_server_ts AS "root_origin_server_ts!",
                latest_reply.latest_event_id AS "latest_event_id?",
                latest_reply.latest_sender AS "latest_sender?",
                latest_reply.latest_content AS "latest_content?",
                latest_reply.latest_origin_server_ts AS "latest_origin_server_ts?",
                reply_stats.reply_count AS "reply_count!",
                participants.participants AS "participants!",
                root.is_frozen AS "is_frozen!",
                root.created_ts AS "created_ts!",
                GREATEST(
                    root.base_updated_ts,
                    COALESCE(latest_reply.latest_origin_server_ts, root.base_updated_ts)
                ) AS "updated_ts!"
            FROM root
            LEFT JOIN latest_reply ON TRUE
            CROSS JOIN reply_stats
            CROSS JOIN participants
            "#,
            room_id,
            thread_id
        )
        .fetch_optional(&*self.pool)
        .await
    }

    /// See [`get_thread_statistics`].
    pub async fn get_thread_statistics(
        &self,
        room_id: &str,
        thread_id: &str,
    ) -> Result<Option<ThreadStatistics>, sqlx::Error> {
        sqlx::query_as!(
            ThreadStatistics,
            r#"
            SELECT
                tr.id,
                tr.room_id,
                COALESCE(tr.thread_id, '') AS "thread_id!",
                COALESCE(reply_stats.total_replies, 0) AS "total_replies!",
                COALESCE(participant_stats.total_participants, 1) AS "total_participants!",
                COALESCE(reply_stats.total_edits, 0) AS "total_edits!",
                COALESCE(reply_stats.total_redactions, 0) AS "total_redactions!",
                reply_stats.first_reply_ts AS "first_reply_ts?",
                reply_stats.last_reply_ts AS "last_reply_ts?",
                reply_stats.avg_reply_time_ms AS "avg_reply_time_ms?",
                tr.created_ts,
                COALESCE(tr.updated_ts, tr.created_ts) AS "updated_ts!"
            FROM thread_roots tr
            LEFT JOIN events e
                ON e.event_id = tr.root_event_id
               AND e.room_id = tr.room_id
            LEFT JOIN LATERAL (
                SELECT
                    COUNT(*)::INTEGER AS total_replies,
                    COUNT(*) FILTER (WHERE is_edited)::INTEGER AS total_edits,
                    COUNT(*) FILTER (WHERE is_redacted)::INTEGER AS total_redactions,
                    MIN(origin_server_ts) AS first_reply_ts,
                    MAX(origin_server_ts) AS last_reply_ts,
                    AVG(origin_server_ts - COALESCE(e.origin_server_ts, tr.created_ts))::BIGINT AS avg_reply_time_ms
                FROM thread_replies rr
                WHERE rr.room_id = tr.room_id
                  AND rr.thread_id = tr.thread_id
            ) AS reply_stats ON TRUE
            LEFT JOIN LATERAL (
                SELECT COUNT(*)::INTEGER AS total_participants
                FROM (
                    SELECT tr.sender AS sender
                    UNION
                    SELECT DISTINCT rr.sender
                    FROM thread_replies rr
                    WHERE rr.room_id = tr.room_id
                      AND rr.thread_id = tr.thread_id
                ) AS participant_set
            ) AS participant_stats ON TRUE
            WHERE tr.room_id = $1 AND tr.thread_id = $2
            "#,
            room_id,
            thread_id
        )
        .fetch_optional(&*self.pool)
        .await
    }

    /// See [`search_threads`].
    pub async fn search_threads(
        &self,
        room_id: &str,
        query: &str,
        limit: Option<i32>,
    ) -> Result<Vec<ThreadSummary>, sqlx::Error> {
        let limit = i64::from(limit.unwrap_or(20));
        // Escape special characters in the query for ILIKE and plainto_tsquery
        // Double % for literal % in LIKE patterns. _ needs escaping too. Single quotes need escaping.
        let escaped_query = query.replace('%', r"%%").replace('_', r"\_").replace('\'', r"''");

        // NOTE(C9): `search_relevance` is a ranking-only column with no `ThreadSummary`
        // field, and `query_as!` constructs the struct from *every* described column, so
        // the ranked query is wrapped in a subquery that projects exactly the struct's
        // columns. Ordering is unchanged: `q.search_relevance DESC` then the same sort
        // timestamp (`sort_ts`) as before.
        sqlx::query_as!(
            ThreadSummary,
            r#"
            SELECT
                q.id,
                q.room_id,
                q.thread_id AS "thread_id!",
                q.root_event_id,
                q.root_sender,
                q.root_content AS "root_content!",
                q.root_origin_server_ts AS "root_origin_server_ts!",
                q.latest_event_id AS "latest_event_id?",
                q.latest_sender AS "latest_sender?",
                q.latest_content AS "latest_content?",
                q.latest_origin_server_ts AS "latest_origin_server_ts?",
                q.reply_count AS "reply_count!",
                q.participants AS "participants!",
                q.is_frozen AS "is_frozen!",
                q.created_ts,
                q.updated_ts AS "updated_ts!"
            FROM (
                SELECT
                    tr.id,
                    tr.room_id,
                    COALESCE(tr.thread_id, '') AS thread_id,
                    tr.root_event_id,
                    tr.sender AS root_sender,
                    COALESCE(e.content, '{}'::jsonb) AS root_content,
                    COALESCE(e.origin_server_ts, tr.created_ts) AS root_origin_server_ts,
                    latest_reply.latest_event_id,
                    latest_reply.latest_sender,
                    latest_reply.latest_content,
                    latest_reply.latest_origin_server_ts,
                    COALESCE(reply_stats.reply_count, 0) AS reply_count,
                    COALESCE(participants.participants, jsonb_build_array(tr.sender)) AS participants,
                    tr.is_fetched AS is_frozen,
                    tr.created_ts,
                    GREATEST(
                        COALESCE(tr.updated_ts, tr.created_ts),
                        COALESCE(latest_reply.latest_origin_server_ts, COALESCE(tr.updated_ts, tr.created_ts))
                    ) AS updated_ts,
                    -- Calculate relevance for ordering
                    GREATEST(
                        COALESCE(ts_rank_cd(to_tsvector('english', COALESCE(e.content->>'body', '')), plainto_tsquery('english', $2)), 0.0),
                        COALESCE(similarity(COALESCE(e.content->>'body', ''), $2), 0.0),
                        COALESCE(ts_rank_cd(to_tsvector('english', COALESCE(latest_reply.latest_content->>'body', '')), plainto_tsquery('english', $2)), 0.0),
                        COALESCE(similarity(COALESCE(latest_reply.latest_content->>'body', ''), $2), 0.0)
                    ) AS search_relevance,
                    COALESCE(latest_reply.latest_origin_server_ts, e.origin_server_ts, tr.created_ts) AS sort_ts
                FROM thread_roots tr
                LEFT JOIN events e
                    ON e.event_id = tr.root_event_id
                   AND e.room_id = tr.room_id
                LEFT JOIN LATERAL (
                    SELECT
                        rr.event_id AS latest_event_id,
                        rr.sender AS latest_sender,
                        rr.content AS latest_content,
                        rr.origin_server_ts AS latest_origin_server_ts
                    FROM thread_replies rr
                    WHERE rr.room_id = tr.room_id
                      AND rr.thread_id = tr.thread_id
                    ORDER BY rr.origin_server_ts DESC, rr.id DESC
                    LIMIT 1
                ) AS latest_reply ON TRUE
                LEFT JOIN LATERAL (
                    SELECT COUNT(*)::INTEGER AS reply_count
                    FROM thread_replies rr
                    WHERE rr.room_id = tr.room_id
                      AND rr.thread_id = tr.thread_id
                ) AS reply_stats ON TRUE
                LEFT JOIN LATERAL (
                    SELECT COALESCE(jsonb_agg(sender ORDER BY sender), '[]'::jsonb) AS participants
                    FROM (
                        SELECT tr.sender AS sender -- Corrected: direct reference to tr from outer query
                        UNION
                        SELECT DISTINCT rr.sender
                        FROM thread_replies rr
                        WHERE rr.room_id = tr.room_id
                          AND rr.thread_id = tr.thread_id
                    ) AS participant_set
                ) AS participants ON TRUE
                WHERE tr.room_id = $1
                  AND (
                      COALESCE(e.content->>'body', '') ILIKE '%' || $2 || '%'
                      OR COALESCE(latest_reply.latest_content->>'body', '') ILIKE '%' || $2 || '%'
                      OR COALESCE(e.content->>'body', '') % $2
                      OR COALESCE(latest_reply.latest_content->>'body', '') % $2
                  )
            ) AS q
            ORDER BY q.search_relevance DESC, q.sort_ts DESC NULLS LAST
            LIMIT $3
            "#,
            room_id,
            &escaped_query,
            limit
        )
        .fetch_all(&*self.pool)
        .await
    }

    /// See [`freeze_thread`].
    pub async fn freeze_thread(&self, room_id: &str, thread_id: &str) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();
        sqlx::query!(
            r#"
            UPDATE thread_roots
            SET is_fetched = TRUE, updated_ts = $3
            WHERE room_id = $1 AND thread_id = $2
            "#,
            room_id,
            thread_id,
            now
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// See [`unfreeze_thread`].
    pub async fn unfreeze_thread(&self, room_id: &str, thread_id: &str) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();
        sqlx::query!(
            r#"
            UPDATE thread_roots
            SET is_fetched = FALSE, updated_ts = $3
            WHERE room_id = $1 AND thread_id = $2
            "#,
            room_id,
            thread_id,
            now
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }
}

/// The `ThreadStoreApi` trait.
#[async_trait]
pub trait ThreadStoreApi: Send + Sync {
    /// See [`create_thread_root`].
    async fn create_thread_root(&self, params: CreateThreadRootParams) -> Result<ThreadRoot, sqlx::Error>;
    /// See [`get_thread_root`].
    async fn get_thread_root(&self, room_id: &str, thread_id: &str) -> Result<Option<ThreadRoot>, sqlx::Error>;
    /// See [`get_thread_root_by_event`].
    async fn get_thread_root_by_event(
        &self,
        room_id: &str,
        root_event_id: &str,
    ) -> Result<Option<ThreadRoot>, sqlx::Error>;
    /// See [`list_thread_roots`].
    async fn list_thread_roots(&self, params: ThreadListParams) -> Result<Vec<ThreadRoot>, sqlx::Error>;
    /// See [`list_all_thread_roots`].
    async fn list_all_thread_roots(
        &self,
        limit: Option<i32>,
        from: Option<String>,
    ) -> Result<Vec<ThreadRoot>, sqlx::Error>;
    /// See [`create_thread_reply`].
    async fn create_thread_reply(&self, params: CreateThreadReplyParams) -> Result<ThreadReply, sqlx::Error>;
    /// See [`get_thread_replies`].
    async fn get_thread_replies(
        &self,
        room_id: &str,
        thread_id: &str,
        limit: Option<i32>,
        from: Option<String>,
    ) -> Result<Vec<ThreadReply>, sqlx::Error>;
    /// See [`get_reply_count`].
    async fn get_reply_count(&self, room_id: &str, thread_id: &str) -> Result<i32, sqlx::Error>;
    /// See [`get_thread_participants`].
    async fn get_thread_participants(&self, room_id: &str, thread_id: &str) -> Result<Vec<String>, sqlx::Error>;
    /// See [`subscribe_to_thread`].
    async fn subscribe_to_thread(
        &self,
        room_id: &str,
        thread_id: &str,
        user_id: &str,
        notification_level: &str,
    ) -> Result<ThreadSubscription, sqlx::Error>;
    /// See [`unsubscribe_from_thread`].
    async fn unsubscribe_from_thread(&self, room_id: &str, thread_id: &str, user_id: &str) -> Result<(), sqlx::Error>;
    /// See [`mute_thread`].
    async fn mute_thread(
        &self,
        room_id: &str,
        thread_id: &str,
        user_id: &str,
    ) -> Result<ThreadSubscription, sqlx::Error>;
    /// See [`get_thread_subscription`].
    async fn get_thread_subscription(
        &self,
        room_id: &str,
        thread_id: &str,
        user_id: &str,
    ) -> Result<Option<ThreadSubscription>, sqlx::Error>;
    /// See [`get_user_thread_subscriptions`].
    async fn get_user_thread_subscriptions(
        &self,
        user_id: &str,
        limit: Option<i32>,
        from: Option<String>,
    ) -> Result<Vec<ThreadSubscription>, sqlx::Error>;
    /// See [`update_read_receipt`].
    async fn update_read_receipt(
        &self,
        room_id: &str,
        thread_id: &str,
        user_id: &str,
        event_id: &str,
        origin_server_ts: i64,
    ) -> Result<ThreadReadReceipt, sqlx::Error>;
    /// See [`get_read_receipt`].
    async fn get_read_receipt(
        &self,
        room_id: &str,
        thread_id: &str,
        user_id: &str,
    ) -> Result<Option<ThreadReadReceipt>, sqlx::Error>;
    /// See [`increment_unread_count`].
    async fn increment_unread_count(&self, room_id: &str, thread_id: &str, user_id: &str) -> Result<(), sqlx::Error>;
    /// See [`create_thread_relation`].
    async fn create_thread_relation(
        &self,
        room_id: &str,
        event_id: &str,
        relates_to_event_id: &str,
        relation_type: &str,
        thread_id: Option<&str>,
        is_falling_back: bool,
    ) -> Result<ThreadRelation, sqlx::Error>;
    /// See [`mark_reply_edited`].
    async fn mark_reply_edited(&self, room_id: &str, event_id: &str) -> Result<(), sqlx::Error>;
    /// See [`mark_reply_redacted`].
    async fn mark_reply_redacted(&self, room_id: &str, event_id: &str) -> Result<(), sqlx::Error>;
    /// See [`delete_thread`].
    async fn delete_thread(&self, room_id: &str, thread_id: &str) -> Result<(), sqlx::Error>;
    /// See [`get_threads_with_unread`].
    async fn get_threads_with_unread(
        &self,
        user_id: &str,
        room_id: Option<&str>,
    ) -> Result<Vec<ThreadReadReceipt>, sqlx::Error>;
    /// See [`get_thread_summary`].
    async fn get_thread_summary(&self, room_id: &str, thread_id: &str) -> Result<Option<ThreadSummary>, sqlx::Error>;
    /// See [`get_thread_statistics`].
    async fn get_thread_statistics(
        &self,
        room_id: &str,
        thread_id: &str,
    ) -> Result<Option<ThreadStatistics>, sqlx::Error>;
    /// See [`search_threads`].
    async fn search_threads(
        &self,
        room_id: &str,
        query: &str,
        limit: Option<i32>,
    ) -> Result<Vec<ThreadSummary>, sqlx::Error>;
    /// See [`freeze_thread`].
    async fn freeze_thread(&self, room_id: &str, thread_id: &str) -> Result<(), sqlx::Error>;
    /// See [`unfreeze_thread`].
    async fn unfreeze_thread(&self, room_id: &str, thread_id: &str) -> Result<(), sqlx::Error>;
}

#[async_trait]
impl ThreadStoreApi for ThreadStorage {
    async fn create_thread_root(&self, params: CreateThreadRootParams) -> Result<ThreadRoot, sqlx::Error> {
        self.create_thread_root(params).await
    }

    async fn get_thread_root(&self, room_id: &str, thread_id: &str) -> Result<Option<ThreadRoot>, sqlx::Error> {
        self.get_thread_root(room_id, thread_id).await
    }

    async fn get_thread_root_by_event(
        &self,
        room_id: &str,
        root_event_id: &str,
    ) -> Result<Option<ThreadRoot>, sqlx::Error> {
        self.get_thread_root_by_event(room_id, root_event_id).await
    }

    async fn list_thread_roots(&self, params: ThreadListParams) -> Result<Vec<ThreadRoot>, sqlx::Error> {
        self.list_thread_roots(params).await
    }

    async fn list_all_thread_roots(
        &self,
        limit: Option<i32>,
        from: Option<String>,
    ) -> Result<Vec<ThreadRoot>, sqlx::Error> {
        self.list_all_thread_roots(limit, from).await
    }

    async fn create_thread_reply(&self, params: CreateThreadReplyParams) -> Result<ThreadReply, sqlx::Error> {
        self.create_thread_reply(params).await
    }

    async fn get_thread_replies(
        &self,
        room_id: &str,
        thread_id: &str,
        limit: Option<i32>,
        from: Option<String>,
    ) -> Result<Vec<ThreadReply>, sqlx::Error> {
        self.get_thread_replies(room_id, thread_id, limit, from).await
    }

    async fn get_reply_count(&self, room_id: &str, thread_id: &str) -> Result<i32, sqlx::Error> {
        self.get_reply_count(room_id, thread_id).await
    }

    async fn get_thread_participants(&self, room_id: &str, thread_id: &str) -> Result<Vec<String>, sqlx::Error> {
        self.get_thread_participants(room_id, thread_id).await
    }

    async fn subscribe_to_thread(
        &self,
        room_id: &str,
        thread_id: &str,
        user_id: &str,
        notification_level: &str,
    ) -> Result<ThreadSubscription, sqlx::Error> {
        self.subscribe_to_thread(room_id, thread_id, user_id, notification_level).await
    }

    async fn unsubscribe_from_thread(&self, room_id: &str, thread_id: &str, user_id: &str) -> Result<(), sqlx::Error> {
        self.unsubscribe_from_thread(room_id, thread_id, user_id).await
    }

    async fn mute_thread(
        &self,
        room_id: &str,
        thread_id: &str,
        user_id: &str,
    ) -> Result<ThreadSubscription, sqlx::Error> {
        self.mute_thread(room_id, thread_id, user_id).await
    }

    async fn get_thread_subscription(
        &self,
        room_id: &str,
        thread_id: &str,
        user_id: &str,
    ) -> Result<Option<ThreadSubscription>, sqlx::Error> {
        self.get_thread_subscription(room_id, thread_id, user_id).await
    }

    async fn get_user_thread_subscriptions(
        &self,
        user_id: &str,
        limit: Option<i32>,
        from: Option<String>,
    ) -> Result<Vec<ThreadSubscription>, sqlx::Error> {
        self.get_user_thread_subscriptions(user_id, limit, from).await
    }

    async fn update_read_receipt(
        &self,
        room_id: &str,
        thread_id: &str,
        user_id: &str,
        event_id: &str,
        origin_server_ts: i64,
    ) -> Result<ThreadReadReceipt, sqlx::Error> {
        self.update_read_receipt(room_id, thread_id, user_id, event_id, origin_server_ts).await
    }

    async fn get_read_receipt(
        &self,
        room_id: &str,
        thread_id: &str,
        user_id: &str,
    ) -> Result<Option<ThreadReadReceipt>, sqlx::Error> {
        self.get_read_receipt(room_id, thread_id, user_id).await
    }

    async fn increment_unread_count(&self, room_id: &str, thread_id: &str, user_id: &str) -> Result<(), sqlx::Error> {
        self.increment_unread_count(room_id, thread_id, user_id).await
    }

    async fn create_thread_relation(
        &self,
        room_id: &str,
        event_id: &str,
        relates_to_event_id: &str,
        relation_type: &str,
        thread_id: Option<&str>,
        is_falling_back: bool,
    ) -> Result<ThreadRelation, sqlx::Error> {
        self.create_thread_relation(room_id, event_id, relates_to_event_id, relation_type, thread_id, is_falling_back)
            .await
    }

    async fn mark_reply_edited(&self, room_id: &str, event_id: &str) -> Result<(), sqlx::Error> {
        self.mark_reply_edited(room_id, event_id).await
    }

    async fn mark_reply_redacted(&self, room_id: &str, event_id: &str) -> Result<(), sqlx::Error> {
        self.mark_reply_redacted(room_id, event_id).await
    }

    async fn delete_thread(&self, room_id: &str, thread_id: &str) -> Result<(), sqlx::Error> {
        self.delete_thread(room_id, thread_id).await
    }

    async fn get_threads_with_unread(
        &self,
        user_id: &str,
        room_id: Option<&str>,
    ) -> Result<Vec<ThreadReadReceipt>, sqlx::Error> {
        self.get_threads_with_unread(user_id, room_id).await
    }

    async fn get_thread_summary(&self, room_id: &str, thread_id: &str) -> Result<Option<ThreadSummary>, sqlx::Error> {
        self.get_thread_summary(room_id, thread_id).await
    }

    async fn get_thread_statistics(
        &self,
        room_id: &str,
        thread_id: &str,
    ) -> Result<Option<ThreadStatistics>, sqlx::Error> {
        self.get_thread_statistics(room_id, thread_id).await
    }

    async fn search_threads(
        &self,
        room_id: &str,
        query: &str,
        limit: Option<i32>,
    ) -> Result<Vec<ThreadSummary>, sqlx::Error> {
        self.search_threads(room_id, query, limit).await
    }

    async fn freeze_thread(&self, room_id: &str, thread_id: &str) -> Result<(), sqlx::Error> {
        self.freeze_thread(room_id, thread_id).await
    }

    async fn unfreeze_thread(&self, room_id: &str, thread_id: &str) -> Result<(), sqlx::Error> {
        self.unfreeze_thread(room_id, thread_id).await
    }
}
