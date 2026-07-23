use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};
use std::sync::Arc;
use synapse_common::current_timestamp_millis;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ThreadRoot {
    pub id: i64,
    pub room_id: String,
    pub root_event_id: String,
    pub sender: String,
    pub thread_id: Option<String>,
    pub reply_count: Option<i64>,
    pub last_reply_event_id: Option<String>,
    pub last_reply_sender: Option<String>,
    pub last_reply_ts: Option<i64>,
    pub participants: Option<serde_json::Value>,
    pub is_fetched: bool,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ThreadReply {
    pub id: i64,
    pub room_id: String,
    pub thread_id: String,
    pub event_id: String,
    pub root_event_id: String,
    pub sender: String,
    pub in_reply_to_event_id: Option<String>,
    pub content: serde_json::Value,
    pub origin_server_ts: i64,
    pub is_edited: bool,
    pub is_redacted: bool,
    pub created_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ThreadSubscription {
    pub id: i64,
    pub room_id: String,
    pub thread_id: String,
    pub user_id: String,
    pub notification_level: String,
    pub is_muted: bool,
    pub is_pinned: bool,
    pub subscribed_ts: i64,
    pub updated_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ThreadReadReceipt {
    pub id: i64,
    pub room_id: String,
    pub thread_id: String,
    pub user_id: String,
    pub last_read_event_id: Option<String>,
    pub last_read_ts: i64,
    pub unread_count: i32,
    pub updated_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ThreadRelation {
    pub id: i64,
    pub room_id: String,
    pub event_id: String,
    pub relates_to_event_id: String,
    pub relation_type: String,
    pub thread_id: Option<String>,
    pub is_falling_back: bool,
    pub created_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ThreadSummary {
    pub id: i64,
    pub room_id: String,
    pub thread_id: String,
    pub root_event_id: String,
    pub root_sender: String,
    pub root_content: serde_json::Value,
    pub root_origin_server_ts: i64,
    pub latest_event_id: Option<String>,
    pub latest_sender: Option<String>,
    pub latest_content: Option<serde_json::Value>,
    pub latest_origin_server_ts: Option<i64>,
    pub reply_count: i32,
    pub participants: serde_json::Value,
    pub is_frozen: bool,
    pub created_ts: i64,
    pub updated_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ThreadStatistics {
    pub id: i64,
    pub room_id: String,
    pub thread_id: String,
    pub total_replies: i32,
    pub total_participants: i32,
    pub total_edits: i32,
    pub total_redactions: i32,
    pub first_reply_ts: Option<i64>,
    pub last_reply_ts: Option<i64>,
    pub avg_reply_time_ms: Option<i64>,
    pub created_ts: i64,
    pub updated_ts: i64,
}

#[derive(Debug, Clone)]
pub struct CreateThreadRootParams {
    pub room_id: String,
    pub root_event_id: String,
    pub sender: String,
    pub thread_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CreateThreadReplyParams {
    pub room_id: String,
    pub thread_id: String,
    pub event_id: String,
    pub root_event_id: String,
    pub sender: String,
    pub in_reply_to_event_id: Option<String>,
    pub content: serde_json::Value,
    pub origin_server_ts: i64,
}

#[derive(Debug, Clone)]
pub struct ThreadListParams {
    pub room_id: String,
    pub limit: Option<i32>,
    pub from: Option<String>,
    pub include_all: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadWithReplies {
    pub root: ThreadRoot,
    pub replies: Vec<ThreadReply>,
    pub reply_count: i32,
    pub participants: Vec<String>,
}

#[derive(Clone)]
pub struct ThreadStorage {
    pub pool: Arc<Pool<Postgres>>,
}

impl ThreadStorage {
    pub fn new(pool: &Arc<Pool<Postgres>>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create_thread_root(&self, params: CreateThreadRootParams) -> Result<ThreadRoot, sqlx::Error> {
        let now = current_timestamp_millis();

        sqlx::query_as::<_, ThreadRoot>(
            r"
            INSERT INTO thread_roots (
                room_id, root_event_id, sender, thread_id, participants, created_ts
            )
            VALUES ($1, $2, $3, $4, jsonb_build_array($3), $5)
            RETURNING id, room_id, root_event_id, sender, thread_id, reply_count,
                      last_reply_event_id, last_reply_sender, last_reply_ts,
                      participants, is_fetched, created_ts, updated_ts
            ",
        )
        .bind(&params.room_id)
        .bind(&params.root_event_id)
        .bind(&params.sender)
        .bind(&params.thread_id)
        .bind(now)
        .fetch_one(&*self.pool)
        .await
    }

    pub async fn get_thread_root(&self, room_id: &str, thread_id: &str) -> Result<Option<ThreadRoot>, sqlx::Error> {
        sqlx::query_as::<_, ThreadRoot>(
            r"
            SELECT id, room_id, root_event_id, sender, thread_id, reply_count,
                   last_reply_event_id, last_reply_sender, last_reply_ts,
                   participants, is_fetched, created_ts, updated_ts
            FROM thread_roots
            WHERE room_id = $1 AND thread_id = $2
            ",
        )
        .bind(room_id)
        .bind(thread_id)
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn get_thread_root_by_event(
        &self,
        room_id: &str,
        root_event_id: &str,
    ) -> Result<Option<ThreadRoot>, sqlx::Error> {
        sqlx::query_as::<_, ThreadRoot>(
            r"
            SELECT id, room_id, root_event_id, sender, thread_id, reply_count,
                   last_reply_event_id, last_reply_sender, last_reply_ts,
                   participants, is_fetched, created_ts, updated_ts
            FROM thread_roots
            WHERE room_id = $1 AND root_event_id = $2
            ",
        )
        .bind(room_id)
        .bind(root_event_id)
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn list_thread_roots(&self, params: ThreadListParams) -> Result<Vec<ThreadRoot>, sqlx::Error> {
        let limit = params.limit.unwrap_or(50);

        if let Some(from) = params.from {
            sqlx::query_as::<_, ThreadRoot>(
                r"
                SELECT id, room_id, root_event_id, sender, thread_id, reply_count,
                       last_reply_event_id, last_reply_sender, last_reply_ts,
                       participants, is_fetched, created_ts, updated_ts
                FROM thread_roots
                WHERE room_id = $1 AND thread_id > $2
                ORDER BY thread_id ASC
                LIMIT $3
                ",
            )
            .bind(&params.room_id)
            .bind(from)
            .bind(limit)
            .fetch_all(&*self.pool)
            .await
        } else {
            // Use thread_id ASC ordering to keep pagination stable: the cursor
            // path above filters by `thread_id > $2`, so the no-cursor branch
            // must use the same ordering for the cursor to be meaningful.
            sqlx::query_as::<_, ThreadRoot>(
                r"
                SELECT id, room_id, root_event_id, sender, thread_id, reply_count,
                       last_reply_event_id, last_reply_sender, last_reply_ts,
                       participants, is_fetched, created_ts, updated_ts
                FROM thread_roots
                WHERE room_id = $1
                ORDER BY thread_id ASC
                LIMIT $2
                ",
            )
            .bind(&params.room_id)
            .bind(limit)
            .fetch_all(&*self.pool)
            .await
        }
    }

    pub async fn list_all_thread_roots(
        &self,
        limit: Option<i32>,
        from: Option<String>,
    ) -> Result<Vec<ThreadRoot>, sqlx::Error> {
        let limit = limit.unwrap_or(50);

        if let Some(from) = from {
            sqlx::query_as::<_, ThreadRoot>(
                r"
                SELECT id, room_id, root_event_id, sender, thread_id, reply_count,
                       last_reply_event_id, last_reply_sender, last_reply_ts,
                       participants, is_fetched, created_ts, updated_ts
                FROM thread_roots
                WHERE thread_id > $1
                ORDER BY thread_id ASC
                LIMIT $2
                ",
            )
            .bind(from)
            .bind(limit)
            .fetch_all(&*self.pool)
            .await
        } else {
            // Match the cursor path's `ORDER BY thread_id ASC` so the cursor
            // returned by the first page can be reused on subsequent pages.
            sqlx::query_as::<_, ThreadRoot>(
                r"
                SELECT id, room_id, root_event_id, sender, thread_id, reply_count,
                       last_reply_event_id, last_reply_sender, last_reply_ts,
                       participants, is_fetched, created_ts, updated_ts
                FROM thread_roots
                ORDER BY thread_id ASC
                LIMIT $1
                ",
            )
            .bind(limit)
            .fetch_all(&*self.pool)
            .await
        }
    }

    pub async fn create_thread_reply(&self, params: CreateThreadReplyParams) -> Result<ThreadReply, sqlx::Error> {
        let now = current_timestamp_millis();
        let mut tx = self.pool.begin().await?;

        let reply = sqlx::query_as::<_, ThreadReply>(
            r"
            INSERT INTO thread_replies (
                room_id, thread_id, event_id, root_event_id, sender,
                in_reply_to_event_id, content, origin_server_ts, created_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            RETURNING id, room_id, thread_id, event_id, root_event_id, sender,
                      in_reply_to_event_id, content, origin_server_ts, is_edited, is_redacted, created_ts
            ",
        )
        .bind(&params.room_id)
        .bind(&params.thread_id)
        .bind(&params.event_id)
        .bind(&params.root_event_id)
        .bind(&params.sender)
        .bind(&params.in_reply_to_event_id)
        .bind(&params.content)
        .bind(params.origin_server_ts)
        .bind(now)
        .fetch_one(&mut *tx)
        .await?;

        sqlx::query(
            r"
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
            ",
        )
        .bind(&params.room_id)
        .bind(&params.thread_id)
        .bind(&params.event_id)
        .bind(&params.sender)
        .bind(params.origin_server_ts)
        .bind(now)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(reply)
    }

    pub async fn get_thread_replies(
        &self,
        room_id: &str,
        thread_id: &str,
        limit: Option<i32>,
        from: Option<String>,
    ) -> Result<Vec<ThreadReply>, sqlx::Error> {
        let limit = limit.unwrap_or(50);

        if let Some(from) = from {
            sqlx::query_as::<_, ThreadReply>(
                r"
                SELECT id, room_id, thread_id, event_id, root_event_id, sender,
                       in_reply_to_event_id, content, origin_server_ts, is_edited, is_redacted, created_ts
                FROM thread_replies
                WHERE room_id = $1 AND thread_id = $2 AND event_id > $3
                ORDER BY origin_server_ts ASC
                LIMIT $4
                ",
            )
            .bind(room_id)
            .bind(thread_id)
            .bind(from)
            .bind(limit)
            .fetch_all(&*self.pool)
            .await
        } else {
            sqlx::query_as::<_, ThreadReply>(
                r"
                SELECT id, room_id, thread_id, event_id, root_event_id, sender,
                       in_reply_to_event_id, content, origin_server_ts, is_edited, is_redacted, created_ts
                FROM thread_replies
                WHERE room_id = $1 AND thread_id = $2
                ORDER BY origin_server_ts ASC
                LIMIT $3
                ",
            )
            .bind(room_id)
            .bind(thread_id)
            .bind(limit)
            .fetch_all(&*self.pool)
            .await
        }
    }

    pub async fn get_reply_count(&self, room_id: &str, thread_id: &str) -> Result<i32, sqlx::Error> {
        let result: Option<(i64,)> = sqlx::query_as(
            r"
            SELECT COUNT(*) FROM thread_replies
            WHERE room_id = $1 AND thread_id = $2
            ",
        )
        .bind(room_id)
        .bind(thread_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(result.map_or(0, |r| r.0 as i32))
    }

    pub async fn get_thread_participants(&self, room_id: &str, thread_id: &str) -> Result<Vec<String>, sqlx::Error> {
        let result: Vec<(String,)> = sqlx::query_as(
            r"
            SELECT DISTINCT sender FROM (
                SELECT sender FROM thread_roots WHERE room_id = $1 AND thread_id = $2
                UNION
                SELECT sender FROM thread_replies WHERE room_id = $1 AND thread_id = $2
            ) AS participants
            ",
        )
        .bind(room_id)
        .bind(thread_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(result.into_iter().map(|r| r.0).collect())
    }

    pub async fn subscribe_to_thread(
        &self,
        room_id: &str,
        thread_id: &str,
        user_id: &str,
        notification_level: &str,
    ) -> Result<ThreadSubscription, sqlx::Error> {
        let now = current_timestamp_millis();

        sqlx::query_as::<_, ThreadSubscription>(
            r"
            INSERT INTO thread_subscriptions (
                room_id, thread_id, user_id, notification_level, subscribed_ts, updated_ts
            )
            VALUES ($1, $2, $3, $4, $5, $5)
            ON CONFLICT (room_id, thread_id, user_id) DO UPDATE SET
                notification_level = EXCLUDED.notification_level,
                is_muted = FALSE,
                updated_ts = EXCLUDED.updated_ts
            RETURNING *
            ",
        )
        .bind(room_id)
        .bind(thread_id)
        .bind(user_id)
        .bind(notification_level)
        .bind(now)
        .fetch_one(&*self.pool)
        .await
    }

    pub async fn unsubscribe_from_thread(
        &self,
        room_id: &str,
        thread_id: &str,
        user_id: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r"
            DELETE FROM thread_subscriptions
            WHERE room_id = $1 AND thread_id = $2 AND user_id = $3
            ",
        )
        .bind(room_id)
        .bind(thread_id)
        .bind(user_id)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn mute_thread(
        &self,
        room_id: &str,
        thread_id: &str,
        user_id: &str,
    ) -> Result<ThreadSubscription, sqlx::Error> {
        let now = current_timestamp_millis();

        sqlx::query_as::<_, ThreadSubscription>(
            r"
            INSERT INTO thread_subscriptions (
                room_id, thread_id, user_id, notification_level, is_muted, subscribed_ts, updated_ts
            )
            VALUES ($1, $2, $3, 'none', TRUE, $4, $4)
            ON CONFLICT (room_id, thread_id, user_id) DO UPDATE SET
                is_muted = TRUE,
                updated_ts = EXCLUDED.updated_ts
            RETURNING *
            ",
        )
        .bind(room_id)
        .bind(thread_id)
        .bind(user_id)
        .bind(now)
        .fetch_one(&*self.pool)
        .await
    }

    pub async fn get_thread_subscription(
        &self,
        room_id: &str,
        thread_id: &str,
        user_id: &str,
    ) -> Result<Option<ThreadSubscription>, sqlx::Error> {
        sqlx::query_as::<_, ThreadSubscription>(
            r"
            SELECT id, room_id, thread_id, user_id, notification_level, is_muted, is_pinned, subscribed_ts, updated_ts
            FROM thread_subscriptions
            WHERE room_id = $1 AND thread_id = $2 AND user_id = $3
            ",
        )
        .bind(room_id)
        .bind(thread_id)
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn get_user_thread_subscriptions(
        &self,
        user_id: &str,
        limit: Option<i32>,
    ) -> Result<Vec<ThreadSubscription>, sqlx::Error> {
        let limit = limit.unwrap_or(50);

        sqlx::query_as::<_, ThreadSubscription>(
            r"
            SELECT id, room_id, thread_id, user_id, notification_level, is_muted, is_pinned, subscribed_ts, updated_ts
            FROM thread_subscriptions
            WHERE user_id = $1
            ORDER BY updated_ts DESC
            LIMIT $2
            ",
        )
        .bind(user_id)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn update_read_receipt(
        &self,
        room_id: &str,
        thread_id: &str,
        user_id: &str,
        event_id: &str,
        origin_server_ts: i64,
    ) -> Result<ThreadReadReceipt, sqlx::Error> {
        let now = current_timestamp_millis();

        sqlx::query_as::<_, ThreadReadReceipt>(
            r"
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
            ",
        )
        .bind(room_id)
        .bind(thread_id)
        .bind(user_id)
        .bind(event_id)
        .bind(origin_server_ts)
        .bind(now)
        .fetch_one(&*self.pool)
        .await
    }

    pub async fn get_read_receipt(
        &self,
        room_id: &str,
        thread_id: &str,
        user_id: &str,
    ) -> Result<Option<ThreadReadReceipt>, sqlx::Error> {
        sqlx::query_as::<_, ThreadReadReceipt>(
            r"
            SELECT id, room_id, thread_id, user_id, last_read_event_id, last_read_ts, unread_count, updated_ts
            FROM thread_read_receipts
            WHERE room_id = $1 AND thread_id = $2 AND user_id = $3
            ",
        )
        .bind(room_id)
        .bind(thread_id)
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn increment_unread_count(
        &self,
        room_id: &str,
        thread_id: &str,
        user_id: &str,
    ) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();

        sqlx::query(
            r"
            INSERT INTO thread_read_receipts (
                room_id, thread_id, user_id, last_read_ts, unread_count, updated_ts
            )
            VALUES ($1, $2, $3, 0, 1, $4)
            ON CONFLICT (room_id, thread_id, user_id) DO UPDATE SET
                unread_count = thread_read_receipts.unread_count + 1,
                updated_ts = EXCLUDED.updated_ts
            ",
        )
        .bind(room_id)
        .bind(thread_id)
        .bind(user_id)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

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

        sqlx::query_as::<_, ThreadRelation>(
            r"
            INSERT INTO thread_relations (
                room_id, event_id, relates_to_event_id, relation_type,
                thread_id, is_falling_back, created_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id, room_id, event_id, relates_to_event_id, relation_type, thread_id, is_falling_back, created_ts
            ",
        )
        .bind(room_id)
        .bind(event_id)
        .bind(relates_to_event_id)
        .bind(relation_type)
        .bind(thread_id)
        .bind(is_falling_back)
        .bind(now)
        .fetch_one(&*self.pool)
        .await
    }

    pub async fn mark_reply_edited(&self, room_id: &str, event_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r"
            UPDATE thread_replies
            SET is_edited = TRUE
            WHERE room_id = $1 AND event_id = $2 AND is_edited = FALSE
            ",
        )
        .bind(room_id)
        .bind(event_id)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn mark_reply_redacted(&self, room_id: &str, event_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r"
            UPDATE thread_replies
            SET is_redacted = TRUE, content = '{}'
            WHERE room_id = $1 AND event_id = $2 AND is_redacted = FALSE
            ",
        )
        .bind(room_id)
        .bind(event_id)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn delete_thread(&self, room_id: &str, thread_id: &str) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;

        sqlx::query(r"DELETE FROM thread_relations WHERE room_id = $1 AND thread_id = $2")
            .bind(room_id)
            .bind(thread_id)
            .execute(&mut *tx)
            .await?;

        sqlx::query(r"DELETE FROM thread_replies WHERE room_id = $1 AND thread_id = $2")
            .bind(room_id)
            .bind(thread_id)
            .execute(&mut *tx)
            .await?;

        sqlx::query(r"DELETE FROM thread_roots WHERE room_id = $1 AND thread_id = $2")
            .bind(room_id)
            .bind(thread_id)
            .execute(&mut *tx)
            .await?;

        sqlx::query(r"DELETE FROM thread_subscriptions WHERE room_id = $1 AND thread_id = $2")
            .bind(room_id)
            .bind(thread_id)
            .execute(&mut *tx)
            .await?;

        sqlx::query(r"DELETE FROM thread_read_receipts WHERE room_id = $1 AND thread_id = $2")
            .bind(room_id)
            .bind(thread_id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(())
    }

    pub async fn get_threads_with_unread(
        &self,
        user_id: &str,
        room_id: Option<&str>,
    ) -> Result<Vec<ThreadReadReceipt>, sqlx::Error> {
        if let Some(room_id) = room_id {
            sqlx::query_as::<_, ThreadReadReceipt>(
                r"
                SELECT id, room_id, thread_id, user_id, last_read_event_id, last_read_ts, unread_count, updated_ts
                FROM thread_read_receipts
                WHERE user_id = $1 AND room_id = $2 AND unread_count > 0
                ORDER BY updated_ts DESC
                ",
            )
            .bind(user_id)
            .bind(room_id)
            .fetch_all(&*self.pool)
            .await
        } else {
            sqlx::query_as::<_, ThreadReadReceipt>(
                r"
                SELECT id, room_id, thread_id, user_id, last_read_event_id, last_read_ts, unread_count, updated_ts
                FROM thread_read_receipts
                WHERE user_id = $1 AND unread_count > 0
                ORDER BY updated_ts DESC
                ",
            )
            .bind(user_id)
            .fetch_all(&*self.pool)
            .await
        }
    }

    pub async fn get_thread_summary(
        &self,
        room_id: &str,
        thread_id: &str,
    ) -> Result<Option<ThreadSummary>, sqlx::Error> {
        sqlx::query_as::<_, ThreadSummary>(
            r"
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
                root.id,
                root.room_id,
                root.thread_id,
                root.root_event_id,
                root.root_sender,
                root.root_content,
                root.root_origin_server_ts,
                latest_reply.latest_event_id,
                latest_reply.latest_sender,
                latest_reply.latest_content,
                latest_reply.latest_origin_server_ts,
                reply_stats.reply_count,
                participants.participants,
                root.is_frozen,
                root.created_ts,
                GREATEST(
                    root.base_updated_ts,
                    COALESCE(latest_reply.latest_origin_server_ts, root.base_updated_ts)
                ) AS updated_ts
            FROM root
            LEFT JOIN latest_reply ON TRUE
            CROSS JOIN reply_stats
            CROSS JOIN participants
            ",
        )
        .bind(room_id)
        .bind(thread_id)
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn get_thread_statistics(
        &self,
        room_id: &str,
        thread_id: &str,
    ) -> Result<Option<ThreadStatistics>, sqlx::Error> {
        sqlx::query_as::<_, ThreadStatistics>(
            r"
            SELECT
                tr.id,
                tr.room_id,
                COALESCE(tr.thread_id, '') AS thread_id,
                COALESCE(reply_stats.total_replies, 0) AS total_replies,
                COALESCE(participant_stats.total_participants, 1) AS total_participants,
                COALESCE(reply_stats.total_edits, 0) AS total_edits,
                COALESCE(reply_stats.total_redactions, 0) AS total_redactions,
                reply_stats.first_reply_ts,
                reply_stats.last_reply_ts,
                reply_stats.avg_reply_time_ms,
                tr.created_ts,
                COALESCE(tr.updated_ts, tr.created_ts) AS updated_ts
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
            ",
        )
        .bind(room_id)
        .bind(thread_id)
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn search_threads(
        &self,
        room_id: &str,
        query: &str,
        limit: Option<i32>,
    ) -> Result<Vec<ThreadSummary>, sqlx::Error> {
        let limit = limit.unwrap_or(20);
        // Escape special characters in the query for ILIKE and plainto_tsquery
        // Double % for literal % in LIKE patterns. _ needs escaping too. Single quotes need escaping.
        let escaped_query = query.replace('%', r"%%").replace('_', r"\_").replace('\'', r"''");

        sqlx::query_as::<_, ThreadSummary>(
            r"
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
                ) AS search_relevance
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
            ORDER BY search_relevance DESC, COALESCE(latest_reply.latest_origin_server_ts, e.origin_server_ts, tr.created_ts) DESC NULLS LAST
            LIMIT $3
            ",
        )
        .bind(room_id)
        .bind(&escaped_query)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn freeze_thread(&self, room_id: &str, thread_id: &str) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();
        sqlx::query(
            r"
            UPDATE thread_roots
            SET is_fetched = TRUE, updated_ts = $3
            WHERE room_id = $1 AND thread_id = $2
            ",
        )
        .bind(room_id)
        .bind(thread_id)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn unfreeze_thread(&self, room_id: &str, thread_id: &str) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();
        sqlx::query(
            r"
            UPDATE thread_roots
            SET is_fetched = FALSE, updated_ts = $3
            WHERE room_id = $1 AND thread_id = $2
            ",
        )
        .bind(room_id)
        .bind(thread_id)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_thread_root() -> ThreadRoot {
        ThreadRoot {
            id: 1,
            room_id: "!test:example.com".to_string(),
            root_event_id: "$event1".to_string(),
            sender: "@user:example.com".to_string(),
            thread_id: Some("thread-001".to_string()),
            reply_count: Some(0),
            last_reply_event_id: None,
            last_reply_sender: None,
            last_reply_ts: None,
            participants: Some(serde_json::json!(["@user:example.com"])),
            is_fetched: false,
            created_ts: 1234567890,
            updated_ts: None,
        }
    }

    fn create_test_thread_reply() -> ThreadReply {
        ThreadReply {
            id: 1,
            room_id: "!test:example.com".to_string(),
            thread_id: "thread-001".to_string(),
            event_id: "$reply1".to_string(),
            root_event_id: "$event1".to_string(),
            sender: "@user2:example.com".to_string(),
            in_reply_to_event_id: Some("$event1".to_string()),
            content: serde_json::json!({"body": "Reply"}),
            origin_server_ts: 1234567891,
            is_edited: false,
            is_redacted: false,
            created_ts: 1234567891,
        }
    }

    fn create_test_thread_subscription() -> ThreadSubscription {
        ThreadSubscription {
            id: 1,
            room_id: "!test:example.com".to_string(),
            thread_id: "thread-001".to_string(),
            user_id: "@user:example.com".to_string(),
            notification_level: "all".to_string(),
            is_muted: false,
            is_pinned: false,
            subscribed_ts: 1234567890,
            updated_ts: 1234567890,
        }
    }

    #[test]
    fn test_thread_root_creation() {
        let thread = create_test_thread_root();
        assert_eq!(thread.id, 1);
        assert_eq!(thread.room_id, "!test:example.com");
        assert_eq!(thread.sender, "@user:example.com");
        assert_eq!(thread.reply_count, Some(0));
        assert!(!thread.is_fetched);
    }

    #[test]
    fn test_thread_reply_creation() {
        let reply = create_test_thread_reply();
        assert_eq!(reply.thread_id, "thread-001");
        assert_eq!(reply.sender, "@user2:example.com");
        assert!(reply.in_reply_to_event_id.is_some());
        assert!(!reply.is_edited);
        assert!(!reply.is_redacted);
    }

    #[test]
    fn test_thread_subscription_creation() {
        let subscription = create_test_thread_subscription();
        assert_eq!(subscription.notification_level, "all");
        assert!(!subscription.is_muted);
    }

    #[test]
    fn test_thread_list_params_defaults() {
        let params =
            ThreadListParams { room_id: "!test:example.com".to_string(), from: None, limit: None, include_all: false };
        assert_eq!(params.room_id, "!test:example.com");
        assert!(params.from.is_none());
        assert!(params.limit.is_none());
        assert!(!params.include_all);
    }

    #[test]
    fn test_create_thread_root_params() {
        let params = CreateThreadRootParams {
            room_id: "!test:example.com".to_string(),
            root_event_id: "$event1".to_string(),
            sender: "@user:example.com".to_string(),
            thread_id: Some("thread-001".to_string()),
        };
        assert_eq!(params.room_id, "!test:example.com");
        assert_eq!(params.root_event_id, "$event1");
        assert!(params.thread_id.is_some());
    }

    #[test]
    fn test_create_thread_reply_params() {
        let params = CreateThreadReplyParams {
            room_id: "!test:example.com".to_string(),
            thread_id: "thread-001".to_string(),
            event_id: "$reply1".to_string(),
            root_event_id: "$event1".to_string(),
            sender: "@user2:example.com".to_string(),
            in_reply_to_event_id: Some("$event1".to_string()),
            content: serde_json::json!({"body": "Reply"}),
            origin_server_ts: 1234567891,
        };
        assert_eq!(params.thread_id, "thread-001");
        assert!(params.in_reply_to_event_id.is_some());
    }

    #[test]
    fn test_notification_level_values() {
        let levels = vec!["all", "mentions", "none"];
        for level in levels {
            let subscription = ThreadSubscription {
                id: 1,
                room_id: "!test:example.com".to_string(),
                thread_id: "thread-001".to_string(),
                user_id: "@user:example.com".to_string(),
                notification_level: level.to_string(),
                is_muted: false,
                is_pinned: false,
                subscribed_ts: 1234567890,
                updated_ts: 1234567890,
            };
            assert_eq!(subscription.notification_level, level);
        }
    }

    #[test]
    fn test_thread_fetched_status() {
        let mut thread = create_test_thread_root();
        assert!(!thread.is_fetched);

        thread.is_fetched = true;
        assert!(thread.is_fetched);
    }

    #[test]
    fn test_thread_reply_edit_status() {
        let mut reply = create_test_thread_reply();
        assert!(!reply.is_edited);

        reply.is_edited = true;
        assert!(reply.is_edited);
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;

    async fn test_pool() -> Arc<Pool<Postgres>> {
        let db_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
        let pool =
            PgPoolOptions::new().max_connections(2).connect(&db_url).await.expect("Failed to connect to test database");
        Arc::new(pool)
    }

    async fn ensure_test_user(pool: &Pool<Postgres>, user_id: &str) {
        let now = current_timestamp_millis();
        let username = user_id.strip_prefix('@').and_then(|u| u.split(':').next()).unwrap_or("testuser");
        sqlx::query(
            r#"INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, $3) ON CONFLICT (user_id) DO NOTHING"#,
        )
        .bind(user_id)
        .bind(username)
        .bind(now)
        .execute(pool)
        .await
        .expect("failed to create test user");
    }

    async fn ensure_test_room(pool: &Pool<Postgres>, room_id: &str) {
        sqlx::query(
            r#"INSERT INTO rooms (room_id, room_version, is_public, creator, created_ts) VALUES ($1, '10', false, $2, $3) ON CONFLICT (room_id) DO NOTHING"#,
        )
        .bind(room_id)
        .bind("@test:localhost")
        .bind(current_timestamp_millis())
        .execute(pool)
        .await
        .expect("failed to create test room");
    }

    async fn cleanup_thread_data(pool: &Pool<Postgres>, room_id: &str, thread_id: &str) {
        sqlx::query("DELETE FROM thread_read_receipts WHERE room_id = $1 AND thread_id = $2")
            .bind(room_id)
            .bind(thread_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM thread_subscriptions WHERE room_id = $1 AND thread_id = $2")
            .bind(room_id)
            .bind(thread_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM thread_relations WHERE room_id = $1 AND thread_id = $2")
            .bind(room_id)
            .bind(thread_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM thread_replies WHERE room_id = $1 AND thread_id = $2")
            .bind(room_id)
            .bind(thread_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM thread_roots WHERE room_id = $1 AND thread_id = $2")
            .bind(room_id)
            .bind(thread_id)
            .execute(pool)
            .await
            .ok();
    }

    // 1. test_create_thread_root
    #[tokio::test]
    async fn test_create_thread_root() {
        let pool = test_pool().await;
        let storage = ThreadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_cr_{suffix}:localhost");
        let thread_id = format!("thread-cr-{suffix}");

        ensure_test_room(&pool, &room_id).await;
        cleanup_thread_data(&pool, &room_id, &thread_id).await;

        let root = storage
            .create_thread_root(CreateThreadRootParams {
                room_id: room_id.clone(),
                root_event_id: format!("$root_{suffix}"),
                sender: "@sender:localhost".to_string(),
                thread_id: Some(thread_id.clone()),
            })
            .await
            .expect("should create thread root");

        assert!(root.id > 0);
        assert_eq!(root.room_id, room_id);
        assert_eq!(root.thread_id, Some(thread_id.clone()));
        assert_eq!(root.reply_count, Some(0));
        assert!(!root.is_fetched);

        cleanup_thread_data(&pool, &room_id, &thread_id).await;
    }

    // 2. test_get_thread_root_found
    #[tokio::test]
    async fn test_get_thread_root_found() {
        let pool = test_pool().await;
        let storage = ThreadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_gt_{suffix}:localhost");
        let thread_id = format!("thread-gt-{suffix}");

        ensure_test_room(&pool, &room_id).await;
        cleanup_thread_data(&pool, &room_id, &thread_id).await;

        storage
            .create_thread_root(CreateThreadRootParams {
                room_id: room_id.clone(),
                root_event_id: format!("$root_{suffix}"),
                sender: "@sender:localhost".to_string(),
                thread_id: Some(thread_id.clone()),
            })
            .await
            .expect("should create thread root");

        let found = storage.get_thread_root(&room_id, &thread_id).await.expect("query should succeed");

        assert!(found.is_some());
        let root = found.unwrap();
        assert_eq!(root.room_id, room_id);
        assert_eq!(root.thread_id, Some(thread_id.clone()));

        cleanup_thread_data(&pool, &room_id, &thread_id).await;
    }

    // 3. test_get_thread_root_not_found
    #[tokio::test]
    async fn test_get_thread_root_not_found() {
        let pool = test_pool().await;
        let storage = ThreadStorage::new(&pool);

        let result = storage
            .get_thread_root("!nonexistent:localhost", "nonexistent-thread")
            .await
            .expect("query should succeed");

        assert!(result.is_none(), "nonexistent thread should return None");
    }

    // 4. test_get_thread_root_by_event
    #[tokio::test]
    async fn test_get_thread_root_by_event() {
        let pool = test_pool().await;
        let storage = ThreadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_gte_{suffix}:localhost");
        let thread_id = format!("thread-gte-{suffix}");

        ensure_test_room(&pool, &room_id).await;
        cleanup_thread_data(&pool, &room_id, &thread_id).await;

        storage
            .create_thread_root(CreateThreadRootParams {
                room_id: room_id.clone(),
                root_event_id: format!("$root_{suffix}"),
                sender: "@sender:localhost".to_string(),
                thread_id: Some(thread_id.clone()),
            })
            .await
            .expect("should create thread root");

        let found =
            storage.get_thread_root_by_event(&room_id, &format!("$root_{suffix}")).await.expect("query should succeed");

        assert!(found.is_some());
        let root = found.unwrap();
        assert_eq!(root.root_event_id, format!("$root_{suffix}"));

        cleanup_thread_data(&pool, &room_id, &thread_id).await;
    }

    // 5. test_list_thread_roots
    #[tokio::test]
    async fn test_list_thread_roots() {
        let pool = test_pool().await;
        let storage = ThreadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_lt_{suffix}:localhost");
        let t1 = format!("thread-lt-a-{suffix}");
        let t2 = format!("thread-lt-b-{suffix}");

        ensure_test_room(&pool, &room_id).await;
        cleanup_thread_data(&pool, &room_id, &t1).await;
        cleanup_thread_data(&pool, &room_id, &t2).await;

        for tid in &[&t1, &t2] {
            storage
                .create_thread_root(CreateThreadRootParams {
                    room_id: room_id.clone(),
                    root_event_id: format!("$ev_{}", tid),
                    sender: "@sender:localhost".to_string(),
                    thread_id: Some((*tid).clone()),
                })
                .await
                .expect("should create thread root");
        }

        let roots = storage
            .list_thread_roots(ThreadListParams {
                room_id: room_id.clone(),
                limit: Some(10),
                from: None,
                include_all: false,
            })
            .await
            .expect("should list roots");

        assert!(roots.len() >= 2);

        cleanup_thread_data(&pool, &room_id, &t1).await;
        cleanup_thread_data(&pool, &room_id, &t2).await;
    }

    // 6. test_list_all_thread_roots
    #[tokio::test]
    async fn test_list_all_thread_roots() {
        let pool = test_pool().await;
        let storage = ThreadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_la_{suffix}:localhost");
        let thread_id = format!("thread-la-{suffix}");

        ensure_test_room(&pool, &room_id).await;
        cleanup_thread_data(&pool, &room_id, &thread_id).await;

        storage
            .create_thread_root(CreateThreadRootParams {
                room_id: room_id.clone(),
                root_event_id: format!("$root_{suffix}"),
                sender: "@sender:localhost".to_string(),
                thread_id: Some(thread_id.clone()),
            })
            .await
            .expect("should create thread root");

        let roots = storage.list_all_thread_roots(Some(10), None).await.expect("should list all roots");

        assert!(!roots.is_empty());

        cleanup_thread_data(&pool, &room_id, &thread_id).await;
    }

    // 7. test_create_thread_reply
    #[tokio::test]
    async fn test_create_thread_reply() {
        let pool = test_pool().await;
        let storage = ThreadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_rp_{suffix}:localhost");
        let thread_id = format!("thread-rp-{suffix}");

        ensure_test_room(&pool, &room_id).await;
        cleanup_thread_data(&pool, &room_id, &thread_id).await;

        storage
            .create_thread_root(CreateThreadRootParams {
                room_id: room_id.clone(),
                root_event_id: format!("$root_{suffix}"),
                sender: "@sender:localhost".to_string(),
                thread_id: Some(thread_id.clone()),
            })
            .await
            .expect("should create thread root");

        let ts = current_timestamp_millis();
        let reply = storage
            .create_thread_reply(CreateThreadReplyParams {
                room_id: room_id.clone(),
                thread_id: thread_id.clone(),
                event_id: format!("$reply_{suffix}"),
                root_event_id: format!("$root_{suffix}"),
                sender: "@replier:localhost".to_string(),
                in_reply_to_event_id: Some(format!("$root_{suffix}")),
                content: serde_json::json!({"body": "test reply"}),
                origin_server_ts: ts,
            })
            .await
            .expect("should create reply");

        assert!(reply.id > 0);
        assert_eq!(reply.thread_id, thread_id);
        assert_eq!(reply.sender, "@replier:localhost");
        assert!(!reply.is_edited);
        assert!(!reply.is_redacted);

        cleanup_thread_data(&pool, &room_id, &thread_id).await;
    }

    // 8. test_get_thread_replies
    #[tokio::test]
    async fn test_get_thread_replies() {
        let pool = test_pool().await;
        let storage = ThreadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_gtr_{suffix}:localhost");
        let thread_id = format!("thread-gtr-{suffix}");

        ensure_test_room(&pool, &room_id).await;
        cleanup_thread_data(&pool, &room_id, &thread_id).await;

        storage
            .create_thread_root(CreateThreadRootParams {
                room_id: room_id.clone(),
                root_event_id: format!("$root_{suffix}"),
                sender: "@sender:localhost".to_string(),
                thread_id: Some(thread_id.clone()),
            })
            .await
            .expect("should create thread root");

        let ts1 = current_timestamp_millis();
        let ts2 = ts1 + 1;
        storage
            .create_thread_reply(CreateThreadReplyParams {
                room_id: room_id.clone(),
                thread_id: thread_id.clone(),
                event_id: format!("$r1_{suffix}"),
                root_event_id: format!("$root_{suffix}"),
                sender: "@r1:localhost".to_string(),
                in_reply_to_event_id: None,
                content: serde_json::json!({"body": "first reply"}),
                origin_server_ts: ts1,
            })
            .await
            .expect("should create reply1");
        storage
            .create_thread_reply(CreateThreadReplyParams {
                room_id: room_id.clone(),
                thread_id: thread_id.clone(),
                event_id: format!("$r2_{suffix}"),
                root_event_id: format!("$root_{suffix}"),
                sender: "@r2:localhost".to_string(),
                in_reply_to_event_id: None,
                content: serde_json::json!({"body": "second reply"}),
                origin_server_ts: ts2,
            })
            .await
            .expect("should create reply2");

        let replies =
            storage.get_thread_replies(&room_id, &thread_id, Some(10), None).await.expect("should get replies");

        assert!(replies.len() >= 2);

        cleanup_thread_data(&pool, &room_id, &thread_id).await;
    }

    // 9. test_get_reply_count
    #[tokio::test]
    async fn test_get_reply_count() {
        let pool = test_pool().await;
        let storage = ThreadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_rc_{suffix}:localhost");
        let thread_id = format!("thread-rc-{suffix}");

        ensure_test_room(&pool, &room_id).await;
        cleanup_thread_data(&pool, &room_id, &thread_id).await;

        // Zero when no thread exists yet
        let count0 = storage.get_reply_count(&room_id, &thread_id).await.expect("should get count");
        assert_eq!(count0, 0);

        storage
            .create_thread_root(CreateThreadRootParams {
                room_id: room_id.clone(),
                root_event_id: format!("$root_{suffix}"),
                sender: "@sender:localhost".to_string(),
                thread_id: Some(thread_id.clone()),
            })
            .await
            .expect("should create root");

        storage
            .create_thread_reply(CreateThreadReplyParams {
                room_id: room_id.clone(),
                thread_id: thread_id.clone(),
                event_id: format!("$r1_{suffix}"),
                root_event_id: format!("$root_{suffix}"),
                sender: "@r1:localhost".to_string(),
                in_reply_to_event_id: None,
                content: serde_json::json!({"body": "reply1"}),
                origin_server_ts: current_timestamp_millis(),
            })
            .await
            .expect("should create reply1");

        let count1 = storage.get_reply_count(&room_id, &thread_id).await.expect("should get count");
        assert_eq!(count1, 1);

        storage
            .create_thread_reply(CreateThreadReplyParams {
                room_id: room_id.clone(),
                thread_id: thread_id.clone(),
                event_id: format!("$r2_{suffix}"),
                root_event_id: format!("$root_{suffix}"),
                sender: "@r2:localhost".to_string(),
                in_reply_to_event_id: None,
                content: serde_json::json!({"body": "reply2"}),
                origin_server_ts: current_timestamp_millis(),
            })
            .await
            .expect("should create reply2");

        let count2 = storage.get_reply_count(&room_id, &thread_id).await.expect("should get count");
        assert_eq!(count2, 2);

        cleanup_thread_data(&pool, &room_id, &thread_id).await;
    }

    // 10. test_subscribe_to_thread
    #[tokio::test]
    async fn test_subscribe_to_thread() {
        let pool = test_pool().await;
        let storage = ThreadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_sub_{suffix}:localhost");
        let thread_id = format!("thread-sub-{suffix}");
        let user_id = format!("@user_sub_{suffix}:localhost");

        ensure_test_room(&pool, &room_id).await;
        ensure_test_user(&pool, &user_id).await;
        cleanup_thread_data(&pool, &room_id, &thread_id).await;

        storage
            .create_thread_root(CreateThreadRootParams {
                room_id: room_id.clone(),
                root_event_id: format!("$root_{suffix}"),
                sender: user_id.clone(),
                thread_id: Some(thread_id.clone()),
            })
            .await
            .expect("should create thread root");

        let sub = storage.subscribe_to_thread(&room_id, &thread_id, &user_id, "all").await.expect("should subscribe");

        assert!(sub.id > 0);
        assert_eq!(sub.room_id, room_id);
        assert_eq!(sub.thread_id, thread_id);
        assert_eq!(sub.user_id, user_id);
        assert_eq!(sub.notification_level, "all");
        assert!(!sub.is_muted);

        cleanup_thread_data(&pool, &room_id, &thread_id).await;
    }

    // 11. test_unsubscribe_from_thread
    #[tokio::test]
    async fn test_unsubscribe_from_thread() {
        let pool = test_pool().await;
        let storage = ThreadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_unsub_{suffix}:localhost");
        let thread_id = format!("thread-unsub-{suffix}");
        let user_id = format!("@user_unsub_{suffix}:localhost");

        ensure_test_room(&pool, &room_id).await;
        ensure_test_user(&pool, &user_id).await;
        cleanup_thread_data(&pool, &room_id, &thread_id).await;

        storage
            .create_thread_root(CreateThreadRootParams {
                room_id: room_id.clone(),
                root_event_id: format!("$root_{suffix}"),
                sender: user_id.clone(),
                thread_id: Some(thread_id.clone()),
            })
            .await
            .expect("should create root");

        storage.subscribe_to_thread(&room_id, &thread_id, &user_id, "all").await.expect("should subscribe");

        storage.unsubscribe_from_thread(&room_id, &thread_id, &user_id).await.expect("should unsubscribe");

        let sub = storage.get_thread_subscription(&room_id, &thread_id, &user_id).await.expect("query should succeed");

        assert!(sub.is_none(), "subscription should be removed after unsubscribe");

        cleanup_thread_data(&pool, &room_id, &thread_id).await;
    }

    // 12. test_get_thread_subscription — found / not found
    #[tokio::test]
    async fn test_get_thread_subscription() {
        let pool = test_pool().await;
        let storage = ThreadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_gsub_{suffix}:localhost");
        let thread_id = format!("thread-gsub-{suffix}");
        let user_id = format!("@user_gsub_{suffix}:localhost");

        ensure_test_room(&pool, &room_id).await;
        ensure_test_user(&pool, &user_id).await;
        cleanup_thread_data(&pool, &room_id, &thread_id).await;

        // Not found initially
        let sub = storage.get_thread_subscription(&room_id, &thread_id, &user_id).await.expect("query should succeed");
        assert!(sub.is_none(), "should not exist before subscribe");

        storage
            .create_thread_root(CreateThreadRootParams {
                room_id: room_id.clone(),
                root_event_id: format!("$root_{suffix}"),
                sender: user_id.clone(),
                thread_id: Some(thread_id.clone()),
            })
            .await
            .expect("should create root");

        storage.subscribe_to_thread(&room_id, &thread_id, &user_id, "all").await.expect("should subscribe");

        // Found after subscription
        let sub = storage.get_thread_subscription(&room_id, &thread_id, &user_id).await.expect("query should succeed");
        assert!(sub.is_some(), "should exist after subscribe");
        let sub = sub.unwrap();
        assert_eq!(sub.notification_level, "all");
        assert!(!sub.is_muted);

        cleanup_thread_data(&pool, &room_id, &thread_id).await;
    }

    // 13. test_get_user_thread_subscriptions
    #[tokio::test]
    async fn test_get_user_thread_subscriptions() {
        let pool = test_pool().await;
        let storage = ThreadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_uts_{suffix}:localhost");
        let user_id = format!("@user_uts_{suffix}:localhost");
        let t1 = format!("thread-uts-a-{suffix}");
        let t2 = format!("thread-uts-b-{suffix}");

        ensure_test_room(&pool, &room_id).await;
        ensure_test_user(&pool, &user_id).await;
        cleanup_thread_data(&pool, &room_id, &t1).await;
        cleanup_thread_data(&pool, &room_id, &t2).await;

        for tid in &[&t1, &t2] {
            storage
                .create_thread_root(CreateThreadRootParams {
                    room_id: room_id.clone(),
                    root_event_id: format!("$ev_{}", tid),
                    sender: user_id.clone(),
                    thread_id: Some((*tid).clone()),
                })
                .await
                .expect("should create root");
            storage.subscribe_to_thread(&room_id, tid, &user_id, "all").await.expect("should subscribe");
        }

        let subs = storage.get_user_thread_subscriptions(&user_id, Some(10)).await.expect("should get subscriptions");

        assert!(subs.len() >= 2, "expected at least 2 subscriptions, got {}", subs.len());

        cleanup_thread_data(&pool, &room_id, &t1).await;
        cleanup_thread_data(&pool, &room_id, &t2).await;
    }

    // 14. test_update_read_receipt
    #[tokio::test]
    async fn test_update_read_receipt() {
        let pool = test_pool().await;
        let storage = ThreadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_urr_{suffix}:localhost");
        let thread_id = format!("thread-urr-{suffix}");
        let user_id = format!("@user_urr_{suffix}:localhost");

        ensure_test_room(&pool, &room_id).await;
        ensure_test_user(&pool, &user_id).await;
        cleanup_thread_data(&pool, &room_id, &thread_id).await;

        storage
            .create_thread_root(CreateThreadRootParams {
                room_id: room_id.clone(),
                root_event_id: format!("$root_{suffix}"),
                sender: user_id.clone(),
                thread_id: Some(thread_id.clone()),
            })
            .await
            .expect("should create thread root");

        let ts = current_timestamp_millis();
        let receipt = storage
            .update_read_receipt(&room_id, &thread_id, &user_id, "$event_last", ts)
            .await
            .expect("should update receipt");

        assert!(receipt.id > 0);
        assert_eq!(receipt.room_id, room_id);
        assert_eq!(receipt.thread_id, thread_id);
        assert_eq!(receipt.user_id, user_id);
        assert_eq!(receipt.last_read_event_id, Some("$event_last".to_string()));
        assert_eq!(receipt.unread_count, 0);

        cleanup_thread_data(&pool, &room_id, &thread_id).await;
    }

    // 15. test_get_read_receipt
    #[tokio::test]
    async fn test_get_read_receipt() {
        let pool = test_pool().await;
        let storage = ThreadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_grr_{suffix}:localhost");
        let thread_id = format!("thread-grr-{suffix}");
        let user_id = format!("@user_grr_{suffix}:localhost");

        ensure_test_room(&pool, &room_id).await;
        ensure_test_user(&pool, &user_id).await;
        cleanup_thread_data(&pool, &room_id, &thread_id).await;

        // Not found initially
        let rr = storage.get_read_receipt(&room_id, &thread_id, &user_id).await.expect("query should succeed");
        assert!(rr.is_none(), "read receipt should not exist initially");

        storage
            .create_thread_root(CreateThreadRootParams {
                room_id: room_id.clone(),
                root_event_id: format!("$root_{suffix}"),
                sender: user_id.clone(),
                thread_id: Some(thread_id.clone()),
            })
            .await
            .expect("should create root");

        storage
            .update_read_receipt(&room_id, &thread_id, &user_id, "$event_123", current_timestamp_millis())
            .await
            .expect("should update receipt");

        let rr = storage.get_read_receipt(&room_id, &thread_id, &user_id).await.expect("query should succeed");

        assert!(rr.is_some(), "read receipt should exist after update");

        cleanup_thread_data(&pool, &room_id, &thread_id).await;
    }

    // 16. test_delete_thread
    #[tokio::test]
    async fn test_delete_thread() {
        let pool = test_pool().await;
        let storage = ThreadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_del_{suffix}:localhost");
        let thread_id = format!("thread-del-{suffix}");

        ensure_test_room(&pool, &room_id).await;
        cleanup_thread_data(&pool, &room_id, &thread_id).await;

        storage
            .create_thread_root(CreateThreadRootParams {
                room_id: room_id.clone(),
                root_event_id: format!("$root_{suffix}"),
                sender: "@sender:localhost".to_string(),
                thread_id: Some(thread_id.clone()),
            })
            .await
            .expect("should create thread root");

        // Create a reply too, to verify cascading delete
        storage
            .create_thread_reply(CreateThreadReplyParams {
                room_id: room_id.clone(),
                thread_id: thread_id.clone(),
                event_id: format!("$reply_{suffix}"),
                root_event_id: format!("$root_{suffix}"),
                sender: "@replier:localhost".to_string(),
                in_reply_to_event_id: None,
                content: serde_json::json!({"body": "reply"}),
                origin_server_ts: current_timestamp_millis(),
            })
            .await
            .expect("should create reply");

        // Verify thread and reply exist before delete
        assert!(storage.get_thread_root(&room_id, &thread_id).await.unwrap().is_some());
        assert_eq!(storage.get_reply_count(&room_id, &thread_id).await.unwrap(), 1);

        storage.delete_thread(&room_id, &thread_id).await.expect("should delete");

        // Root should be gone
        let root = storage.get_thread_root(&room_id, &thread_id).await.expect("query should succeed");
        assert!(root.is_none(), "thread root should be deleted");

        // Reply should be gone
        let count = storage.get_reply_count(&room_id, &thread_id).await.expect("query should succeed");
        assert_eq!(count, 0, "replies should be deleted");

        cleanup_thread_data(&pool, &room_id, &thread_id).await;
    }

    // === Helper: insert a minimal event row for search_threads tests ===
    async fn insert_test_event(
        pool: &Pool<Postgres>,
        event_id: &str,
        room_id: &str,
        sender: &str,
        body: &str,
        origin_server_ts: i64,
    ) {
        sqlx::query(
            r#"INSERT INTO events (event_id, room_id, sender, event_type, content, origin_server_ts)
               VALUES ($1, $2, $3, 'm.room.message', $4, $5)
               ON CONFLICT (event_id) DO UPDATE SET content = EXCLUDED.content"#,
        )
        .bind(event_id)
        .bind(room_id)
        .bind(sender)
        .bind(serde_json::json!({"body": body}))
        .bind(origin_server_ts)
        .execute(pool)
        .await
        .expect("failed to insert test event");
    }

    // 17. test_get_thread_participants
    #[tokio::test]
    async fn test_get_thread_participants() {
        let pool = test_pool().await;
        let storage = ThreadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_gp_{suffix}:localhost");
        let thread_id = format!("thread-gp-{suffix}");

        ensure_test_room(&pool, &room_id).await;
        cleanup_thread_data(&pool, &room_id, &thread_id).await;

        // Create root with sender A
        storage
            .create_thread_root(CreateThreadRootParams {
                room_id: room_id.clone(),
                root_event_id: format!("$root_{suffix}"),
                sender: "@senderA:localhost".to_string(),
                thread_id: Some(thread_id.clone()),
            })
            .await
            .expect("should create root");

        // Create reply from sender B
        storage
            .create_thread_reply(CreateThreadReplyParams {
                room_id: room_id.clone(),
                thread_id: thread_id.clone(),
                event_id: format!("$reply_{suffix}"),
                root_event_id: format!("$root_{suffix}"),
                sender: "@senderB:localhost".to_string(),
                in_reply_to_event_id: None,
                content: serde_json::json!({"body": "reply"}),
                origin_server_ts: current_timestamp_millis(),
            })
            .await
            .expect("should create reply");

        let participants =
            storage.get_thread_participants(&room_id, &thread_id).await.expect("should get participants");
        assert!(participants.contains(&"@senderA:localhost".to_string()), "should include root sender");
        assert!(participants.contains(&"@senderB:localhost".to_string()), "should include reply sender");

        cleanup_thread_data(&pool, &room_id, &thread_id).await;
    }

    // 18. test_mute_thread (creates a new muted subscription)
    #[tokio::test]
    async fn test_mute_thread() {
        let pool = test_pool().await;
        let storage = ThreadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_mt_{suffix}:localhost");
        let thread_id = format!("thread-mt-{suffix}");
        let user_id = format!("@user_mt_{suffix}:localhost");

        ensure_test_room(&pool, &room_id).await;
        ensure_test_user(&pool, &user_id).await;
        cleanup_thread_data(&pool, &room_id, &thread_id).await;

        storage
            .create_thread_root(CreateThreadRootParams {
                room_id: room_id.clone(),
                root_event_id: format!("$root_{suffix}"),
                sender: user_id.clone(),
                thread_id: Some(thread_id.clone()),
            })
            .await
            .expect("should create root");

        let muted = storage.mute_thread(&room_id, &thread_id, &user_id).await.expect("should mute thread");
        assert!(muted.is_muted, "subscription should be muted");
        assert_eq!(muted.notification_level, "none");

        cleanup_thread_data(&pool, &room_id, &thread_id).await;
    }

    // 19. test_mute_thread_updates_existing_subscription
    #[tokio::test]
    async fn test_mute_thread_updates_existing_subscription() {
        let pool = test_pool().await;
        let storage = ThreadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_mt2_{suffix}:localhost");
        let thread_id = format!("thread-mt2-{suffix}");
        let user_id = format!("@user_mt2_{suffix}:localhost");

        ensure_test_room(&pool, &room_id).await;
        ensure_test_user(&pool, &user_id).await;
        cleanup_thread_data(&pool, &room_id, &thread_id).await;

        storage
            .create_thread_root(CreateThreadRootParams {
                room_id: room_id.clone(),
                root_event_id: format!("$root_{suffix}"),
                sender: user_id.clone(),
                thread_id: Some(thread_id.clone()),
            })
            .await
            .expect("should create root");

        // Subscribe first (not muted)
        let sub = storage.subscribe_to_thread(&room_id, &thread_id, &user_id, "all").await.expect("should subscribe");
        assert!(!sub.is_muted);

        // Now mute — should update the existing subscription
        let muted = storage.mute_thread(&room_id, &thread_id, &user_id).await.expect("should mute");
        assert!(muted.is_muted, "subscription should now be muted");

        // Verify via get_thread_subscription
        let fetched = storage.get_thread_subscription(&room_id, &thread_id, &user_id).await.unwrap().unwrap();
        assert!(fetched.is_muted, "fetched subscription should be muted");

        cleanup_thread_data(&pool, &room_id, &thread_id).await;
    }

    // 20. test_increment_unread_count (from 0 to 1)
    #[tokio::test]
    async fn test_increment_unread_count() {
        let pool = test_pool().await;
        let storage = ThreadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_iu_{suffix}:localhost");
        let thread_id = format!("thread-iu-{suffix}");
        let user_id = format!("@user_iu_{suffix}:localhost");

        ensure_test_room(&pool, &room_id).await;
        ensure_test_user(&pool, &user_id).await;
        cleanup_thread_data(&pool, &room_id, &thread_id).await;

        // No receipt initially
        let before = storage.get_read_receipt(&room_id, &thread_id, &user_id).await.unwrap();
        assert!(before.is_none());

        // Increment once
        storage.increment_unread_count(&room_id, &thread_id, &user_id).await.expect("should increment");

        let after = storage.get_read_receipt(&room_id, &thread_id, &user_id).await.unwrap().unwrap();
        assert_eq!(after.unread_count, 1, "unread count should be 1 after first increment");

        cleanup_thread_data(&pool, &room_id, &thread_id).await;
    }

    // 21. test_increment_unread_count_accumulates
    #[tokio::test]
    async fn test_increment_unread_count_accumulates() {
        let pool = test_pool().await;
        let storage = ThreadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_iu2_{suffix}:localhost");
        let thread_id = format!("thread-iu2-{suffix}");
        let user_id = format!("@user_iu2_{suffix}:localhost");

        ensure_test_room(&pool, &room_id).await;
        ensure_test_user(&pool, &user_id).await;
        cleanup_thread_data(&pool, &room_id, &thread_id).await;

        // Increment 3 times
        for _ in 0..3 {
            storage.increment_unread_count(&room_id, &thread_id, &user_id).await.expect("should increment");
        }

        let after = storage.get_read_receipt(&room_id, &thread_id, &user_id).await.unwrap().unwrap();
        assert_eq!(after.unread_count, 3, "unread count should accumulate to 3");

        cleanup_thread_data(&pool, &room_id, &thread_id).await;
    }

    // 22. test_create_thread_relation
    #[tokio::test]
    async fn test_create_thread_relation() {
        let pool = test_pool().await;
        let storage = ThreadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_tr_{suffix}:localhost");
        let thread_id = format!("thread-tr-{suffix}");

        ensure_test_room(&pool, &room_id).await;
        cleanup_thread_data(&pool, &room_id, &thread_id).await;

        let event_id = format!("$evt_{suffix}");
        let relates_to = format!("$root_{suffix}");

        let relation = storage
            .create_thread_relation(&room_id, &event_id, &relates_to, "m.thread", Some(&thread_id), false)
            .await
            .expect("should create relation");

        assert!(relation.id > 0);
        assert_eq!(relation.room_id, room_id);
        assert_eq!(relation.event_id, event_id);
        assert_eq!(relation.relates_to_event_id, relates_to);
        assert_eq!(relation.relation_type, "m.thread");
        assert_eq!(relation.thread_id, Some(thread_id.clone()));
        assert!(!relation.is_falling_back);

        // Cleanup the relation (cleanup_thread_data doesn't delete relations by thread_id when thread_id is None in the row)
        let _ = sqlx::query("DELETE FROM thread_relations WHERE room_id = $1 AND event_id = $2")
            .bind(&room_id)
            .bind(&event_id)
            .execute(&*pool)
            .await;
        cleanup_thread_data(&pool, &room_id, &thread_id).await;
    }

    // 23. test_create_thread_relation_with_fallback
    #[tokio::test]
    async fn test_create_thread_relation_with_fallback() {
        let pool = test_pool().await;
        let storage = ThreadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_trfb_{suffix}:localhost");
        let thread_id = format!("thread-trfb-{suffix}");

        ensure_test_room(&pool, &room_id).await;
        cleanup_thread_data(&pool, &room_id, &thread_id).await;

        let event_id = format!("$evt_fb_{suffix}");
        let relates_to = format!("$root_fb_{suffix}");

        let relation = storage
            .create_thread_relation(&room_id, &event_id, &relates_to, "m.thread", Some(&thread_id), true)
            .await
            .expect("should create relation with fallback");

        assert!(relation.is_falling_back, "relation should have is_falling_back=true");

        let _ = sqlx::query("DELETE FROM thread_relations WHERE room_id = $1 AND event_id = $2")
            .bind(&room_id)
            .bind(&event_id)
            .execute(&*pool)
            .await;
        cleanup_thread_data(&pool, &room_id, &thread_id).await;
    }

    // 24. test_mark_reply_edited
    #[tokio::test]
    async fn test_mark_reply_edited() {
        let pool = test_pool().await;
        let storage = ThreadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_me_{suffix}:localhost");
        let thread_id = format!("thread-me-{suffix}");

        ensure_test_room(&pool, &room_id).await;
        cleanup_thread_data(&pool, &room_id, &thread_id).await;

        storage
            .create_thread_root(CreateThreadRootParams {
                room_id: room_id.clone(),
                root_event_id: format!("$root_{suffix}"),
                sender: "@sender:localhost".to_string(),
                thread_id: Some(thread_id.clone()),
            })
            .await
            .expect("should create root");

        let event_id = format!("$reply_{suffix}");
        storage
            .create_thread_reply(CreateThreadReplyParams {
                room_id: room_id.clone(),
                thread_id: thread_id.clone(),
                event_id: event_id.clone(),
                root_event_id: format!("$root_{suffix}"),
                sender: "@replier:localhost".to_string(),
                in_reply_to_event_id: None,
                content: serde_json::json!({"body": "original"}),
                origin_server_ts: current_timestamp_millis(),
            })
            .await
            .expect("should create reply");

        // Verify not edited initially
        let replies = storage.get_thread_replies(&room_id, &thread_id, Some(10), None).await.unwrap();
        let reply = replies.iter().find(|r| r.event_id == event_id).unwrap();
        assert!(!reply.is_edited);

        // Mark as edited
        storage.mark_reply_edited(&room_id, &event_id).await.expect("should mark edited");

        // Verify edited
        let replies = storage.get_thread_replies(&room_id, &thread_id, Some(10), None).await.unwrap();
        let reply = replies.iter().find(|r| r.event_id == event_id).unwrap();
        assert!(reply.is_edited, "reply should be marked as edited");

        cleanup_thread_data(&pool, &room_id, &thread_id).await;
    }

    // 25. test_mark_reply_redacted
    #[tokio::test]
    async fn test_mark_reply_redacted() {
        let pool = test_pool().await;
        let storage = ThreadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_mr_{suffix}:localhost");
        let thread_id = format!("thread-mr-{suffix}");

        ensure_test_room(&pool, &room_id).await;
        cleanup_thread_data(&pool, &room_id, &thread_id).await;

        storage
            .create_thread_root(CreateThreadRootParams {
                room_id: room_id.clone(),
                root_event_id: format!("$root_{suffix}"),
                sender: "@sender:localhost".to_string(),
                thread_id: Some(thread_id.clone()),
            })
            .await
            .expect("should create root");

        let event_id = format!("$reply_{suffix}");
        storage
            .create_thread_reply(CreateThreadReplyParams {
                room_id: room_id.clone(),
                thread_id: thread_id.clone(),
                event_id: event_id.clone(),
                root_event_id: format!("$root_{suffix}"),
                sender: "@replier:localhost".to_string(),
                in_reply_to_event_id: None,
                content: serde_json::json!({"body": "original content"}),
                origin_server_ts: current_timestamp_millis(),
            })
            .await
            .expect("should create reply");

        // Verify not redacted initially
        let replies = storage.get_thread_replies(&room_id, &thread_id, Some(10), None).await.unwrap();
        let reply = replies.iter().find(|r| r.event_id == event_id).unwrap();
        assert!(!reply.is_redacted);

        // Mark as redacted
        storage.mark_reply_redacted(&room_id, &event_id).await.expect("should mark redacted");

        // Verify redacted and content cleared
        let replies = storage.get_thread_replies(&room_id, &thread_id, Some(10), None).await.unwrap();
        let reply = replies.iter().find(|r| r.event_id == event_id).unwrap();
        assert!(reply.is_redacted, "reply should be marked as redacted");
        assert_eq!(reply.content, serde_json::json!({}), "content should be cleared to empty object");

        cleanup_thread_data(&pool, &room_id, &thread_id).await;
    }

    // 26. test_get_threads_with_unread_with_room_id
    #[tokio::test]
    async fn test_get_threads_with_unread_with_room_id() {
        let pool = test_pool().await;
        let storage = ThreadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_twu_{suffix}:localhost");
        let thread_id = format!("thread-twu-{suffix}");
        let user_id = format!("@user_twu_{suffix}:localhost");

        ensure_test_room(&pool, &room_id).await;
        ensure_test_user(&pool, &user_id).await;
        cleanup_thread_data(&pool, &room_id, &thread_id).await;

        // No unread threads initially
        let unread = storage.get_threads_with_unread(&user_id, Some(&room_id)).await.expect("should query");
        assert!(
            unread.iter().all(|u| u.room_id != room_id || u.thread_id != thread_id),
            "should not contain our thread yet"
        );

        // Increment unread
        storage.increment_unread_count(&room_id, &thread_id, &user_id).await.expect("should increment");

        // Now should appear
        let unread = storage.get_threads_with_unread(&user_id, Some(&room_id)).await.expect("should query");
        assert!(unread.iter().any(|u| u.room_id == room_id && u.thread_id == thread_id), "should contain our thread");

        cleanup_thread_data(&pool, &room_id, &thread_id).await;
    }

    // 27. test_get_threads_with_unread_without_room_id
    #[tokio::test]
    async fn test_get_threads_with_unread_without_room_id() {
        let pool = test_pool().await;
        let storage = ThreadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_twu2_{suffix}:localhost");
        let thread_id = format!("thread-twu2-{suffix}");
        let user_id = format!("@user_twu2_{suffix}:localhost");

        ensure_test_room(&pool, &room_id).await;
        ensure_test_user(&pool, &user_id).await;
        cleanup_thread_data(&pool, &room_id, &thread_id).await;

        // Increment unread
        storage.increment_unread_count(&room_id, &thread_id, &user_id).await.expect("should increment");

        // Query across all rooms
        let unread = storage.get_threads_with_unread(&user_id, None).await.expect("should query");
        assert!(unread.iter().any(|u| u.room_id == room_id && u.thread_id == thread_id), "should contain our thread");

        cleanup_thread_data(&pool, &room_id, &thread_id).await;
    }

    // 28. test_get_thread_summary
    #[tokio::test]
    async fn test_get_thread_summary() {
        let pool = test_pool().await;
        let storage = ThreadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_ts_{suffix}:localhost");
        let thread_id = format!("thread-ts-{suffix}");

        ensure_test_room(&pool, &room_id).await;
        cleanup_thread_data(&pool, &room_id, &thread_id).await;

        storage
            .create_thread_root(CreateThreadRootParams {
                room_id: room_id.clone(),
                root_event_id: format!("$root_{suffix}"),
                sender: "@root_sender:localhost".to_string(),
                thread_id: Some(thread_id.clone()),
            })
            .await
            .expect("should create root");

        // Add a reply
        storage
            .create_thread_reply(CreateThreadReplyParams {
                room_id: room_id.clone(),
                thread_id: thread_id.clone(),
                event_id: format!("$reply_{suffix}"),
                root_event_id: format!("$root_{suffix}"),
                sender: "@reply_sender:localhost".to_string(),
                in_reply_to_event_id: None,
                content: serde_json::json!({"body": "a reply"}),
                origin_server_ts: current_timestamp_millis(),
            })
            .await
            .expect("should create reply");

        let summary = storage.get_thread_summary(&room_id, &thread_id).await.expect("should get summary");
        assert!(summary.is_some(), "summary should exist for existing thread");
        let s = summary.unwrap();
        assert_eq!(s.room_id, room_id);
        assert_eq!(s.thread_id, thread_id);
        assert_eq!(s.root_sender, "@root_sender:localhost");
        assert_eq!(s.reply_count, 1, "should have 1 reply");
        assert!(s.latest_event_id.is_some(), "should have a latest reply event_id");

        cleanup_thread_data(&pool, &room_id, &thread_id).await;
    }

    // 29. test_get_thread_summary_not_found
    #[tokio::test]
    async fn test_get_thread_summary_not_found() {
        let pool = test_pool().await;
        let storage = ThreadStorage::new(&pool);

        let summary = storage
            .get_thread_summary("!nonexistent:localhost", "nonexistent-thread")
            .await
            .expect("query should succeed");
        assert!(summary.is_none(), "nonexistent thread should return None");
    }

    // 30. test_get_thread_statistics
    #[tokio::test]
    async fn test_get_thread_statistics() {
        let pool = test_pool().await;
        let storage = ThreadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_tstat_{suffix}:localhost");
        let thread_id = format!("thread-tstat-{suffix}");

        ensure_test_room(&pool, &room_id).await;
        cleanup_thread_data(&pool, &room_id, &thread_id).await;

        let ts = current_timestamp_millis();
        storage
            .create_thread_root(CreateThreadRootParams {
                room_id: room_id.clone(),
                root_event_id: format!("$root_{suffix}"),
                sender: "@root_sender:localhost".to_string(),
                thread_id: Some(thread_id.clone()),
            })
            .await
            .expect("should create root");

        // Add 2 replies
        for i in 0..2 {
            storage
                .create_thread_reply(CreateThreadReplyParams {
                    room_id: room_id.clone(),
                    thread_id: thread_id.clone(),
                    event_id: format!("$reply_{i}_{suffix}"),
                    root_event_id: format!("$root_{suffix}"),
                    sender: format!("@replyer{i}:localhost"),
                    in_reply_to_event_id: None,
                    content: serde_json::json!({"body": "reply"}),
                    origin_server_ts: ts + i,
                })
                .await
                .expect("should create reply");
        }

        let stats = storage.get_thread_statistics(&room_id, &thread_id).await.expect("should get statistics");
        assert!(stats.is_some(), "statistics should exist for existing thread");
        let s = stats.unwrap();
        assert_eq!(s.room_id, room_id);
        assert_eq!(s.total_replies, 2, "should have 2 replies");
        assert!(s.total_participants >= 2, "should have at least 2 participants (root + at least 1 replyer)");
        assert!(s.last_reply_ts.is_some(), "should have a last_reply_ts");

        cleanup_thread_data(&pool, &room_id, &thread_id).await;
    }

    // 31. test_get_thread_statistics_not_found
    #[tokio::test]
    async fn test_get_thread_statistics_not_found() {
        let pool = test_pool().await;
        let storage = ThreadStorage::new(&pool);

        let stats = storage
            .get_thread_statistics("!nonexistent:localhost", "nonexistent-thread")
            .await
            .expect("query should succeed");
        assert!(stats.is_none(), "nonexistent thread should return None");
    }

    // 32. test_search_threads_finds_match
    #[tokio::test]
    async fn test_search_threads_finds_match() {
        let pool = test_pool().await;
        let storage = ThreadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_st_{suffix}:localhost");
        let thread_id = format!("thread-st-{suffix}");
        let root_event_id = format!("$root_st_{suffix}");

        ensure_test_room(&pool, &room_id).await;
        cleanup_thread_data(&pool, &room_id, &thread_id).await;

        // Insert an event with searchable body content
        let ts = current_timestamp_millis();
        insert_test_event(&pool, &root_event_id, &room_id, "@sender:localhost", "UniqueSearchableKeyword content", ts)
            .await;

        storage
            .create_thread_root(CreateThreadRootParams {
                room_id: room_id.clone(),
                root_event_id: root_event_id.clone(),
                sender: "@sender:localhost".to_string(),
                thread_id: Some(thread_id.clone()),
            })
            .await
            .expect("should create root");

        // Search for the unique keyword
        let results =
            storage.search_threads(&room_id, "UniqueSearchableKeyword", Some(10)).await.expect("search should succeed");
        assert!(results.iter().any(|s| s.thread_id == thread_id), "should find our thread by keyword");

        // Cleanup the event
        let _ = sqlx::query("DELETE FROM events WHERE event_id = $1").bind(&root_event_id).execute(&*pool).await;
        cleanup_thread_data(&pool, &room_id, &thread_id).await;
    }

    // 33. test_search_threads_no_match
    #[tokio::test]
    async fn test_search_threads_no_match() {
        let pool = test_pool().await;
        let storage = ThreadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_stnm_{suffix}:localhost");
        let thread_id = format!("thread-stnm-{suffix}");
        let root_event_id = format!("$root_stnm_{suffix}");

        ensure_test_room(&pool, &room_id).await;
        cleanup_thread_data(&pool, &room_id, &thread_id).await;

        let ts = current_timestamp_millis();
        insert_test_event(&pool, &root_event_id, &room_id, "@sender:localhost", "some body text", ts).await;

        storage
            .create_thread_root(CreateThreadRootParams {
                room_id: room_id.clone(),
                root_event_id: root_event_id.clone(),
                sender: "@sender:localhost".to_string(),
                thread_id: Some(thread_id.clone()),
            })
            .await
            .expect("should create root");

        // Search for a keyword that doesn't match
        let results =
            storage.search_threads(&room_id, "ZZZNoMatchAtAllZZZ", Some(10)).await.expect("search should succeed");
        assert!(results.iter().all(|s| s.thread_id != thread_id), "should not find our thread with non-matching query");

        let _ = sqlx::query("DELETE FROM events WHERE event_id = $1").bind(&root_event_id).execute(&*pool).await;
        cleanup_thread_data(&pool, &room_id, &thread_id).await;
    }

    // 34. test_freeze_thread
    #[tokio::test]
    async fn test_freeze_thread() {
        let pool = test_pool().await;
        let storage = ThreadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_fr_{suffix}:localhost");
        let thread_id = format!("thread-fr-{suffix}");

        ensure_test_room(&pool, &room_id).await;
        cleanup_thread_data(&pool, &room_id, &thread_id).await;

        storage
            .create_thread_root(CreateThreadRootParams {
                room_id: room_id.clone(),
                root_event_id: format!("$root_{suffix}"),
                sender: "@sender:localhost".to_string(),
                thread_id: Some(thread_id.clone()),
            })
            .await
            .expect("should create root");

        // Verify not frozen initially
        let root = storage.get_thread_root(&room_id, &thread_id).await.unwrap().unwrap();
        assert!(!root.is_fetched, "thread should not be frozen initially");

        // Freeze
        storage.freeze_thread(&room_id, &thread_id).await.expect("should freeze");

        // Verify frozen (is_fetched maps to is_frozen in summaries)
        let root = storage.get_thread_root(&room_id, &thread_id).await.unwrap().unwrap();
        assert!(root.is_fetched, "thread should be frozen (is_fetched=true) after freeze_thread");

        cleanup_thread_data(&pool, &room_id, &thread_id).await;
    }

    // 35. test_unfreeze_thread
    #[tokio::test]
    async fn test_unfreeze_thread() {
        let pool = test_pool().await;
        let storage = ThreadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_uf_{suffix}:localhost");
        let thread_id = format!("thread-uf-{suffix}");

        ensure_test_room(&pool, &room_id).await;
        cleanup_thread_data(&pool, &room_id, &thread_id).await;

        storage
            .create_thread_root(CreateThreadRootParams {
                room_id: room_id.clone(),
                root_event_id: format!("$root_{suffix}"),
                sender: "@sender:localhost".to_string(),
                thread_id: Some(thread_id.clone()),
            })
            .await
            .expect("should create root");

        // Freeze first
        storage.freeze_thread(&room_id, &thread_id).await.expect("should freeze");
        let root = storage.get_thread_root(&room_id, &thread_id).await.unwrap().unwrap();
        assert!(root.is_fetched, "should be frozen after freeze");

        // Unfreeze
        storage.unfreeze_thread(&room_id, &thread_id).await.expect("should unfreeze");

        // Verify unfrozen
        let root = storage.get_thread_root(&room_id, &thread_id).await.unwrap().unwrap();
        assert!(!root.is_fetched, "thread should not be frozen after unfreeze_thread");

        cleanup_thread_data(&pool, &room_id, &thread_id).await;
    }

    // 36. test_list_thread_roots_with_from_cursor
    #[tokio::test]
    async fn test_list_thread_roots_with_from_cursor() {
        let pool = test_pool().await;
        let storage = ThreadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_ltf_{suffix}:localhost");
        // Use sortable thread IDs so the `from` cursor (thread_id > $2) works deterministically
        let t1 = format!("aaa-thread-ltf-{suffix}");
        let t2 = format!("bbb-thread-ltf-{suffix}");

        ensure_test_room(&pool, &room_id).await;
        cleanup_thread_data(&pool, &room_id, &t1).await;
        cleanup_thread_data(&pool, &room_id, &t2).await;

        for tid in [&t1, &t2] {
            storage
                .create_thread_root(CreateThreadRootParams {
                    room_id: room_id.clone(),
                    root_event_id: format!("$ev_{}", tid),
                    sender: "@sender:localhost".to_string(),
                    thread_id: Some((*tid).to_string()),
                })
                .await
                .expect("should create root");
        }

        // First page: get all roots ordered by thread_id ASC, take the first one
        let first_page = storage
            .list_thread_roots(ThreadListParams {
                room_id: room_id.clone(),
                limit: Some(1),
                from: None,
                include_all: false,
            })
            .await
            .expect("first page should succeed");
        assert_eq!(first_page.len(), 1, "first page should have 1 root");
        let first_tid = first_page[0].thread_id.clone().unwrap();

        // Second page: use the first thread_id as the `from` cursor
        let second_page = storage
            .list_thread_roots(ThreadListParams {
                room_id: room_id.clone(),
                limit: Some(10),
                from: Some(first_tid.clone()),
                include_all: false,
            })
            .await
            .expect("second page should succeed");
        assert!(
            second_page.iter().all(|r| r.thread_id.as_deref().unwrap_or("") > first_tid.as_str()),
            "all second-page roots should have thread_id > from cursor"
        );
        assert!(
            second_page.iter().any(|r| r.thread_id.as_deref() == Some(t2.as_str())),
            "second page should contain t2"
        );

        cleanup_thread_data(&pool, &room_id, &t1).await;
        cleanup_thread_data(&pool, &room_id, &t2).await;
    }

    // 37. test_list_all_thread_roots_with_from_cursor
    #[tokio::test]
    async fn test_list_all_thread_roots_with_from_cursor() {
        let pool = test_pool().await;
        let storage = ThreadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_latf_{suffix}:localhost");
        let t1 = format!("aaa-thread-latf-{suffix}");
        let t2 = format!("bbb-thread-latf-{suffix}");

        ensure_test_room(&pool, &room_id).await;
        cleanup_thread_data(&pool, &room_id, &t1).await;
        cleanup_thread_data(&pool, &room_id, &t2).await;

        for tid in [&t1, &t2] {
            storage
                .create_thread_root(CreateThreadRootParams {
                    room_id: room_id.clone(),
                    root_event_id: format!("$ev_{}", tid),
                    sender: "@sender:localhost".to_string(),
                    thread_id: Some((*tid).to_string()),
                })
                .await
                .expect("should create root");
        }

        // First page: limit=1, no cursor
        let first_page = storage.list_all_thread_roots(Some(1), None).await.expect("first page should succeed");
        assert_eq!(first_page.len(), 1, "first page should have 1 root");
        let first_tid = first_page[0].thread_id.clone().unwrap();

        // Second page: use first thread_id as from cursor
        let second_page =
            storage.list_all_thread_roots(Some(10), Some(first_tid.clone())).await.expect("second page should succeed");
        assert!(
            second_page.iter().all(|r| r.thread_id.as_deref().unwrap_or("") > first_tid.as_str()),
            "all second-page roots should have thread_id > from cursor"
        );
        assert!(
            second_page.iter().any(|r| r.thread_id.as_deref() == Some(t2.as_str())),
            "second page should contain t2"
        );

        cleanup_thread_data(&pool, &room_id, &t1).await;
        cleanup_thread_data(&pool, &room_id, &t2).await;
    }

    // 38. test_get_thread_replies_with_from_cursor
    #[tokio::test]
    async fn test_get_thread_replies_with_from_cursor() {
        let pool = test_pool().await;
        let storage = ThreadStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_gtrf_{suffix}:localhost");
        let thread_id = format!("thread-gtrf-{suffix}");

        ensure_test_room(&pool, &room_id).await;
        cleanup_thread_data(&pool, &room_id, &thread_id).await;

        storage
            .create_thread_root(CreateThreadRootParams {
                room_id: room_id.clone(),
                root_event_id: format!("$root_{suffix}"),
                sender: "@sender:localhost".to_string(),
                thread_id: Some(thread_id.clone()),
            })
            .await
            .expect("should create root");

        // Create two replies with deterministic event_ids for cursor pagination
        // The `from` cursor filters by event_id > $3, so use sortable IDs
        let event_a = format!("aaa_reply_{suffix}");
        let event_b = format!("bbb_reply_{suffix}");
        let ts = current_timestamp_millis();
        for (i, eid) in [event_a.clone(), event_b.clone()].iter().enumerate() {
            storage
                .create_thread_reply(CreateThreadReplyParams {
                    room_id: room_id.clone(),
                    thread_id: thread_id.clone(),
                    event_id: eid.clone(),
                    root_event_id: format!("$root_{suffix}"),
                    sender: "@replier:localhost".to_string(),
                    in_reply_to_event_id: None,
                    content: serde_json::json!({"body": "reply"}),
                    origin_server_ts: ts + i as i64,
                })
                .await
                .expect("should create reply");
        }

        // First page: get all replies, take the first one (ordered by origin_server_ts ASC)
        let first_page =
            storage.get_thread_replies(&room_id, &thread_id, Some(1), None).await.expect("first page should succeed");
        assert_eq!(first_page.len(), 1, "first page should have 1 reply");
        let first_eid = first_page[0].event_id.clone();

        // Second page: use first event_id as from cursor
        let second_page = storage
            .get_thread_replies(&room_id, &thread_id, Some(10), Some(first_eid.clone()))
            .await
            .expect("second page should succeed");
        assert!(
            second_page.iter().all(|r| r.event_id > first_eid),
            "all second-page replies should have event_id > from cursor"
        );
        assert_eq!(second_page.len(), 1, "second page should have the remaining 1 reply");

        cleanup_thread_data(&pool, &room_id, &thread_id).await;
    }
}

#[async_trait]
pub trait ThreadStoreApi: Send + Sync {
    async fn create_thread_root(&self, params: CreateThreadRootParams) -> Result<ThreadRoot, sqlx::Error>;
    async fn get_thread_root(&self, room_id: &str, thread_id: &str) -> Result<Option<ThreadRoot>, sqlx::Error>;
    async fn get_thread_root_by_event(
        &self,
        room_id: &str,
        root_event_id: &str,
    ) -> Result<Option<ThreadRoot>, sqlx::Error>;
    async fn list_thread_roots(&self, params: ThreadListParams) -> Result<Vec<ThreadRoot>, sqlx::Error>;
    async fn list_all_thread_roots(
        &self,
        limit: Option<i32>,
        from: Option<String>,
    ) -> Result<Vec<ThreadRoot>, sqlx::Error>;
    async fn create_thread_reply(&self, params: CreateThreadReplyParams) -> Result<ThreadReply, sqlx::Error>;
    async fn get_thread_replies(
        &self,
        room_id: &str,
        thread_id: &str,
        limit: Option<i32>,
        from: Option<String>,
    ) -> Result<Vec<ThreadReply>, sqlx::Error>;
    async fn get_reply_count(&self, room_id: &str, thread_id: &str) -> Result<i32, sqlx::Error>;
    async fn get_thread_participants(&self, room_id: &str, thread_id: &str) -> Result<Vec<String>, sqlx::Error>;
    async fn subscribe_to_thread(
        &self,
        room_id: &str,
        thread_id: &str,
        user_id: &str,
        notification_level: &str,
    ) -> Result<ThreadSubscription, sqlx::Error>;
    async fn unsubscribe_from_thread(&self, room_id: &str, thread_id: &str, user_id: &str) -> Result<(), sqlx::Error>;
    async fn mute_thread(
        &self,
        room_id: &str,
        thread_id: &str,
        user_id: &str,
    ) -> Result<ThreadSubscription, sqlx::Error>;
    async fn get_thread_subscription(
        &self,
        room_id: &str,
        thread_id: &str,
        user_id: &str,
    ) -> Result<Option<ThreadSubscription>, sqlx::Error>;
    async fn get_user_thread_subscriptions(
        &self,
        user_id: &str,
        limit: Option<i32>,
    ) -> Result<Vec<ThreadSubscription>, sqlx::Error>;
    async fn update_read_receipt(
        &self,
        room_id: &str,
        thread_id: &str,
        user_id: &str,
        event_id: &str,
        origin_server_ts: i64,
    ) -> Result<ThreadReadReceipt, sqlx::Error>;
    async fn get_read_receipt(
        &self,
        room_id: &str,
        thread_id: &str,
        user_id: &str,
    ) -> Result<Option<ThreadReadReceipt>, sqlx::Error>;
    async fn increment_unread_count(&self, room_id: &str, thread_id: &str, user_id: &str) -> Result<(), sqlx::Error>;
    async fn create_thread_relation(
        &self,
        room_id: &str,
        event_id: &str,
        relates_to_event_id: &str,
        relation_type: &str,
        thread_id: Option<&str>,
        is_falling_back: bool,
    ) -> Result<ThreadRelation, sqlx::Error>;
    async fn mark_reply_edited(&self, room_id: &str, event_id: &str) -> Result<(), sqlx::Error>;
    async fn mark_reply_redacted(&self, room_id: &str, event_id: &str) -> Result<(), sqlx::Error>;
    async fn delete_thread(&self, room_id: &str, thread_id: &str) -> Result<(), sqlx::Error>;
    async fn get_threads_with_unread(
        &self,
        user_id: &str,
        room_id: Option<&str>,
    ) -> Result<Vec<ThreadReadReceipt>, sqlx::Error>;
    async fn get_thread_summary(&self, room_id: &str, thread_id: &str) -> Result<Option<ThreadSummary>, sqlx::Error>;
    async fn get_thread_statistics(
        &self,
        room_id: &str,
        thread_id: &str,
    ) -> Result<Option<ThreadStatistics>, sqlx::Error>;
    async fn search_threads(
        &self,
        room_id: &str,
        query: &str,
        limit: Option<i32>,
    ) -> Result<Vec<ThreadSummary>, sqlx::Error>;
    async fn freeze_thread(&self, room_id: &str, thread_id: &str) -> Result<(), sqlx::Error>;
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
    ) -> Result<Vec<ThreadSubscription>, sqlx::Error> {
        self.get_user_thread_subscriptions(user_id, limit).await
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
