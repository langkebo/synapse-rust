use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres, Row};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchIndexEntry {
    pub id: i64,
    pub event_id: String,
    pub room_id: String,
    pub user_id: String,
    pub event_type: String,
    #[serde(rename = "type")]
    pub content_type: String,
    pub content: String,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub event_id: String,
    pub room_id: String,
    pub sender: String,
    pub event_type: String,
    pub content: String,
    pub origin_server_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    pub search_term: String,
    pub room_ids: Option<Vec<String>>,
    pub not_room_ids: Option<Vec<String>>,
    pub sender: Option<String>,
    pub event_types: Option<Vec<String>>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// 搜索索引存储模块
pub struct SearchIndexStorage {
    pool: Pool<Postgres>,
}

impl SearchIndexStorage {
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }

    /// 索引单个事件
    pub async fn index_event(&self, entry: &SearchIndexEntry) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO search_index (event_id, room_id, user_id, event_type, type, content, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (event_id) DO UPDATE SET
                content = EXCLUDED.content,
                updated_ts = EXCLUDED.updated_ts
            "#,
        )
        .bind(&entry.event_id)
        .bind(&entry.room_id)
        .bind(&entry.user_id)
        .bind(&entry.event_type)
        .bind(&entry.content_type)
        .bind(&entry.content)
        .bind(entry.created_ts)
        .bind(entry.updated_ts)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// 批量索引事件
    pub async fn index_events(&self, entries: &[SearchIndexEntry]) -> Result<usize, sqlx::Error> {
        let mut count = 0;
        for entry in entries {
            if self.index_event(entry).await.is_ok() {
                count += 1;
            }
        }
        Ok(count)
    }

    /// 搜索事件
    pub async fn search_events(
        &self,
        query: &SearchQuery,
    ) -> Result<Vec<SearchResult>, sqlx::Error> {
        let search_pattern = format!("%{}%", query.search_term.to_lowercase());
        let limit = query.limit.unwrap_or(20).min(100);
        let offset = query.offset.unwrap_or(0).max(0);

        let rows = sqlx::query(
            "SELECT event_id, room_id, user_id, event_type, content, created_ts 
             FROM search_index 
             WHERE LOWER(content) LIKE $1
             ORDER BY created_ts DESC 
             LIMIT $2 OFFSET $3",
        )
        .bind(&search_pattern)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        let results: Vec<SearchResult> = rows
            .iter()
            .map(|row| SearchResult {
                event_id: row.get("event_id"),
                room_id: row.get("room_id"),
                sender: row.get("user_id"),
                event_type: row.get("event_type"),
                content: row.get("content"),
                origin_server_ts: row.get("created_ts"),
            })
            .collect();

        Ok(results)
    }

    /// 删除事件索引
    pub async fn delete_event(&self, event_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM search_index WHERE event_id = $1")
            .bind(event_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// 删除房间的所有索引
    pub async fn delete_room_index(&self, room_id: &str) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM search_index WHERE room_id = $1")
            .bind(room_id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected())
    }

    /// 重建房间索引（从 events 表重新导入）
    pub async fn rebuild_room_index(&self, room_id: &str) -> Result<usize, sqlx::Error> {
        // 先删除现有索引
        self.delete_room_index(room_id).await?;

        // 从 events 表重新导入
        let rows = sqlx::query(
            r#"
            SELECT event_id, room_id, sender as user_id, event_type, 
                   event_type as type, content::text as content, origin_server_ts as created_ts
            FROM events 
            WHERE room_id = $1 AND event_type IN ('m.room.message', 'm.room.name', 'm.room.topic')
            "#,
        )
        .bind(room_id)
        .fetch_all(&self.pool)
        .await?;

        let mut count = 0;
        for row in rows {
            let entry = SearchIndexEntry {
                id: 0,
                event_id: row.get("event_id"),
                room_id: row.get("room_id"),
                user_id: row.get("user_id"),
                event_type: row.get("event_type"),
                content_type: row.get("type"),
                content: row.get("content"),
                created_ts: row.get("created_ts"),
                updated_ts: None,
            };
            if self.index_event(&entry).await.is_ok() {
                count += 1;
            }
        }

        Ok(count)
    }

    /// 获取索引统计
    pub async fn get_stats(&self) -> Result<SearchIndexStats, sqlx::Error> {
        let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM search_index")
            .fetch_one(&self.pool)
            .await?;

        let by_type: Vec<(String, i64)> =
            sqlx::query_as("SELECT event_type, COUNT(*) FROM search_index GROUP BY event_type")
                .fetch_all(&self.pool)
                .await?;

        Ok(SearchIndexStats {
            total_count: total.0,
            by_event_type: by_type.into_iter().collect(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchIndexStats {
    pub total_count: i64,
    pub by_event_type: std::collections::HashMap<String, i64>,
}
