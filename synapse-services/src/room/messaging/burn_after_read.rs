//! Burn-after-read message processing.

use crate::common::background_job::BackgroundJob;
use crate::common::constants::BURN_AFTER_READ_DELAY_SECS;
use crate::common::error::ApiResult;
use std::time::Duration;

use super::service::MessagingService;

impl MessagingService {
    pub async fn process_read_receipt(
        &self,
        room_id: &str,
        event_id: &str,
        _user_id: &str,
        _custom_delay_secs: Option<u64>,
    ) -> ApiResult<()> {
        let event = match self.event_reader.get_event(event_id).await {
            Ok(Some(e)) => e,
            _ => return Ok(()),
        };

        let content = match event.content.as_object() {
            Some(c) => c,
            None => return Ok(()),
        };

        if !content.contains_key("burn_after_read") {
            return Ok(());
        }

        let queue = match self.task_queue.clone() {
            Some(q) => q,
            None => return Ok(()),
        };

        let delay_secs = content
            .get("burn_after_read_delay_seconds")
            .and_then(|v| v.as_i64())
            .map_or(BURN_AFTER_READ_DELAY_SECS, |v| v as u64);

        let rid = room_id.to_string();
        let eid = event_id.to_string();
        let task_id = format!("burn_after_read:{rid}:{eid}:{delay_secs}");

        ::tracing::info!(
            room_id = %rid,
            event_id = %eid,
            task_id = %task_id,
            delay_secs,
            "Scheduling burn-after-read"
        );

        let handle = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(delay_secs)).await;

            let job = BackgroundJob::RedactEvent {
                event_id: eid.clone(),
                room_id: rid.clone(),
                reason: Some("Burn after read".to_string()),
            };

            match queue.submit(job).await {
                Ok(_) => {
                    ::tracing::info!(room_id = %rid, event_id = %eid, delay_secs, "Submitted redaction job");
                }
                Err(e) => {
                    ::tracing::error!(
                        room_id = %rid,
                        event_id = %eid,
                        delay_secs,
                        error = %e,
                        "Failed to submit redaction job"
                    );
                }
            }
        });

        self.active_tasks.write().await.insert(task_id, handle);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::room::messaging::service::{MessagingService, MessagingServiceConfig};
    use crate::room::summary::RoomSummaryService;
    use std::sync::Arc;
    use synapse_cache::{CacheConfig, CacheManager};
    use synapse_common::task_queue::RedisTaskQueue;
    use synapse_storage::event::RoomEvent;
    use synapse_storage::test_mocks::{
        InMemoryEventStore, InMemoryMemberStore, InMemoryRelationsStore, InMemoryRoomStore,
        InMemoryRoomSummaryStore,
    };

    /// Build a minimal MessagingService with `task_queue=None` and seeded events.
    async fn make_service(seeded: Vec<RoomEvent>) -> MessagingService {
        make_service_with_queue(seeded, None).await
    }

    /// Build a MessagingService with an optional task_queue. When `queue` is
    /// `Some`, the scheduling path is exercised (the spawned task sleeps, so
    /// with `tokio::time::pause()` the submit() call never fires and no real
    /// Redis connection is needed).
    async fn make_service_with_queue(
        seeded: Vec<RoomEvent>,
        queue: Option<Arc<RedisTaskQueue>>,
    ) -> MessagingService {
        let event_store = Arc::new(InMemoryEventStore::new());
        event_store.seed_events(seeded).await;

        let room_summary_service = Arc::new(RoomSummaryService {
            storage: Arc::new(InMemoryRoomSummaryStore::new()),
            event_reader: event_store.clone(),
            member_storage: Some(Arc::new(InMemoryMemberStore::new())),
        });

        let cache = Arc::new(CacheManager::new(&CacheConfig::default()));

        MessagingService::new(MessagingServiceConfig {
            event_reader: event_store.clone(),
            event_writer: event_store,
            room_storage: Arc::new(InMemoryRoomStore::new()),
            member_storage: Arc::new(InMemoryMemberStore::new()),
            server_name: "test.example.com".to_string(),
            beacon_service: None,
            task_queue: queue,
            relations_storage: Arc::new(InMemoryRelationsStore::new()),
            event_broadcaster: None,
            app_service_manager: None,
            key_rotation_manager: None,
            room_summary_service,
            cache,
        })
    }

    fn make_event(event_id: &str, content: serde_json::Value) -> RoomEvent {
        RoomEvent {
            event_id: event_id.to_string(),
            room_id: "!room:ex.com".to_string(),
            user_id: "@alice:ex.com".to_string(),
            event_type: "m.room.message".to_string(),
            content,
            state_key: None,
            depth: 0,
            origin_server_ts: 0,
            processed_ts: 0,
            not_before: 0,
            status: None,
            reference_image: None,
            origin: String::new(),
            stream_ordering: None,
            redacts: None,
        }
    }

    /// Construct a `RedisTaskQueue` whose underlying pool points at a
    /// non-routable Redis URL. The pool object is created lazily — no
    /// connection attempt happens until `submit()` is called. Combined with
    /// `tokio::time::pause()`, the spawned task's `sleep` never completes,
    /// so `submit()` is never invoked and no real Redis is required.
    fn make_fake_queue() -> Arc<RedisTaskQueue> {
        use deadpool_redis::{Config, Runtime};
        let cfg = Config::from_url("redis://127.0.0.1:1");
        let pool = cfg.create_pool(Some(Runtime::Tokio1)).expect("fake pool must construct");
        Arc::new(RedisTaskQueue::from_pool(pool))
    }

    #[tokio::test]
    async fn process_read_receipt_returns_ok_when_event_not_found() {
        let svc = make_service(vec![]).await;
        // event_id does not exist — early return Ok(()).
        svc.process_read_receipt("!room:ex.com", "$missing:ex.com", "@alice:ex.com", None)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn process_read_receipt_returns_ok_when_content_is_not_object() {
        let event = make_event("$e1:ex.com", serde_json::json!(42));
        let svc = make_service(vec![event]).await;
        svc.process_read_receipt("!room:ex.com", "$e1:ex.com", "@alice:ex.com", None)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn process_read_receipt_returns_ok_when_content_lacks_burn_after_read_key() {
        let event = make_event("$e2:ex.com", serde_json::json!({"body": "hi"}));
        let svc = make_service(vec![event]).await;
        svc.process_read_receipt("!room:ex.com", "$e2:ex.com", "@alice:ex.com", None)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn process_read_receipt_returns_ok_when_task_queue_is_none() {
        // Content has burn_after_read but task_queue is None — early return Ok(()).
        let event = make_event("$e3:ex.com", serde_json::json!({"burn_after_read": true}));
        let svc = make_service(vec![event]).await;
        svc.process_read_receipt("!room:ex.com", "$e3:ex.com", "@alice:ex.com", None)
            .await
            .unwrap();
        // active_tasks must remain empty because we never spawn a task.
        assert!(svc.active_tasks.read().await.is_empty(), "no task should be scheduled without a queue");
    }

    #[tokio::test(start_paused = true)]
    async fn process_read_receipt_schedules_burn_when_queue_present_and_uses_default_delay() {
        // Content has burn_after_read but no explicit delay_seconds — exercises
        // the `map_or(BURN_AFTER_READ_DELAY_SECS, ...)` fallback branch. The
        // spawned task's `sleep` is parked indefinitely because of
        // `start_paused = true`, so the fake Redis pool is never contacted.
        let event = make_event(
            "$e4:ex.com",
            serde_json::json!({"burn_after_read": true, "body": "self-destruct"}),
        );
        let svc = make_service_with_queue(vec![event], Some(make_fake_queue())).await;
        svc.process_read_receipt("!room:ex.com", "$e4:ex.com", "@alice:ex.com", None)
            .await
            .unwrap();

        let tasks = svc.active_tasks.read().await;
        assert_eq!(tasks.len(), 1, "exactly one burn task must be scheduled");
        let expected_id = format!("burn_after_read:!room:ex.com:$e4:ex.com:{BURN_AFTER_READ_DELAY_SECS}");
        assert!(tasks.contains_key(&expected_id), "task_id must match expected format; got keys: {:?}", tasks.keys().collect::<Vec<_>>());
    }

    #[tokio::test(start_paused = true)]
    async fn process_read_receipt_uses_custom_delay_seconds_from_content() {
        // Content specifies burn_after_read_delay_seconds=10 — exercises the
        // `content.get("burn_after_read_delay_seconds").and_then(...)` branch
        // producing 10 (not the default).
        let event = make_event(
            "$e5:ex.com",
            serde_json::json!({"burn_after_read": true, "burn_after_read_delay_seconds": 10}),
        );
        let svc = make_service_with_queue(vec![event], Some(make_fake_queue())).await;
        svc.process_read_receipt("!room:ex.com", "$e5:ex.com", "@alice:ex.com", None)
            .await
            .unwrap();

        let tasks = svc.active_tasks.read().await;
        assert_eq!(tasks.len(), 1);
        let expected_id = "burn_after_read:!room:ex.com:$e5:ex.com:10";
        assert!(tasks.contains_key(expected_id), "task_id must encode custom delay=10; got keys: {:?}", tasks.keys().collect::<Vec<_>>());
    }
}
