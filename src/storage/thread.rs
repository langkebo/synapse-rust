use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ThreadRoot {
    pub id: i64,
    pub room_id: String,
    pub root_event_id: String,
    pub sender: String,
    pub thread_id: String,
    pub content: serde_json::Value,
    pub origin_server_ts: i64,
    pub last_reply_event_id: Option<String>,
    pub last_reply_sender: Option<String>,
    pub last_reply_ts: Option<i64>,
    pub reply_count: i32,
    pub is_frozen: bool,
    pub created_ts: i64,
    pub updated_ts: i64,
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
    pub thread_id: String,
    pub content: serde_json::Value,
    pub origin_server_ts: i64,
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

    pub async fn create_thread_root(
        &self,
        params: CreateThreadRootParams,
    ) -> Result<ThreadRoot, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        
        sqlx::query_as::<_, ThreadRoot>(
            r#"
            INSERT INTO thread_roots (
                room_id, root_event_id, sender, thread_id, content, 
                origin_server_ts, created_ts, updated_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $7)
            RETURNING *
            "#,
        )
        .bind(&params.room_id)
        .bind(&params.root_event_id)
        .bind(&params.sender)
        .bind(&params.thread_id)
        .bind(&params.content)
        .bind(params.origin_server_ts)
        .bind(now)
        .fetch_one(&*self.pool)
        .await
    }

    pub async fn get_thread_root(
        &self,
        room_id: &str,
        thread_id: &str,
    ) -> Result<Option<ThreadRoot>, sqlx::Error> {
        sqlx::query_as::<_, ThreadRoot>(
            r#"
            SELECT * FROM thread_roots
            WHERE room_id = $1 AND thread_id = $2
            "#,
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
            r#"
            SELECT * FROM thread_roots
            WHERE room_id = $1 AND root_event_id = $2
            "#,
        )
        .bind(room_id)
        .bind(root_event_id)
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn list_thread_roots(
        &self,
        params: ThreadListParams,
    ) -> Result<Vec<ThreadRoot>, sqlx::Error> {
        let limit = params.limit.unwrap_or(50);
        
        if let Some(from) = params.from {
            sqlx::query_as::<_, ThreadRoot>(
                r#"
                SELECT * FROM thread_roots
                WHERE room_id = $1 AND thread_id > $2
                ORDER BY thread_id ASC
                LIMIT $3
                "#,
            )
            .bind(&params.room_id)
            .bind(from)
            .bind(limit)
            .fetch_all(&*self.pool)
            .await
        } else {
            sqlx::query_as::<_, ThreadRoot>(
                r#"
                SELECT * FROM thread_roots
                WHERE room_id = $1
                ORDER BY last_reply_ts DESC NULLS LAST, origin_server_ts DESC
                LIMIT $2
                "#,
            )
            .bind(&params.room_id)
            .bind(limit)
            .fetch_all(&*self.pool)
            .await
        }
    }

    pub async fn create_thread_reply(
        &self,
        params: CreateThreadReplyParams,
    ) -> Result<ThreadReply, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        
        sqlx::query_as::<_, ThreadReply>(
            r#"
            INSERT INTO thread_replies (
                room_id, thread_id, event_id, root_event_id, sender,
                in_reply_to_event_id, content, origin_server_ts, created_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            RETURNING *
            "#,
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
        .fetch_one(&*self.pool)
        .await
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
                r#"
                SELECT * FROM thread_replies
                WHERE room_id = $1 AND thread_id = $2 AND event_id > $3
                ORDER BY origin_server_ts ASC
                LIMIT $4
                "#,
            )
            .bind(room_id)
            .bind(thread_id)
            .bind(from)
            .bind(limit)
            .fetch_all(&*self.pool)
            .await
        } else {
            sqlx::query_as::<_, ThreadReply>(
                r#"
                SELECT * FROM thread_replies
                WHERE room_id = $1 AND thread_id = $2
                ORDER BY origin_server_ts ASC
                LIMIT $3
                "#,
            )
            .bind(room_id)
            .bind(thread_id)
            .bind(limit)
            .fetch_all(&*self.pool)
            .await
        }
    }

    pub async fn get_reply_count(
        &self,
        room_id: &str,
        thread_id: &str,
    ) -> Result<i32, sqlx::Error> {
        let result: Option<(i32,)> = sqlx::query_as(
            r#"
            SELECT COUNT(*) FROM thread_replies
            WHERE room_id = $1 AND thread_id = $2
            "#,
        )
        .bind(room_id)
        .bind(thread_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(result.map(|r| r.0).unwrap_or(0))
    }

    pub async fn get_thread_participants(
        &self,
        room_id: &str,
        thread_id: &str,
    ) -> Result<Vec<String>, sqlx::Error> {
        let result: Vec<(String,)> = sqlx::query_as(
            r#"
            SELECT DISTINCT sender FROM (
                SELECT sender FROM thread_roots WHERE room_id = $1 AND thread_id = $2
                UNION
                SELECT sender FROM thread_replies WHERE room_id = $1 AND thread_id = $2
            ) AS participants
            "#,
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
        let now = chrono::Utc::now().timestamp_millis();
        
        sqlx::query_as::<_, ThreadSubscription>(
            r#"
            INSERT INTO thread_subscriptions (
                room_id, thread_id, user_id, notification_level, subscribed_ts, updated_ts
            )
            VALUES ($1, $2, $3, $4, $5, $5)
            ON CONFLICT (room_id, thread_id, user_id) DO UPDATE SET
                notification_level = EXCLUDED.notification_level,
                is_muted = FALSE,
                updated_ts = EXCLUDED.updated_ts
            RETURNING *
            "#,
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
            r#"
            DELETE FROM thread_subscriptions
            WHERE room_id = $1 AND thread_id = $2 AND user_id = $3
            "#,
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
        let now = chrono::Utc::now().timestamp_millis();
        
        sqlx::query_as::<_, ThreadSubscription>(
            r#"
            INSERT INTO thread_subscriptions (
                room_id, thread_id, user_id, notification_level, is_muted, subscribed_ts, updated_ts
            )
            VALUES ($1, $2, $3, 'none', TRUE, $4, $4)
            ON CONFLICT (room_id, thread_id, user_id) DO UPDATE SET
                is_muted = TRUE,
                updated_ts = EXCLUDED.updated_ts
            RETURNING *
            "#,
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
            r#"
            SELECT * FROM thread_subscriptions
            WHERE room_id = $1 AND thread_id = $2 AND user_id = $3
            "#,
        )
        .bind(room_id)
        .bind(thread_id)
        .bind(user_id)
        .fetch_optional(&*self.pool)
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
        let now = chrono::Utc::now().timestamp_millis();
        
        sqlx::query_as::<_, ThreadReadReceipt>(
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
            RETURNING *
            "#,
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
            r#"
            SELECT * FROM thread_read_receipts
            WHERE room_id = $1 AND thread_id = $2 AND user_id = $3
            "#,
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
        sqlx::query(
            r#"
            INSERT INTO thread_read_receipts (
                room_id, thread_id, user_id, last_read_ts, unread_count, updated_ts
            )
            VALUES ($1, $2, $3, 0, 1, EXTRACT(EPOCH FROM NOW()) * 1000)
            ON CONFLICT (room_id, thread_id, user_id) DO UPDATE SET
                unread_count = thread_read_receipts.unread_count + 1,
                updated_ts = EXCLUDED.updated_ts
            "#,
        )
        .bind(room_id)
        .bind(thread_id)
        .bind(user_id)
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
        let now = chrono::Utc::now().timestamp_millis();
        
        sqlx::query_as::<_, ThreadRelation>(
            r#"
            INSERT INTO thread_relations (
                room_id, event_id, relates_to_event_id, relation_type, 
                thread_id, is_falling_back, created_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING *
            "#,
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

    pub async fn get_thread_summary(
        &self,
        room_id: &str,
        thread_id: &str,
    ) -> Result<Option<ThreadSummary>, sqlx::Error> {
        sqlx::query_as::<_, ThreadSummary>(
            r#"
            SELECT * FROM thread_summaries
            WHERE room_id = $1 AND thread_id = $2
            "#,
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
            r#"
            SELECT * FROM thread_statistics
            WHERE room_id = $1 AND thread_id = $2
            "#,
        )
        .bind(room_id)
        .bind(thread_id)
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn mark_reply_edited(
        &self,
        room_id: &str,
        event_id: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE thread_replies
            SET is_edited = TRUE
            WHERE room_id = $1 AND event_id = $2
            "#,
        )
        .bind(room_id)
        .bind(event_id)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn mark_reply_redacted(
        &self,
        room_id: &str,
        event_id: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE thread_replies
            SET is_redacted = TRUE, content = '{}'
            WHERE room_id = $1 AND event_id = $2
            "#,
        )
        .bind(room_id)
        .bind(event_id)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn freeze_thread(
        &self,
        room_id: &str,
        thread_id: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE thread_roots
            SET is_frozen = TRUE, updated_ts = EXTRACT(EPOCH FROM NOW()) * 1000
            WHERE room_id = $1 AND thread_id = $2
            "#,
        )
        .bind(room_id)
        .bind(thread_id)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn unfreeze_thread(
        &self,
        room_id: &str,
        thread_id: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE thread_roots
            SET is_frozen = FALSE, updated_ts = EXTRACT(EPOCH FROM NOW()) * 1000
            WHERE room_id = $1 AND thread_id = $2
            "#,
        )
        .bind(room_id)
        .bind(thread_id)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn delete_thread(
        &self,
        room_id: &str,
        thread_id: &str,
    ) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            r#"DELETE FROM thread_replies WHERE room_id = $1 AND thread_id = $2"#,
        )
        .bind(room_id)
        .bind(thread_id)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"DELETE FROM thread_roots WHERE room_id = $1 AND thread_id = $2"#,
        )
        .bind(room_id)
        .bind(thread_id)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"DELETE FROM thread_subscriptions WHERE room_id = $1 AND thread_id = $2"#,
        )
        .bind(room_id)
        .bind(thread_id)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"DELETE FROM thread_read_receipts WHERE room_id = $1 AND thread_id = $2"#,
        )
        .bind(room_id)
        .bind(thread_id)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"DELETE FROM thread_summaries WHERE room_id = $1 AND thread_id = $2"#,
        )
        .bind(room_id)
        .bind(thread_id)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"DELETE FROM thread_statistics WHERE room_id = $1 AND thread_id = $2"#,
        )
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
                r#"
                SELECT * FROM thread_read_receipts
                WHERE user_id = $1 AND room_id = $2 AND unread_count > 0
                ORDER BY updated_ts DESC
                "#,
            )
            .bind(user_id)
            .bind(room_id)
            .fetch_all(&*self.pool)
            .await
        } else {
            sqlx::query_as::<_, ThreadReadReceipt>(
                r#"
                SELECT * FROM thread_read_receipts
                WHERE user_id = $1 AND unread_count > 0
                ORDER BY updated_ts DESC
                "#,
            )
            .bind(user_id)
            .fetch_all(&*self.pool)
            .await
        }
    }

    pub async fn search_threads(
        &self,
        room_id: &str,
        query: &str,
        limit: Option<i32>,
    ) -> Result<Vec<ThreadSummary>, sqlx::Error> {
        let limit = limit.unwrap_or(20);
        let search_pattern = format!("%{}%", query);
        
        sqlx::query_as::<_, ThreadSummary>(
            r#"
            SELECT * FROM thread_summaries
            WHERE room_id = $1 
            AND (
                root_content::text ILIKE $2 
                OR latest_content::text ILIKE $2
            )
            ORDER BY latest_origin_server_ts DESC NULLS LAST
            LIMIT $3
            "#,
        )
        .bind(room_id)
        .bind(&search_pattern)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await
    }
}
