use super::*;
use crate::delayed_events::{CreateDelayedEventRequest, DelayedEvent, DelayedEventStorageApi};

/// In-memory [`DelayedEventStorageApi`] for service-layer unit tests.
///
/// Mirrors the production status transitions: `pending` → `cancelled` /
/// `sent`, with `restart` only resetting the schedule of a still-pending row.
pub struct InMemoryDelayedEventStore {
    events: Arc<RwLock<std::collections::HashMap<i64, DelayedEvent>>>,
    next_id: Arc<RwLock<i64>>,
}

impl Default for InMemoryDelayedEventStore {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryDelayedEventStore {
    /// See [`new`].
    pub fn new() -> Self {
        Self { events: Arc::new(RwLock::new(std::collections::HashMap::new())), next_id: Arc::new(RwLock::new(1)) }
    }

    /// Current status of `delay_id`, for assertions after a transition.
    pub async fn status_of(&self, delay_id: i64) -> Option<String> {
        self.events.read().await.get(&delay_id).map(|e| e.status.clone())
    }

    async fn set_status_if_pending(&self, delay_id: i64, status: &str) -> Result<bool, ApiError> {
        let mut events = self.events.write().await;
        match events.get_mut(&delay_id) {
            Some(event) if event.status == "pending" => {
                event.status = status.to_string();
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}

#[async_trait::async_trait]
impl DelayedEventStorageApi for InMemoryDelayedEventStore {
    async fn create_delayed_event(&self, request: CreateDelayedEventRequest) -> Result<DelayedEvent, ApiError> {
        let mut next_id = self.next_id.write().await;
        let id = *next_id;
        *next_id += 1;
        let now = synapse_common::current_timestamp_millis();
        let event = DelayedEvent {
            id,
            room_id: request.room_id,
            user_id: request.user_id,
            device_id: request.device_id,
            event_id: format!("$delayed{id}"),
            event_type: request.event_type,
            state_key: request.state_key,
            content: request.content,
            delay_ms: request.delay_ms,
            scheduled_ts: now + request.delay_ms,
            created_ts: now,
            status: "pending".to_string(),
            retry_count: 0,
            last_error: None,
        };
        self.events.write().await.insert(id, event.clone());
        Ok(event)
    }

    async fn get_delayed_event(&self, delay_id: i64) -> Result<Option<DelayedEvent>, ApiError> {
        Ok(self.events.read().await.get(&delay_id).cloned())
    }

    async fn list_delayed_events_for_user(&self, user_id: &str) -> Result<Vec<DelayedEvent>, ApiError> {
        let mut events: Vec<DelayedEvent> =
            self.events.read().await.values().filter(|e| e.user_id == user_id).cloned().collect();
        events.sort_by_key(|e| e.id);
        Ok(events)
    }

    async fn restart_delayed_event(&self, delay_id: i64) -> Result<bool, ApiError> {
        let mut events = self.events.write().await;
        match events.get_mut(&delay_id) {
            Some(event) if event.status == "pending" => {
                event.scheduled_ts = synapse_common::current_timestamp_millis() + event.delay_ms;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    async fn cancel_delayed_event(&self, delay_id: i64) -> Result<bool, ApiError> {
        self.set_status_if_pending(delay_id, "cancelled").await
    }

    async fn mark_sent(&self, delay_id: i64) -> Result<bool, ApiError> {
        self.set_status_if_pending(delay_id, "sent").await
    }

    async fn get_due_events(&self, now_ts: i64, limit: i64) -> Result<Vec<DelayedEvent>, ApiError> {
        let mut due: Vec<DelayedEvent> = self
            .events
            .read()
            .await
            .values()
            .filter(|e| e.status == "pending" && e.scheduled_ts <= now_ts)
            .cloned()
            .collect();
        due.sort_by_key(|e| e.id);
        due.truncate(limit.max(0) as usize);
        Ok(due)
    }
}
