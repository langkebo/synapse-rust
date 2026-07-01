// Worker API Tests - Real type assertions
// Rewritten from JSON smoke tests to use synapse_storage::worker real types

use synapse_storage::worker::{
    AssignTaskRequest, HeartbeatRequest, RegisterWorkerRequest, SendCommandRequest,
    StreamPosition, WorkerCapabilities, WorkerEvent, WorkerInfo, WorkerLoadStatsUpdate,
    WorkerRuntimeConfig, WorkerStatus, WorkerType,
};

// --- WorkerType enum tests ---

#[test]
fn test_worker_type_as_str_matches_spec() {
    assert_eq!(WorkerType::Master.as_str(), "master");
    assert_eq!(WorkerType::Frontend.as_str(), "frontend");
    assert_eq!(WorkerType::EventPersister.as_str(), "event_persister");
    assert_eq!(WorkerType::FederationSender.as_str(), "federation_sender");
    assert_eq!(WorkerType::MediaRepository.as_str(), "media_repository");
    assert_eq!(WorkerType::AppService.as_str(), "appservice");
}

#[test]
fn test_worker_type_from_str_roundtrip() {
    for wt in WorkerType::all() {
        let s = wt.as_str();
        let restored: WorkerType = s.parse().unwrap();
        assert_eq!(restored, wt, "roundtrip failed for {s}");
    }
}

#[test]
fn test_worker_type_from_str_invalid_returns_error() {
    assert!("invalid_type".parse::<WorkerType>().is_err());
    assert!("".parse::<WorkerType>().is_err());
}

#[test]
fn test_worker_type_can_handle_http() {
    assert!(WorkerType::Master.can_handle_http());
    assert!(WorkerType::Frontend.can_handle_http());
    assert!(WorkerType::Synchrotron.can_handle_http());
    assert!(!WorkerType::Background.can_handle_http());
    assert!(!WorkerType::Pusher.can_handle_http());
}

#[test]
fn test_worker_type_can_handle_federation() {
    assert!(WorkerType::Master.can_handle_federation());
    assert!(WorkerType::FederationSender.can_handle_federation());
    assert!(WorkerType::FederationReader.can_handle_federation());
    assert!(!WorkerType::Frontend.can_handle_federation());
}

#[test]
fn test_worker_type_can_persist_events() {
    assert!(WorkerType::Master.can_persist_events());
    assert!(WorkerType::EventPersister.can_persist_events());
    assert!(!WorkerType::Frontend.can_persist_events());
    assert!(!WorkerType::Pusher.can_persist_events());
}

#[test]
fn test_worker_type_all_returns_all_variants() {
    let all = WorkerType::all();
    assert_eq!(all.len(), 10, "expected 10 worker type variants");
}

#[test]
fn test_worker_type_responsibility_domains_not_empty() {
    for wt in WorkerType::all() {
        assert!(
            !wt.responsibility_domains().is_empty(),
            "{:?} should have responsibility domains",
            wt
        );
    }
}

#[test]
fn test_worker_type_serde_roundtrip() {
    let wt = WorkerType::EventPersister;
    let json = serde_json::to_string(&wt).unwrap();
    assert_eq!(json, "\"event_persister\"");
    let restored: WorkerType = serde_json::from_str(&json).unwrap();
    assert_eq!(restored, wt);
}

// --- WorkerStatus enum tests ---

#[test]
fn test_worker_status_as_str() {
    assert_eq!(WorkerStatus::Starting.as_str(), "starting");
    assert_eq!(WorkerStatus::Running.as_str(), "running");
    assert_eq!(WorkerStatus::Stopping.as_str(), "stopping");
    assert_eq!(WorkerStatus::Stopped.as_str(), "stopped");
    assert_eq!(WorkerStatus::Error.as_str(), "error");
}

#[test]
fn test_worker_status_from_str_roundtrip() {
    for variant in [
        WorkerStatus::Starting,
        WorkerStatus::Running,
        WorkerStatus::Stopping,
        WorkerStatus::Stopped,
        WorkerStatus::Error,
    ] {
        let s = variant.as_str();
        let restored: WorkerStatus = s.parse().unwrap();
        assert_eq!(restored, variant);
    }
}

#[test]
fn test_worker_status_from_str_invalid_returns_error() {
    assert!("invalid".parse::<WorkerStatus>().is_err());
}

#[test]
fn test_worker_status_serde_roundtrip() {
    let status = WorkerStatus::Running;
    let json = serde_json::to_string(&status).unwrap();
    assert_eq!(json, "\"running\"");
    let restored: WorkerStatus = serde_json::from_str(&json).unwrap();
    assert_eq!(restored, status);
}

// --- WorkerRuntimeConfig tests ---

#[test]
fn test_worker_runtime_config_default() {
    let config = WorkerRuntimeConfig::default();
    assert_eq!(config.worker_type, WorkerType::Frontend);
    assert_eq!(config.host, "localhost");
    assert_eq!(config.port, 8080);
    assert!(config.max_connections.is_some());
    assert!(config.heartbeat_interval_ms.is_some());
}

#[test]
fn test_worker_runtime_config_serde_roundtrip() {
    let config = WorkerRuntimeConfig {
        worker_id: "worker-001".to_string(),
        worker_name: "Test Worker".to_string(),
        worker_type: WorkerType::Synchrotron,
        host: "10.0.0.1".to_string(),
        port: 9090,
        master_host: Some("master.local".to_string()),
        master_port: Some(8000),
        replication_host: None,
        replication_port: None,
        http_port: Some(9091),
        bind_address: Some("0.0.0.0".to_string()),
        max_connections: Some(500),
        heartbeat_interval_ms: Some(3000),
        command_timeout_ms: Some(10000),
        extra_config: std::collections::HashMap::new(),
    };
    let json = serde_json::to_string(&config).unwrap();
    let restored: WorkerRuntimeConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.worker_id, config.worker_id);
    assert_eq!(restored.worker_type, config.worker_type);
    assert_eq!(restored.port, config.port);
}

// --- WorkerInfo struct tests ---

#[test]
fn test_worker_info_serde_roundtrip() {
    let info = WorkerInfo {
        id: 1,
        worker_id: "worker-001".to_string(),
        worker_name: "Test Worker".to_string(),
        worker_type: "frontend".to_string(),
        host: "localhost".to_string(),
        port: 8080,
        status: "running".to_string(),
        last_heartbeat_ts: Some(1_700_000_000_000),
        started_ts: 1_699_900_000_000,
        stopped_ts: None,
        config: serde_json::json!({}),
        metadata: serde_json::json!({}),
        version: Some("1.0.0".to_string()),
    };
    let json = serde_json::to_string(&info).unwrap();
    let restored: WorkerInfo = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.worker_id, info.worker_id);
    assert_eq!(restored.status, info.status);
}

// --- Request type tests ---

#[test]
fn test_register_worker_request_serde_roundtrip() {
    let req = RegisterWorkerRequest {
        worker_id: "worker-001".to_string(),
        worker_name: "Test".to_string(),
        worker_type: WorkerType::Frontend,
        host: "localhost".to_string(),
        port: 8080,
        config: None,
        metadata: None,
        version: Some("1.0.0".to_string()),
    };
    let json = serde_json::to_string(&req).unwrap();
    let restored: RegisterWorkerRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.worker_id, req.worker_id);
    assert_eq!(restored.worker_type, req.worker_type);
}

#[test]
fn test_send_command_request_serde_roundtrip() {
    let req = SendCommandRequest {
        target_worker_id: "worker-001".to_string(),
        command_type: "sync".to_string(),
        command_data: serde_json::json!({}),
        priority: Some(5),
        max_retries: Some(3),
    };
    let json = serde_json::to_string(&req).unwrap();
    let restored: SendCommandRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.target_worker_id, req.target_worker_id);
    assert_eq!(restored.command_type, "sync");
}

#[test]
fn test_assign_task_request_serde_roundtrip() {
    let req = AssignTaskRequest {
        task_type: "event_processing".to_string(),
        task_data: serde_json::json!({"event_id": "$event:localhost"}),
        priority: Some(1),
        preferred_worker_id: Some("worker-001".to_string()),
    };
    let json = serde_json::to_string(&req).unwrap();
    let restored: AssignTaskRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.task_type, "event_processing");
}

#[test]
fn test_heartbeat_request_serde_roundtrip() {
    let req = HeartbeatRequest {
        worker_id: "worker-001".to_string(),
        status: WorkerStatus::Running,
        load_stats: Some(WorkerLoadStatsUpdate {
            cpu_usage: Some(45.5),
            memory_usage: Some(1024),
            active_connections: Some(100),
            requests_per_second: None,
            average_latency_ms: None,
            queue_depth: Some(5),
        }),
    };
    let json = serde_json::to_string(&req).unwrap();
    let restored: HeartbeatRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.status, WorkerStatus::Running);
    assert!(restored.load_stats.is_some());
}

// --- StreamPosition tests ---

#[test]
fn test_stream_position_serde_roundtrip() {
    let pos = StreamPosition {
        stream_name: "events".to_string(),
        position: 5000,
    };
    let json = serde_json::to_string(&pos).unwrap();
    let restored: StreamPosition = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.stream_name, pos.stream_name);
    assert_eq!(restored.position, pos.position);
}

// --- WorkerEvent tests ---

#[test]
fn test_worker_event_serde_roundtrip() {
    let event = WorkerEvent {
        id: 1,
        event_id: "evt-001".to_string(),
        stream_id: 100,
        event_type: "status_change".to_string(),
        room_id: Some("!room:example.com".to_string()),
        sender: Some("@admin:example.com".to_string()),
        event_data: serde_json::json!({"old": "starting", "new": "running"}),
        created_ts: 1_700_000_000_000,
        processed_by: Some(vec!["worker-001".to_string()]),
    };
    let json = serde_json::to_string(&event).unwrap();
    let restored: WorkerEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.event_type, event.event_type);
    assert_eq!(restored.stream_id, event.stream_id);
}

// --- WorkerCapabilities tests ---

#[test]
fn test_worker_capabilities_default() {
    let caps = WorkerCapabilities::default();
    assert!(!caps.can_handle_http);
    assert_eq!(caps.max_concurrent_requests, 0);
    assert!(caps.supported_protocols.is_empty());
}

#[test]
fn test_worker_capabilities_serde_roundtrip() {
    let caps = WorkerCapabilities {
        can_handle_http: true,
        can_handle_federation: false,
        can_persist_events: true,
        can_send_push: false,
        can_handle_media: true,
        can_run_background_tasks: false,
        max_concurrent_requests: 1000,
        supported_protocols: vec!["replication".to_string(), "http".to_string()],
    };
    let json = serde_json::to_string(&caps).unwrap();
    let restored: WorkerCapabilities = serde_json::from_str(&json).unwrap();
    assert!(restored.can_handle_http);
    assert!(!restored.can_handle_federation);
    assert_eq!(restored.max_concurrent_requests, 1000);
    assert_eq!(restored.supported_protocols.len(), 2);
}

#[test]
fn test_worker_capabilities_for_type_master() {
    let caps = WorkerCapabilities::for_type(&WorkerType::Master);
    // Master can do everything
    assert!(caps.can_handle_http);
    assert!(caps.can_handle_federation);
    assert!(caps.can_persist_events);
}
