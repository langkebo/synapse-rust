use async_trait::async_trait;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres, Row};

use crate::trigram_ranking::TrigramRanking;

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

/// Trait abstraction over [`SearchIndexStorage`] for testability.
#[async_trait]
pub trait SearchIndexStoreApi: Send + Sync {
    async fn index_event(&self, entry: &SearchIndexEntry) -> Result<(), sqlx::Error>;
    async fn index_events(&self, entries: &[SearchIndexEntry]) -> Result<usize, sqlx::Error>;
    async fn search_events(&self, query: &SearchQuery) -> Result<(Vec<SearchResult>, Option<String>), sqlx::Error>;
    async fn delete_event(&self, event_id: &str) -> Result<(), sqlx::Error>;
    async fn delete_room_index(&self, room_id: &str) -> Result<u64, sqlx::Error>;
    async fn rebuild_room_index(&self, room_id: &str) -> Result<usize, sqlx::Error>;
    async fn get_stats(&self) -> Result<SearchIndexStats, sqlx::Error>;
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
            r"
            INSERT INTO search_index (event_id, room_id, user_id, event_type, type, content, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (event_id) DO UPDATE SET
                content = EXCLUDED.content,
                updated_ts = EXCLUDED.updated_ts
            ",
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

    /// 搜索事件（ILIKE + trigram 混合搜索，使用 TrigramRanking 辅助构建查询）
    pub async fn search_events(&self, query: &SearchQuery) -> Result<(Vec<SearchResult>, Option<String>), sqlx::Error> {
        let escaped = query.search_term.to_lowercase().replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
        let exact_pattern = escaped.clone();
        let prefix_pattern = format!("{escaped}%");
        let contains_pattern = format!("%{escaped}%");
        let limit = query.limit.unwrap_or(20).min(100);
        let cursor = decode_search_index_cursor(query.from.as_deref());

        let content_rank = TrigramRanking::new("content", "search_index");
        let where_clause = content_rank.where_clause();

        let rows = if let Some(cursor) = cursor {
            let sql = format!(
                "SELECT id, event_id, room_id, user_id, event_type, content, created_ts
                 FROM search_index
                 WHERE ({where_clause})
                   AND (created_ts < $5 OR (created_ts = $5 AND id < $6))
                 ORDER BY created_ts DESC, id DESC
                 LIMIT $7"
            );
            sqlx::query(&sql)
                .bind(&exact_pattern)
                .bind(&prefix_pattern)
                .bind(&contains_pattern)
                .bind(&query.search_term)
                .bind(cursor.created_ts)
                .bind(cursor.id)
                .bind(limit + 1)
                .fetch_all(&self.pool)
                .await?
        } else {
            let sql = format!(
                "SELECT id, event_id, room_id, user_id, event_type, content, created_ts
                 FROM search_index
                 WHERE ({where_clause})
                 ORDER BY created_ts DESC, id DESC
                 LIMIT $5"
            );
            sqlx::query(&sql)
                .bind(&exact_pattern)
                .bind(&prefix_pattern)
                .bind(&contains_pattern)
                .bind(&query.search_term)
                .bind(limit + 1)
                .fetch_all(&self.pool)
                .await?
        };

        let has_more = rows.len() as i64 > limit;
        let visible_rows = if has_more { &rows[..limit as usize] } else { &rows[..] };
        let next_batch = if has_more {
            visible_rows.last().map(|row| {
                encode_search_index_cursor(&SearchIndexCursor { created_ts: row.get("created_ts"), id: row.get("id") })
            })
        } else {
            None
        };

        let results: Vec<SearchResult> = visible_rows
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

        Ok((results, next_batch))
    }

    /// 删除事件索引
    pub async fn delete_event(&self, event_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM search_index WHERE event_id = $1").bind(event_id).execute(&self.pool).await?;
        Ok(())
    }

    /// 删除房间的所有索引
    pub async fn delete_room_index(&self, room_id: &str) -> Result<u64, sqlx::Error> {
        let result =
            sqlx::query("DELETE FROM search_index WHERE room_id = $1").bind(room_id).execute(&self.pool).await?;
        Ok(result.rows_affected())
    }

    /// 重建房间索引（从 events 表重新导入）
    pub async fn rebuild_room_index(&self, room_id: &str) -> Result<usize, sqlx::Error> {
        // 先删除现有索引
        self.delete_room_index(room_id).await?;

        // 从 events 表重新导入
        let rows = sqlx::query(
            r"
            SELECT event_id, room_id, sender as user_id, event_type,
                   event_type as type, content::text as content, origin_server_ts as created_ts
            FROM events
            WHERE room_id = $1 AND event_type IN ('m.room.message', 'm.room.name', 'm.room.topic')
            ",
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
        let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM search_index").fetch_one(&self.pool).await?;

        let by_type: Vec<(String, i64)> =
            sqlx::query_as("SELECT event_type, COUNT(*) FROM search_index GROUP BY event_type")
                .fetch_all(&self.pool)
                .await?;

        Ok(SearchIndexStats { total_count: total.0, by_event_type: by_type.into_iter().collect() })
    }
}

#[async_trait]
impl SearchIndexStoreApi for SearchIndexStorage {
    async fn index_event(&self, entry: &SearchIndexEntry) -> Result<(), sqlx::Error> {
        self.index_event(entry).await
    }
    async fn index_events(&self, entries: &[SearchIndexEntry]) -> Result<usize, sqlx::Error> {
        self.index_events(entries).await
    }
    async fn search_events(&self, query: &SearchQuery) -> Result<(Vec<SearchResult>, Option<String>), sqlx::Error> {
        self.search_events(query).await
    }
    async fn delete_event(&self, event_id: &str) -> Result<(), sqlx::Error> {
        self.delete_event(event_id).await
    }
    async fn delete_room_index(&self, room_id: &str) -> Result<u64, sqlx::Error> {
        self.delete_room_index(room_id).await
    }
    async fn rebuild_room_index(&self, room_id: &str) -> Result<usize, sqlx::Error> {
        self.rebuild_room_index(room_id).await
    }
    async fn get_stats(&self) -> Result<SearchIndexStats, sqlx::Error> {
        self.get_stats().await
    }
}

#[cfg(test)]
mod cursor_tests {
    use super::{
        decode_search_index_cursor, encode_search_index_cursor, SearchIndexCursor, SearchIndexEntry, SearchIndexStats,
        SearchQuery, SearchResult,
    };

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

    #[test]
    fn search_index_cursor_rejects_none() {
        assert_eq!(decode_search_index_cursor(None), None);
    }

    #[test]
    fn search_index_cursor_rejects_empty() {
        assert_eq!(decode_search_index_cursor(Some("")), None);
    }

    #[test]
    fn search_index_cursor_edge_values() {
        let cursor = SearchIndexCursor { created_ts: 0, id: 0 };
        let encoded = encode_search_index_cursor(&cursor);
        let decoded = decode_search_index_cursor(Some(&encoded));
        assert_eq!(decoded, Some(SearchIndexCursor { created_ts: 0, id: 0 }));
    }

    #[test]
    fn search_index_cursor_max_values() {
        let cursor = SearchIndexCursor { created_ts: i64::MAX, id: i32::MAX };
        let encoded = encode_search_index_cursor(&cursor);
        let decoded = decode_search_index_cursor(Some(&encoded));
        assert_eq!(decoded, Some(cursor));
    }

    #[test]
    fn search_index_cursor_rejects_corrupted_values() {
        assert_eq!(decode_search_index_cursor(Some("not_a_number|123")), None);
        assert_eq!(decode_search_index_cursor(Some("123|not_a_number")), None);
        assert_eq!(decode_search_index_cursor(Some("123")), None);
    }

    #[test]
    fn test_search_index_entry_fields() {
        let entry = SearchIndexEntry {
            id: 1,
            event_id: "$event1:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            user_id: "@alice:example.com".to_string(),
            event_type: "m.room.message".to_string(),
            content_type: "text/plain".to_string(),
            content: "Hello world".to_string(),
            created_ts: 1700000000000,
            updated_ts: None,
        };
        assert_eq!(entry.event_id, "$event1:example.com");
        assert_eq!(entry.content, "Hello world");
        assert_eq!(entry.content_type, "text/plain");
    }

    #[test]
    fn test_search_result_fields() {
        let result = SearchResult {
            event_id: "$event1:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            sender: "@alice:example.com".to_string(),
            event_type: "m.room.message".to_string(),
            content: "search result".to_string(),
            origin_server_ts: 1700000000000,
        };
        assert_eq!(result.event_id, "$event1:example.com");
        assert_eq!(result.sender, "@alice:example.com");
    }

    #[test]
    fn test_search_query_fields() {
        let query = SearchQuery {
            search_term: "hello".to_string(),
            room_ids: Some(vec!["!room1:example.com".to_string()]),
            not_room_ids: None,
            sender: None,
            event_types: None,
            limit: Some(50),
            from: None,
        };
        assert_eq!(query.search_term, "hello");
        assert_eq!(query.limit, Some(50));
        assert_eq!(query.room_ids.unwrap().len(), 1);
    }

    #[test]
    fn test_search_query_minimal() {
        let query = SearchQuery {
            search_term: "test".to_string(),
            room_ids: None,
            not_room_ids: None,
            sender: None,
            event_types: None,
            limit: None,
            from: None,
        };
        assert_eq!(query.search_term, "test");
        assert!(query.room_ids.is_none());
    }

    #[test]
    fn test_search_index_stats_fields() {
        let stats = SearchIndexStats {
            total_count: 1000,
            by_event_type: {
                let mut m = std::collections::HashMap::new();
                m.insert("m.room.message".to_string(), 800);
                m
            },
        };
        assert_eq!(stats.total_count, 1000);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchIndexStats {
    pub total_count: i64,
    pub by_event_type: std::collections::HashMap<String, i64>,
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;
    use sqlx::PgPool;
    use std::sync::Arc;

    async fn test_pool() -> Arc<PgPool> {
        let db_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
        let pool =
            PgPoolOptions::new().max_connections(2).connect(&db_url).await.expect("Failed to connect to test database");
        Arc::new(pool)
    }

    async fn cleanup_search_index(pool: &PgPool, suffix: &str) {
        let pattern = format!("%{suffix}");
        let _ = sqlx::query("DELETE FROM search_index WHERE event_id LIKE $1").bind(&pattern).execute(pool).await;
    }

    fn make_suffix() -> String {
        uuid::Uuid::new_v4().to_string().replace('-', "")
    }

    fn make_entry(suffix: &str, idx: i64, content: &str) -> SearchIndexEntry {
        SearchIndexEntry {
            id: 0,
            event_id: format!("$event_{suffix}_{idx}:test.com"),
            room_id: format!("!room_{suffix}:test.com"),
            user_id: format!("@user_{suffix}:test.com"),
            event_type: "m.room.message".to_string(),
            content_type: "m.text".to_string(),
            content: format!("{content} {suffix}"),
            created_ts: 1_700_000_000_000 + idx,
            updated_ts: None,
        }
    }

    // ── index_event ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_index_event_insert() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        cleanup_search_index(&pool, &suffix).await;

        let storage = SearchIndexStorage::new((*pool).clone());
        let entry = make_entry(&suffix, 1, "Hello world, this is a test message");

        storage.index_event(&entry).await.expect("index_event should succeed");

        // Verify via search using unique suffix
        let query = SearchQuery {
            search_term: suffix.clone(),
            room_ids: None,
            not_room_ids: None,
            sender: None,
            event_types: None,
            limit: Some(10),
            from: None,
        };
        let (results, _) = storage.search_events(&query).await.expect("search should succeed");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].event_id, entry.event_id);
        assert_eq!(results[0].sender, entry.user_id);
        assert_eq!(results[0].content, entry.content);

        cleanup_search_index(&pool, &suffix).await;
    }

    // ── index_event upsert ───────────────────────────────────────

    #[tokio::test]
    async fn test_index_event_upsert_updates_content() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        cleanup_search_index(&pool, &suffix).await;

        let storage = SearchIndexStorage::new((*pool).clone());
        let event_id = format!("$upsert_{suffix}:test.com");

        // First insert with unique content
        let entry1 = SearchIndexEntry {
            id: 0,
            event_id: event_id.clone(),
            room_id: format!("!room_{suffix}:test.com"),
            user_id: format!("@alice_{suffix}:test.com"),
            event_type: "m.room.message".to_string(),
            content_type: "m.text".to_string(),
            content: format!("Original content {suffix}"),
            created_ts: 1_700_000_000_001,
            updated_ts: None,
        };
        storage.index_event(&entry1).await.expect("first insert");

        // Second insert with same event_id (upsert)
        let entry2 = SearchIndexEntry {
            id: 0,
            event_id: event_id.clone(),
            room_id: format!("!room_{suffix}:test.com"),
            user_id: format!("@alice_{suffix}:test.com"),
            event_type: "m.room.message".to_string(),
            content_type: "m.text".to_string(),
            content: format!("Updated content {suffix}"),
            created_ts: 1_700_000_000_002,
            updated_ts: Some(1_700_000_000_100),
        };
        storage.index_event(&entry2).await.expect("upsert insert");

        // Search for the unique suffix to find only our event
        let query = SearchQuery {
            search_term: suffix.clone(),
            room_ids: None,
            not_room_ids: None,
            sender: None,
            event_types: None,
            limit: Some(10),
            from: None,
        };
        let (results, _) = storage.search_events(&query).await.expect("search should succeed");
        assert_eq!(results.len(), 1, "should find exactly our event by suffix");
        assert_eq!(results[0].content, format!("Updated content {suffix}"));

        cleanup_search_index(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_index_event_upsert_content_replaced() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        cleanup_search_index(&pool, &suffix).await;

        let storage = SearchIndexStorage::new((*pool).clone());
        let event_id = format!("$upsert2_{suffix}:test.com");
        let room_id = format!("!room2_{suffix}:test.com");
        let user_id = format!("@bob_{suffix}:test.com");

        // Insert with word "alpha"
        let e1 = SearchIndexEntry {
            id: 0,
            event_id: event_id.clone(),
            room_id: room_id.clone(),
            user_id: user_id.clone(),
            event_type: "m.room.message".to_string(),
            content_type: "m.text".to_string(),
            content: "alpha bravo charlie".to_string(),
            created_ts: 1_700_000_000_010,
            updated_ts: None,
        };
        storage.index_event(&e1).await.unwrap();

        // Upsert with completely different words "xray yankee zulu"
        let e2 = SearchIndexEntry {
            id: 0,
            event_id: event_id.clone(),
            room_id: room_id.clone(),
            user_id: user_id.clone(),
            event_type: "m.room.message".to_string(),
            content_type: "m.text".to_string(),
            content: "xray yankee zulu".to_string(),
            created_ts: 1_700_000_000_011,
            updated_ts: Some(1_700_000_000_100),
        };
        storage.index_event(&e2).await.unwrap();

        // Search for "xray" should find it
        let q = SearchQuery {
            search_term: "xray".to_string(),
            room_ids: None,
            not_room_ids: None,
            sender: None,
            event_types: None,
            limit: Some(10),
            from: None,
        };
        let (r, _) = storage.search_events(&q).await.unwrap();
        // At least one result with content "xray yankee zulu"
        let found = r.iter().any(|sr| sr.event_id == event_id && sr.content == "xray yankee zulu");
        assert!(found, "should find updated content, but found results: {:?}", r);

        // Search for "alpha" should NOT find this event (since content was replaced)
        let q2 = SearchQuery {
            search_term: "alpha".to_string(),
            room_ids: None,
            not_room_ids: None,
            sender: None,
            event_types: None,
            limit: Some(10),
            from: None,
        };
        let (r2, _) = storage.search_events(&q2).await.unwrap();
        let found_alpha = r2.iter().any(|sr| sr.event_id == event_id);
        assert!(!found_alpha, "should NOT find old content 'alpha' after upsert to 'xray yankee zulu'");

        cleanup_search_index(&pool, &suffix).await;
    }

    // ── index_events batch ───────────────────────────────────────

    #[tokio::test]
    async fn test_index_events_batch_inserts_all() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        cleanup_search_index(&pool, &suffix).await;

        let storage = SearchIndexStorage::new((*pool).clone());
        let entries: Vec<SearchIndexEntry> =
            (1..=5).map(|i| make_entry(&suffix, i, &format!("Batch message {i}"))).collect();

        let count = storage.index_events(&entries).await.expect("index_events should succeed");
        assert_eq!(count, 5);

        // Search for unique suffix should find all 5
        let query = SearchQuery {
            search_term: suffix.clone(),
            room_ids: None,
            not_room_ids: None,
            sender: None,
            event_types: None,
            limit: Some(20),
            from: None,
        };
        let (results, _) = storage.search_events(&query).await.expect("search should succeed");
        assert_eq!(results.len(), 5);

        cleanup_search_index(&pool, &suffix).await;
    }

    // ── search_events ────────────────────────────────────────────

    #[tokio::test]
    async fn test_search_events_exact_match() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        cleanup_search_index(&pool, &suffix).await;

        let storage = SearchIndexStorage::new((*pool).clone());
        let entry = make_entry(&suffix, 1, "UniquePhrase ForSearchTesting");
        storage.index_event(&entry).await.unwrap();

        // Search for the unique suffix (exact match via ILIKE)
        let query = SearchQuery {
            search_term: suffix.clone(),
            room_ids: None,
            not_room_ids: None,
            sender: None,
            event_types: None,
            limit: Some(10),
            from: None,
        };
        let (results, _) = storage.search_events(&query).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].event_id, entry.event_id);

        cleanup_search_index(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_search_events_case_insensitive() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        cleanup_search_index(&pool, &suffix).await;

        let storage = SearchIndexStorage::new((*pool).clone());
        let entry = make_entry(&suffix, 1, "CASESENSITIVITY Test");
        storage.index_event(&entry).await.unwrap();

        // Lowercase search for the unique suffix (ILIKE is case-insensitive)
        let query = SearchQuery {
            search_term: suffix.to_lowercase(),
            room_ids: None,
            not_room_ids: None,
            sender: None,
            event_types: None,
            limit: Some(10),
            from: None,
        };
        let (results, _) = storage.search_events(&query).await.unwrap();
        assert_eq!(results.len(), 1, "ILIKE should be case-insensitive");
        assert_eq!(results[0].event_id, entry.event_id);

        cleanup_search_index(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_search_events_partial_word_match() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        cleanup_search_index(&pool, &suffix).await;

        let storage = SearchIndexStorage::new((*pool).clone());
        let entry = make_entry(&suffix, 1, "Extraordinary phenomena observed");
        storage.index_event(&entry).await.unwrap();

        // Search for prefix of suffix should match (ILIKE %suffix% uses contains, %prefix matches prefix)
        let prefix = &suffix[..8];
        let query = SearchQuery {
            search_term: prefix.to_string(),
            room_ids: None,
            not_room_ids: None,
            sender: None,
            event_types: None,
            limit: Some(10),
            from: None,
        };
        let (results, _) = storage.search_events(&query).await.unwrap();
        assert_eq!(results.len(), 1, "prefix match should work");

        // Search for substring of suffix (middle chars)
        let mid = &suffix[4..12];
        let query2 = SearchQuery {
            search_term: mid.to_string(),
            room_ids: None,
            not_room_ids: None,
            sender: None,
            event_types: None,
            limit: Some(10),
            from: None,
        };
        let (results2, _) = storage.search_events(&query2).await.unwrap();
        assert_eq!(results2.len(), 1, "contains match should work");

        cleanup_search_index(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_search_events_no_results() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        cleanup_search_index(&pool, &suffix).await;

        let storage = SearchIndexStorage::new((*pool).clone());
        let entry = make_entry(&suffix, 1, "Some ordinary content here");
        storage.index_event(&entry).await.unwrap();

        // Search for something not in the content
        let query = SearchQuery {
            search_term: "xyznonexistent12345".to_string(),
            room_ids: None,
            not_room_ids: None,
            sender: None,
            event_types: None,
            limit: Some(10),
            from: None,
        };
        let (results, _) = storage.search_events(&query).await.unwrap();
        assert_eq!(results.len(), 0);

        cleanup_search_index(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_search_events_limit_respected() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        cleanup_search_index(&pool, &suffix).await;

        let storage = SearchIndexStorage::new((*pool).clone());
        for i in 1..=10 {
            let entry = make_entry(&suffix, i, &format!("{suffix} test message {i}"));
            storage.index_event(&entry).await.unwrap();
        }

        // Search for the unique suffix, request limit 3
        let query = SearchQuery {
            search_term: suffix.clone(),
            room_ids: None,
            not_room_ids: None,
            sender: None,
            event_types: None,
            limit: Some(3),
            from: None,
        };
        let (results, next_batch) = storage.search_events(&query).await.unwrap();
        assert_eq!(results.len(), 3);
        assert!(next_batch.is_some(), "should have next_batch cursor since more results exist");

        cleanup_search_index(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_search_events_pagination_with_cursor() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        cleanup_search_index(&pool, &suffix).await;

        let storage = SearchIndexStorage::new((*pool).clone());
        for i in 1..=5 {
            let entry = make_entry(&suffix, i, &format!("{suffix} test message {i}"));
            storage.index_event(&entry).await.unwrap();
        }

        // First page: limit 2
        let query = SearchQuery {
            search_term: suffix.clone(),
            room_ids: None,
            not_room_ids: None,
            sender: None,
            event_types: None,
            limit: Some(2),
            from: None,
        };
        let (page1, cursor1) = storage.search_events(&query).await.unwrap();
        assert_eq!(page1.len(), 2);
        assert!(cursor1.is_some(), "should have a cursor for next page");

        // Second page: use cursor
        let query2 = SearchQuery {
            search_term: suffix.clone(),
            room_ids: None,
            not_room_ids: None,
            sender: None,
            event_types: None,
            limit: Some(2),
            from: cursor1,
        };
        let (page2, cursor2) = storage.search_events(&query2).await.unwrap();
        assert_eq!(page2.len(), 2);
        assert!(cursor2.is_some(), "should have cursor for third page");

        // Third page: should have remaining 1
        let query3 = SearchQuery {
            search_term: suffix.clone(),
            room_ids: None,
            not_room_ids: None,
            sender: None,
            event_types: None,
            limit: Some(2),
            from: cursor2,
        };
        let (page3, cursor3) = storage.search_events(&query3).await.unwrap();
        assert_eq!(page3.len(), 1);
        assert!(cursor3.is_none(), "no more pages, cursor should be None");

        // Ensure no duplicate event_ids across pages
        let all_ids: Vec<String> =
            page1.iter().chain(page2.iter()).chain(page3.iter()).map(|r| r.event_id.clone()).collect();
        let unique_ids: std::collections::HashSet<_> = all_ids.iter().collect();
        assert_eq!(all_ids.len(), unique_ids.len(), "all event_ids should be unique across pages");

        cleanup_search_index(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_search_events_order_desc_by_created_ts() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        cleanup_search_index(&pool, &suffix).await;

        let storage = SearchIndexStorage::new((*pool).clone());
        for i in 1..=3 {
            let entry = SearchIndexEntry {
                id: 0,
                event_id: format!("$order_{}_{i}:test.com", suffix),
                room_id: format!("!order_{suffix}:test.com"),
                user_id: format!("@u_{i}:test.com"),
                event_type: "m.room.message".to_string(),
                content_type: "m.text".to_string(),
                content: format!("Order test {suffix} {i}"),
                created_ts: 1_700_000_000_000 + (i * 1000), // i=1 -> 1700000001000, i=3 -> 1700000003000
                updated_ts: None,
            };
            storage.index_event(&entry).await.unwrap();
        }

        let query = SearchQuery {
            search_term: suffix.clone(),
            room_ids: None,
            not_room_ids: None,
            sender: None,
            event_types: None,
            limit: Some(10),
            from: None,
        };
        let (results, _) = storage.search_events(&query).await.unwrap();
        assert_eq!(results.len(), 3);
        // Results should be in DESC order by created_ts (newest first)
        for window in results.windows(2) {
            assert!(
                window[0].origin_server_ts >= window[1].origin_server_ts,
                "results should be in desc order: {} >= {}",
                window[0].origin_server_ts,
                window[1].origin_server_ts
            );
        }

        cleanup_search_index(&pool, &suffix).await;
    }

    // ── delete_event ─────────────────────────────────────────────

    #[tokio::test]
    async fn test_delete_event_removes_from_index() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        cleanup_search_index(&pool, &suffix).await;

        let storage = SearchIndexStorage::new((*pool).clone());
        let entry = make_entry(&suffix, 1, "Delete me please");
        storage.index_event(&entry).await.unwrap();

        // Verify it exists via unique suffix
        let q = SearchQuery {
            search_term: suffix.clone(),
            room_ids: None,
            not_room_ids: None,
            sender: None,
            event_types: None,
            limit: Some(10),
            from: None,
        };
        let (r1, _) = storage.search_events(&q).await.unwrap();
        assert_eq!(r1.len(), 1);

        // Delete
        storage.delete_event(&entry.event_id).await.expect("delete_event should succeed");

        // Verify gone
        let (r2, _) = storage.search_events(&q).await.unwrap();
        assert_eq!(r2.len(), 0);

        cleanup_search_index(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_delete_event_nonexistent_is_noop() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        cleanup_search_index(&pool, &suffix).await;

        let storage = SearchIndexStorage::new((*pool).clone());
        // Deleting an event_id that doesn't exist should not error
        let result = storage.delete_event("$nonexistent_event_12345:test.com").await;
        assert!(result.is_ok(), "delete of nonexistent event should be ok");

        cleanup_search_index(&pool, &suffix).await;
    }

    // ── delete_room_index ────────────────────────────────────────

    #[tokio::test]
    async fn test_delete_room_index_removes_all_events_for_room() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        cleanup_search_index(&pool, &suffix).await;

        let storage = SearchIndexStorage::new((*pool).clone());
        let room_id = format!("!roomdel_{suffix}:test.com");

        // Insert 3 events for the target room with suffix in content
        for i in 1..=3 {
            let entry = SearchIndexEntry {
                id: 0,
                event_id: format!("$rmdel_{suffix}_{i}:test.com"),
                room_id: room_id.clone(),
                user_id: format!("@u_{suffix}:test.com"),
                event_type: "m.room.message".to_string(),
                content_type: "m.text".to_string(),
                content: format!("Room deletion test {suffix} {i}"),
                created_ts: 1_700_000_000_000 + i,
                updated_ts: None,
            };
            storage.index_event(&entry).await.unwrap();
        }

        // Also insert 1 event for a different room (should survive)
        let other_suffix = make_suffix();
        let other_entry = SearchIndexEntry {
            id: 0,
            event_id: format!("$other_{other_suffix}:test.com"),
            room_id: format!("!other_{other_suffix}:test.com"),
            user_id: "@other:test.com".to_string(),
            event_type: "m.room.message".to_string(),
            content_type: "m.text".to_string(),
            content: format!("Other room content {other_suffix}"),
            created_ts: 1_700_000_000_100,
            updated_ts: None,
        };
        storage.index_event(&other_entry).await.unwrap();

        // Delete the room
        let deleted = storage.delete_room_index(&room_id).await.expect("delete_room_index");
        assert_eq!(deleted, 3);

        // Verify the room's events are gone (search for suffix)
        let q = SearchQuery {
            search_term: suffix.clone(),
            room_ids: None,
            not_room_ids: None,
            sender: None,
            event_types: None,
            limit: Some(10),
            from: None,
        };
        let (results, _) = storage.search_events(&q).await.unwrap();
        assert_eq!(results.len(), 0, "room events should be deleted");

        // Verify the other room's event survived (search for other_suffix)
        let q2 = SearchQuery {
            search_term: other_suffix.clone(),
            room_ids: None,
            not_room_ids: None,
            sender: None,
            event_types: None,
            limit: Some(10),
            from: None,
        };
        let (r2, _) = storage.search_events(&q2).await.unwrap();
        assert_eq!(r2.len(), 1);
        assert_eq!(r2[0].event_id, other_entry.event_id);

        // Clean up both rooms' entries
        cleanup_search_index(&pool, &suffix).await;
        let pattern = format!("%{other_suffix}");
        let _ = sqlx::query("DELETE FROM search_index WHERE event_id LIKE $1").bind(&pattern).execute(&*pool).await;
    }

    #[tokio::test]
    async fn test_delete_room_index_returns_zero_for_nonexistent_room() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        cleanup_search_index(&pool, &suffix).await;

        let storage = SearchIndexStorage::new((*pool).clone());
        let deleted = storage.delete_room_index("!nonexistent_room:test.com").await.expect("should succeed");
        assert_eq!(deleted, 0);

        cleanup_search_index(&pool, &suffix).await;
    }

    // ── get_stats ────────────────────────────────────────────────

    #[tokio::test]
    async fn test_get_stats_counts_events() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        cleanup_search_index(&pool, &suffix).await;

        let storage = SearchIndexStorage::new((*pool).clone());

        let entry = SearchIndexEntry {
            id: 0,
            event_id: format!("$stats_{suffix}:test.com"),
            room_id: format!("!stats_{suffix}:test.com"),
            user_id: "@stats:test.com".to_string(),
            event_type: "m.room.message".to_string(),
            content_type: "m.text".to_string(),
            content: "Stats test content".to_string(),
            created_ts: 1_700_000_000_000,
            updated_ts: None,
        };
        storage.index_event(&entry).await.unwrap();

        let stats = storage.get_stats().await.expect("get_stats should succeed");
        assert!(stats.total_count >= 1, "total_count should be at least 1");
        assert!(stats.by_event_type.contains_key("m.room.message"), "by_event_type should contain m.room.message");

        cleanup_search_index(&pool, &suffix).await;
    }

    // ── rebuild_room_index ───────────────────────────────────────

    async fn ensure_test_room(pool: &PgPool, room_id: &str) {
        sqlx::query(
            "INSERT INTO rooms (room_id, room_version, is_public, creator, created_ts)
             VALUES ($1, '1', false, '@test:localhost', EXTRACT(EPOCH FROM NOW()) * 1000)
             ON CONFLICT (room_id) DO NOTHING",
        )
        .bind(room_id)
        .execute(pool)
        .await
        .ok();
    }

    async fn cleanup_room_and_events(pool: &PgPool, suffix: &str) {
        let pattern = format!("%{suffix}");
        let _ = sqlx::query("DELETE FROM search_index WHERE event_id LIKE $1").bind(&pattern).execute(pool).await;
        let _ = sqlx::query("DELETE FROM events WHERE event_id LIKE $1").bind(&pattern).execute(pool).await;
        let _ = sqlx::query("DELETE FROM rooms WHERE room_id LIKE $1").bind(&pattern).execute(pool).await;
    }

    #[tokio::test]
    async fn test_rebuild_room_index_from_events_table() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        cleanup_room_and_events(&pool, &suffix).await;

        let storage = SearchIndexStorage::new((*pool).clone());
        let room_id = format!("!rebuild_{suffix}:localhost");
        ensure_test_room(&pool, &room_id).await;

        // Insert events into the events table with suffix-based content
        let events_data = vec![
            (
                "$ev_rb_1_{suffix}:localhost",
                "m.room.message",
                format!("{{\"body\":\"First {suffix} message\",\"msgtype\":\"m.text\"}}"),
            ),
            (
                "$ev_rb_2_{suffix}:localhost",
                "m.room.message",
                format!("{{\"body\":\"Second {suffix} message\",\"msgtype\":\"m.text\"}}"),
            ),
            ("$ev_rb_3_{suffix}:localhost", "m.room.name", "{\"name\":\"Rebuild Test Room\"}".to_string()),
        ];

        for (event_id, event_type, content) in &events_data {
            let event_id = event_id.replace("{suffix}", &suffix);
            sqlx::query(
                "INSERT INTO events (event_id, room_id, sender, event_type, content, origin_server_ts)
                 VALUES ($1, $2, '@sender:localhost', $3, $4::jsonb, EXTRACT(EPOCH FROM NOW()) * 1000)
                 ON CONFLICT (event_id) DO NOTHING",
            )
            .bind(&event_id)
            .bind(&room_id)
            .bind(event_type)
            .bind(content)
            .execute(&*pool)
            .await
            .expect("insert into events");
        }

        // Rebuild
        let count = storage.rebuild_room_index(&room_id).await.expect("rebuild_room_index should succeed");
        assert_eq!(count, 3, "should rebuild 3 events from events table");

        // Verify via search using unique suffix
        let q = SearchQuery {
            search_term: suffix.clone(),
            room_ids: None,
            not_room_ids: None,
            sender: None,
            event_types: None,
            limit: Some(10),
            from: None,
        };
        let (results, _) = storage.search_events(&q).await.unwrap();
        assert_eq!(results.len(), 2, "should find both m.room.message events with unique suffix");

        cleanup_room_and_events(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_rebuild_room_index_empty_room() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        cleanup_room_and_events(&pool, &suffix).await;

        let storage = SearchIndexStorage::new((*pool).clone());
        let room_id = format!("!emptyrebuild_{suffix}:localhost");
        ensure_test_room(&pool, &room_id).await;

        // No events inserted — rebuild should return 0
        let count = storage.rebuild_room_index(&room_id).await.expect("rebuild_room_index should succeed");
        assert_eq!(count, 0);

        cleanup_room_and_events(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_rebuild_room_index_replaces_existing() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        cleanup_room_and_events(&pool, &suffix).await;

        let storage = SearchIndexStorage::new((*pool).clone());
        let room_id = format!("!replace_{suffix}:localhost");
        ensure_test_room(&pool, &room_id).await;

        // Manually insert a stale search_index entry with suffix
        let old_entry = SearchIndexEntry {
            id: 0,
            event_id: format!("$stale_{suffix}:localhost"),
            room_id: room_id.clone(),
            user_id: "@old:localhost".to_string(),
            event_type: "m.room.message".to_string(),
            content_type: "m.text".to_string(),
            content: format!("Stale content {suffix}"),
            created_ts: 1_000_000_000_000,
            updated_ts: None,
        };
        storage.index_event(&old_entry).await.unwrap();

        // Insert a real event into events table with suffix in content
        sqlx::query(
            "INSERT INTO events (event_id, room_id, sender, event_type, content, origin_server_ts)
             VALUES ($1, $2, '@sender:localhost', 'm.room.message', $3::jsonb, EXTRACT(EPOCH FROM NOW()) * 1000)
             ON CONFLICT (event_id) DO NOTHING",
        )
        .bind(format!("$new_{suffix}:localhost"))
        .bind(&room_id)
        .bind(format!("{{\"body\":\"Fresh {suffix} content\",\"msgtype\":\"m.text\"}}"))
        .execute(&*pool)
        .await
        .unwrap();

        // Rebuild
        let count = storage.rebuild_room_index(&room_id).await.expect("rebuild_room_index");
        assert_eq!(count, 1, "should index the 1 event from events table; stale entry deleted");

        // Search for suffix should find the fresh content (not stale)
        let q = SearchQuery {
            search_term: suffix.clone(),
            room_ids: None,
            not_room_ids: None,
            sender: None,
            event_types: None,
            limit: Some(10),
            from: None,
        };
        let (r, _) = storage.search_events(&q).await.unwrap();
        assert_eq!(r.len(), 1, "fresh content from events table should be indexed, stale entry deleted");
        assert_eq!(r[0].room_id, room_id);

        cleanup_room_and_events(&pool, &suffix).await;
    }
}
