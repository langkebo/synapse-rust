use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{Postgres, Row};
use std::sync::Arc;
use synapse_common::*;
use synapse_storage::{EventStorage, RoomStorage};

#[derive(Debug, Clone, Default)]
pub struct SearchFilters {
    pub sender_id: Option<String>,
    pub room_id: Option<String>,
    pub message_type: Option<String>,
    pub start_ts: Option<i64>,
    pub end_ts: Option<i64>,
    pub has_media: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct AdvancedSearchOptions {
    pub query: String,
    pub filters: SearchFilters,
    pub limit: i64,
    pub offset: i64,
    pub highlight: bool,
    pub fuzzy: bool,
}

impl Default for AdvancedSearchOptions {
    fn default() -> Self {
        Self {
            query: String::new(),
            filters: SearchFilters::default(),
            limit: 20,
            offset: 0,
            highlight: true,
            fuzzy: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub results: Vec<SearchResultItem>,
    pub total_count: usize,
    pub next_batch: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResultItem {
    pub event_id: String,
    pub room_id: String,
    pub sender: String,
    pub content: String,
    pub event_type: String,
    pub origin_server_ts: i64,
    pub highlights: Option<Vec<String>>,
    pub room_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
struct PostgresSearchCursor {
    rank: f64,
    origin_server_ts: i64,
    event_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ElasticsearchSearchCursor {
    origin_server_ts: i64,
    event_id: String,
}

fn encode_postgres_search_cursor(cursor: &PostgresSearchCursor) -> String {
    let raw = format!("{}|{}|{}", cursor.rank, cursor.origin_server_ts, cursor.event_id);
    URL_SAFE_NO_PAD.encode(raw.as_bytes())
}

fn decode_postgres_search_cursor(cursor: Option<&str>) -> Option<PostgresSearchCursor> {
    let cursor = cursor?;
    let decoded = URL_SAFE_NO_PAD.decode(cursor).ok()?;
    let decoded = String::from_utf8(decoded).ok()?;
    let (rank, rest) = decoded.split_once('|')?;
    let (origin_server_ts, event_id) = rest.split_once('|')?;
    if event_id.is_empty() {
        return None;
    }
    Some(PostgresSearchCursor {
        rank: rank.parse().ok()?,
        origin_server_ts: origin_server_ts.parse().ok()?,
        event_id: event_id.to_string(),
    })
}

fn encode_elasticsearch_search_cursor(cursor: &ElasticsearchSearchCursor) -> String {
    let raw = format!("{}|{}", cursor.origin_server_ts, cursor.event_id);
    URL_SAFE_NO_PAD.encode(raw.as_bytes())
}

fn decode_elasticsearch_search_cursor(cursor: Option<&str>) -> Option<ElasticsearchSearchCursor> {
    let cursor = cursor?;
    let decoded = URL_SAFE_NO_PAD.decode(cursor).ok()?;
    let decoded = String::from_utf8(decoded).ok()?;
    let (origin_server_ts, event_id) = decoded.split_once('|')?;
    if event_id.is_empty() {
        return None;
    }
    Some(ElasticsearchSearchCursor { origin_server_ts: origin_server_ts.parse().ok()?, event_id: event_id.to_string() })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RoomEventsCursor {
    origin_server_ts: i64,
    event_id: String,
}

fn encode_room_events_cursor(cursor: &RoomEventsCursor) -> String {
    format!("{}|{}", cursor.origin_server_ts, cursor.event_id)
}

fn decode_room_events_cursor(cursor: Option<&str>) -> Option<RoomEventsCursor> {
    let cursor = cursor?;
    let mut parts = cursor.splitn(2, '|');
    let origin_server_ts = parts.next()?.parse::<i64>().ok()?;
    let event_id = parts.next()?.to_string();
    if event_id.is_empty() {
        return None;
    }
    Some(RoomEventsCursor { origin_server_ts, event_id })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedEvent {
    pub event_id: String,
    pub room_id: String,
    pub sender: String,
    pub content: String,
    pub event_type: String,
    pub message_type: Option<String>,
    pub origin_server_ts: i64,
    pub index_ts: i64,
    pub keys: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RoomEventsSearchFilter {
    pub rooms: Option<Vec<String>>,
    pub not_rooms: Option<Vec<String>>,
    pub types: Option<Vec<String>>,
    pub senders: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchRoomEvent {
    pub event_id: String,
    pub room_id: String,
    pub sender: String,
    pub event_type: String,
    pub content: Value,
    pub origin_server_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchRoomEventsPage {
    pub results: Vec<SearchRoomEvent>,
    pub next_batch: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimestampDirection {
    Forward,
    Backward,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TimestampEventMatch {
    pub event_id: String,
    pub origin_server_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventContextEntry {
    pub event_id: String,
    pub sender: String,
    pub event_type: String,
    pub content: Value,
    pub origin_server_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventContextWindow {
    pub events_before: Vec<EventContextEntry>,
    pub events_after: Vec<EventContextEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchRoomSummary {
    pub room_id: String,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub avatar_url: Option<String>,
    pub is_public: bool,
}

#[derive(Clone)]
pub struct SearchService {
    client: reqwest::Client,
    enabled: bool,
    base_url: String,
    index_name: String,
    /// PostgreSQL 连接池（用于本地全文搜索）
    postgres_pool: Option<sqlx::Pool<Postgres>>,
    /// 搜索服务提供商: "elasticsearch" | "postgres"
    provider: String,
}

impl SearchService {
    pub fn new(url: &str, enabled: bool, index_name: &str) -> Self {
        Self::with_postgres(url, enabled, index_name, None, "postgres".to_string())
    }

    pub fn with_postgres(
        url: &str,
        enabled: bool,
        index_name: &str,
        postgres_pool: Option<sqlx::Pool<Postgres>>,
        provider: String,
    ) -> Self {
        let base_url = url.trim_end_matches('/').to_string();

        Self {
            client: reqwest::Client::new(),
            enabled,
            base_url,
            index_name: index_name.to_string(),
            postgres_pool,
            provider,
        }
    }

    /// 使用 PostgreSQL 全文搜索搜索消息
    pub async fn search_postgres(
        &self,
        user_id: &str,
        query: &str,
        limit: i64,
        next_batch: Option<&str>,
    ) -> ApiResult<SearchResult> {
        let event_storage = self.event_storage()?;
        let cursor = decode_postgres_search_cursor(next_batch);

        // 构建 FTS 查询
        let rows = event_storage
            .search_postgres_messages(
                user_id,
                query,
                cursor.as_ref().map(|cursor| cursor.rank),
                cursor.as_ref().map(|cursor| cursor.origin_server_ts),
                cursor.as_ref().map(|cursor| cursor.event_id.as_str()),
                limit + 1,
            )
            .await
            .map_err(|e| ApiError::internal_with_log("Search failed", &e))?;

        let has_more = rows.len() > limit as usize;
        let visible_rows = if has_more { &rows[..limit as usize] } else { &rows[..] };
        let mut results = Vec::new();
        for (event_id, room_id, sender, event_type, content, origin_server_ts, _rank) in visible_rows {
            results.push(SearchResultItem {
                event_id: event_id.clone(),
                room_id: room_id.clone(),
                sender: sender.clone(),
                event_type: event_type.clone(),
                content: content.as_str().unwrap_or("").to_string(),
                origin_server_ts: *origin_server_ts,
                highlights: None,
                room_name: None,
            });
        }

        let total_count = results.len();
        let next_batch = if has_more {
            visible_rows.last().map(|(event_id, _room_id, _sender, _event_type, _content, origin_server_ts, rank)| {
                encode_postgres_search_cursor(&PostgresSearchCursor {
                    rank: *rank,
                    origin_server_ts: *origin_server_ts,
                    event_id: event_id.clone(),
                })
            })
        } else {
            None
        };

        Ok(SearchResult { results, total_count, next_batch })
    }

    /// 创建 PostgreSQL 全文搜索索引
    pub async fn create_fts_index(&self) -> ApiResult<()> {
        let event_storage = self.event_storage()?;
        event_storage
            .create_postgres_fts_index()
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to create FTS index", &e))?;

        ::tracing::info!(
            provider = %"postgres",
            index_name = %"events_fts_idx",
            "PostgreSQL FTS index created successfully"
        );
        Ok(())
    }

    /// 检查是否为 PostgreSQL 搜索
    pub fn is_postgres_enabled(&self) -> bool {
        self.provider == "postgres" && self.postgres_pool.is_some()
    }

    pub async fn init_indices(&self) -> ApiResult<()> {
        if !self.enabled {
            return Ok(());
        }

        let mapping = json!({
            "mappings": {
                "properties": {
                    "event_id": { "type": "keyword" },
                    "room_id": { "type": "keyword" },
                    "sender": { "type": "keyword" },
                    "content": { "type": "text", "analyzer": "standard" },
                    "event_type": { "type": "keyword" },
                    "message_type": { "type": "keyword" },
                    "origin_server_ts": { "type": "date", "format": "epoch_millis" },
                    "index_ts": { "type": "date", "format": "epoch_millis" },
                    "keys": { "type": "text", "analyzer": "standard" }
                }
            },
            "settings": {
                "number_of_shards": 1,
                "number_of_replicas": 0
            }
        });

        let url = format!("{}/{}", self.base_url, self.index_name);
        let response = self.client.put(&url).json(&mapping).send().await;

        match response {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    ::tracing::info!(
                        index_name = %self.index_name,
                        base_url = %self.base_url,
                        "Created Elasticsearch index"
                    );
                } else if status.as_u16() == 400 {
                    ::tracing::info!(
                        index_name = %self.index_name,
                        base_url = %self.base_url,
                        "Elasticsearch index already exists"
                    );
                } else {
                    ::tracing::warn!(
                        status = %status,
                        index_name = %self.index_name,
                        base_url = %self.base_url,
                        "Index creation returned status"
                    );
                }
            }
            Err(e) => {
                ::tracing::warn!(
                    error = %e,
                    index_name = %self.index_name,
                    base_url = %self.base_url,
                    "Index creation request failed"
                );
            }
        }

        Ok(())
    }

    pub async fn index_event(&self, event: &IndexedEvent) -> ApiResult<()> {
        if !self.enabled {
            return Ok(());
        }

        let doc = json!({
            "event_id": event.event_id,
            "room_id": event.room_id,
            "sender": event.sender,
            "content": event.content,
            "event_type": event.event_type,
            "message_type": event.message_type,
            "origin_server_ts": event.origin_server_ts,
            "index_ts": event.index_ts,
            "keys": event.keys
        });

        let url = format!("{}/{}/_doc/{}", self.base_url, self.index_name, event.event_id);
        let response = self
            .client
            .put(&url)
            .json(&doc)
            .send()
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to index event", &e))?;

        if !response.status().is_success() {
            return Err(ApiError::internal("Failed to index event".to_string()));
        }

        Ok(())
    }

    pub async fn bulk_index(&self, events: &[IndexedEvent]) -> ApiResult<()> {
        if !self.enabled || events.is_empty() {
            return Ok(());
        }

        let mut body = String::with_capacity(events.len() * 512);
        for event in events {
            let action = json!({ "index": { "_index": self.index_name, "_id": event.event_id } });
            let doc = json!({
                "event_id": event.event_id,
                "room_id": event.room_id,
                "sender": event.sender,
                "content": event.content,
                "event_type": event.event_type,
                "message_type": event.message_type,
                "origin_server_ts": event.origin_server_ts,
                "index_ts": event.index_ts,
                "keys": event.keys
            });
            body.push_str(&serde_json::to_string(&action).unwrap_or_default());
            body.push('\n');
            body.push_str(&serde_json::to_string(&doc).unwrap_or_default());
            body.push('\n');
        }

        let url = format!("{}/_bulk", self.base_url);
        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/x-ndjson")
            .body(body)
            .send()
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to bulk index", &e))?;

        let status = response.status();
        if !status.is_success() {
            ::tracing::warn!(
                status = %status,
                index_name = %self.index_name,
                base_url = %self.base_url,
                batch_size = events.len(),
                "Bulk index returned non-success status"
            );
        }

        Ok(())
    }

    pub async fn delete_event(&self, event_id: &str) -> ApiResult<()> {
        if !self.enabled {
            return Ok(());
        }

        let url = format!("{}/{}/_doc/{}", self.base_url, self.index_name, event_id);
        let response = self
            .client
            .delete(&url)
            .send()
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to delete event", &e))?;

        let status = response.status();
        if !status.is_success() && status.as_u16() != 404 {
            return Err(ApiError::internal("Failed to delete event".to_string()));
        }

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn index_message(
        &self,
        event_id: &str,
        room_id: &str,
        sender: &str,
        content: &str,
        event_type: &str,
        message_type: Option<&str>,
        origin_server_ts: i64,
    ) -> ApiResult<()> {
        let event = IndexedEvent {
            event_id: event_id.to_string(),
            room_id: room_id.to_string(),
            sender: sender.to_string(),
            content: content.to_string(),
            event_type: event_type.to_string(),
            message_type: message_type.map(|s| s.to_string()),
            origin_server_ts,
            index_ts: chrono::Utc::now().timestamp_millis(),
            keys: Self::extract_keys(content),
        };

        self.index_event(&event).await
    }

    fn extract_keys(content: &str) -> Vec<String> {
        content.split_whitespace().take(10).map(|s| s.to_lowercase()).collect()
    }

    pub async fn search_messages(
        &self,
        user_id: &str,
        query: &str,
        limit: i64,
        next_batch: Option<&str>,
    ) -> ApiResult<SearchResult> {
        // 优先使用 PostgreSQL 全文搜索
        if self.is_postgres_enabled() {
            return self.search_postgres(user_id, query, limit, next_batch).await;
        }

        // 回退到 Elasticsearch
        let options = AdvancedSearchOptions {
            query: query.to_string(),
            filters: SearchFilters::default(),
            limit,
            offset: 0,
            highlight: true,
            fuzzy: true,
        };
        self.advanced_search(&options, next_batch).await
    }

    pub async fn advanced_search(
        &self,
        options: &AdvancedSearchOptions,
        next_batch: Option<&str>,
    ) -> ApiResult<SearchResult> {
        if !self.enabled {
            return Err(ApiError::internal("Elasticsearch is disabled".to_string()));
        }
        let cursor = decode_elasticsearch_search_cursor(next_batch);

        let mut must_clauses = Vec::new();
        let mut filter_clauses = Vec::new();

        must_clauses.push(json!({
            "multi_match": {
                "query": options.query,
                "fields": ["content^2", "keys"],
                "fuzziness": if options.fuzzy { "AUTO" } else { "0" },
                "prefix_length": if options.fuzzy { 2 } else { 0 }
            }
        }));

        if let Some(ref room_id) = options.filters.room_id {
            filter_clauses.push(json!({ "term": { "room_id": room_id } }));
        }

        if let Some(ref sender_id) = options.filters.sender_id {
            filter_clauses.push(json!({ "term": { "sender": sender_id } }));
        }

        if let Some(ref message_type) = options.filters.message_type {
            filter_clauses.push(json!({ "term": { "message_type": message_type } }));
        }

        if let Some(start_ts) = options.filters.start_ts {
            filter_clauses.push(json!({ "range": { "origin_server_ts": { "gte": start_ts } } }));
        }

        if let Some(end_ts) = options.filters.end_ts {
            filter_clauses.push(json!({ "range": { "origin_server_ts": { "lte": end_ts } } }));
        }

        let query_builder = json!({
            "bool": {
                "must": must_clauses,
                "filter": filter_clauses
            }
        });

        let mut search_body = json!({
            "query": query_builder,
            "size": options.limit + 1,
            "sort": [
                { "origin_server_ts": { "order": "desc" } },
                { "_id": { "order": "desc" } }
            ]
        });

        if let Some(cursor) = cursor {
            search_body["search_after"] = json!([cursor.origin_server_ts, cursor.event_id]);
        }

        if options.highlight {
            search_body["highlight"] = json!({
                "fields": {
                    "content": {
                        "pre_tags": ["<em>"],
                        "post_tags": ["</em>"],
                        "fragment_size": 150,
                        "number_of_fragments": 3
                    }
                }
            });
        }

        let url = format!("{}/{}/_search", self.base_url, self.index_name);
        let response = self
            .client
            .post(&url)
            .json(&search_body)
            .send()
            .await
            .map_err(|e| ApiError::internal_with_log("Search failed", &e))?;

        let response_json: Value =
            response.json().await.map_err(|e| ApiError::internal_with_log("Failed to parse search response", &e))?;

        let hits_array = response_json
            .get("hits")
            .and_then(|h| h.get("hits"))
            .and_then(|h| h.as_array())
            .cloned()
            .unwrap_or_default();

        let total_count = response_json
            .get("hits")
            .and_then(|h| h.get("total"))
            .and_then(|t| t.get("value"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;

        let has_more = hits_array.len() > options.limit as usize;
        let visible_hits = if has_more { &hits_array[..options.limit as usize] } else { &hits_array[..] };

        let results: Vec<SearchResultItem> = visible_hits
            .iter()
            .map(|hit| {
                let source = hit.get("_source").cloned().unwrap_or(json!({}));
                let highlight = hit.get("highlight").and_then(|h| h.as_object());

                SearchResultItem {
                    event_id: source.get("event_id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    room_id: source.get("room_id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    sender: source.get("sender").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    content: source.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    event_type: source.get("event_type").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    origin_server_ts: source.get("origin_server_ts").and_then(|v| v.as_i64()).unwrap_or(0),
                    highlights: highlight
                        .and_then(|h| h.get("content"))
                        .and_then(|c| c.as_array())
                        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect()),
                    room_name: None,
                }
            })
            .collect();

        let next_batch = if has_more {
            visible_hits.last().and_then(|hit| {
                let sort = hit.get("sort")?.as_array()?;
                let origin_server_ts = sort.first()?.as_i64()?;
                let event_id = sort.get(1)?.as_str()?.to_string();
                Some(encode_elasticsearch_search_cursor(&ElasticsearchSearchCursor { origin_server_ts, event_id }))
            })
        } else {
            None
        };

        Ok(SearchResult { results, total_count, next_batch })
    }

    pub async fn delete_room_index(&self, room_id: &str) -> ApiResult<()> {
        if !self.enabled {
            return Ok(());
        }

        let delete_by_query = json!({
            "query": { "term": { "room_id": room_id } }
        });

        let url = format!("{}/{}/_delete_by_query", self.base_url, self.index_name);
        let response = self
            .client
            .post(&url)
            .json(&delete_by_query)
            .send()
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to delete room index", &e))?;

        let status = response.status();
        if !status.is_success() {
            ::tracing::warn!(
                status = %status,
                index_name = %self.index_name,
                base_url = %self.base_url,
                room_id = %room_id,
                "Delete room index returned non-success status"
            );
        }

        Ok(())
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn postgres_pool(&self) -> ApiResult<&sqlx::Pool<Postgres>> {
        self.postgres_pool.as_ref().ok_or_else(|| ApiError::internal("PostgreSQL search not configured".to_string()))
    }

    fn room_storage(&self) -> ApiResult<RoomStorage> {
        let pool = Arc::new(self.postgres_pool()?.clone());
        Ok(RoomStorage::new(&pool))
    }

    fn event_storage(&self) -> ApiResult<EventStorage> {
        let pool = Arc::new(self.postgres_pool()?.clone());
        Ok(EventStorage::new(&pool, String::new()))
    }

    fn context_entry_from_value(value: Value) -> EventContextEntry {
        EventContextEntry {
            event_id: value
                .get("event_id")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_string(),
            sender: value
                .get("sender")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_string(),
            event_type: value
                .get("type")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_string(),
            content: value.get("content").cloned().unwrap_or(Value::Null),
            origin_server_ts: value.get("origin_server_ts").and_then(|value| value.as_i64()).unwrap_or_default(),
        }
    }

    /// 使用 PostgreSQL 全文搜索搜索房间事件
    #[::tracing::instrument(
        skip_all,
        fields(
            user_id = %user_id,
            search_term_len = search_term.len(),
            has_filter = filter.is_some(),
            limit = limit,
            has_next_batch = next_batch.is_some()
        )
    )]
    pub async fn search_room_events(
        &self,
        user_id: &str,
        search_term: &str,
        filter: Option<&RoomEventsSearchFilter>,
        limit: i64,
        next_batch: Option<&str>,
    ) -> ApiResult<SearchRoomEventsPage> {
        let room_storage = self.room_storage()?;
        let event_storage = self.event_storage()?;
        let cursor = decode_room_events_cursor(next_batch);

        if next_batch.is_some() && cursor.is_none() {
            return Err(ApiError::bad_request("Invalid next_batch cursor"));
        }

        let joined_rooms = room_storage
            .get_user_rooms(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get joined rooms", &e))?;

        if joined_rooms.is_empty() {
            return Ok(SearchRoomEventsPage { results: Vec::new(), next_batch: None });
        }

        let search_pattern = format!("%{}%", search_term.to_lowercase());
        let rows = event_storage
            .search_joined_room_events(
                &joined_rooms,
                &search_pattern,
                filter.and_then(|value| value.rooms.as_deref()),
                filter.and_then(|value| value.not_rooms.as_deref()),
                filter.and_then(|value| value.types.as_deref()),
                filter.and_then(|value| value.senders.as_deref()),
                cursor.as_ref().map(|cursor| (cursor.event_id.as_str(), cursor.origin_server_ts)),
                limit + 1,
            )
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        let has_more = rows.len() > limit as usize;
        let visible_rows = if has_more { &rows[..limit as usize] } else { &rows[..] };
        let results = visible_rows
            .iter()
            .map(|(event_id, room_id, sender, event_type, content, origin_server_ts)| SearchRoomEvent {
                event_id: event_id.clone(),
                room_id: room_id.clone(),
                sender: sender.clone(),
                event_type: event_type.clone(),
                content: content.clone(),
                origin_server_ts: *origin_server_ts,
            })
            .collect();
        let next_batch = if has_more {
            visible_rows.last().map(|(event_id, _room_id, _sender, _event_type, _content, origin_server_ts)| {
                encode_room_events_cursor(&RoomEventsCursor {
                    origin_server_ts: *origin_server_ts,
                    event_id: event_id.clone(),
                })
            })
        } else {
            None
        };

        Ok(SearchRoomEventsPage { results, next_batch })
    }

    #[::tracing::instrument(skip_all, fields(room_id = %room_id, ts = ts, direction = ?direction))]
    pub async fn find_event_by_timestamp(
        &self,
        room_id: &str,
        ts: i64,
        direction: TimestampDirection,
    ) -> ApiResult<Option<TimestampEventMatch>> {
        let event_storage = self.event_storage()?;
        let row = event_storage
            .find_event_id_by_timestamp(room_id, ts, matches!(direction, TimestampDirection::Forward))
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        Ok(row.map(|(event_id, origin_server_ts)| TimestampEventMatch { event_id, origin_server_ts }))
    }

    #[::tracing::instrument(skip_all, fields(room_id = %room_id, target_ts = target_ts, limit = limit))]
    pub async fn get_event_context_window(
        &self,
        room_id: &str,
        target_ts: i64,
        limit: i64,
    ) -> ApiResult<EventContextWindow> {
        let event_storage = self.event_storage()?;

        let events_before = event_storage
            .get_events_before_context(room_id, target_ts, limit)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        let events_after = event_storage
            .get_events_after_context(room_id, target_ts, limit)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        Ok(EventContextWindow {
            events_before: events_before.into_iter().map(Self::context_entry_from_value).collect(),
            events_after: events_after.into_iter().map(Self::context_entry_from_value).collect(),
        })
    }

    #[::tracing::instrument(skip_all, fields(user_id = %user_id, search_term_len = search_term.len(), limit = limit))]
    pub async fn search_rooms_for_user(
        &self,
        user_id: &str,
        search_term: &str,
        limit: i64,
    ) -> ApiResult<Vec<SearchRoomSummary>> {
        let room_storage = self.room_storage()?;
        let search_pattern = format!("%{}%", search_term.to_lowercase());

        let rows = room_storage
            .search_rooms_for_user(user_id, &search_pattern, limit)
            .await
            .map_err(|e| ApiError::internal_with_log("Search failed", &e))?;

        Ok(rows
            .into_iter()
            .map(|(room_id, name, topic, avatar_url, is_public)| SearchRoomSummary {
                room_id,
                name,
                topic,
                avatar_url,
                is_public,
            })
            .collect())
    }

    #[::tracing::instrument(skip_all, fields(room_id = %room_id, search_term_len = search_term.len(), limit = limit))]
    pub async fn search_room_messages(
        &self,
        room_id: &str,
        search_term: &str,
        limit: i64,
    ) -> ApiResult<Vec<serde_json::Value>> {
        let event_storage = self.event_storage()?;
        let search_pattern = format!("%{}%", search_term.to_lowercase());
        let rows = event_storage
            .search_room_messages_admin(room_id, &search_pattern, limit)
            .await
        .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let event_id = row.get("event_id").cloned().unwrap_or(Value::Null);
                let sender = row.get("sender").cloned().unwrap_or(Value::Null);
                let event_type = row.get("type").cloned().unwrap_or(Value::Null);
                let content = row.get("content").cloned().unwrap_or(Value::Null);
                let origin_server_ts = row.get("origin_server_ts").cloned().unwrap_or(Value::Null);
                json!({
                    "rank": 1.0,
                    "result": {
                        "event_id": event_id,
                        "room_id": room_id,
                        "sender": sender,
                        "type": event_type,
                        "content": content,
                        "origin_server_ts": origin_server_ts
                    }
                })
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_service_new() {
        let service = SearchService::new("http://localhost:9200", true, "test_index");

        assert!(service.is_enabled());
        assert_eq!(service.base_url, "http://localhost:9200");
        assert_eq!(service.index_name, "test_index");
    }

    #[test]
    fn test_search_service_new_with_trailing_slash() {
        let service = SearchService::new("http://localhost:9200/", true, "test_index");

        assert_eq!(service.base_url, "http://localhost:9200");
    }

    #[test]
    fn test_search_service_disabled() {
        let service = SearchService::new("http://localhost:9200", false, "test_index");

        assert!(!service.is_enabled());
    }

    #[test]
    fn test_advanced_search_options_default() {
        let options = AdvancedSearchOptions::default();

        assert!(options.query.is_empty());
        assert_eq!(options.limit, 20);
        assert_eq!(options.offset, 0);
        assert!(options.highlight);
        assert!(options.fuzzy);
    }

    #[test]
    fn test_search_filters_default() {
        let filters = SearchFilters::default();

        assert!(filters.sender_id.is_none());
        assert!(filters.room_id.is_none());
        assert!(filters.message_type.is_none());
        assert!(filters.start_ts.is_none());
        assert!(filters.end_ts.is_none());
        assert!(filters.has_media.is_none());
    }

    #[test]
    fn test_extract_keys_basic() {
        let content = "Hello World Test Message";
        let keys = SearchService::extract_keys(content);

        assert_eq!(keys.len(), 4);
        assert_eq!(keys[0], "hello");
        assert_eq!(keys[1], "world");
        assert_eq!(keys[2], "test");
        assert_eq!(keys[3], "message");
    }

    #[test]
    fn test_extract_keys_empty_content() {
        let content = "";
        let keys = SearchService::extract_keys(content);

        assert!(keys.is_empty());
    }

    #[test]
    fn test_extract_keys_whitespace_only() {
        let content = "   \t\n   ";
        let keys = SearchService::extract_keys(content);

        assert!(keys.is_empty());
    }

    #[test]
    fn test_extract_keys_truncates_to_ten() {
        let content = "one two three four five six seven eight nine ten eleven twelve";
        let keys = SearchService::extract_keys(content);

        assert_eq!(keys.len(), 10);
    }

    #[test]
    fn test_extract_keys_lowercase() {
        let content = "HELLO World TEST";
        let keys = SearchService::extract_keys(content);

        assert_eq!(keys[0], "hello");
        assert_eq!(keys[1], "world");
        assert_eq!(keys[2], "test");
    }

    #[test]
    fn test_search_result_item_serialization() {
        let item = SearchResultItem {
            event_id: "$event123".to_string(),
            room_id: "!room456:server.com".to_string(),
            sender: "@user:server.com".to_string(),
            content: "Test message".to_string(),
            event_type: "m.room.message".to_string(),
            origin_server_ts: 1234567890,
            highlights: Some(vec!["<em>Test</em>".to_string()]),
            room_name: Some("Test Room".to_string()),
        };

        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("$event123"));
        assert!(json.contains("!room456:server.com"));
        assert!(json.contains("Test message"));

        let deserialized: SearchResultItem = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.event_id, item.event_id);
        assert_eq!(deserialized.room_id, item.room_id);
        assert_eq!(deserialized.sender, item.sender);
    }

    #[test]
    fn test_search_result_serialization() {
        let result = SearchResult {
            results: vec![SearchResultItem {
                event_id: "$event1".to_string(),
                room_id: "!room1:server.com".to_string(),
                sender: "@user:server.com".to_string(),
                content: "Hello".to_string(),
                event_type: "m.room.message".to_string(),
                origin_server_ts: 1000,
                highlights: None,
                room_name: None,
            }],
            total_count: 1,
            next_batch: Some("20".to_string()),
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"total_count\":1"));
        assert!(json.contains("\"next_batch\":\"20\""));

        let deserialized: SearchResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.total_count, 1);
        assert_eq!(deserialized.next_batch, Some("20".to_string()));
        assert_eq!(deserialized.results.len(), 1);
    }

    #[test]
    fn test_indexed_event_creation() {
        let event = IndexedEvent {
            event_id: "$event123".to_string(),
            room_id: "!room456:server.com".to_string(),
            sender: "@user:server.com".to_string(),
            content: "Test message content".to_string(),
            event_type: "m.room.message".to_string(),
            message_type: Some("m.text".to_string()),
            origin_server_ts: 1234567890,
            index_ts: 1234567900,
            keys: vec!["test".to_string(), "message".to_string()],
        };

        assert_eq!(event.event_id, "$event123");
        assert_eq!(event.room_id, "!room456:server.com");
        assert_eq!(event.sender, "@user:server.com");
        assert_eq!(event.content, "Test message content");
        assert_eq!(event.event_type, "m.room.message");
        assert_eq!(event.message_type, Some("m.text".to_string()));
        assert_eq!(event.origin_server_ts, 1234567890);
        assert_eq!(event.index_ts, 1234567900);
        assert_eq!(event.keys.len(), 2);
    }

    #[test]
    fn test_indexed_event_serialization() {
        let event = IndexedEvent {
            event_id: "$event123".to_string(),
            room_id: "!room456:server.com".to_string(),
            sender: "@user:server.com".to_string(),
            content: "Test message".to_string(),
            event_type: "m.room.message".to_string(),
            message_type: None,
            origin_server_ts: 1234567890,
            index_ts: 1234567900,
            keys: vec!["test".to_string()],
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("$event123"));
        assert!(json.contains("!room456:server.com"));

        let deserialized: IndexedEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.event_id, event.event_id);
        assert_eq!(deserialized.message_type, None);
    }

    #[test]
    fn test_search_filters_with_values() {
        let filters = SearchFilters {
            sender_id: Some("@alice:server.com".to_string()),
            room_id: Some("!room123:server.com".to_string()),
            message_type: Some("m.text".to_string()),
            start_ts: Some(1000000),
            end_ts: Some(2000000),
            has_media: Some(true),
        };

        assert_eq!(filters.sender_id, Some("@alice:server.com".to_string()));
        assert_eq!(filters.room_id, Some("!room123:server.com".to_string()));
        assert_eq!(filters.message_type, Some("m.text".to_string()));
        assert_eq!(filters.start_ts, Some(1000000));
        assert_eq!(filters.end_ts, Some(2000000));
        assert_eq!(filters.has_media, Some(true));
    }

    #[test]
    fn test_advanced_search_options_with_filters() {
        let options = AdvancedSearchOptions {
            query: "search query".to_string(),
            filters: SearchFilters { room_id: Some("!room:server.com".to_string()), ..Default::default() },
            limit: 50,
            offset: 10,
            highlight: false,
            fuzzy: false,
        };

        assert_eq!(options.query, "search query");
        assert_eq!(options.filters.room_id, Some("!room:server.com".to_string()));
        assert_eq!(options.limit, 50);
        assert_eq!(options.offset, 10);
        assert!(!options.highlight);
        assert!(!options.fuzzy);
    }

    #[test]
    fn test_search_result_item_without_highlights() {
        let item = SearchResultItem {
            event_id: "$event".to_string(),
            room_id: "!room:server.com".to_string(),
            sender: "@user:server.com".to_string(),
            content: "Message".to_string(),
            event_type: "m.room.message".to_string(),
            origin_server_ts: 0,
            highlights: None,
            room_name: None,
        };

        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("\"highlights\":null"));
    }

    #[test]
    fn test_search_result_empty_results() {
        let result = SearchResult { results: vec![], total_count: 0, next_batch: None };

        assert!(result.results.is_empty());
        assert_eq!(result.total_count, 0);
        assert!(result.next_batch.is_none());
    }

    #[test]
    fn test_postgres_search_cursor_round_trip() {
        let cursor = PostgresSearchCursor {
            rank: 0.75,
            origin_server_ts: 1_700_000_000_000,
            event_id: "$event:example.com".to_string(),
        };
        let encoded = encode_postgres_search_cursor(&cursor);
        assert_eq!(decode_postgres_search_cursor(Some(&encoded)), Some(cursor));
    }

    #[test]
    fn test_postgres_search_cursor_rejects_invalid_value() {
        assert_eq!(decode_postgres_search_cursor(Some("bad-cursor")), None);
        assert_eq!(decode_postgres_search_cursor(Some("")), None);
    }

    #[test]
    fn test_elasticsearch_search_cursor_round_trip() {
        let cursor = ElasticsearchSearchCursor {
            origin_server_ts: 1_700_000_000_000,
            event_id: "$event:example.com".to_string(),
        };
        let encoded = encode_elasticsearch_search_cursor(&cursor);
        assert_eq!(decode_elasticsearch_search_cursor(Some(&encoded)), Some(cursor));
    }

    #[test]
    fn test_elasticsearch_search_cursor_rejects_invalid_value() {
        assert_eq!(decode_elasticsearch_search_cursor(Some("bad-cursor")), None);
        assert_eq!(decode_elasticsearch_search_cursor(Some("")), None);
    }

    #[test]
    fn test_room_events_cursor_round_trip() {
        let cursor =
            RoomEventsCursor { origin_server_ts: 1_746_700_000_000, event_id: "$event:example.com".to_string() };
        let encoded = encode_room_events_cursor(&cursor);
        assert_eq!(decode_room_events_cursor(Some(&encoded)), Some(cursor));
    }

    #[test]
    fn test_room_events_cursor_rejects_invalid_values() {
        assert_eq!(decode_room_events_cursor(Some("bad")), None);
        assert_eq!(decode_room_events_cursor(Some("123|")), None);
    }
}
