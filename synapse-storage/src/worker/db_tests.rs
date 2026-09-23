use super::*;
use sqlx::PgPool;
use std::sync::Arc;

/// 每个测试一个从迁移 baseline 克隆出来的独立 schema（返回 guard 与 pool）。
///
/// 2026-09-21：原先用共享 `public` 池。共享池的问题：测试结果取决于环境里 `public` 的
/// 状态（本地 `public` 落后于迁移 baseline 时会直接 42P01），且并行测试互相影响。
/// 按铁律 7 消除状态共享：per-test schema 由模板克隆，表一定存在、行数从 0 开始。
async fn test_pool() -> (crate::test_isolation::IsolatedTestPool, Arc<sqlx::PgPool>) {
    let isolated = crate::test_isolation::isolated_test_pool().await.expect("isolated test pool");
    let pool = isolated.pool();
    (isolated, pool)
}

fn make_register_request(worker_id: &str, worker_type: WorkerType) -> RegisterWorkerRequest {
    RegisterWorkerRequest {
        worker_id: worker_id.to_string(),
        worker_name: format!("test-worker-{worker_id}"),
        worker_type,
        host: "localhost".to_string(),
        port: 8080,
        config: Some(serde_json::json!({"key": "value"})),
        metadata: Some(serde_json::json!({"region": "us-east"})),
        version: Some("1.0.0".to_string()),
    }
}

async fn cleanup_worker(pool: &Arc<PgPool>, worker_id: &str) {
    let _ = sqlx::query(r"DELETE FROM worker_task_assignments WHERE assigned_worker_id = $1")
        .bind(worker_id)
        .execute(&**pool)
        .await;
    let _ =
        sqlx::query(r"DELETE FROM worker_commands WHERE target_worker_id = $1").bind(worker_id).execute(&**pool).await;
    let _ =
        sqlx::query(r"DELETE FROM replication_positions WHERE worker_id = $1").bind(worker_id).execute(&**pool).await;
    let _ = sqlx::query(r"DELETE FROM workers WHERE worker_id = $1").bind(worker_id).execute(&**pool).await;
}

async fn cleanup_event(pool: &Arc<PgPool>, event_id: &str) {
    let _ = sqlx::query(r"DELETE FROM worker_events WHERE event_id = $1").bind(event_id).execute(&**pool).await;
}

async fn cleanup_command(pool: &Arc<PgPool>, command_id: &str) {
    let _ = sqlx::query(r"DELETE FROM worker_commands WHERE command_id = $1").bind(command_id).execute(&**pool).await;
}

async fn cleanup_task(pool: &Arc<PgPool>, task_id: &str) {
    let _ = sqlx::query(r"DELETE FROM worker_task_assignments WHERE task_id = $1").bind(task_id).execute(&**pool).await;
}

// === register_worker ===
#[tokio::test]
async fn test_register_worker_creates_worker() {
    let (_isolated, pool) = test_pool().await;
    let storage = WorkerStorage::new(&pool);
    let worker_id = format!("w-reg-{}", uuid::Uuid::new_v4());
    cleanup_worker(&pool, &worker_id).await;

    let worker = storage
        .register_worker(make_register_request(&worker_id, WorkerType::Frontend))
        .await
        .expect("register_worker should succeed");

    assert_eq!(worker.worker_id, worker_id);
    assert_eq!(worker.worker_type, "frontend");
    assert_eq!(worker.host, "localhost");
    assert_eq!(worker.port, 8080);
    assert_eq!(worker.status, "starting");
    assert_eq!(worker.version, Some("1.0.0".to_string()));
    assert!(worker.last_heartbeat_ts.is_none());
    assert!(worker.stopped_ts.is_none());

    cleanup_worker(&pool, &worker_id).await;
}

#[tokio::test]
async fn test_register_worker_minimal_fields() {
    let (_isolated, pool) = test_pool().await;
    let storage = WorkerStorage::new(&pool);
    let worker_id = format!("w-min-{}", uuid::Uuid::new_v4());
    cleanup_worker(&pool, &worker_id).await;

    let request = RegisterWorkerRequest {
        worker_id: worker_id.clone(),
        worker_name: "minimal".to_string(),
        worker_type: WorkerType::Background,
        host: "10.0.0.1".to_string(),
        port: 9000,
        config: None,
        metadata: None,
        version: None,
    };
    let worker = storage.register_worker(request).await.expect("register_worker should succeed");
    assert_eq!(worker.worker_type, "background");
    assert!(worker.version.is_none());
    assert_eq!(worker.config, serde_json::json!({}));
    assert_eq!(worker.metadata, serde_json::json!({}));

    cleanup_worker(&pool, &worker_id).await;
}

// === get_worker ===
#[tokio::test]
async fn test_get_worker_found() {
    let (_isolated, pool) = test_pool().await;
    let storage = WorkerStorage::new(&pool);
    let worker_id = format!("w-get-{}", uuid::Uuid::new_v4());
    cleanup_worker(&pool, &worker_id).await;

    storage
        .register_worker(make_register_request(&worker_id, WorkerType::Frontend))
        .await
        .expect("register_worker should succeed");

    let found = storage.get_worker(&worker_id).await.expect("get_worker should succeed");
    assert!(found.is_some());
    assert_eq!(found.unwrap().worker_id, worker_id);

    cleanup_worker(&pool, &worker_id).await;
}

#[tokio::test]
async fn test_get_worker_not_found() {
    let (_isolated, pool) = test_pool().await;
    let storage = WorkerStorage::new(&pool);
    let result = storage.get_worker("nonexistent-worker-id").await.expect("get_worker should succeed");
    assert!(result.is_none());
}

// === get_workers_by_type ===
#[tokio::test]
async fn test_get_workers_by_type() {
    let (_isolated, pool) = test_pool().await;
    let storage = WorkerStorage::new(&pool);
    let worker_id = format!("w-type-{}", uuid::Uuid::new_v4());
    cleanup_worker(&pool, &worker_id).await;

    storage
        .register_worker(make_register_request(&worker_id, WorkerType::MediaRepository))
        .await
        .expect("register_worker should succeed");

    let workers = storage.get_workers_by_type("media_repository").await.expect("get_workers_by_type should succeed");
    assert!(workers.iter().any(|w| w.worker_id == worker_id));

    cleanup_worker(&pool, &worker_id).await;
}

#[tokio::test]
async fn test_get_workers_by_type_empty_result() {
    let (_isolated, pool) = test_pool().await;
    let storage = WorkerStorage::new(&pool);
    let workers =
        storage.get_workers_by_type("nonexistent_type_xyz").await.expect("get_workers_by_type should succeed");
    assert!(workers.is_empty());
}

// === get_active_workers ===
#[tokio::test]
async fn test_get_active_workers_includes_starting() {
    let (_isolated, pool) = test_pool().await;
    let storage = WorkerStorage::new(&pool);
    let worker_id = format!("w-active-{}", uuid::Uuid::new_v4());
    cleanup_worker(&pool, &worker_id).await;

    storage
        .register_worker(make_register_request(&worker_id, WorkerType::Frontend))
        .await
        .expect("register_worker should succeed");

    let active = storage.get_active_workers().await.expect("get_active_workers should succeed");
    assert!(active.iter().any(|w| w.worker_id == worker_id));

    cleanup_worker(&pool, &worker_id).await;
}

#[tokio::test]
async fn test_get_active_workers_excludes_stopped() {
    let (_isolated, pool) = test_pool().await;
    let storage = WorkerStorage::new(&pool);
    let worker_id = format!("w-stopped-{}", uuid::Uuid::new_v4());
    cleanup_worker(&pool, &worker_id).await;

    storage
        .register_worker(make_register_request(&worker_id, WorkerType::Frontend))
        .await
        .expect("register_worker should succeed");
    storage.update_worker_status(&worker_id, "stopped").await.expect("update_worker_status should succeed");

    let active = storage.get_active_workers().await.expect("get_active_workers should succeed");
    assert!(!active.iter().any(|w| w.worker_id == worker_id));

    cleanup_worker(&pool, &worker_id).await;
}

// === update_worker_status ===
#[tokio::test]
async fn test_update_worker_status_to_running() {
    let (_isolated, pool) = test_pool().await;
    let storage = WorkerStorage::new(&pool);
    let worker_id = format!("w-run-{}", uuid::Uuid::new_v4());
    cleanup_worker(&pool, &worker_id).await;

    storage
        .register_worker(make_register_request(&worker_id, WorkerType::Frontend))
        .await
        .expect("register_worker should succeed");
    storage.update_worker_status(&worker_id, "running").await.expect("update_worker_status should succeed");

    let worker = storage.get_worker(&worker_id).await.unwrap().unwrap();
    assert_eq!(worker.status, "running");
    assert!(worker.stopped_ts.is_none());

    cleanup_worker(&pool, &worker_id).await;
}

#[tokio::test]
async fn test_update_worker_status_to_stopped_sets_stopped_ts() {
    let (_isolated, pool) = test_pool().await;
    let storage = WorkerStorage::new(&pool);
    let worker_id = format!("w-stop-{}", uuid::Uuid::new_v4());
    cleanup_worker(&pool, &worker_id).await;

    storage
        .register_worker(make_register_request(&worker_id, WorkerType::Frontend))
        .await
        .expect("register_worker should succeed");
    storage.update_worker_status(&worker_id, "stopped").await.expect("update_worker_status should succeed");

    let worker = storage.get_worker(&worker_id).await.unwrap().unwrap();
    assert_eq!(worker.status, "stopped");
    assert!(worker.stopped_ts.is_some());

    cleanup_worker(&pool, &worker_id).await;
}

#[tokio::test]
async fn test_update_worker_status_to_error_releases_in_flight_tasks() {
    let (_isolated, pool) = test_pool().await;
    let storage = WorkerStorage::new(&pool);
    let worker_id = format!("w-err-{}", uuid::Uuid::new_v4());
    cleanup_worker(&pool, &worker_id).await;

    storage
        .register_worker(make_register_request(&worker_id, WorkerType::Frontend))
        .await
        .expect("register_worker should succeed");

    let task = storage
        .assign_task(AssignTaskRequest {
            task_type: "background_job".to_string(),
            task_data: serde_json::json!({"job": "cleanup"}),
            priority: Some(5),
            preferred_worker_id: Some(worker_id.clone()),
        })
        .await
        .expect("assign_task should succeed");

    storage.assign_task_to_worker(&task.task_id, &worker_id).await.expect("assign_task_to_worker should succeed");

    // Set worker to error -> should release the in-flight task back to pending
    storage.update_worker_status(&worker_id, "error").await.expect("update_worker_status should succeed");

    let pending = storage.get_pending_tasks(10).await.expect("get_pending_tasks should succeed");
    assert!(pending.iter().any(|t| t.task_id == task.task_id));

    cleanup_task(&pool, &task.task_id).await;
    cleanup_worker(&pool, &worker_id).await;
}

// === update_heartbeat ===
#[tokio::test]
async fn test_update_heartbeat_sets_running_and_ts() {
    let (_isolated, pool) = test_pool().await;
    let storage = WorkerStorage::new(&pool);
    let worker_id = format!("w-hb-{}", uuid::Uuid::new_v4());
    cleanup_worker(&pool, &worker_id).await;

    storage
        .register_worker(make_register_request(&worker_id, WorkerType::Frontend))
        .await
        .expect("register_worker should succeed");
    storage.update_heartbeat(&worker_id).await.expect("update_heartbeat should succeed");

    let worker = storage.get_worker(&worker_id).await.unwrap().unwrap();
    assert_eq!(worker.status, "running");
    assert!(worker.last_heartbeat_ts.is_some());

    cleanup_worker(&pool, &worker_id).await;
}

// === unregister_worker ===
#[tokio::test]
async fn test_unregister_worker_sets_stopped() {
    let (_isolated, pool) = test_pool().await;
    let storage = WorkerStorage::new(&pool);
    let worker_id = format!("w-unreg-{}", uuid::Uuid::new_v4());
    cleanup_worker(&pool, &worker_id).await;

    storage
        .register_worker(make_register_request(&worker_id, WorkerType::Frontend))
        .await
        .expect("register_worker should succeed");
    storage.unregister_worker(&worker_id).await.expect("unregister_worker should succeed");

    let worker = storage.get_worker(&worker_id).await.unwrap().unwrap();
    assert_eq!(worker.status, "stopped");
    assert!(worker.stopped_ts.is_some());

    cleanup_worker(&pool, &worker_id).await;
}

#[tokio::test]
async fn test_unregister_worker_releases_in_flight_tasks() {
    let (_isolated, pool) = test_pool().await;
    let storage = WorkerStorage::new(&pool);
    let worker_id = format!("w-unreg2-{}", uuid::Uuid::new_v4());
    cleanup_worker(&pool, &worker_id).await;

    storage
        .register_worker(make_register_request(&worker_id, WorkerType::Frontend))
        .await
        .expect("register_worker should succeed");

    let task = storage
        .assign_task(AssignTaskRequest {
            task_type: "job_x".to_string(),
            task_data: serde_json::json!({}),
            priority: None,
            preferred_worker_id: Some(worker_id.clone()),
        })
        .await
        .expect("assign_task should succeed");
    storage.assign_task_to_worker(&task.task_id, &worker_id).await.expect("assign_task_to_worker should succeed");

    storage.unregister_worker(&worker_id).await.expect("unregister_worker should succeed");

    let pending = storage.get_pending_tasks(10).await.expect("get_pending_tasks should succeed");
    assert!(pending.iter().any(|t| t.task_id == task.task_id));

    cleanup_task(&pool, &task.task_id).await;
    cleanup_worker(&pool, &worker_id).await;
}

// === create_command ===
#[tokio::test]
async fn test_create_command_returns_pending() {
    let (_isolated, pool) = test_pool().await;
    let storage = WorkerStorage::new(&pool);
    let worker_id = format!("w-cmd-{}", uuid::Uuid::new_v4());
    cleanup_worker(&pool, &worker_id).await;

    let request = SendCommandRequest {
        target_worker_id: worker_id.clone(),
        command_type: "reload".to_string(),
        command_data: serde_json::json!({"section": "logging"}),
        priority: Some(7),
        max_retries: Some(2),
    };
    let cmd = storage.create_command(request).await.expect("create_command should succeed");
    assert_eq!(cmd.target_worker_id, worker_id);
    assert_eq!(cmd.command_type, "reload");
    assert_eq!(cmd.priority, 7);
    assert_eq!(cmd.status, "pending");
    assert_eq!(cmd.max_retries, 2);
    assert_eq!(cmd.retry_count, 0);

    cleanup_command(&pool, &cmd.command_id).await;
}

#[tokio::test]
async fn test_create_command_default_priority_and_retries() {
    let (_isolated, pool) = test_pool().await;
    let storage = WorkerStorage::new(&pool);
    let worker_id = format!("w-cmd2-{}", uuid::Uuid::new_v4());

    let request = SendCommandRequest {
        target_worker_id: worker_id.clone(),
        command_type: "ping".to_string(),
        command_data: serde_json::json!({}),
        priority: None,
        max_retries: None,
    };
    let cmd = storage.create_command(request).await.expect("create_command should succeed");
    assert_eq!(cmd.priority, 0);
    assert_eq!(cmd.max_retries, 3);

    cleanup_command(&pool, &cmd.command_id).await;
}

// === get_pending_commands ===
#[tokio::test]
async fn test_get_pending_commands_orders_by_priority() {
    let (_isolated, pool) = test_pool().await;
    let storage = WorkerStorage::new(&pool);
    let worker_id = format!("w-pend-{}", uuid::Uuid::new_v4());

    let low = storage
        .create_command(SendCommandRequest {
            target_worker_id: worker_id.clone(),
            command_type: "low".to_string(),
            command_data: serde_json::json!({}),
            priority: Some(1),
            max_retries: None,
        })
        .await
        .expect("create_command should succeed");
    let high = storage
        .create_command(SendCommandRequest {
            target_worker_id: worker_id.clone(),
            command_type: "high".to_string(),
            command_data: serde_json::json!({}),
            priority: Some(10),
            max_retries: None,
        })
        .await
        .expect("create_command should succeed");

    let pending = storage.get_pending_commands(&worker_id, 10).await.expect("get_pending_commands should succeed");
    assert!(pending.len() >= 2);
    // Higher priority should come first
    assert_eq!(pending[0].command_id, high.command_id);

    cleanup_command(&pool, &low.command_id).await;
    cleanup_command(&pool, &high.command_id).await;
}

#[tokio::test]
async fn test_get_pending_commands_excludes_non_pending() {
    let (_isolated, pool) = test_pool().await;
    let storage = WorkerStorage::new(&pool);
    let worker_id = format!("w-pend2-{}", uuid::Uuid::new_v4());

    let cmd = storage
        .create_command(SendCommandRequest {
            target_worker_id: worker_id.clone(),
            command_type: "send".to_string(),
            command_data: serde_json::json!({}),
            priority: None,
            max_retries: None,
        })
        .await
        .expect("create_command should succeed");
    storage.mark_command_sent(&cmd.command_id).await.expect("mark_command_sent should succeed");

    let pending = storage.get_pending_commands(&worker_id, 10).await.expect("get_pending_commands should succeed");
    assert!(!pending.iter().any(|c| c.command_id == cmd.command_id));

    cleanup_command(&pool, &cmd.command_id).await;
}

// === mark_command_sent ===
#[tokio::test]
async fn test_mark_command_sent_updates_status() {
    let (_isolated, pool) = test_pool().await;
    let storage = WorkerStorage::new(&pool);
    let worker_id = format!("w-sent-{}", uuid::Uuid::new_v4());

    let cmd = storage
        .create_command(SendCommandRequest {
            target_worker_id: worker_id.clone(),
            command_type: "send".to_string(),
            command_data: serde_json::json!({}),
            priority: None,
            max_retries: None,
        })
        .await
        .expect("create_command should succeed");
    storage.mark_command_sent(&cmd.command_id).await.expect("mark_command_sent should succeed");

    cleanup_command(&pool, &cmd.command_id).await;
}

// === complete_command ===
#[tokio::test]
async fn test_complete_command_updates_status() {
    let (_isolated, pool) = test_pool().await;
    let storage = WorkerStorage::new(&pool);
    let worker_id = format!("w-comp-{}", uuid::Uuid::new_v4());

    let cmd = storage
        .create_command(SendCommandRequest {
            target_worker_id: worker_id.clone(),
            command_type: "do".to_string(),
            command_data: serde_json::json!({}),
            priority: None,
            max_retries: None,
        })
        .await
        .expect("create_command should succeed");
    storage.complete_command(&cmd.command_id).await.expect("complete_command should succeed");

    cleanup_command(&pool, &cmd.command_id).await;
}

// === fail_command ===
#[tokio::test]
async fn test_fail_command_under_max_retries_stays_pending() {
    let (_isolated, pool) = test_pool().await;
    let storage = WorkerStorage::new(&pool);
    let worker_id = format!("w-fail-{}", uuid::Uuid::new_v4());

    // max_retries = 3, so first failure should keep it pending
    let cmd = storage
        .create_command(SendCommandRequest {
            target_worker_id: worker_id.clone(),
            command_type: "do".to_string(),
            command_data: serde_json::json!({}),
            priority: None,
            max_retries: Some(3),
        })
        .await
        .expect("create_command should succeed");

    storage.fail_command(&cmd.command_id, "transient error").await.expect("fail_command should succeed");

    // Should still be retryable (pending)
    let pending = storage.get_pending_commands(&worker_id, 10).await.expect("get_pending_commands should succeed");
    assert!(pending.iter().any(|c| c.command_id == cmd.command_id));

    cleanup_command(&pool, &cmd.command_id).await;
}

#[tokio::test]
async fn test_fail_command_at_max_retries_becomes_failed() {
    let (_isolated, pool) = test_pool().await;
    let storage = WorkerStorage::new(&pool);
    let worker_id = format!("w-fail2-{}", uuid::Uuid::new_v4());

    let cmd = storage
        .create_command(SendCommandRequest {
            target_worker_id: worker_id.clone(),
            command_type: "do".to_string(),
            command_data: serde_json::json!({}),
            priority: None,
            max_retries: Some(1),
        })
        .await
        .expect("create_command should succeed");

    // First failure: retry_count goes 0->1, since 0 >= 1 is false, stays pending
    storage.fail_command(&cmd.command_id, "first error").await.expect("fail_command should succeed");
    // Second call: retry_count is now 1, 1 >= 1 is true -> failed
    storage.fail_command(&cmd.command_id, "second error").await.expect("fail_command should succeed");

    let pending = storage.get_pending_commands(&worker_id, 10).await.expect("get_pending_commands should succeed");
    assert!(!pending.iter().any(|c| c.command_id == cmd.command_id));

    cleanup_command(&pool, &cmd.command_id).await;
}

// === add_event / get_events_since / mark_event_processed ===
#[tokio::test]
async fn test_add_event_creates_event() {
    let (_isolated, pool) = test_pool().await;
    let storage = WorkerStorage::new(&pool);
    let event_id = format!("$evt-add-{}", uuid::Uuid::new_v4());
    cleanup_event(&pool, &event_id).await;

    let event = storage
        .add_event(
            &event_id,
            "m.room.message",
            Some("!room:localhost"),
            Some("@user:localhost"),
            serde_json::json!({"body": "hello"}),
        )
        .await
        .expect("add_event should succeed");
    assert_eq!(event.event_id, event_id);
    assert_eq!(event.event_type, "m.room.message");
    assert_eq!(event.room_id, Some("!room:localhost".to_string()));

    cleanup_event(&pool, &event_id).await;
}

#[tokio::test]
async fn test_get_events_since_returns_ordered() {
    let (_isolated, pool) = test_pool().await;
    let storage = WorkerStorage::new(&pool);
    let event_id1 = format!("$evt-g1-{}", uuid::Uuid::new_v4());
    let event_id2 = format!("$evt-g2-{}", uuid::Uuid::new_v4());
    cleanup_event(&pool, &event_id1).await;
    cleanup_event(&pool, &event_id2).await;

    let e1 = storage
        .add_event(&event_id1, "type_a", None, None, serde_json::json!({}))
        .await
        .expect("add_event should succeed");
    let _e2 = storage
        .add_event(&event_id2, "type_b", None, None, serde_json::json!({}))
        .await
        .expect("add_event should succeed");

    let events = storage.get_events_since(e1.stream_id, 10).await.expect("get_events_since should succeed");
    // e2 has a higher stream_id than e1
    assert!(events.iter().any(|e| e.event_id == event_id2));
    assert!(!events.iter().any(|e| e.event_id == event_id1));

    cleanup_event(&pool, &event_id1).await;
    cleanup_event(&pool, &event_id2).await;
}

#[tokio::test]
async fn test_mark_event_processed_appends_worker() {
    let (_isolated, pool) = test_pool().await;
    let storage = WorkerStorage::new(&pool);
    let event_id = format!("$evt-proc-{}", uuid::Uuid::new_v4());
    cleanup_event(&pool, &event_id).await;

    let event = storage
        .add_event(&event_id, "m.room.message", None, None, serde_json::json!({}))
        .await
        .expect("add_event should succeed");
    assert!(event.processed_by.is_none() || event.processed_by.as_ref().is_none_or(|p| p.is_empty()));

    storage.mark_event_processed(&event_id, "worker-001").await.expect("mark_event_processed should succeed");
    storage.mark_event_processed(&event_id, "worker-002").await.expect("mark_event_processed should succeed");

    cleanup_event(&pool, &event_id).await;
}

// === update_replication_position / get_replication_position ===
#[tokio::test]
async fn test_update_and_get_replication_position() {
    let (_isolated, pool) = test_pool().await;
    let storage = WorkerStorage::new(&pool);
    let worker_id = format!("w-repl-{}", uuid::Uuid::new_v4());
    cleanup_worker(&pool, &worker_id).await;

    storage
        .register_worker(make_register_request(&worker_id, WorkerType::EventPersister))
        .await
        .expect("register_worker should succeed");

    storage
        .update_replication_position(&worker_id, "events", 5000)
        .await
        .expect("update_replication_position should succeed");

    let pos =
        storage.get_replication_position(&worker_id, "events").await.expect("get_replication_position should succeed");
    assert_eq!(pos, Some(5000));

    // Upsert: update existing position
    storage
        .update_replication_position(&worker_id, "events", 6000)
        .await
        .expect("update_replication_position should succeed");
    let pos2 = storage.get_replication_position(&worker_id, "events").await.unwrap();
    assert_eq!(pos2, Some(6000));

    cleanup_worker(&pool, &worker_id).await;
}

#[tokio::test]
async fn test_get_replication_position_not_found() {
    let (_isolated, pool) = test_pool().await;
    let storage = WorkerStorage::new(&pool);
    let worker_id = format!("w-repl2-{}", uuid::Uuid::new_v4());

    let pos = storage
        .get_replication_position(&worker_id, "nonexistent_stream")
        .await
        .expect("get_replication_position should succeed");
    assert!(pos.is_none());
}

// === assign_task ===
#[tokio::test]
async fn test_assign_task_creates_pending_task() {
    let (_isolated, pool) = test_pool().await;
    let storage = WorkerStorage::new(&pool);

    let task = storage
        .assign_task(AssignTaskRequest {
            task_type: "process_media".to_string(),
            task_data: serde_json::json!({"media_id": "abc"}),
            priority: Some(10),
            preferred_worker_id: None,
        })
        .await
        .expect("assign_task should succeed");

    assert_eq!(task.task_type, "process_media");
    assert_eq!(task.priority, 10);
    assert_eq!(task.status, "pending");
    assert!(task.assigned_worker_id.is_none());

    cleanup_task(&pool, &task.task_id).await;
}

// === get_pending_tasks ===
#[tokio::test]
async fn test_get_pending_tasks_returns_pending_only() {
    let (_isolated, pool) = test_pool().await;
    let storage = WorkerStorage::new(&pool);

    let task = storage
        .assign_task(AssignTaskRequest {
            task_type: "job_pending".to_string(),
            task_data: serde_json::json!({}),
            priority: None,
            preferred_worker_id: None,
        })
        .await
        .expect("assign_task should succeed");

    let pending = storage.get_pending_tasks(100).await.expect("get_pending_tasks should succeed");
    assert!(pending.iter().any(|t| t.task_id == task.task_id));

    cleanup_task(&pool, &task.task_id).await;
}

// === claim_next_pending_task ===
#[tokio::test]
async fn test_claim_next_pending_task_assigns_to_worker() {
    // IsolatedTestPool: each test gets a fresh schema, so parallel
    // `claim_next_pending_task()` calls can't steal our pending task.
    let isolated = crate::test_isolation::isolated_test_pool().await.expect("isolated pool");
    let pool = isolated.pool();
    let storage = WorkerStorage::new(&pool);
    let worker_id = format!("w-claim-{}", uuid::Uuid::new_v4());
    cleanup_worker(&pool, &worker_id).await;

    storage
        .register_worker(make_register_request(&worker_id, WorkerType::Background))
        .await
        .expect("register_worker should succeed");

    let task = storage
        .assign_task(AssignTaskRequest {
            task_type: "claimable_job".to_string(),
            task_data: serde_json::json!({}),
            priority: Some(5),
            preferred_worker_id: None,
        })
        .await
        .expect("assign_task should succeed");

    let claimed = storage.claim_next_pending_task(&worker_id).await.expect("claim_next_pending_task should succeed");
    assert!(claimed.is_some());
    let claimed = claimed.unwrap();
    assert_eq!(claimed.task_id, task.task_id);
    assert_eq!(claimed.assigned_worker_id, Some(worker_id.clone()));
    assert_eq!(claimed.status, "running");

    cleanup_task(&pool, &task.task_id).await;
    cleanup_worker(&pool, &worker_id).await;
}

#[tokio::test]
async fn test_claim_next_pending_task_none_when_empty() {
    // IsolatedTestPool: each test gets a fresh schema, so orphaned tasks
    // from other tests can't leak in to make this test's claim return Some.
    // No need to call `cleanup_orphaned_tasks` because the schema is empty.
    let isolated = crate::test_isolation::isolated_test_pool().await.expect("isolated pool");
    let pool = isolated.pool();
    let storage = WorkerStorage::new(&pool);
    let worker_id = format!("w-claim2-{}", uuid::Uuid::new_v4());
    cleanup_worker(&pool, &worker_id).await;

    // Register the worker first so the FK constraint on
    // worker_task_assignments.assigned_worker_id is satisfied even if a
    // leftover pending task from another test gets matched by the UPDATE's
    // subquery. Without this, claim_next_pending_task fails with
    // fk_worker_task_assignments_worker violation.
    storage
        .register_worker(make_register_request(&worker_id, WorkerType::Pusher))
        .await
        .expect("register_worker should succeed");

    let claimed = storage.claim_next_pending_task(&worker_id).await.expect("claim_next_pending_task should succeed");
    assert!(claimed.is_none());

    cleanup_worker(&pool, &worker_id).await;
}

// === claim_next_pending_task_for_types ===
#[tokio::test]
async fn test_claim_next_pending_task_for_types_matches() {
    // IsolatedTestPool: parallel claims can't steal our push_delivery task.
    let isolated = crate::test_isolation::isolated_test_pool().await.expect("isolated pool");
    let pool = isolated.pool();
    let storage = WorkerStorage::new(&pool);
    let worker_id = format!("w-claimt-{}", uuid::Uuid::new_v4());
    cleanup_worker(&pool, &worker_id).await;

    storage
        .register_worker(make_register_request(&worker_id, WorkerType::Pusher))
        .await
        .expect("register_worker should succeed");

    let task = storage
        .assign_task(AssignTaskRequest {
            task_type: "push_delivery".to_string(),
            task_data: serde_json::json!({}),
            priority: None,
            preferred_worker_id: None,
        })
        .await
        .expect("assign_task should succeed");

    let allowed = vec!["push_delivery".to_string()];
    let claimed = storage
        .claim_next_pending_task_for_types(&worker_id, &allowed)
        .await
        .expect("claim_next_pending_task_for_types should succeed");
    assert!(claimed.is_some());
    assert_eq!(claimed.unwrap().task_id, task.task_id);

    cleanup_task(&pool, &task.task_id).await;
    cleanup_worker(&pool, &worker_id).await;
}

#[tokio::test]
async fn test_claim_next_pending_task_for_types_no_match() {
    // IsolatedTestPool: orphaned tasks from other tests can't leak in.
    let isolated = crate::test_isolation::isolated_test_pool().await.expect("isolated pool");
    let pool = isolated.pool();
    let storage = WorkerStorage::new(&pool);
    let worker_id = format!("w-claimt2-{}", uuid::Uuid::new_v4());
    cleanup_worker(&pool, &worker_id).await;

    // Register the worker first so the FK constraint is satisfied
    storage
        .register_worker(make_register_request(&worker_id, WorkerType::Pusher))
        .await
        .expect("register_worker should succeed");

    let task = storage
        .assign_task(AssignTaskRequest {
            task_type: "other_type".to_string(),
            task_data: serde_json::json!({}),
            priority: None,
            preferred_worker_id: None,
        })
        .await
        .expect("assign_task should succeed");

    let allowed = vec!["push_delivery".to_string()];
    let claimed = storage
        .claim_next_pending_task_for_types(&worker_id, &allowed)
        .await
        .expect("claim_next_pending_task_for_types should succeed");
    assert!(claimed.is_none());

    // Clean up: delete the task we just created
    cleanup_task(&pool, &task.task_id).await;
    cleanup_worker(&pool, &worker_id).await;
}

// === assign_task_to_worker ===
#[tokio::test]
async fn test_assign_task_to_worker_succeeds_for_pending() {
    let (_isolated, pool) = test_pool().await;
    let storage = WorkerStorage::new(&pool);
    let worker_id = format!("w-asgntw-{}", uuid::Uuid::new_v4());
    cleanup_worker(&pool, &worker_id).await;

    storage
        .register_worker(make_register_request(&worker_id, WorkerType::Frontend))
        .await
        .expect("register_worker should succeed");

    let task = storage
        .assign_task(AssignTaskRequest {
            task_type: "assignable".to_string(),
            task_data: serde_json::json!({}),
            priority: None,
            preferred_worker_id: None,
        })
        .await
        .expect("assign_task should succeed");

    let ok =
        storage.assign_task_to_worker(&task.task_id, &worker_id).await.expect("assign_task_to_worker should succeed");
    assert!(ok);

    cleanup_task(&pool, &task.task_id).await;
    cleanup_worker(&pool, &worker_id).await;
}

#[tokio::test]
async fn test_assign_task_to_worker_fails_for_nonexistent() {
    let (_isolated, pool) = test_pool().await;
    let storage = WorkerStorage::new(&pool);
    let worker_id = format!("w-asgntw2-{}", uuid::Uuid::new_v4());

    let ok = storage
        .assign_task_to_worker("nonexistent-task-id", &worker_id)
        .await
        .expect("assign_task_to_worker should succeed");
    assert!(!ok);
}

// === complete_task ===
#[tokio::test]
async fn test_complete_task_sets_completed() {
    let (_isolated, pool) = test_pool().await;
    let storage = WorkerStorage::new(&pool);

    let task = storage
        .assign_task(AssignTaskRequest {
            task_type: "completable".to_string(),
            task_data: serde_json::json!({}),
            priority: None,
            preferred_worker_id: None,
        })
        .await
        .expect("assign_task should succeed");

    storage
        .complete_task(&task.task_id, Some(serde_json::json!({"result": "ok"})))
        .await
        .expect("complete_task should succeed");

    cleanup_task(&pool, &task.task_id).await;
}

// === fail_task ===
#[tokio::test]
async fn test_fail_task_sets_failed() {
    let (_isolated, pool) = test_pool().await;
    let storage = WorkerStorage::new(&pool);

    let task = storage
        .assign_task(AssignTaskRequest {
            task_type: "failable".to_string(),
            task_data: serde_json::json!({}),
            priority: None,
            preferred_worker_id: None,
        })
        .await
        .expect("assign_task should succeed");

    storage.fail_task(&task.task_id, "something went wrong").await.expect("fail_task should succeed");

    // Should no longer appear in pending
    let pending = storage.get_pending_tasks(100).await.expect("get_pending_tasks should succeed");
    assert!(!pending.iter().any(|t| t.task_id == task.task_id));

    cleanup_task(&pool, &task.task_id).await;
}

// === get_type_statistics ===
#[tokio::test]
async fn test_get_type_statistics_returns_rows() {
    let (_isolated, pool) = test_pool().await;
    let storage = WorkerStorage::new(&pool);
    let worker_id = format!("w-typstat-{}", uuid::Uuid::new_v4());
    cleanup_worker(&pool, &worker_id).await;

    storage
        .register_worker(make_register_request(&worker_id, WorkerType::Frontend))
        .await
        .expect("register_worker should succeed");

    let stats = storage.get_type_statistics().await.expect("get_type_statistics should succeed");
    // The view groups by worker_type; frontend should appear
    assert!(stats.iter().any(|s| s.get("worker_type").and_then(|v| v.as_str()) == Some("frontend")));

    cleanup_worker(&pool, &worker_id).await;
}

// === sync (non-DB) methods: record_load_stats, record_connection, update_connection_stats ===
// These methods don't touch the DB, but sqlx::PgPool::connect_lazy still
// needs a Tokio runtime to spawn its internal housekeeping tasks, so we
// run them inside a tokio test context.
#[tokio::test]
async fn test_record_load_stats_returns_ok() {
    let pool = Arc::new(
        sqlx::PgPool::connect_lazy("postgres://synapse:synapse@localhost:5432/synapse_test")
            .expect("connect_lazy should succeed"),
    );
    let storage = WorkerStorage::new(&pool);
    let stats = WorkerLoadStatsUpdate {
        cpu_usage: Some(45.0),
        memory_usage: Some(1024),
        active_connections: Some(10),
        requests_per_second: Some(100.0),
        average_latency_ms: Some(5.0),
        queue_depth: Some(2),
    };
    storage.record_load_stats("worker-1", &stats).expect("record_load_stats should succeed");
}

#[tokio::test]
async fn test_record_connection_returns_ok() {
    let pool = Arc::new(
        sqlx::PgPool::connect_lazy("postgres://synapse:synapse@localhost:5432/synapse_test")
            .expect("connect_lazy should succeed"),
    );
    let storage = WorkerStorage::new(&pool);
    storage.record_connection("source-1", "target-1", "direct").expect("record_connection should succeed");
}

#[tokio::test]
async fn test_update_connection_stats_returns_ok() {
    let pool = Arc::new(
        sqlx::PgPool::connect_lazy("postgres://synapse:synapse@localhost:5432/synapse_test")
            .expect("connect_lazy should succeed"),
    );
    let storage = WorkerStorage::new(&pool);
    let request = UpdateConnectionStatsRequest::new("src", "tgt", "relay")
        .bytes_sent(100)
        .bytes_received(200)
        .messages_sent(5)
        .messages_received(10);
    storage.update_connection_stats(&request).expect("update_connection_stats should succeed");
}

/// `get_statistics` 契约修复回归（2026-09-23）。
///
/// 旧实现对 `worker_statistics` 选择了 15 个**两张表都没有**的列
/// （psql 42703 `column "worker_name" does not exist`），端点从未返回过任何行。
/// 现在身份/状态取自 `workers`，计数与负载指标 LEFT JOIN `worker_statistics`：
/// 有计数行的 worker 必须带上计数；没有计数行的 worker 计数必须为 null（而不是解码错误）；
/// 六个负载指标键已由心跳写入（`upsert_statistics`）支撑，因此**始终存在**，未上报时为 null。
#[tokio::test]
async fn test_get_statistics_reads_real_columns_and_tolerates_missing_counters() {
    let (_isolated, pool) = test_pool().await;
    let storage = WorkerStorage::new(&pool);

    let with_stats = format!("w-stats-{}", uuid::Uuid::new_v4());
    let without_stats = format!("w-nostats-{}", uuid::Uuid::new_v4());
    cleanup_worker(&pool, &with_stats).await;
    cleanup_worker(&pool, &without_stats).await;

    storage
        .register_worker(make_register_request(&with_stats, WorkerType::Frontend))
        .await
        .expect("register worker with stats");
    storage
        .register_worker(make_register_request(&without_stats, WorkerType::Frontend))
        .await
        .expect("register worker without stats");

    let now = synapse_common::current_timestamp_millis();
    let _ = sqlx::query("DELETE FROM worker_statistics WHERE worker_id = $1").bind(&with_stats).execute(&*pool).await;
    sqlx::query(
        "INSERT INTO worker_statistics (worker_id, total_messages_sent, total_errors, uptime_seconds, created_ts, updated_ts) \
         VALUES ($1, 42, 3, 900, $2, $2)",
    )
    .bind(&with_stats)
    .bind(now)
    .execute(&*pool)
    .await
    .expect("insert worker_statistics row");

    let rows = storage.get_statistics(50).await.expect("get_statistics must succeed against the real schema");

    let with_row =
        rows.iter().find(|row| row["worker_id"] == serde_json::json!(with_stats)).expect("worker with stats present");
    assert_eq!(with_row["total_messages_sent"], serde_json::json!(42));
    assert_eq!(with_row["total_errors"], serde_json::json!(3));
    assert_eq!(with_row["uptime_seconds"], serde_json::json!(900));
    assert_eq!(with_row["status"], serde_json::json!("starting"));
    // The load-metric keys are now backed by columns written from the heartbeat
    // payload (`upsert_statistics`), so they are always emitted; a worker whose
    // row predates any load report must still decode them as null.
    for metric in
        ["cpu_usage", "memory_usage", "active_connections", "requests_per_second", "average_latency_ms", "queue_depth"]
    {
        assert!(
            with_row.get(metric).is_some_and(serde_json::Value::is_null),
            "load metric `{metric}` must be emitted as null when never reported"
        );
    }

    let without_row = rows
        .iter()
        .find(|row| row["worker_id"] == serde_json::json!(without_stats))
        .expect("worker without stats present");
    assert!(
        without_row["total_messages_sent"].is_null(),
        "missing counters must decode as null rather than raising a decode error"
    );
}

/// `upsert_statistics` 写入一行后，`get_statistics` 必须返回**非 NULL** 的负载指标。
#[tokio::test]
async fn test_upsert_statistics_inserts_row_and_get_statistics_returns_metrics() {
    let (_isolated, pool) = test_pool().await;
    let storage = WorkerStorage::new(&pool);

    let worker_id = format!("w-upsert-{}", uuid::Uuid::new_v4());
    cleanup_worker(&pool, &worker_id).await;
    storage.register_worker(make_register_request(&worker_id, WorkerType::Frontend)).await.expect("register worker");

    let now = synapse_common::current_timestamp_millis();
    let stats = WorkerLoadStatsUpdate {
        cpu_usage: Some(12.5),
        memory_usage: Some(2048),
        active_connections: Some(7),
        requests_per_second: Some(33.25),
        average_latency_ms: Some(4.5),
        queue_depth: Some(3),
    };
    storage.upsert_statistics(&worker_id, &stats, now).await.expect("upsert_statistics");

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM worker_statistics WHERE worker_id = $1")
        .bind(&worker_id)
        .fetch_one(&*pool)
        .await
        .expect("count statistics rows");
    assert_eq!(count, 1, "first upsert must create exactly one row");

    let rows = storage.get_statistics(50).await.expect("get_statistics must succeed");
    let row = rows.iter().find(|row| row["worker_id"] == serde_json::json!(worker_id)).expect("worker present");
    assert_eq!(row["cpu_usage"], serde_json::json!(12.5));
    assert_eq!(row["memory_usage"], serde_json::json!(2048));
    assert_eq!(row["active_connections"], serde_json::json!(7));
    assert_eq!(row["requests_per_second"], serde_json::json!(33.25));
    assert_eq!(row["average_latency_ms"], serde_json::json!(4.5));
    assert_eq!(row["queue_depth"], serde_json::json!(3));
}

/// 重复 upsert 同一 worker 必须**更新同一行**（不是追加），且新值生效；
/// `created_ts` 保持首次写入值。
#[tokio::test]
async fn test_upsert_statistics_updates_in_place_and_later_values_win() {
    let (_isolated, pool) = test_pool().await;
    let storage = WorkerStorage::new(&pool);

    let worker_id = format!("w-upsert-twice-{}", uuid::Uuid::new_v4());
    cleanup_worker(&pool, &worker_id).await;

    let first = synapse_common::current_timestamp_millis();
    let second = first + 5_000;

    let stats_v1 = WorkerLoadStatsUpdate {
        cpu_usage: Some(10.0),
        memory_usage: Some(100),
        active_connections: Some(1),
        requests_per_second: Some(2.0),
        average_latency_ms: Some(3.0),
        queue_depth: Some(4),
    };
    storage.upsert_statistics(&worker_id, &stats_v1, first).await.expect("first upsert");

    let stats_v2 = WorkerLoadStatsUpdate {
        cpu_usage: Some(20.0),
        memory_usage: Some(200),
        active_connections: Some(2),
        requests_per_second: Some(4.0),
        average_latency_ms: Some(6.0),
        queue_depth: Some(8),
    };
    storage.upsert_statistics(&worker_id, &stats_v2, second).await.expect("second upsert");

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM worker_statistics WHERE worker_id = $1")
        .bind(&worker_id)
        .fetch_one(&*pool)
        .await
        .expect("count statistics rows");
    assert_eq!(count, 1, "second upsert must not append a duplicate row");

    let (created_ts, updated_ts, last_heartbeat_ts): (i64, i64, i64) =
        sqlx::query_as("SELECT created_ts, updated_ts, last_heartbeat_ts FROM worker_statistics WHERE worker_id = $1")
            .bind(&worker_id)
            .fetch_one(&*pool)
            .await
            .expect("read timestamps");
    assert_eq!(created_ts, first, "created_ts must survive the update");
    assert_eq!(updated_ts, second, "updated_ts must be the later timestamp");
    assert_eq!(last_heartbeat_ts, second, "last_heartbeat_ts must be the later timestamp");

    // No `workers` row is needed for the read: assert via the raw row so this
    // test is independent of `get_statistics`' join.
    let (cpu, queue): (f32, i32) =
        sqlx::query_as("SELECT cpu_usage, queue_depth FROM worker_statistics WHERE worker_id = $1")
            .bind(&worker_id)
            .fetch_one(&*pool)
            .await
            .expect("read upserted metrics");
    assert_eq!(cpu, 20.0, "the second call's values must win");
    assert_eq!(queue, 8, "the second call's values must win");
}

/// 只上报 `queue_depth` 的后续心跳必须**保留**此前已上报的其余指标（COALESCE 语义）。
#[tokio::test]
async fn test_upsert_statistics_preserves_unreported_metrics() {
    let (_isolated, pool) = test_pool().await;
    let storage = WorkerStorage::new(&pool);

    let worker_id = format!("w-upsert-partial-{}", uuid::Uuid::new_v4());
    cleanup_worker(&pool, &worker_id).await;

    let now = synapse_common::current_timestamp_millis();
    let full = WorkerLoadStatsUpdate {
        cpu_usage: Some(55.5),
        memory_usage: Some(4096),
        active_connections: Some(11),
        requests_per_second: Some(7.75),
        average_latency_ms: Some(8.25),
        queue_depth: Some(1),
    };
    storage.upsert_statistics(&worker_id, &full, now).await.expect("full upsert");

    let partial = WorkerLoadStatsUpdate {
        cpu_usage: None,
        memory_usage: None,
        active_connections: None,
        requests_per_second: None,
        average_latency_ms: None,
        queue_depth: Some(9),
    };
    storage.upsert_statistics(&worker_id, &partial, now + 1).await.expect("partial upsert");

    let (cpu, memory, connections, rps, latency, queue): (
        Option<f32>,
        Option<i64>,
        Option<i32>,
        Option<f32>,
        Option<f32>,
        Option<i32>,
    ) = sqlx::query_as(
        "SELECT cpu_usage, memory_usage, active_connections, requests_per_second, \
             average_latency_ms, queue_depth FROM worker_statistics WHERE worker_id = $1",
    )
    .bind(&worker_id)
    .fetch_one(&*pool)
    .await
    .expect("read preserved metrics");

    assert_eq!(cpu, Some(55.5), "unreported cpu_usage must be preserved");
    assert_eq!(memory, Some(4096), "unreported memory_usage must be preserved");
    assert_eq!(connections, Some(11), "unreported active_connections must be preserved");
    assert_eq!(rps, Some(7.75), "unreported requests_per_second must be preserved");
    assert_eq!(latency, Some(8.25), "unreported average_latency_ms must be preserved");
    assert_eq!(queue, Some(9), "reported queue_depth must be updated");

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM worker_statistics WHERE worker_id = $1")
        .bind(&worker_id)
        .fetch_one(&*pool)
        .await
        .expect("count statistics rows");
    assert_eq!(count, 1, "partial upsert must update in place");
}
