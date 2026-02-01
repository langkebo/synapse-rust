use crate::common::*;
use elasticsearch::{
    http::transport::Transport,
    indices::{IndicesCreateParts, IndicesExistsParts},
    Elasticsearch, IndexParts, SearchParts,
};
use serde_json::{json, Value};

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
        let client = self.client.as_ref().unwrap();

        let exists = client
            .indices()
            .exists(IndicesExistsParts::Index(&["private_messages"]))
            .send()
            .await
            .map_err(|e| ApiError::internal(format!("ES error: {}", e)))?;

        if exists.status_code() == http::StatusCode::NOT_FOUND {
            client
                .indices()
                .create(IndicesCreateParts::Index("private_messages"))
                .body(json!({
                    "settings": {
                        "analysis": {
                            "analyzer": {
                                "ik_analyzer": {
                                    "type": "custom",
                                    "tokenizer": "ik_max_word"
                                }
                            }
                        }
                    },
                    "mappings": {
                        "properties": {
                            "message_id": { "type": "keyword" },
                            "session_id": { "type": "keyword" },
                            "sender_id": { "type": "keyword" },
                            "content": {
                                "type": "text",
                                "analyzer": "ik_analyzer",
                                "search_analyzer": "ik_smart"
                            },
                            "created_ts": { "type": "date" }
                        }
                    }
                }))
                .send()
                .await
                .map_err(|e| ApiError::internal(format!("Failed to create ES index: {}", e)))?;
        }

        Ok(())
    }

    pub async fn index_message(
        &self,
        message_id: i64,
        session_id: &str,
        sender_id: &str,
        content: &str,
        created_ts: i64,
    ) -> ApiResult<()> {
        if !self.enabled {
            return Ok(());
        }
        let client = self.client.as_ref().unwrap();

        client
            .index(IndexParts::IndexId(
                "private_messages",
                &message_id.to_string(),
            ))
            .body(json!({
                "message_id": message_id,
                "session_id": session_id,
                "sender_id": sender_id,
                "content": content,
                "created_ts": created_ts
            }))
            .send()
            .await
            .map_err(|e| ApiError::internal(format!("Failed to index message in ES: {}", e)))?;

        Ok(())
    }

    pub async fn search_messages(
        &self,
        _user_id: &str,
        query: &str,
        limit: i64,
    ) -> ApiResult<Vec<Value>> {
        if !self.enabled {
            return Err(ApiError::internal("Elasticsearch is disabled".to_string()));
        }
        let client = self.client.as_ref().unwrap();

        let response = client
            .search(SearchParts::Index(&["private_messages"]))
            .body(json!({
                "query": {
                    "match": {
                        "content": query
                    }
                },
                "size": limit,
                "sort": [
                    { "created_ts": { "order": "desc" } }
                ]
            }))
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
                if let Some(source) = hit["_source"].as_object() {
                    results.push(json!(source));
                }
            }
        }

        Ok(results)
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}
