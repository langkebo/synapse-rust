use super::*;

#[derive(Clone, Default)]
pub struct InMemoryAuditEventStore {
    events: Arc<tokio::sync::RwLock<HashMap<String, AuditEvent>>>,
}

impl InMemoryAuditEventStore {
    pub fn new() -> Self {
        Self { events: Arc::new(tokio::sync::RwLock::new(HashMap::new())) }
    }
}

#[async_trait::async_trait]
impl AuditEventStoreApi for InMemoryAuditEventStore {
    async fn create_event(
        &self,
        event_id: &str,
        created_ts: i64,
        request: &CreateAuditEventRequest,
    ) -> Result<AuditEvent, sqlx::Error> {
        let event = AuditEvent {
            event_id: event_id.to_string(),
            actor_id: request.actor_id.clone(),
            action: request.action.clone(),
            resource_type: request.resource_type.clone(),
            resource_id: request.resource_id.clone(),
            result: request.result.clone(),
            request_id: request.request_id.clone(),
            details: request.details.clone().unwrap_or(serde_json::json!({})),
            created_ts,
        };
        self.events.write().await.insert(event_id.to_string(), event.clone());
        Ok(event)
    }

    async fn get_event(&self, event_id: &str) -> Result<Option<AuditEvent>, sqlx::Error> {
        Ok(self.events.read().await.get(event_id).cloned())
    }

    async fn list_events(
        &self,
        filters: &AuditEventFilters,
    ) -> Result<(Vec<AuditEvent>, i64, Option<String>), sqlx::Error> {
        let events = self.events.read().await;
        let mut results: Vec<AuditEvent> = events.values().cloned().collect();

        if let Some(ref actor_id) = filters.actor_id {
            results.retain(|e| e.actor_id == *actor_id);
        }
        if let Some(ref action) = filters.action {
            results.retain(|e| e.action == *action);
        }
        if let Some(ref resource_type) = filters.resource_type {
            results.retain(|e| e.resource_type == *resource_type);
        }
        if let Some(ref resource_id) = filters.resource_id {
            results.retain(|e| e.resource_id == *resource_id);
        }
        if let Some(ref result) = filters.result {
            results.retain(|e| e.result == *result);
        }

        results.sort_by(|a, b| b.created_ts.cmp(&a.created_ts).then_with(|| b.event_id.cmp(&a.event_id)));

        if let Some(ref cursor) = filters.from {
            results.retain(|e| (e.created_ts, e.event_id.as_str()) < (cursor.created_ts, cursor.event_id.as_str()));
        }

        let total = results.len() as i64;
        let next_batch = if results.len() > filters.limit as usize {
            results.get(filters.limit as usize).map(|event| {
                encode_audit_event_cursor(&AuditEventCursor {
                    created_ts: event.created_ts,
                    event_id: event.event_id.clone(),
                })
            })
        } else {
            None
        };

        results.truncate(filters.limit as usize);
        Ok((results, total, next_batch))
    }

    async fn delete_events_before(&self, cutoff_ts: i64) -> Result<u64, sqlx::Error> {
        let mut events = self.events.write().await;
        let before = events.len() as u64;
        events.retain(|_, e| e.created_ts >= cutoff_ts);
        Ok(before - events.len() as u64)
    }
}
