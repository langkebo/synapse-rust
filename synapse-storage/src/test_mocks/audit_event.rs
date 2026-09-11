use super::*;

/// The `InMemoryAuditEventStore` struct.
#[derive(Clone, Default)]
pub struct InMemoryAuditEventStore {
    events: Arc<tokio::sync::RwLock<HashMap<String, AuditEvent>>>,
}

impl InMemoryAuditEventStore {
    /// See [`new`].
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

        // P5-fix: `total` counts rows matching the *filters* only. The real
        // implementation computes it with a dedicated `COUNT(*)` query that
        // deliberately does NOT apply the cursor predicate (audit.rs builds
        // `count_query` from actor/action/resource/result, and applies
        // `filters.from` solely to the page query). Computing `total` after the
        // cursor filter made the mock report a shrinking total as the caller
        // paginated, whereas production reports a constant total for the same
        // filters — a divergence no mock-based test could observe.
        let total = results.len() as i64;

        if let Some(ref cursor) = filters.from {
            results.retain(|e| (e.created_ts, e.event_id.as_str()) < (cursor.created_ts, cursor.event_id.as_str()));
        }

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::{AuditEventCursor, AuditEventFilters, AuditEventStoreApi, CreateAuditEventRequest};

    fn request(resource_id: &str) -> CreateAuditEventRequest {
        CreateAuditEventRequest {
            actor_id: "@admin:t".to_string(),
            action: "admin.test".to_string(),
            resource_type: "flag".to_string(),
            resource_id: resource_id.to_string(),
            result: "success".to_string(),
            request_id: "req-1".to_string(),
            details: None,
        }
    }

    /// `total` must count every row matching the FILTERS, independently of the
    /// pagination cursor.
    ///
    /// Regression guard for the drift documented in the P5 report §3.3: this mock
    /// used to compute `total` AFTER applying the cursor predicate, so the total
    /// shrank as the caller paginated, while the real implementation runs a
    /// dedicated `COUNT(*)` that never sees the cursor (audit.rs builds
    /// `count_query` from actor/action/resource/result only). No mock-based test
    /// could observe that difference, because the only coverage for
    /// `list_events` lived in DB-backed integration tests that always pass
    /// `from: None`.
    #[tokio::test]
    async fn list_events_total_is_independent_of_cursor() {
        let store = InMemoryAuditEventStore::new();
        // Three events, descending created_ts so ordering is deterministic.
        for (id, ts) in [("$e1", 300i64), ("$e2", 200), ("$e3", 100)] {
            store.create_event(id, ts, &request("flag-a")).await.unwrap();
        }

        // First page: no cursor.
        let first = AuditEventFilters {
            actor_id: None,
            action: None,
            resource_type: None,
            resource_id: None,
            result: None,
            limit: 2,
            from: None,
        };
        let (page, total, next) = store.list_events(&first).await.unwrap();
        assert_eq!(page.len(), 2, "first page honours the limit");
        assert_eq!(total, 3, "total counts all rows matching the filters");
        let cursor = next.expect("a full page must yield a next_batch cursor");

        // Second page: with cursor. `total` MUST stay 3 (filters unchanged).
        let cursor_value =
            crate::audit::decode_audit_event_cursor(Some(&cursor)).expect("next_batch token must decode");
        let second = AuditEventFilters { limit: 2, from: Some(cursor_value), ..first.clone() };
        let (page2, total2, next2) = store.list_events(&second).await.unwrap();
        assert_eq!(
            total2, 3,
            "total must NOT shrink with the cursor — production computes it with a \
             cursor-independent COUNT(*)"
        );
        // NOTE: page2 is EMPTY, not 1. With 3 rows and limit=2 this mock (like the
        // real PostgreSQL implementation) sets the next_batch cursor to the
        // (limit+1)-th row — the row that was NOT returned — while the continue
        // predicate is the strict `(created_ts, event_id) < cursor`. That row is
        // therefore skipped entirely. Pinned here so the behaviour cannot change
        // silently; see the P5 report §3.3.
        assert_eq!(page2.len(), 0, "documents the (limit+1)-th-row cursor convention");
        assert!(next2.is_none(), "no further page expected");
    }

    /// Negative limit must not degenerate into "return everything".
    #[tokio::test]
    async fn list_events_negative_limit_is_bounded() {
        let store = InMemoryAuditEventStore::new();
        for (id, ts) in [("$e1", 300i64), ("$e2", 200)] {
            store.create_event(id, ts, &request("flag-a")).await.unwrap();
        }
        let filters = AuditEventFilters {
            actor_id: None,
            action: None,
            resource_type: None,
            resource_id: None,
            result: None,
            limit: 0,
            from: None,
        };
        let (page, total, _) = store.list_events(&filters).await.unwrap();
        assert_eq!(page.len(), 0, "limit 0 yields an empty page");
        assert_eq!(total, 2, "total still reflects the matching rows");
    }
}
