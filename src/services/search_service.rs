use crate::common::*;
use elasticsearch::{
    http::transport::Transport,
    Elasticsearch, SearchParts,
};
use serde_json::{json, Value};

#[derive(Debug, Clone, Default)]
pub struct SearchFilters {
    pub sender_id: Option<String>,
    pub session_id: Option<String>,
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

#[derive(Clone)]
pub struct SearchService {
    client: Option<Elasticsearch>,
    enabled: bool,
}

impl SearchService {
    pub fn new(url: &str, enabled: bool) -> Self {
        if !enabled {
            return Self {
                client: None,
                enabled: false,
            };
        }

        match Transport::single_node(url) {
            Ok(transport) => Self {
                client: Some(Elasticsearch::new(transport)),
                enabled: true,
            },
            Err(e) => {
                ::tracing::error!("Failed to initialize Elasticsearch: {}", e);
                Self {
                    client: None,
                    enabled: false,
                }
            }
        }
    }

    pub async fn init_indices(&self) -> ApiResult<()> {
        if !self.enabled {
            return Ok(());
        }
        // Elasticsearch indices are now managed through the events table
        // The old private_messages index has been deprecated in favor of
        // standard Matrix room messages stored in the events table
        Ok(())
    }

    pub async fn index_message(
        &self,
        _message_id: i64,
        _session_id: &str,
        _sender_id: &str,
        _content: &str,
        _created_ts: i64,
    ) -> ApiResult<()> {
        // Deprecated: This function was for the old private_messages table
        // Messages are now stored in the events table as part of standard Matrix rooms
        // Use the event indexing mechanism instead
        if !self.enabled {
            return Ok(());
        }
        // Stub implementation - returns success without doing anything
        Ok(())
    }

    pub async fn search_messages(
        &self,
        user_id: &str,
        query: &str,
        limit: i64,
    ) -> ApiResult<Vec<Value>> {
        let options = AdvancedSearchOptions {
            query: query.to_string(),
            filters: SearchFilters {
                sender_id: None,
                session_id: None,
                message_type: None,
                start_ts: None,
                end_ts: None,
                has_media: None,
            },
            limit,
            offset: 0,
            highlight: false,
            fuzzy: true,
        };
        self.advanced_search(user_id, &options).await
    }

    pub async fn advanced_search(
        &self,
        _user_id: &str,
        options: &AdvancedSearchOptions,
    ) -> ApiResult<Vec<Value>> {
        if !self.enabled {
            return Err(ApiError::internal("Elasticsearch is disabled".to_string()));
        }
        let client = self.client.as_ref()
            .ok_or_else(|| ApiError::internal("Elasticsearch client not initialized".to_string()))?;

        let mut must_clauses = Vec::new();
        let mut filter_clauses = Vec::new();

        must_clauses.push(json!({
            "match": {
                "content": {
                    "query": options.query,
                    "fuzziness": if options.fuzzy { "AUTO" } else { "0" }
                }
            }
        }));

        if let Some(ref sender_id) = options.filters.sender_id {
            filter_clauses.push(json!({
                "term": { "sender_id": sender_id }
            }));
        }

        if let Some(ref session_id) = options.filters.session_id {
            filter_clauses.push(json!({
                "term": { "session_id": session_id }
            }));
        }

        if let Some(ref message_type) = options.filters.message_type {
            filter_clauses.push(json!({
                "term": { "message_type": message_type }
            }));
        }

        if let Some(start_ts) = options.filters.start_ts {
            filter_clauses.push(json!({
                "range": { "created_ts": { "gte": start_ts } }
            }));
        }

        if let Some(end_ts) = options.filters.end_ts {
            filter_clauses.push(json!({
                "range": { "created_ts": { "lte": end_ts } }
            }));
        }

        let query_builder = json!({
            "bool": {
                "must": must_clauses,
                "filter": filter_clauses
            }
        });

        let mut search_body = json!({
            "query": query_builder,
            "from": options.offset,
            "size": options.limit,
            "sort": [
                { "created_ts": { "order": "desc" } }
            ]
        });

        if options.highlight {
            search_body["highlight"] = json!({
                "fields": {
                    "content": {
                        "pre_tags": ["<em>"],
                        "post_tags": ["</em>"]
                    }
                }
            });
        }

        let response = client
            .search(SearchParts::Index(&["events"]))
            .body(search_body)
            .send()
            .await
            .map_err(|e| ApiError::internal(format!("ES search error: {}", e)))?;

        let body: Value = response
            .json()
            .await
            .map_err(|e| ApiError::internal(format!("Failed to parse ES response: {}", e)))?;

        let mut results = Vec::new();
        if let Some(hits) = body["hits"]["hits"].as_array() {
            for hit in hits {
                let mut source = json!({});
                if let Some(source_obj) = hit["_source"].as_object() {
                    for (key, value) in source_obj.iter() {
                        source[key] = value.clone();
                    }
                }

                if options.highlight {
                    if let Some(highlight) = hit["highlight"].as_object() {
                        if let Some(content) = highlight.get("content") {
                            source["highlighted_content"] = content.clone();
                        }
                    }
                }

                source["score"] = hit["_score"].clone();
                results.push(source);
            }
        }

        Ok(results)
    }

    pub async fn search_users(&self, search_term: &str, limit: i64) -> ApiResult<Vec<Value>> {
        if !self.enabled {
            return Err(ApiError::internal("Elasticsearch is disabled".to_string()));
        }
        let client = self.client.as_ref()
            .ok_or_else(|| ApiError::internal("Elasticsearch client not initialized".to_string()))?;

        let response = client
            .search(SearchParts::Index(&["users"]))
            .body(json!({
                "query": {
                    "multi_match": {
                        "query": search_term,
                        "fields": ["username^2", "displayname", "user_id"],
                        "type": "best_fields",
                        "fuzziness": "AUTO"
                    }
                },
                "size": limit
            }))
            .send()
            .await
            .map_err(|e| ApiError::internal(format!("ES user search error: {}", e)))?;

        let body: Value = response
            .json()
            .await
            .map_err(|e| ApiError::internal(format!("Failed to parse ES response: {}", e)))?;

        let mut results = Vec::new();
        if let Some(hits) = body["hits"]["hits"].as_array() {
            for hit in hits {
                if let Some(source) = hit["_source"].as_object() {
                    results.push(json!(source));
                }
            }
        }

        Ok(results)
    }

    pub async fn suggest_completions(
        &self,
        field: &str,
        prefix: &str,
        limit: i64,
    ) -> ApiResult<Vec<Value>> {
        if !self.enabled {
            return Err(ApiError::internal("Elasticsearch is disabled".to_string()));
        }
        let client = self.client.as_ref()
            .ok_or_else(|| ApiError::internal("Elasticsearch client not initialized".to_string()))?;

        let response = client
            .search(SearchParts::Index(&["users"]))
            .body(json!({
                "query": {
                    "match_phrase_prefix": {
                        field: {
                            "query": prefix,
                            "max_expansions": 10
                        }
                    }
                },
                "size": limit,
                "_source": [field]
            }))
            .send()
            .await
            .map_err(|e| ApiError::internal(format!("ES suggest error: {}", e)))?;

        let body: Value = response
            .json()
            .await
            .map_err(|e| ApiError::internal(format!("Failed to parse ES response: {}", e)))?;

        let mut suggestions = Vec::new();
        if let Some(hits) = body["hits"]["hits"].as_array() {
            for hit in hits {
                if let Some(source) = hit["_source"].as_object() {
                    if let Some(value) = source.get(field) {
                        suggestions.push(value.clone());
                    }
                }
            }
        }

        Ok(suggestions)
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}
