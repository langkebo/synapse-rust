use serde_json::json;
use std::sync::Arc;
use synapse_common::current_timestamp_millis;
use synapse_services::worker::{
    AssignTaskRequest, HealthCheckConfig, HealthChecker, LoadBalanceStrategy, RegisterWorkerRequest,
    WorkerLoadBalancer, WorkerLoadStats, WorkerManager, WorkerStatus, WorkerType,
};
use synapse_storage::worker::WorkerStorage;

async fn worker_test_pool() -> Arc<sqlx::PgPool> {
    synapse_rust::test_utils::prepare_isolated_test_pool()
        .await
        .expect("worker recovery integration tests require an isolated database pool")
}

fn test_worker_manager(pool: &Arc<sqlx::PgPool>) -> Arc<WorkerManager> {
    Arc::new(WorkerManager::new(Arc::new(WorkerStorage::new(pool)), "test-server".to_string()))
}

#[tokio::test]
async fn test_unregister_requeues_running_tasks_for_other_workers() {
    let pool = worker_test_pool().await;
    let manager = test_worker_manager(&pool);

    let suffix = uuid::Uuid::new_v4().to_string();
    let worker_a = format!("worker-requeue-a-{suffix}");
    let worker_b = format!("worker-requeue-b-{suffix}");

    for (worker_id, port) in [(&worker_a, 8101_u16), (&worker_b, 8102_u16)] {
        manager
            .register(RegisterWorkerRequest {
                worker_id: worker_id.clone(),
                worker_name: worker_id.clone(),
                worker_type: WorkerType::Background,
                host: "127.0.0.1".to_string(),
                port,
                config: None,
                metadata: None,
                version: Some("test".to_string()),
            })
            .await
            .expect("worker registration should succeed");

        manager
            .heartbeat(worker_id, WorkerStatus::Running, None)
            .await
            .expect("worker heartbeat should mark worker running");
    }

    let task = manager
        .assign_task(AssignTaskRequest {
            task_type: "background_jobs".to_string(),
            task_data: json!({ "kind": "stale-worker-requeue" }),
            priority: Some(10),
            preferred_worker_id: Some(worker_a.clone()),
        })
        .await
        .expect("task assignment should succeed");

    let before_unregister = sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT status, assigned_worker_id FROM worker_task_assignments WHERE task_id = $1",
    )
    .bind(&task.task_id)
    .fetch_one(&*pool)
    .await
    .expect("task should exist before unregister");
    assert_eq!(before_unregister.0, "running");
    assert_eq!(before_unregister.1.as_deref(), Some(worker_a.as_str()));

    manager.unregister(&worker_a).await.expect("worker unregister should succeed");

    let after_unregister = sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT status, assigned_worker_id FROM worker_task_assignments WHERE task_id = $1",
    )
    .bind(&task.task_id)
    .fetch_one(&*pool)
    .await
    .expect("task should still exist after unregister");
    assert_eq!(after_unregister.0, "pending");
    assert_eq!(after_unregister.1, None);

    let reclaimed = manager
        .claim_next_pending_task(&worker_b)
        .await
        .expect("another worker should be able to reclaim the requeued task");
    assert_eq!(reclaimed.task_id, task.task_id);
    assert_eq!(reclaimed.status, "running");
    assert_eq!(reclaimed.assigned_worker_id.as_deref(), Some(worker_b.as_str()));

    sqlx::query("DELETE FROM worker_task_assignments WHERE task_id = $1")
        .bind(&task.task_id)
        .execute(&*pool)
        .await
        .expect("task cleanup should succeed");
    sqlx::query("DELETE FROM workers WHERE worker_id = $1 OR worker_id = $2")
        .bind(&worker_a)
        .bind(&worker_b)
        .execute(&*pool)
        .await
        .expect("worker cleanup should succeed");
}

#[tokio::test]
async fn test_active_workers_and_replication_positions_remain_isolated_across_workers() {
    let pool = worker_test_pool().await;
    let manager = test_worker_manager(&pool);

    let suffix = uuid::Uuid::new_v4().to_string();
    let worker_a = format!("worker-state-a-{suffix}");
    let worker_b = format!("worker-state-b-{suffix}");

    for (worker_id, port) in [(&worker_a, 8111_u16), (&worker_b, 8112_u16)] {
        manager
            .register(RegisterWorkerRequest {
                worker_id: worker_id.clone(),
                worker_name: worker_id.clone(),
                worker_type: WorkerType::Background,
                host: "127.0.0.1".to_string(),
                port,
                config: None,
                metadata: None,
                version: Some("test".to_string()),
            })
            .await
            .expect("worker registration should succeed");

        manager
            .heartbeat(worker_id, WorkerStatus::Running, None)
            .await
            .expect("worker heartbeat should mark worker running");
    }

    manager
        .update_replication_position(&worker_a, "events", 10)
        .await
        .expect("worker_a replication position should update");
    manager
        .update_replication_position(&worker_b, "events", 20)
        .await
        .expect("worker_b replication position should update");

    let worker_rows = sqlx::query_as::<_, (String, String)>(
        "SELECT worker_id, status FROM workers WHERE worker_id = $1 OR worker_id = $2 ORDER BY worker_id",
    )
    .bind(&worker_a)
    .bind(&worker_b)
    .fetch_all(&*pool)
    .await
    .expect("worker rows should load");
    let active_before = manager.get_active().await.expect("active workers should load before unregister");
    assert_eq!(worker_rows.len(), 2, "workers={worker_rows:?}");
    assert!(active_before.iter().any(|worker| worker.worker_id == worker_a));
    assert!(active_before.iter().any(|worker| worker.worker_id == worker_b));

    assert_eq!(
        manager.get_replication_position(&worker_a, "events").await.expect("worker_a replication position should load"),
        Some(10)
    );
    assert_eq!(
        manager.get_replication_position(&worker_b, "events").await.expect("worker_b replication position should load"),
        Some(20)
    );

    manager
        .update_replication_position(&worker_a, "events", 15)
        .await
        .expect("worker_a replication position should advance");

    assert_eq!(
        manager.get_replication_position(&worker_a, "events").await.expect("worker_a advanced position should load"),
        Some(15)
    );
    assert_eq!(
        manager.get_replication_position(&worker_b, "events").await.expect("worker_b position should remain isolated"),
        Some(20)
    );

    manager.unregister(&worker_a).await.expect("worker unregister should succeed");

    let active_after = manager.get_active().await.expect("active workers should load after unregister");
    assert!(!active_after.iter().any(|worker| worker.worker_id == worker_a));
    assert!(active_after.iter().any(|worker| worker.worker_id == worker_b));

    assert_eq!(
        manager
            .get_replication_position(&worker_a, "events")
            .await
            .expect("stopped worker position should remain queryable"),
        Some(15)
    );
    assert_eq!(
        manager
            .get_replication_position(&worker_b, "events")
            .await
            .expect("running worker position should remain queryable"),
        Some(20)
    );

    sqlx::query("DELETE FROM replication_positions WHERE worker_id = $1 OR worker_id = $2")
        .bind(&worker_a)
        .bind(&worker_b)
        .execute(&*pool)
        .await
        .expect("replication position cleanup should succeed");
    sqlx::query("DELETE FROM workers WHERE worker_id = $1 OR worker_id = $2")
        .bind(&worker_a)
        .bind(&worker_b)
        .execute(&*pool)
        .await
        .expect("worker cleanup should succeed");
}

#[tokio::test]
async fn test_stopped_heartbeat_requeues_running_tasks_and_removes_worker_from_lb_candidates() {
    let pool = worker_test_pool().await;
    let storage = Arc::new(WorkerStorage::new(&pool));
    let load_balancer = Arc::new(WorkerLoadBalancer::new(LoadBalanceStrategy::LeastConnections));
    let manager = WorkerManager::new(storage, "test-server".to_string()).with_load_balancer(load_balancer.clone());

    let suffix = uuid::Uuid::new_v4().to_string();
    let worker_a = format!("worker-heartbeat-stop-a-{suffix}");
    let worker_b = format!("worker-heartbeat-stop-b-{suffix}");

    for (worker_id, port) in [(&worker_a, 8113_u16), (&worker_b, 8114_u16)] {
        manager
            .register(RegisterWorkerRequest {
                worker_id: worker_id.clone(),
                worker_name: worker_id.clone(),
                worker_type: WorkerType::Frontend,
                host: "127.0.0.1".to_string(),
                port,
                config: None,
                metadata: None,
                version: Some("test".to_string()),
            })
            .await
            .expect("worker registration should succeed");

        manager
            .heartbeat(worker_id, WorkerStatus::Running, None)
            .await
            .expect("worker heartbeat should mark worker running");
    }

    let task = manager
        .assign_task(AssignTaskRequest {
            task_type: "http".to_string(),
            task_data: json!({ "kind": "stopped-heartbeat-requeue" }),
            priority: Some(20),
            preferred_worker_id: Some(worker_a.clone()),
        })
        .await
        .expect("task assignment should succeed");

    load_balancer
        .update_worker_load(
            &worker_a,
            WorkerLoadStats {
                worker_id: worker_a.clone(),
                active_connections: 0,
                pending_tasks: 0,
                cpu_usage: 0.1,
                memory_usage: 0.1,
                last_update_ts: current_timestamp_millis(),
            },
        )
        .await;
    load_balancer
        .update_worker_load(
            &worker_b,
            WorkerLoadStats {
                worker_id: worker_b.clone(),
                active_connections: 10,
                pending_tasks: 5,
                cpu_usage: 0.6,
                memory_usage: 0.6,
                last_update_ts: current_timestamp_millis(),
            },
        )
        .await;

    manager.heartbeat(&worker_a, WorkerStatus::Stopped, None).await.expect("stopped heartbeat should succeed");

    let after_stopped = sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT status, assigned_worker_id FROM worker_task_assignments WHERE task_id = $1",
    )
    .bind(&task.task_id)
    .fetch_one(&*pool)
    .await
    .expect("task should remain visible after stopped heartbeat");
    assert_eq!(after_stopped.0, "pending");
    assert_eq!(after_stopped.1, None);

    let active_after = manager.get_active().await.expect("active workers should load after stopped heartbeat");
    assert!(!active_after.iter().any(|worker| worker.worker_id == worker_a));
    assert!(active_after.iter().any(|worker| worker.worker_id == worker_b));

    let selected =
        manager.select_worker_for_task("http").await.expect("worker selection should succeed after stopped heartbeat");
    assert_eq!(selected.as_deref(), Some(worker_b.as_str()));

    let reclaimed = manager
        .claim_next_pending_task(&worker_b)
        .await
        .expect("healthy running peer should reclaim the requeued task");
    assert_eq!(reclaimed.task_id, task.task_id);
    assert_eq!(reclaimed.assigned_worker_id.as_deref(), Some(worker_b.as_str()));

    sqlx::query("DELETE FROM worker_task_assignments WHERE task_id = $1")
        .bind(&task.task_id)
        .execute(&*pool)
        .await
        .expect("task cleanup should succeed");
    sqlx::query("DELETE FROM workers WHERE worker_id = $1 OR worker_id = $2")
        .bind(&worker_a)
        .bind(&worker_b)
        .execute(&*pool)
        .await
        .expect("worker cleanup should succeed");
}

#[tokio::test]
async fn test_stopping_heartbeat_drains_inflight_task_but_rejects_new_selection_and_claims() {
    let pool = worker_test_pool().await;
    let storage = Arc::new(WorkerStorage::new(&pool));
    let load_balancer = Arc::new(WorkerLoadBalancer::new(LoadBalanceStrategy::LeastConnections));
    let manager = WorkerManager::new(storage, "test-server".to_string()).with_load_balancer(load_balancer.clone());

    let suffix = uuid::Uuid::new_v4().to_string();
    let worker_a = format!("worker-heartbeat-drain-a-{suffix}");
    let worker_b = format!("worker-heartbeat-drain-b-{suffix}");

    for (worker_id, port) in [(&worker_a, 8115_u16), (&worker_b, 8116_u16)] {
        manager
            .register(RegisterWorkerRequest {
                worker_id: worker_id.clone(),
                worker_name: worker_id.clone(),
                worker_type: WorkerType::Frontend,
                host: "127.0.0.1".to_string(),
                port,
                config: None,
                metadata: None,
                version: Some("test".to_string()),
            })
            .await
            .expect("worker registration should succeed");

        manager
            .heartbeat(worker_id, WorkerStatus::Running, None)
            .await
            .expect("worker heartbeat should mark worker running");
    }

    let inflight_task = manager
        .assign_task(AssignTaskRequest {
            task_type: "http".to_string(),
            task_data: json!({ "kind": "stopping-heartbeat-drain" }),
            priority: Some(30),
            preferred_worker_id: Some(worker_a.clone()),
        })
        .await
        .expect("task assignment should succeed");

    load_balancer
        .update_worker_load(
            &worker_a,
            WorkerLoadStats {
                worker_id: worker_a.clone(),
                active_connections: 0,
                pending_tasks: 0,
                cpu_usage: 0.1,
                memory_usage: 0.1,
                last_update_ts: current_timestamp_millis(),
            },
        )
        .await;
    load_balancer
        .update_worker_load(
            &worker_b,
            WorkerLoadStats {
                worker_id: worker_b.clone(),
                active_connections: 10,
                pending_tasks: 5,
                cpu_usage: 0.6,
                memory_usage: 0.6,
                last_update_ts: current_timestamp_millis(),
            },
        )
        .await;

    manager.heartbeat(&worker_a, WorkerStatus::Stopping, None).await.expect("stopping heartbeat should succeed");

    let inflight_after_stopping = sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT status, assigned_worker_id FROM worker_task_assignments WHERE task_id = $1",
    )
    .bind(&inflight_task.task_id)
    .fetch_one(&*pool)
    .await
    .expect("inflight task should remain visible after stopping heartbeat");
    assert_eq!(inflight_after_stopping.0, "running");
    assert_eq!(inflight_after_stopping.1.as_deref(), Some(worker_a.as_str()));

    let active_after = manager.get_active().await.expect("active workers should load after stopping heartbeat");
    assert!(!active_after.iter().any(|worker| worker.worker_id == worker_a));
    assert!(active_after.iter().any(|worker| worker.worker_id == worker_b));

    let selected =
        manager.select_worker_for_task("http").await.expect("worker selection should succeed after stopping heartbeat");
    assert_eq!(selected.as_deref(), Some(worker_b.as_str()));

    let pending_task = manager
        .assign_task(AssignTaskRequest {
            task_type: "http".to_string(),
            task_data: json!({ "kind": "stopping-heartbeat-pending" }),
            priority: Some(40),
            preferred_worker_id: None,
        })
        .await
        .expect("pending task assignment should succeed");

    let stopping_claim_err =
        manager.claim_next_pending_task(&worker_a).await.expect_err("stopping worker must not claim new tasks");
    assert!(stopping_claim_err.is_conflict());
    assert!(stopping_claim_err.to_string().contains("is not running"));

    let reclaimed = manager
        .claim_next_pending_task(&worker_b)
        .await
        .expect("running peer should claim pending task while stopping worker drains");
    assert_eq!(reclaimed.task_id, pending_task.task_id);
    assert_eq!(reclaimed.assigned_worker_id.as_deref(), Some(worker_b.as_str()));

    sqlx::query("DELETE FROM worker_task_assignments WHERE task_id = $1 OR task_id = $2")
        .bind(&inflight_task.task_id)
        .bind(&pending_task.task_id)
        .execute(&*pool)
        .await
        .expect("task cleanup should succeed");
    sqlx::query("DELETE FROM workers WHERE worker_id = $1 OR worker_id = $2")
        .bind(&worker_a)
        .bind(&worker_b)
        .execute(&*pool)
        .await
        .expect("worker cleanup should succeed");
}

#[tokio::test]
async fn test_error_heartbeat_requeues_running_tasks_and_removes_worker_from_lb_candidates() {
    let pool = worker_test_pool().await;
    let storage = Arc::new(WorkerStorage::new(&pool));
    let load_balancer = Arc::new(WorkerLoadBalancer::new(LoadBalanceStrategy::LeastConnections));
    let manager = WorkerManager::new(storage, "test-server".to_string()).with_load_balancer(load_balancer.clone());

    let suffix = uuid::Uuid::new_v4().to_string();
    let worker_a = format!("worker-heartbeat-error-a-{suffix}");
    let worker_b = format!("worker-heartbeat-error-b-{suffix}");

    for (worker_id, port) in [(&worker_a, 8117_u16), (&worker_b, 8118_u16)] {
        manager
            .register(RegisterWorkerRequest {
                worker_id: worker_id.clone(),
                worker_name: worker_id.clone(),
                worker_type: WorkerType::Frontend,
                host: "127.0.0.1".to_string(),
                port,
                config: None,
                metadata: None,
                version: Some("test".to_string()),
            })
            .await
            .expect("worker registration should succeed");

        manager
            .heartbeat(worker_id, WorkerStatus::Running, None)
            .await
            .expect("worker heartbeat should mark worker running");
    }

    let task = manager
        .assign_task(AssignTaskRequest {
            task_type: "http".to_string(),
            task_data: json!({ "kind": "error-heartbeat-requeue" }),
            priority: Some(20),
            preferred_worker_id: Some(worker_a.clone()),
        })
        .await
        .expect("task assignment should succeed");

    load_balancer
        .update_worker_load(
            &worker_a,
            WorkerLoadStats {
                worker_id: worker_a.clone(),
                active_connections: 0,
                pending_tasks: 0,
                cpu_usage: 0.1,
                memory_usage: 0.1,
                last_update_ts: current_timestamp_millis(),
            },
        )
        .await;
    load_balancer
        .update_worker_load(
            &worker_b,
            WorkerLoadStats {
                worker_id: worker_b.clone(),
                active_connections: 10,
                pending_tasks: 5,
                cpu_usage: 0.6,
                memory_usage: 0.6,
                last_update_ts: current_timestamp_millis(),
            },
        )
        .await;

    manager.heartbeat(&worker_a, WorkerStatus::Error, None).await.expect("error heartbeat should succeed");

    let after_error = sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT status, assigned_worker_id FROM worker_task_assignments WHERE task_id = $1",
    )
    .bind(&task.task_id)
    .fetch_one(&*pool)
    .await
    .expect("task should remain visible after error heartbeat");
    assert_eq!(after_error.0, "pending");
    assert_eq!(after_error.1, None);

    let active_after = manager.get_active().await.expect("active workers should load after error heartbeat");
    assert!(!active_after.iter().any(|worker| worker.worker_id == worker_a));
    assert!(active_after.iter().any(|worker| worker.worker_id == worker_b));

    let selected =
        manager.select_worker_for_task("http").await.expect("worker selection should succeed after error heartbeat");
    assert_eq!(selected.as_deref(), Some(worker_b.as_str()));

    let reclaimed = manager
        .claim_next_pending_task(&worker_b)
        .await
        .expect("healthy running peer should reclaim the requeued task");
    assert_eq!(reclaimed.task_id, task.task_id);
    assert_eq!(reclaimed.assigned_worker_id.as_deref(), Some(worker_b.as_str()));

    sqlx::query("DELETE FROM worker_task_assignments WHERE task_id = $1")
        .bind(&task.task_id)
        .execute(&*pool)
        .await
        .expect("task cleanup should succeed");
    sqlx::query("DELETE FROM workers WHERE worker_id = $1 OR worker_id = $2")
        .bind(&worker_a)
        .bind(&worker_b)
        .execute(&*pool)
        .await
        .expect("worker cleanup should succeed");
}

#[tokio::test]
async fn test_select_worker_for_task_falls_back_from_unhealthy_lb_choice_to_healthy_candidate() {
    let pool = worker_test_pool().await;
    let storage = Arc::new(WorkerStorage::new(&pool));
    let load_balancer = Arc::new(WorkerLoadBalancer::new(LoadBalanceStrategy::LeastConnections));
    let health_checker = Arc::new(HealthChecker::new(HealthCheckConfig::default()));
    let manager = WorkerManager::new(storage, "test-server".to_string())
        .with_load_balancer(load_balancer.clone())
        .with_health_checker(health_checker.clone());

    let suffix = uuid::Uuid::new_v4().to_string();
    let healthy_worker = format!("worker-healthy-{suffix}");
    let unhealthy_worker = format!("worker-unhealthy-{suffix}");

    for (worker_id, port) in [(&healthy_worker, 8121_u16), (&unhealthy_worker, 8122_u16)] {
        manager
            .register(RegisterWorkerRequest {
                worker_id: worker_id.clone(),
                worker_name: worker_id.clone(),
                worker_type: WorkerType::Frontend,
                host: "127.0.0.1".to_string(),
                port,
                config: None,
                metadata: None,
                version: Some("test".to_string()),
            })
            .await
            .expect("worker registration should succeed");

        manager
            .heartbeat(worker_id, WorkerStatus::Running, None)
            .await
            .expect("worker heartbeat should mark worker running");

        let running_info = manager
            .get(worker_id)
            .await
            .expect("worker lookup should succeed")
            .expect("worker should exist after registration");
        load_balancer.register_worker(running_info).await;
    }

    health_checker.check_health(&healthy_worker).await;

    load_balancer
        .update_worker_load(
            &healthy_worker,
            WorkerLoadStats {
                worker_id: healthy_worker.clone(),
                active_connections: 10,
                pending_tasks: 5,
                cpu_usage: 0.5,
                memory_usage: 0.5,
                last_update_ts: current_timestamp_millis(),
            },
        )
        .await;
    load_balancer
        .update_worker_load(
            &unhealthy_worker,
            WorkerLoadStats {
                worker_id: unhealthy_worker.clone(),
                active_connections: 0,
                pending_tasks: 0,
                cpu_usage: 0.1,
                memory_usage: 0.1,
                last_update_ts: current_timestamp_millis(),
            },
        )
        .await;

    let selected = manager.select_worker_for_task("http").await.expect("worker selection should succeed");
    assert_eq!(selected.as_deref(), Some(healthy_worker.as_str()));

    sqlx::query("DELETE FROM workers WHERE worker_id = $1 OR worker_id = $2")
        .bind(&healthy_worker)
        .bind(&unhealthy_worker)
        .execute(&*pool)
        .await
        .expect("worker cleanup should succeed");
}

#[tokio::test]
async fn test_select_worker_for_task_reselects_recovered_worker_after_health_restoration() {
    let pool = worker_test_pool().await;
    let storage = Arc::new(WorkerStorage::new(&pool));
    let load_balancer = Arc::new(WorkerLoadBalancer::new(LoadBalanceStrategy::LeastConnections));
    let health_checker = Arc::new(HealthChecker::new(HealthCheckConfig::default()));
    let manager = WorkerManager::new(storage, "test-server".to_string())
        .with_load_balancer(load_balancer.clone())
        .with_health_checker(health_checker.clone());

    let suffix = uuid::Uuid::new_v4().to_string();
    let stable_worker = format!("worker-stable-{suffix}");
    let recovering_worker = format!("worker-recovering-{suffix}");

    for (worker_id, port) in [(&stable_worker, 8131_u16), (&recovering_worker, 8132_u16)] {
        manager
            .register(RegisterWorkerRequest {
                worker_id: worker_id.clone(),
                worker_name: worker_id.clone(),
                worker_type: WorkerType::Frontend,
                host: "127.0.0.1".to_string(),
                port,
                config: None,
                metadata: None,
                version: Some("test".to_string()),
            })
            .await
            .expect("worker registration should succeed");

        manager
            .heartbeat(worker_id, WorkerStatus::Running, None)
            .await
            .expect("worker heartbeat should mark worker running");

        let running_info = manager
            .get(worker_id)
            .await
            .expect("worker lookup should succeed")
            .expect("worker should exist after registration");
        load_balancer.register_worker(running_info).await;
    }

    health_checker.check_health(&stable_worker).await;

    load_balancer
        .update_worker_load(
            &stable_worker,
            WorkerLoadStats {
                worker_id: stable_worker.clone(),
                active_connections: 8,
                pending_tasks: 4,
                cpu_usage: 0.6,
                memory_usage: 0.6,
                last_update_ts: current_timestamp_millis(),
            },
        )
        .await;
    load_balancer
        .update_worker_load(
            &recovering_worker,
            WorkerLoadStats {
                worker_id: recovering_worker.clone(),
                active_connections: 0,
                pending_tasks: 0,
                cpu_usage: 0.1,
                memory_usage: 0.1,
                last_update_ts: current_timestamp_millis(),
            },
        )
        .await;

    let before_recovery =
        manager.select_worker_for_task("http").await.expect("worker selection before recovery should succeed");
    assert_eq!(before_recovery.as_deref(), Some(stable_worker.as_str()));

    health_checker.check_health(&recovering_worker).await;

    let after_recovery =
        manager.select_worker_for_task("http").await.expect("worker selection after recovery should succeed");
    assert_eq!(after_recovery.as_deref(), Some(recovering_worker.as_str()));

    sqlx::query("DELETE FROM workers WHERE worker_id = $1 OR worker_id = $2")
        .bind(&stable_worker)
        .bind(&recovering_worker)
        .execute(&*pool)
        .await
        .expect("worker cleanup should succeed");
}
