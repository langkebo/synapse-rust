use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchIndexEntry {
    pub id: i32,
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
    pub from: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchIndexCursor {
    pub created_ts: i64,
    pub id: i32,
}

fn encode_search_index_cursor(cursor: &SearchIndexCursor) -> String {
    let raw = format!("{}|{}", cursor.created_ts, cursor.id);
    URL_SAFE_NO_PAD.encode(raw.as_bytes())
}

fn decode_search_index_cursor(cursor: Option<&str>) -> Option<SearchIndexCursor> {
    let cursor = cursor?;
    let decoded = URL_SAFE_NO_PAD.decode(cursor).ok()?;
    let decoded = String::from_utf8(decoded).ok()?;
    let (created_ts, id) = decoded.split_once('|')?;
    Some(SearchIndexCursor { created_ts: created_ts.parse().ok()?, id: id.parse().ok()? })
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
        sqlx::query!(
            r#"
            INSERT INTO search_index (event_id, room_id, user_id, event_type, type, content, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (event_id) DO UPDATE SET
                content = EXCLUDED.content,
                updated_ts = EXCLUDED.updated_ts
            "#,
            &entry.event_id,
            &entry.room_id,
            &entry.user_id,
            &entry.event_type,
            &entry.content_type,
            &entry.content,
            entry.created_ts,
            entry.updated_ts
        )
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

    /// 搜索事件（ILIKE + trigram 混合搜索，对齐 thread.rs 最佳实践）
    pub async fn search_events(&self, query: &SearchQuery) -> Result<(Vec<SearchResult>, Option<String>), sqlx::Error> {
        let search_pattern = format!("%{}%", query.search_term.to_lowercase());
        let limit = query.limit.unwrap_or(20).min(100);
        let cursor = decode_search_index_cursor(query.from.as_deref());

        let (results, next_batch) = if let Some(ref cursor) = cursor {
            let rows = sqlx::query!(
                r#"SELECT id as "id!", event_id as "event_id!", room_id as "room_id!", user_id as "user_id!", event_type as "event_type!", content as "content!", created_ts as "created_ts!"
                 FROM search_index
                 WHERE (content ILIKE $1 OR content % $2)
                   AND (created_ts < $3 OR (created_ts = $3 AND id < $4))
                 ORDER BY created_ts DESC, id DESC
                 LIMIT $5"#,
                &search_pattern,
                &query.search_term,
                cursor.created_ts,
                cursor.id,
                limit + 1
            )
            .fetch_all(&self.pool)
            .await?;

            let has_more = rows.len() as i64 > limit;
            let visible_rows = if has_more { &rows[..limit as usize] } else { &rows[..] };
            let next_batch = if has_more {
                visible_rows.last().map(|row| {
                    encode_search_index_cursor(&SearchIndexCursor { created_ts: row.created_ts, id: row.id })
                })
            } else {
                None
            };
            let results: Vec<SearchResult> = visible_rows
                .iter()
                .map(|row| SearchResult {
                    event_id: row.event_id.clone(),
                    room_id: row.room_id.clone(),
                    sender: row.user_id.clone(),
                    event_type: row.event_type.clone(),
                    content: row.content.clone(),
                    origin_server_ts: row.created_ts,
                })
                .collect();
            (results, next_batch)
        } else {
            let rows = sqlx::query!(
                r#"SELECT id as "id!", event_id as "event_id!", room_id as "room_id!", user_id as "user_id!", event_type as "event_type!", content as "content!", created_ts as "created_ts!"
                 FROM search_index
                 WHERE (content ILIKE $1 OR content % $2)
                 ORDER BY created_ts DESC, id DESC
                 LIMIT $3"#,
                &search_pattern,
                &query.search_term,
                limit + 1
            )
            .fetch_all(&self.pool)
            .await?;

            let has_more = rows.len() as i64 > limit;
            let visible_rows = if has_more { &rows[..limit as usize] } else { &rows[..] };
            let next_batch = if has_more {
                visible_rows.last().map(|row| {
                    encode_search_index_cursor(&SearchIndexCursor { created_ts: row.created_ts, id: row.id })
                })
            } else {
                None
            };
            let results: Vec<SearchResult> = visible_rows
                .iter()
                .map(|row| SearchResult {
                    event_id: row.event_id.clone(),
                    room_id: row.room_id.clone(),
                    sender: row.user_id.clone(),
                    event_type: row.event_type.clone(),
                    content: row.content.clone(),
                    origin_server_ts: row.created_ts,
                })
                .collect();
            (results, next_batch)
        };

        Ok((results, next_batch))
    }

    /// 删除事件索引
    pub async fn delete_event(&self, event_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query!("DELETE FROM search_index WHERE event_id = $1", event_id).execute(&self.pool).await?;
        Ok(())
    }

    /// 删除房间的所有索引
    pub async fn delete_room_index(&self, room_id: &str) -> Result<u64, sqlx::Error> {
        let result = sqlx::query!("DELETE FROM search_index WHERE room_id = $1", room_id).execute(&self.pool).await?;
        Ok(result.rows_affected())
    }

    /// 重建房间索引（从 events 表重新导入）
    pub async fn rebuild_room_index(&self, room_id: &str) -> Result<usize, sqlx::Error> {
        // 先删除现有索引
        self.delete_room_index(room_id).await?;

        // 从 events 表重新导入
        let rows = sqlx::query!(
            r#"
            SELECT event_id as "event_id!", room_id as "room_id!", sender as "user_id!", event_type as "event_type!",
                   event_type as "type!", content::text as "content!", origin_server_ts as "created_ts!"
            FROM events
            WHERE room_id = $1 AND event_type IN ('m.room.message', 'm.room.name', 'm.room.topic')
            "#,
            room_id
        )
        .fetch_all(&self.pool)
        .await?;

        let mut count = 0;
        for row in rows {
            let entry = SearchIndexEntry {
                id: 0,
                event_id: row.event_id,
                room_id: row.room_id,
                user_id: row.user_id,
                event_type: row.event_type,
                content_type: row.r#type,
                content: row.content,
                created_ts: row.created_ts,
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
        let total =
            sqlx::query_scalar!(r#"SELECT COUNT(*) as "count!" FROM search_index"#).fetch_one(&self.pool).await?;

        let by_type_rows = sqlx::query!(
            r#"SELECT event_type as "event_type!", COUNT(*) as "count!" FROM search_index GROUP BY event_type"#
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(SearchIndexStats {
            total_count: total,
            by_event_type: by_type_rows.into_iter().map(|r| (r.event_type, r.count)).collect(),
        })
    }
}

#[cfg(test)]
mod cursor_tests {
    use super::{decode_search_index_cursor, encode_search_index_cursor, SearchIndexCursor};

    #[test]
    fn search_index_cursor_round_trip() {
        let cursor = SearchIndexCursor { created_ts: 1_700_000_000_000, id: 42 };
        let encoded = encode_search_index_cursor(&cursor);
        assert_eq!(decode_search_index_cursor(Some(&encoded)), Some(cursor));
    }

    #[test]
    fn search_index_cursor_rejects_invalid_value() {
        assert_eq!(decode_search_index_cursor(Some("bad")), None);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchIndexStats {
    pub total_count: i64,
    pub by_event_type: std::collections::HashMap<String, i64>,
}
