use crate::common::error::ApiError;
use crate::storage::relations::{EventRelation, RelationQueryParams, RelationsStorage};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tracing::{debug, info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendAnnotationRequest {
    pub room_id: String,
    pub relates_to_event_id: String,
    pub sender: String,
    pub key: String,
    pub origin_server_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendReferenceRequest {
    pub room_id: String,
    pub relates_to_event_id: String,
    pub sender: String,
    pub content: Value,
    pub origin_server_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendReplacementRequest {
    pub room_id: String,
    pub relates_to_event_id: String,
    pub sender: String,
    pub new_content: Value,
    pub origin_server_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationsResponse {
    pub chunk: Vec<Value>,
    pub next_batch: Option<String>,
    pub prev_batch: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregationResponse {
    pub chunk: Vec<AggregationItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregationItem {
    #[serde(rename = "type")]
    pub event_type: String,
    pub key: Option<String>,
    pub count: i64,
    pub sender: Option<String>,
    pub origin_server_ts: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationSendResponse {
    pub event_id: String,
    pub room_id: String,
    pub relates_to: RelationTarget,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationTarget {
    pub event_id: String,
    pub rel_type: String,
}

#[derive(Clone)]
pub struct RelationsService {
    storage: Arc<RelationsStorage>,
}

impl RelationsService {
    pub fn new(storage: Arc<RelationsStorage>) -> Self {
        Self { storage }
    }

    pub async fn send_annotation(
        &self,
        request: SendAnnotationRequest,
    ) -> Result<EventRelation, ApiError> {
        info!(
            room_id = %request.room_id,
            relates_to = %request.relates_to_event_id,
            sender = %request.sender,
            key = %request.key,
            "Sending annotation"
        );

        let event_id = format!(
            "${}",
            crate::common::crypto::generate_event_id(&request.room_id)
        );

        let content = serde_json::json!({
            "body": request.key,
            "m.relates_to": {
                "rel_type": "m.annotation",
                "event_id": request.relates_to_event_id
            }
        });

        let params = crate::storage::relations::CreateRelationParams {
            room_id: request.room_id,
            event_id,
            relates_to_event_id: request.relates_to_event_id,
            relation_type: "m.annotation".to_string(),
            sender: request.sender,
            origin_server_ts: request.origin_server_ts,
            content,
        };

        self.storage
            .create_relation(params)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to create annotation: {}", e)))
    }

    pub async fn send_reference(
        &self,
        request: SendReferenceRequest,
    ) -> Result<EventRelation, ApiError> {
        info!(
            room_id = %request.room_id,
            relates_to = %request.relates_to_event_id,
            sender = %request.sender,
            "Sending reference"
        );

        let event_id = format!(
            "${}",
            crate::common::crypto::generate_event_id(&request.room_id)
        );

        let mut content = request.content;
        if let Some(obj) = content.as_object_mut() {
            obj.insert(
                "m.relates_to".to_string(),
                serde_json::json!({
                    "rel_type": "m.reference",
                    "event_id": request.relates_to_event_id
                }),
            );
        } else {
            content = serde_json::json!({
                "m.relates_to": {
                    "rel_type": "m.reference",
                    "event_id": request.relates_to_event_id
                }
            });
        }

        let params = crate::storage::relations::CreateRelationParams {
            room_id: request.room_id,
            event_id,
            relates_to_event_id: request.relates_to_event_id,
            relation_type: "m.reference".to_string(),
            sender: request.sender,
            origin_server_ts: request.origin_server_ts,
            content,
        };

        self.storage
            .create_relation(params)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to create reference: {}", e)))
    }

    pub async fn send_replacement(
        &self,
        request: SendReplacementRequest,
    ) -> Result<EventRelation, ApiError> {
        info!(
            room_id = %request.room_id,
            relates_to = %request.relates_to_event_id,
            sender = %request.sender,
            "Sending replacement"
        );

        let existing = self
            .storage
            .get_replacement(
                &request.room_id,
                &request.relates_to_event_id,
                &request.sender,
            )
            .await
            .map_err(|e| {
                ApiError::internal(format!("Failed to check existing replacement: {}", e))
            })?;

        let event_id = if let Some(existing) = existing {
            warn!(
                "Replacement already exists for sender {}, updating existing",
                request.sender
            );
            existing.event_id
        } else {
            format!(
                "${}",
                crate::common::crypto::generate_event_id(&request.room_id)
            )
        };

        let content = serde_json::json!({
            "m.new_content": request.new_content,
            "m.relates_to": {
                "rel_type": "m.replace",
                "event_id": request.relates_to_event_id
            }
        });

        let params = crate::storage::relations::CreateRelationParams {
            room_id: request.room_id,
            event_id,
            relates_to_event_id: request.relates_to_event_id,
            relation_type: "m.replace".to_string(),
            sender: request.sender,
            origin_server_ts: request.origin_server_ts,
            content,
        };

        self.storage
            .create_relation(params)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to create replacement: {}", e)))
    }

    pub async fn get_relations(
        &self,
        room_id: &str,
        relates_to_event_id: &str,
        rel_type: Option<&str>,
        limit: Option<i32>,
        from: Option<String>,
        direction: Option<String>,
    ) -> Result<RelationsResponse, ApiError> {
        debug!(
            room_id = %room_id,
            relates_to = %relates_to_event_id,
            rel_type = ?rel_type,
            "Getting relations"
        );

        let params = RelationQueryParams {
            room_id: room_id.to_string(),
            relates_to_event_id: relates_to_event_id.to_string(),
            relation_type: rel_type.map(String::from),
            limit,
            from,
            direction,
        };

        let relations = self
            .storage
            .get_relations(params)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get relations: {}", e)))?;

        let chunk: Vec<Value> = relations
            .into_iter()
            .map(|r| {
                serde_json::json!({
                    "type": "m.relates_to",
                    "event_id": r.event_id,
                    "sender": r.sender,
                    "origin_server_ts": r.origin_server_ts,
                    "content": r.content,
                    "m.relates_to": {
                        "rel_type": r.relation_type,
                        "event_id": r.relates_to_event_id
                    }
                })
            })
            .collect();

        Ok(RelationsResponse {
            chunk,
            next_batch: None,
            prev_batch: None,
        })
    }

    pub async fn get_aggregations(
        &self,
        room_id: &str,
        relates_to_event_id: &str,
    ) -> Result<AggregationResponse, ApiError> {
        debug!(
            room_id = %room_id,
            relates_to = %relates_to_event_id,
            "Getting aggregations"
        );

        let aggregations = self
            .storage
            .aggregate_annotations(room_id, relates_to_event_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get aggregations: {}", e)))?;

        let chunk: Vec<AggregationItem> = aggregations
            .into_iter()
            .map(|agg| AggregationItem {
                event_type: "m.annotation".to_string(),
                key: agg.key,
                count: agg.count,
                sender: agg.sender,
                origin_server_ts: None,
            })
            .collect();

        Ok(AggregationResponse { chunk })
    }

    pub async fn redact_relation(
        &self,
        room_id: &str,
        event_id: &str,
        sender: &str,
    ) -> Result<(), ApiError> {
        let relation = self
            .storage
            .get_relation(room_id, event_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get relation: {}", e)))?;

        if let Some(relation) = relation {
            if relation.sender != sender {
                return Err(ApiError::forbidden(
                    "Cannot redact another user's relation".to_string(),
                ));
            }
        }

        self.storage
            .redact_relation(room_id, event_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to redact relation: {}", e)))?;

        Ok(())
    }

    pub async fn annotation_exists(
        &self,
        room_id: &str,
        relates_to_event_id: &str,
        sender: &str,
        _key: &str,
    ) -> Result<bool, ApiError> {
        self.storage
            .relation_exists(room_id, relates_to_event_id, "m.annotation", sender)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to check annotation: {}", e)))
    }
}
