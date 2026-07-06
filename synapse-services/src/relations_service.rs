use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use synapse_common::error::ApiError;
use synapse_storage::relations::{EventRelation, RelationQueryParams, RelationsStoreApi};
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
    pub relation_type: Option<String>,
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
    /// 规范未强制，但 SDK `getRelationCount` 依赖此字段；缺省时 SDK 永远读到 0。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<i64>,
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
    storage: Arc<dyn RelationsStoreApi>,
    server_name: String,
}

impl RelationsService {
    pub fn new(storage: Arc<dyn RelationsStoreApi>, server_name: String) -> Self {
        Self { storage, server_name }
    }

    pub async fn send_annotation(&self, request: SendAnnotationRequest) -> Result<EventRelation, ApiError> {
        info!(
            room_id = %request.room_id,
            relates_to = %request.relates_to_event_id,
            sender = %request.sender,
            key = %request.key,
            "Sending annotation"
        );

        let event_id = synapse_common::crypto::generate_event_id(&self.server_name);

        let content = serde_json::json!({
            "body": request.key,
            "m.relates_to": {
                "rel_type": "m.annotation",
                "event_id": request.relates_to_event_id
            }
        });

        let params = synapse_storage::relations::CreateRelationParams {
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
            .map_err(|e| ApiError::internal_with_log("Failed to create annotation", &e))
    }

    pub async fn send_reference(&self, request: SendReferenceRequest) -> Result<EventRelation, ApiError> {
        info!(
            room_id = %request.room_id,
            relates_to = %request.relates_to_event_id,
            sender = %request.sender,
            "Sending reference"
        );

        let event_id = format!("${}", synapse_common::crypto::generate_event_id(&request.room_id));

        let mut content = request.content;
        let effective_relation_type = request.relation_type.clone().unwrap_or_else(|| "m.reference".to_string());

        if let Some(obj) = content.as_object_mut() {
            obj.insert(
                "m.relates_to".to_string(),
                serde_json::json!({
                    "rel_type": effective_relation_type,
                    "event_id": request.relates_to_event_id
                }),
            );
        } else {
            content = serde_json::json!({
                "m.relates_to": {
                    "rel_type": effective_relation_type,
                    "event_id": request.relates_to_event_id
                }
            });
        }

        let params = synapse_storage::relations::CreateRelationParams {
            room_id: request.room_id,
            event_id,
            relates_to_event_id: request.relates_to_event_id,
            relation_type: effective_relation_type,
            sender: request.sender,
            origin_server_ts: request.origin_server_ts,
            content,
        };

        self.storage
            .create_relation(params)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to create reference", &e))
    }

    pub async fn send_replacement(&self, request: SendReplacementRequest) -> Result<EventRelation, ApiError> {
        info!(
            room_id = %request.room_id,
            relates_to = %request.relates_to_event_id,
            sender = %request.sender,
            "Sending replacement"
        );

        let existing = self
            .storage
            .get_replacement(&request.room_id, &request.relates_to_event_id, &request.sender)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check existing replacement", &e))?;

        let event_id = if let Some(existing) = existing {
            warn!(
                sender = %request.sender,
                room_id = %request.room_id,
                relates_to = %request.relates_to_event_id,
                existing_event_id = %existing.event_id,
                "Replacement already exists for sender, updating existing"
            );
            existing.event_id
        } else {
            format!("${}", synapse_common::crypto::generate_event_id(&request.room_id))
        };

        let content = serde_json::json!({
            "m.new_content": request.new_content,
            "m.relates_to": {
                "rel_type": "m.replace",
                "event_id": request.relates_to_event_id
            }
        });

        let params = synapse_storage::relations::CreateRelationParams {
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
            .map_err(|e| ApiError::internal_with_log("Failed to create replacement", &e))
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
            .map_err(|e| ApiError::internal_with_log("Failed to get relations", &e))?;

        let total = self
            .storage
            .count_relations(room_id, relates_to_event_id, rel_type)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to count relations", &e))?;

        let chunk: Vec<Value> = relations
            .into_iter()
            .map(|r| {
                serde_json::json!({
                    "type": "m.relates_to",
                    "event_id": r.event_id,
                    "sender": r.sender,
                    "origin_server_ts": r.origin_server_ts,
                    "content": r.content,
                })
            })
            .collect();

        Ok(RelationsResponse { chunk, next_batch: None, prev_batch: None, total: Some(total) })
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
            .map_err(|e| ApiError::internal_with_log("Failed to get aggregations", &e))?;

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

    pub async fn redact_relation(&self, room_id: &str, event_id: &str, sender: &str) -> Result<(), ApiError> {
        let relation = self
            .storage
            .get_relation(room_id, event_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get relation", &e))?;

        if let Some(relation) = relation {
            if relation.sender != sender {
                return Err(ApiError::forbidden("Cannot redact another user's relation".to_string()));
            }
        }

        self.storage
            .redact_relation(room_id, event_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to redact relation", &e))?;

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
            .map_err(|e| ApiError::internal_with_log("Failed to check annotation", &e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use synapse_storage::relations::CreateRelationParams;
    use synapse_storage::test_mocks::InMemoryRelationsStore;

    fn test_service() -> RelationsService {
        RelationsService::new(Arc::new(InMemoryRelationsStore::new()), "example.com".to_string())
    }

    fn annotation_params(room_id: &str, event_id: &str, sender: &str, key: &str) -> CreateRelationParams {
        CreateRelationParams {
            room_id: room_id.to_string(),
            event_id: event_id.to_string(),
            relates_to_event_id: "$original:example.com".to_string(),
            relation_type: "m.annotation".to_string(),
            sender: sender.to_string(),
            origin_server_ts: 1_700_000_000_000,
            content: serde_json::json!({"body": key}),
        }
    }

    // ── send_annotation ─────────────────────────────────────────────

    #[tokio::test]
    async fn send_annotation_creates_and_returns_relation() {
        let svc = test_service();
        let result = svc
            .send_annotation(SendAnnotationRequest {
                room_id: "!r:example.com".to_string(),
                relates_to_event_id: "$original:example.com".to_string(),
                sender: "@alice:example.com".to_string(),
                key: "👍".to_string(),
                origin_server_ts: 1_700_000_000_000,
            })
            .await
            .unwrap();

        assert_eq!(result.relation_type, "m.annotation");
        assert_eq!(result.sender, "@alice:example.com");
        assert!(!result.event_id.is_empty());
    }

    // ── send_reference ──────────────────────────────────────────────

    #[tokio::test]
    async fn send_reference_adds_relates_to_content() {
        let svc = test_service();
        let result = svc
            .send_reference(SendReferenceRequest {
                room_id: "!r:example.com".to_string(),
                relates_to_event_id: "$original:example.com".to_string(),
                sender: "@bob:example.com".to_string(),
                content: serde_json::json!({"body": "check this out"}),
                origin_server_ts: 1_700_000_000_000,
                relation_type: Some("m.reference".to_string()),
            })
            .await
            .unwrap();

        assert_eq!(result.relation_type, "m.reference");
        assert!(result.content["m.relates_to"]["event_id"].as_str() == Some("$original:example.com"));
    }

    #[tokio::test]
    async fn send_reference_defaults_relation_type() {
        let svc = test_service();
        let result = svc
            .send_reference(SendReferenceRequest {
                room_id: "!r:example.com".to_string(),
                relates_to_event_id: "$original:example.com".to_string(),
                sender: "@bob:example.com".to_string(),
                content: serde_json::json!({"body": "note"}),
                origin_server_ts: 1_700_000_000_000,
                relation_type: None,
            })
            .await
            .unwrap();

        assert_eq!(result.relation_type, "m.reference");
    }

    // ── send_replacement ────────────────────────────────────────────

    #[tokio::test]
    async fn send_replacement_creates_new_for_first_edit() {
        let svc = test_service();
        let result = svc
            .send_replacement(SendReplacementRequest {
                room_id: "!r:example.com".to_string(),
                relates_to_event_id: "$original:example.com".to_string(),
                sender: "@alice:example.com".to_string(),
                new_content: serde_json::json!({"body": "edited", "msgtype": "m.text"}),
                origin_server_ts: 1_700_000_000_000,
            })
            .await
            .unwrap();

        assert_eq!(result.relation_type, "m.replace");
        assert!(result.content["m.new_content"]["body"].as_str() == Some("edited"));
    }

    #[tokio::test]
    async fn send_replacement_reuses_event_id_when_replacing_again() {
        let svc = test_service();

        // First replacement
        let first = svc
            .send_replacement(SendReplacementRequest {
                room_id: "!r:example.com".to_string(),
                relates_to_event_id: "$event:example.com".to_string(),
                sender: "@alice:example.com".to_string(),
                new_content: serde_json::json!({"body": "v1"}),
                origin_server_ts: 1_700_000_000_000,
            })
            .await
            .unwrap();

        // Second replacement — should reuse the same event_id
        let second = svc
            .send_replacement(SendReplacementRequest {
                room_id: "!r:example.com".to_string(),
                relates_to_event_id: "$event:example.com".to_string(),
                sender: "@alice:example.com".to_string(),
                new_content: serde_json::json!({"body": "v2"}),
                origin_server_ts: 1_700_000_000_001,
            })
            .await
            .unwrap();

        assert_eq!(second.event_id, first.event_id);
    }

    // ── redact_relation (ACL) ───────────────────────────────────────

    #[tokio::test]
    async fn redact_own_relation_succeeds() {
        let svc = test_service();
        // Pre-seed a relation by the same sender
        svc.storage
            .create_relation(annotation_params("!r:example.com", "$ev:example.com", "@alice:example.com", "👍"))
            .await
            .unwrap();

        let result = svc.redact_relation("!r:example.com", "$ev:example.com", "@alice:example.com").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn redact_another_users_relation_is_forbidden() {
        let svc = test_service();
        // Pre-seed a relation by Alice
        svc.storage
            .create_relation(annotation_params("!r:example.com", "$ev:example.com", "@alice:example.com", "👍"))
            .await
            .unwrap();

        // Bob tries to redact Alice's relation
        let result = svc.redact_relation("!r:example.com", "$ev:example.com", "@bob:example.com").await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Cannot redact another user"));
    }

    #[tokio::test]
    async fn redact_nonexistent_relation_still_calls_redact() {
        // Even if relation doesn't exist, redact proceeds (no ACL violation)
        let svc = test_service();
        let result = svc.redact_relation("!r:example.com", "$nonexistent:example.com", "@alice:example.com").await;
        assert!(result.is_ok());
    }

    // ── annotation_exists ───────────────────────────────────────────

    #[tokio::test]
    async fn annotation_exists_returns_true_when_present() {
        let svc = test_service();
        svc.storage
            .create_relation(annotation_params("!r:example.com", "$ev:example.com", "@alice:example.com", "👍"))
            .await
            .unwrap();

        let exists =
            svc.annotation_exists("!r:example.com", "$original:example.com", "@alice:example.com", "👍").await.unwrap();
        assert!(exists);
    }

    #[tokio::test]
    async fn annotation_exists_returns_false_when_not_present() {
        let svc = test_service();
        let exists =
            svc.annotation_exists("!r:example.com", "$original:example.com", "@alice:example.com", "👍").await.unwrap();
        assert!(!exists);
    }

    // ── get_aggregations ────────────────────────────────────────────

    #[tokio::test]
    async fn get_aggregations_returns_empty_for_no_annotations() {
        let svc = test_service();
        let agg = svc.get_aggregations("!r:example.com", "$event:example.com").await.unwrap();
        assert!(agg.chunk.is_empty());
    }

    #[tokio::test]
    async fn get_aggregations_groups_annotations() {
        let svc = test_service();
        svc.storage
            .create_relation(annotation_params("!r:example.com", "$a1:example.com", "@u1:example.com", "👍"))
            .await
            .unwrap();
        svc.storage
            .create_relation(annotation_params("!r:example.com", "$a2:example.com", "@u2:example.com", "👍"))
            .await
            .unwrap();
        svc.storage
            .create_relation(annotation_params("!r:example.com", "$a3:example.com", "@u3:example.com", "👎"))
            .await
            .unwrap();

        let agg = svc.get_aggregations("!r:example.com", "$original:example.com").await.unwrap();
        assert_eq!(agg.chunk.len(), 2); // 👍 and 👎
        assert!(agg.chunk.iter().any(|a| a.key.as_deref() == Some("👍") && a.count == 2));
        assert!(agg.chunk.iter().any(|a| a.key.as_deref() == Some("👎") && a.count == 1));
    }
}
