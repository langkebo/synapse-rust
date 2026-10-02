use super::*;
use crate::relations::{
    AggregationResult, CreateRelationParams, EventRelation, OrderedEventRelation, RelationQueryParams,
    RelationsStoreApi, MSC3981_RECURSION_DEPTH,
};
use sqlx;
use synapse_common::current_timestamp_millis;
// `create_relation_in_tx` signature requires the sqlx transaction type.
// The in-memory mock does not enforce atomicity; tests using the real
// `RelationsStorage` get full transactional semantics.

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
    /// See [`new`].
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
        let now = current_timestamp_millis();
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

    async fn create_relation_in_tx(
        &self,
        params: CreateRelationParams,
        _tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> Result<EventRelation, sqlx::Error> {
        // In-memory mock: no-op transaction, just delegate to create_relation.
        // Real implementation uses the transaction for atomicity.
        self.create_relation(params).await
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

    async fn get_relations(&self, params: RelationQueryParams) -> Result<Vec<OrderedEventRelation>, sqlx::Error> {
        let limit = params.limit.unwrap_or(50).clamp(1, 100) as usize;
        let backward = params.direction.as_deref() == Some("b");
        let cursor = params.from.as_deref().and_then(crate::relations::parse_keyset_cursor);
        let rels = self.relations.read().await;

        // 关系跳数，镜像 SQL 的 `depth` 列：被请求事件的直接关系在 depth 0。
        // 递归沿**所有**类型的关系边下行（过滤只作用于返回集，与
        // `RelationsStorage::get_relations` 及上游 Synapse 一致）。
        let mut depths: std::collections::HashMap<&str, i32> = std::collections::HashMap::new();
        for rel in rels.iter().filter(|r| !r.is_redacted && r.relates_to_event_id == params.relates_to_event_id) {
            depths.insert(rel.event_id.as_str(), 0);
        }
        if params.recurse {
            loop {
                let mut added = false;
                for rel in rels.iter().filter(|r| !r.is_redacted) {
                    let Some(parent_depth) = depths.get(rel.relates_to_event_id.as_str()).copied() else {
                        continue;
                    };
                    if parent_depth > MSC3981_RECURSION_DEPTH || depths.contains_key(rel.event_id.as_str()) {
                        continue;
                    }
                    depths.insert(rel.event_id.as_str(), parent_depth + 1);
                    added = true;
                }
                if !added {
                    break;
                }
            }
        }

        let mut rows: Vec<OrderedEventRelation> = rels
            .iter()
            .filter(|r| !r.is_redacted && depths.contains_key(r.event_id.as_str()))
            .filter(|r| params.relation_type.as_ref().is_none_or(|t| r.relation_type == *t))
            .map(|r| OrderedEventRelation {
                id: r.id,
                room_id: r.room_id.clone(),
                event_id: r.event_id.clone(),
                relates_to_event_id: r.relates_to_event_id.clone(),
                relation_type: r.relation_type.clone(),
                sender: r.sender.clone(),
                origin_server_ts: r.origin_server_ts,
                content: r.content.clone(),
                is_redacted: r.is_redacted,
                created_ts: r.created_ts,
                // 内存 mock 没有 `events` 表：排序键回退到该关系行自己的
                // `origin_server_ts`，与真实查询对孤儿行的回退一致。
                stream_ordering: r.origin_server_ts,
            })
            .collect();

        rows.sort_by(|a, b| {
            a.stream_ordering
                .cmp(&b.stream_ordering)
                .then_with(|| a.event_id.cmp(&b.event_id))
                .then_with(|| a.relation_type.cmp(&b.relation_type))
        });
        if backward {
            rows.reverse();
        }
        if let Some((key, event_id)) = cursor {
            rows.retain(|r| {
                if backward {
                    (r.stream_ordering, r.event_id.as_str()) < (key, event_id.as_str())
                } else {
                    (r.stream_ordering, r.event_id.as_str()) > (key, event_id.as_str())
                }
            });
        }

        Ok(rows.into_iter().take(limit).collect())
    }

    async fn count_relations(
        &self,
        room_id: &str,
        relates_to_event_id: &str,
        relation_type: Option<&str>,
        // 内存实现只存关系行、没有 `events` 表，无法按被关联事件的类型过滤
        // (`/relations/.../{eventType}`)；与 `get_relations` 的 mock 保持同一限制。
        _event_type: Option<&str>,
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
