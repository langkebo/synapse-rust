use super::*;
use crate::relations::{
    AggregationResult, CreateRelationParams, EventRelation, RelationQueryParams, RelationsStoreApi,
};

/// In-memory relations store for testing [`RelationsService`].
///
/// Stores relations in a `Vec<EventRelation>` behind a `RwLock` with
/// auto-incrementing IDs.
#[derive(Clone, Default)]
pub struct InMemoryRelationsStore {
    relations: Arc<tokio::sync::RwLock<Vec<EventRelation>>>,
    next_id: Arc<tokio::sync::RwLock<i64>>,
}

impl InMemoryRelationsStore {
    pub fn new() -> Self {
        Self {
            relations: Arc::new(tokio::sync::RwLock::new(Vec::new())),
            next_id: Arc::new(tokio::sync::RwLock::new(1)),
        }
    }
}

#[async_trait::async_trait]
impl RelationsStoreApi for InMemoryRelationsStore {
    async fn create_relation(&self, params: CreateRelationParams) -> Result<EventRelation, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let mut next = self.next_id.write().await;
        let id = *next;
        *next += 1;

        // Upsert: replace existing (event_id, relation_type, sender) match
        let mut relations = self.relations.write().await;
        if let Some(existing) = relations.iter_mut().find(|r| {
            r.event_id == params.event_id && r.relation_type == params.relation_type && r.sender == params.sender
        }) {
            existing.content = params.content;
            existing.origin_server_ts = params.origin_server_ts;
            existing.is_redacted = false;
            return Ok(existing.clone());
        }

        let relation = EventRelation {
            id,
            room_id: params.room_id,
            event_id: params.event_id,
            relates_to_event_id: params.relates_to_event_id,
            relation_type: params.relation_type,
            sender: params.sender,
            origin_server_ts: params.origin_server_ts,
            content: params.content,
            is_redacted: false,
            created_ts: now,
        };
        relations.push(relation.clone());
        Ok(relation)
    }

    async fn get_relation(&self, room_id: &str, event_id: &str) -> Result<Option<EventRelation>, sqlx::Error> {
        Ok(self
            .relations
            .read()
            .await
            .iter()
            .find(|r| r.room_id == room_id && r.event_id == event_id && !r.is_redacted)
            .cloned())
    }

    async fn get_relations(&self, params: RelationQueryParams) -> Result<Vec<EventRelation>, sqlx::Error> {
        let limit = params.limit.unwrap_or(50).clamp(1, 100) as usize;
        let rels = self.relations.read().await;
        let mut filtered: Vec<&EventRelation> = rels
            .iter()
            .filter(|r| {
                r.room_id == params.room_id
                    && r.relates_to_event_id == params.relates_to_event_id
                    && params.relation_type.as_ref().is_none_or(|t| r.relation_type == *t)
                    && !r.is_redacted
            })
            .collect();

        let direction = params.direction.as_deref().unwrap_or("f");
        match direction {
            "b" => {
                filtered.sort_by(|a, b| {
                    b.origin_server_ts.cmp(&a.origin_server_ts).then_with(|| b.event_id.cmp(&a.event_id))
                });
                if let Some(ref from) = params.from {
                    if let Some(pos) = filtered.iter().position(|r| r.event_id == *from) {
                        filtered = filtered.into_iter().skip(pos + 1).collect();
                    }
                }
            }
            _ => {
                filtered.sort_by(|a, b| {
                    a.origin_server_ts.cmp(&b.origin_server_ts).then_with(|| a.event_id.cmp(&b.event_id))
                });
                if let Some(ref from) = params.from {
                    if let Some(pos) = filtered.iter().position(|r| r.event_id == *from) {
                        filtered = filtered.into_iter().skip(pos + 1).collect();
                    }
                }
            }
        }

        Ok(filtered.into_iter().take(limit).cloned().collect())
    }

    async fn count_relations(
        &self,
        room_id: &str,
        relates_to_event_id: &str,
        relation_type: Option<&str>,
    ) -> Result<i64, sqlx::Error> {
        let count = self
            .relations
            .read()
            .await
            .iter()
            .filter(|r| {
                r.room_id == room_id
                    && r.relates_to_event_id == relates_to_event_id
                    && relation_type.is_none_or(|t| r.relation_type == t)
                    && !r.is_redacted
            })
            .count();
        Ok(count as i64)
    }

    async fn get_replacement(
        &self,
        room_id: &str,
        relates_to_event_id: &str,
        sender: &str,
    ) -> Result<Option<EventRelation>, sqlx::Error> {
        Ok(self
            .relations
            .read()
            .await
            .iter()
            .filter(|r| {
                r.room_id == room_id
                    && r.relates_to_event_id == relates_to_event_id
                    && r.relation_type == "m.replace"
                    && r.sender == sender
                    && !r.is_redacted
            })
            .max_by_key(|r| r.origin_server_ts)
            .cloned())
    }

    async fn aggregate_annotations(
        &self,
        room_id: &str,
        relates_to_event_id: &str,
    ) -> Result<Vec<AggregationResult>, sqlx::Error> {
        use std::collections::HashMap;
        let rels = self.relations.read().await;
        let mut map: HashMap<String, (i64, Option<String>)> = HashMap::new();
        for r in rels.iter() {
            if r.room_id == room_id
                && r.relates_to_event_id == relates_to_event_id
                && r.relation_type == "m.annotation"
                && !r.is_redacted
            {
                let key = r.content.get("body").and_then(|v| v.as_str()).map(|s| s.to_string());
                let entry = map.entry(key.clone().unwrap_or_default()).or_insert((0, None));
                entry.0 += 1;
                entry.1 = key.clone();
            }
        }
        let mut results: Vec<AggregationResult> = map
            .into_iter()
            .map(|(_, (count, key))| AggregationResult {
                relation_type: "m.annotation".to_string(),
                key,
                count,
                sender: None,
            })
            .collect();
        results.sort_by(|a, b| b.count.cmp(&a.count));
        Ok(results)
    }

    async fn redact_relation(&self, room_id: &str, event_id: &str) -> Result<(), sqlx::Error> {
        if let Some(r) =
            self.relations.write().await.iter_mut().find(|r| r.room_id == room_id && r.event_id == event_id)
        {
            r.is_redacted = true;
            r.content = serde_json::json!({});
        }
        Ok(())
    }

    async fn relation_exists(
        &self,
        room_id: &str,
        relates_to_event_id: &str,
        relation_type: &str,
        sender: &str,
    ) -> Result<bool, sqlx::Error> {
        Ok(self.relations.read().await.iter().any(|r| {
            r.room_id == room_id
                && r.relates_to_event_id == relates_to_event_id
                && r.relation_type == relation_type
                && r.sender == sender
                && !r.is_redacted
        }))
    }
}
