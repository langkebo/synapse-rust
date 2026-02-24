use crate::common::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

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

#[derive(Clone)]
pub struct SearchService {
    client: reqwest::Client,
    enabled: bool,
    base_url: String,
    index_name: String,
}

impl SearchService {
    pub fn new(url: &str, enabled: bool, index_name: &str) -> Self {
        let base_url = url.trim_end_matches('/').to_string();

        Self {
            client: reqwest::Client::new(),
            enabled,
            base_url,
            index_name: index_name.to_string(),
        }
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
                    ::tracing::info!("Created Elasticsearch index: {}", self.index_name);
                } else if status.as_u16() == 400 {
                    ::tracing::info!("Index {} already exists", self.index_name);
                } else {
                    ::tracing::warn!("Index creation returned status: {}", status);
                }
            }
            Err(e) => {
                ::tracing::warn!("Index creation request failed: {}", e);
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
        let response = self.client.put(&url).json(&doc).send().await
            .map_err(|e| ApiError::internal(format!("Failed to index event: {}", e)))?;

        if !response.status().is_success() {
            return Err(ApiError::internal("Failed to index event".to_string()));
        }

        Ok(())
    }

    pub async fn bulk_index(&self, events: &[IndexedEvent]) -> ApiResult<()> {
        if !self.enabled || events.is_empty() {
            return Ok(());
        }

        let mut body = String::new();
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
        let response = self.client.post(&url)
            .header("Content-Type", "application/x-ndjson")
            .body(body)
            .send()
            .await
            .map_err(|e| ApiError::internal(format!("Failed to bulk index: {}", e)))?;

        if !response.status().is_success() {
            ::tracing::warn!("Bulk index returned non-success status");
        }

        Ok(())
    }

    pub async fn delete_event(&self, event_id: &str) -> ApiResult<()> {
        if !self.enabled {
            return Ok(());
        }

        let url = format!("{}/{}/_doc/{}", self.base_url, self.index_name, event_id);
        let response = self.client.delete(&url).send().await
            .map_err(|e| ApiError::internal(format!("Failed to delete event: {}", e)))?;

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
        _user_id: &str,
        query: &str,
        limit: i64,
        next_batch: Option<&str>,
    ) -> ApiResult<SearchResult> {
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

        let from = next_batch.and_then(|nb| nb.parse::<usize>().ok()).unwrap_or(options.offset as usize);

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
            "from": from,
            "size": options.limit,
            "sort": [{ "origin_server_ts": { "order": "desc" } }]
        });

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
        let response = self.client.post(&url).json(&search_body).send().await
            .map_err(|e| ApiError::internal(format!("Search failed: {}", e)))?;

        let response_json: Value = response.json().await.map_err(|e| {
            ApiError::internal(format!("Failed to parse search response: {}", e))
        })?;

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

        let results: Vec<SearchResultItem> = hits_array
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
                    highlights: highlight.and_then(|h| h.get("content")).and_then(|c| c.as_array()).map(|arr| {
                        arr.iter().filter_map(|v| v.as_str().map(String::from)).collect()
                    }),
                    room_name: None,
                }
            })
            .collect();

        let next_batch = if results.len() >= options.limit as usize {
            Some((from + options.limit as usize).to_string())
        } else {
            None
        };

        Ok(SearchResult {
            results,
            total_count,
            next_batch,
        })
    }

    pub async fn delete_room_index(&self, room_id: &str) -> ApiResult<()> {
        if !self.enabled {
            return Ok(());
        }

        let delete_by_query = json!({
            "query": { "term": { "room_id": room_id } }
        });

        let url = format!("{}/{}/_delete_by_query", self.base_url, self.index_name);
        let response = self.client.post(&url).json(&delete_by_query).send().await
            .map_err(|e| ApiError::internal(format!("Failed to delete room index: {}", e)))?;

        if !response.status().is_success() {
            ::tracing::warn!("Delete room index returned non-success status");
        }

        Ok(())
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}
