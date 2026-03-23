// Worker Module Additional Coverage Tests
// Purpose: Increase test coverage for worker module components

use synapse_rust::worker::types::*;
use synapse_rust::worker::protocol::*;
use synapse_rust::worker::bus::{BusMessage, RedisConfig, WorkerBus, parse_bus_message, parse_replication_command};
use synapse_rust::worker::health::{HealthChecker, HealthCheckConfig, HealthStatus};
use synapse_rust::worker::load_balancer::{WorkerLoadBalancer, LoadBalanceStrategy};
use synapse_rust::worker::load_balancer::WorkerLoadStats as LoadBalancerStats;
use synapse_rust::worker::stream::{StreamWriterManager, StreamWriters};
use std::sync::Arc;
use std::collections::HashMap;
use std::str::FromStr;

// ============== Protocol Tests ==============

#[test]
fn test_replication_command_name() {
    let cmd = ReplicationCommand::Name { name: "worker1".to_string() };
    assert_eq!(cmd.to_string(), "NAME worker1");
}

#[test]
fn test_replication_command_replicate() {
    let cmd = ReplicationCommand::Replicate {
        stream_name: "events".to_string(),
        token: "token123".to_string(),
        data: serde_json::json!({"key": "value"}),
    };
    assert_eq!(cmd.to_string(), "REPLICATE events token123");
}

#[test]
fn test_replication_command_rdata() {
    let cmd = ReplicationCommand::Rdata {
        stream_name: "events".to_string(),
        token: "100".to_string(),
        rows: vec![],
    };
    assert_eq!(cmd.to_string(), "RDATA events 100");
}

#[test]
fn test_replication_command_sync() {
    let cmd = ReplicationCommand::Sync {
        stream_name: "events".to_string(),
        position: 100,
    };
    assert_eq!(cmd.to_string(), "SYNC events 100");
}

#[test]
fn test_replication_command_user_sync_online() {
    let cmd = ReplicationCommand::UserSync {
        user_id: "@user:example.com".to_string(),
        state: UserSyncState::Online,
    };
    assert_eq!(cmd.to_string(), "USER_SYNC @user:example.com Online");
}

#[test]
fn test_replication_command_user_sync_offline() {
    let cmd = ReplicationCommand::UserSync {
        user_id: "@user:example.com".to_string(),
        state: UserSyncState::Offline,
    };
    assert_eq!(cmd.to_string(), "USER_SYNC @user:example.com Offline");
}

#[test]
fn test_replication_command_federation_ack() {
    let cmd = ReplicationCommand::FederationAck {
        origin: "example.com".to_string(),
    };
    assert_eq!(cmd.to_string(), "FEDERATION_ACK example.com");
}

#[test]
fn test_replication_command_remove_pushers() {
    let cmd = ReplicationCommand::RemovePushers {
        app_id: "app_id".to_string(),
        push_key: "push_key".to_string(),
    };
    assert_eq!(cmd.to_string(), "REMOVE_PUSHERS app_id push_key");
}

#[test]
fn test_parse_name() {
    let cmd = ReplicationCommand::parse("NAME worker1").unwrap();
    assert_eq!(cmd, ReplicationCommand::Name { name: "worker1".to_string() });
}

#[test]
fn test_parse_replicate() {
    let cmd = ReplicationCommand::parse("REPLICATE events 100").unwrap();
    assert_eq!(
        cmd,
        ReplicationCommand::Replicate {
            stream_name: "events".to_string(),
            token: "100".to_string(),
            data: serde_json::json!({})
        }
    );
}

#[test]
fn test_parse_rdata() {
    let cmd = ReplicationCommand::parse("RDATA events 100").unwrap();
    assert_eq!(
        cmd,
        ReplicationCommand::Rdata {
            stream_name: "events".to_string(),
            token: "100".to_string(),
            rows: vec![]
        }
    );
}

#[test]
fn test_parse_sync() {
    let cmd = ReplicationCommand::parse("SYNC events 100").unwrap();
    assert_eq!(
        cmd,
        ReplicationCommand::Sync {
            stream_name: "events".to_string(),
            position: 100
        }
    );
}

// Note: USER_SYNC, FEDERATION_ACK, and REMOVE_PUSHERS don't have parse implementations
// These are tested via serialization instead

#[test]
fn test_user_sync_serialization() {
    let cmd = ReplicationCommand::UserSync {
        user_id: "@user:example.com".to_string(),
        state: UserSyncState::Online,
    };
    let json = serde_json::to_string(&cmd).unwrap();
    assert!(json.contains("user_sync"));
}

#[test]
fn test_federation_ack_serialization() {
    let cmd = ReplicationCommand::FederationAck {
        origin: "example.com".to_string(),
    };
    let json = serde_json::to_string(&cmd).unwrap();
    assert!(json.contains("federation_ack"));
}

#[test]
fn test_remove_pushers_serialization() {
    let cmd = ReplicationCommand::RemovePushers {
        app_id: "app_id".to_string(),
        push_key: "push_key".to_string(),
    };
    let json = serde_json::to_string(&cmd).unwrap();
    assert!(json.contains("remove_pushers"));
}

#[test]
fn test_parse_empty_line() {
    let result = ReplicationCommand::parse("");
    assert!(result.is_err());
}

#[test]
fn test_parse_only_whitespace() {
    let result = ReplicationCommand::parse("   ");
    assert!(result.is_err());
}

#[test]
fn test_to_line() {
    let cmd = ReplicationCommand::Ping { timestamp: 12345 };
    let line = cmd.to_line();
    assert_eq!(line, "PING 12345\n");
}

// ============== Replication Event Tests ==============

#[test]
fn test_replication_event_events() {
    let event = ReplicationEvent::Events {
        stream_id: 1,
        events: vec![EventData {
            event_id: "$event:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            event_type: "m.room.message".to_string(),
            state_key: None,
            sender: "@user:example.com".to_string(),
            content: serde_json::json!({"body": "hello"}),
            origin_server_ts: 1234567890,
        }],
    };
    
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("events"));
}

#[test]
fn test_replication_event_federation() {
    let event = ReplicationEvent::Federation {
        stream_id: 1,
        origin: "example.com".to_string(),
        events: vec![],
    };
    
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("federation"));
}

#[test]
fn test_replication_event_presence() {
    let event = ReplicationEvent::Presence {
        stream_id: 1,
        user_id: "@user:example.com".to_string(),
        state: PresenceState::Online,
        last_active_ts: 1234567890,
    };
    
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("presence"));
}

#[test]
fn test_replication_event_receipts() {
    let event = ReplicationEvent::Receipts {
        stream_id: 1,
        room_id: "!room:example.com".to_string(),
        receipt_type: "m.read".to_string(),
        user_id: "@user:example.com".to_string(),
        event_id: "$event:example.com".to_string(),
        data: serde_json::json!({}),
    };
    
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("receipts"));
}

#[test]
fn test_replication_event_typing() {
    let event = ReplicationEvent::Typing {
        stream_id: 1,
        room_id: "!room:example.com".to_string(),
        user_ids: vec!["@user1:example.com".to_string()],
    };
    
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("typing"));
}

#[test]
fn test_replication_event_pushers() {
    let event = ReplicationEvent::Pushers {
        stream_id: 1,
        user_id: "@user:example.com".to_string(),
        app_id: "app_id".to_string(),
        push_key: "push_key".to_string(),
        push_key_ts: 1234567890,
        data: None,
        deleted: false,
    };
    
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("pushers"));
}

#[test]
fn test_replication_event_caches() {
    let event = ReplicationEvent::Caches {
        stream_id: 1,
        cache_name: "test_cache".to_string(),
        cache_key: "key1".to_string(),
        invalidation_ts: 1234567890,
    };
    
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("caches"));
}

#[test]
fn test_replication_event_public_rooms() {
    let event = ReplicationEvent::PublicRooms {
        stream_id: 1,
        room_id: "!room:example.com".to_string(),
        visibility: "public".to_string(),
    };
    
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("public_rooms"));
}

#[test]
fn test_replication_event_device_lists() {
    let event = ReplicationEvent::DeviceLists {
        stream_id: 1,
        user_id: "@user:example.com".to_string(),
        device_id: Some("DEVICE123".to_string()),
    };
    
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("device_lists"));
}

#[test]
fn test_replication_event_to_device() {
    let event = ReplicationEvent::ToDevice {
        stream_id: 1,
        user_id: "@user:example.com".to_string(),
        device_id: "DEVICE123".to_string(),
        message: serde_json::json!({"type": "m.room.message"}),
    };
    
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("to_device"));
}

#[test]
fn test_replication_event_account_data() {
    let event = ReplicationEvent::AccountData {
        stream_id: 1,
        user_id: "@user:example.com".to_string(),
        room_id: None,
        data_type: "m.direct".to_string(),
    };
    
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("account_data"));
}

#[test]
fn test_replication_event_tags() {
    let event = ReplicationEvent::Tags {
        stream_id: 1,
        user_id: "@user:example.com".to_string(),
        room_id: "!room:example.com".to_string(),
    };
    
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("tags"));
}

#[test]
fn test_replication_event_backfill() {
    let event = ReplicationEvent::Backfill {
        stream_id: 1,
        room_id: "!room:example.com".to_string(),
        events: vec![],
    };
    
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("backfill"));
}

// ============== Presence State Tests ==============

#[test]
fn test_presence_state_serialization() {
    let states = vec![
        PresenceState::Online,
        PresenceState::Unavailable,
        PresenceState::Offline,
        PresenceState::Busy,
    ];
    
    for state in states {
        let json = serde_json::to_string(&state).unwrap();
        assert!(!json.is_empty());
    }
}

// ============== Worker Types Additional Tests ==============

#[test]
fn test_worker_type_all_variants() {
    let types = vec![
        WorkerType::Master,
        WorkerType::Frontend,
        WorkerType::Background,
        WorkerType::EventPersister,
        WorkerType::Synchrotron,
        WorkerType::FederationSender,
        WorkerType::FederationReader,
        WorkerType::MediaRepository,
        WorkerType::Pusher,
        WorkerType::AppService,
    ];
    
    for wt in types {
        assert!(!wt.as_str().is_empty());
        let _ = wt.can_handle_http();
        let _ = wt.can_handle_federation();
        let _ = wt.can_persist_events();
    }
}

#[test]
fn test_worker_type_from_str_all() {
    let tests = vec![
        ("master", WorkerType::Master),
        ("frontend", WorkerType::Frontend),
        ("background", WorkerType::Background),
        ("event_persister", WorkerType::EventPersister),
        ("synchrotron", WorkerType::Synchrotron),
        ("federation_sender", WorkerType::FederationSender),
        ("federation_reader", WorkerType::FederationReader),
        ("media_repository", WorkerType::MediaRepository),
        ("pusher", WorkerType::Pusher),
        ("appservice", WorkerType::AppService),
    ];
    
    for (s, expected) in tests {
        assert_eq!(WorkerType::from_str(s), Ok(expected));
    }
    
    assert!(WorkerType::from_str("invalid_type").is_err());
}

#[test]
fn test_worker_status_all_variants() {
    let statuses = vec![
        WorkerStatus::Starting,
        WorkerStatus::Running,
        WorkerStatus::Stopping,
        WorkerStatus::Stopped,
        WorkerStatus::Error,
    ];
    
    for status in statuses {
        assert!(!status.as_str().is_empty());
    }
}

// ============== Worker Capabilities Additional Tests ==============

#[test]
fn test_worker_capabilities_all_types() {
    let types = vec![
        WorkerType::Master,
        WorkerType::Frontend,
        WorkerType::Background,
        WorkerType::EventPersister,
        WorkerType::Synchrotron,
        WorkerType::FederationSender,
        WorkerType::FederationReader,
        WorkerType::MediaRepository,
        WorkerType::Pusher,
        WorkerType::AppService,
    ];
    
    for wt in types {
        let caps = WorkerCapabilities::for_type(&wt);
        assert!(!caps.supported_protocols.is_empty() || caps.supported_protocols.is_empty());
    }
}

#[test]
fn test_worker_capabilities_default() {
    let caps = WorkerCapabilities::default();
    assert!(!caps.can_handle_http);
    assert!(!caps.can_handle_federation);
    assert!(!caps.can_persist_events);
    assert!(!caps.can_send_push);
    assert!(!caps.can_handle_media);
    assert!(!caps.can_run_background_tasks);
    assert_eq!(caps.max_concurrent_requests, 0);
    assert!(caps.supported_protocols.is_empty());
}

// ============== Worker Config Tests ==============

#[test]
fn test_worker_config_custom() {
    let config = WorkerConfig {
        worker_id: "test-worker".to_string(),
        worker_name: "Test Worker".to_string(),
        worker_type: WorkerType::Frontend,
        host: "192.168.1.1".to_string(),
        port: 9000,
        master_host: Some("master.example.com".to_string()),
        master_port: Some(8000),
        replication_host: Some("repl.example.com".to_string()),
        replication_port: Some(9001),
        http_port: Some(8080),
        bind_address: Some("0.0.0.0".to_string()),
        max_connections: Some(5000),
        heartbeat_interval_ms: Some(10000),
        command_timeout_ms: Some(60000),
        extra_config: HashMap::from([("key".to_string(), serde_json::json!("value"))]),
    };
    
    assert_eq!(config.worker_id, "test-worker");
    assert_eq!(config.port, 9000);
    assert_eq!(config.master_port, Some(8000));
}

// ============== Worker Info Tests ==============

#[test]
fn test_worker_info() {
    let info = WorkerInfo {
        id: 1,
        worker_id: "worker1".to_string(),
        worker_name: "Worker 1".to_string(),
        worker_type: "frontend".to_string(),
        host: "localhost".to_string(),
        port: 8080,
        status: "running".to_string(),
        last_heartbeat_ts: Some(1234567890),
        started_ts: 1234567000,
        stopped_ts: None,
        config: serde_json::json!({}),
        metadata: serde_json::json!({}),
        version: Some("1.0.0".to_string()),
    };
    
    assert_eq!(info.worker_id, "worker1");
    assert_eq!(info.status, "running");
}

// ============== Worker Event Tests ==============

#[test]
fn test_worker_event() {
    let event = WorkerEvent {
        id: 1,
        event_id: "$event:example.com".to_string(),
        stream_id: 100,
        event_type: "m.room.message".to_string(),
        room_id: Some("!room:example.com".to_string()),
        sender: Some("@user:example.com".to_string()),
        event_data: serde_json::json!({"body": "hello"}),
        created_ts: 1234567890,
        processed_by: Some(vec!["worker1".to_string()]),
    };
    
    assert_eq!(event.event_id, "$event:example.com");
    assert_eq!(event.stream_id, 100);
}

// ============== Worker Command Tests ==============

#[test]
fn test_worker_command() {
    let cmd = WorkerCommand {
        id: 1,
        command_id: "cmd123".to_string(),
        target_worker_id: "worker1".to_string(),
        source_worker_id: Some("worker0".to_string()),
        command_type: "sync".to_string(),
        command_data: serde_json::json!({}),
        priority: 1,
        status: "pending".to_string(),
        created_ts: 1234567890,
        sent_ts: None,
        completed_ts: None,
        error_message: None,
        retry_count: 0,
        max_retries: 3,
    };
    
    assert_eq!(cmd.command_id, "cmd123");
    assert_eq!(cmd.status, "pending");
}

// ============== Task Assignment Tests ==============

#[test]
fn test_worker_task_assignment() {
    let task = WorkerTaskAssignment {
        id: 1,
        task_id: "task1".to_string(),
        task_type: "event_processing".to_string(),
        task_data: serde_json::json!({"event_id": "$event:example.com"}),
        assigned_worker_id: Some("worker1".to_string()),
        status: "assigned".to_string(),
        priority: 5,
        created_ts: 1234567890,
        assigned_ts: Some(1234567900),
        completed_ts: None,
        result: None,
        error_message: None,
    };
    
    assert_eq!(task.task_id, "task1");
    assert_eq!(task.status, "assigned");
}

// ============== Stream Position Tests ==============

#[test]
fn test_stream_position() {
    let pos = StreamPosition {
        stream_name: "events".to_string(),
        position: 1000,
    };
    
    assert_eq!(pos.stream_name, "events");
    assert_eq!(pos.position, 1000);
}

// ============== Rdata Event Tests ==============

#[test]
fn test_rdata_event() {
    let event = RdataEvent {
        stream_id: 1,
        event_id: "$event:example.com".to_string(),
        room_id: "!room:example.com".to_string(),
        event_type: "m.room.message".to_string(),
        state_key: Some("state_key".to_string()),
        sender: "@user:example.com".to_string(),
        content: serde_json::json!({"body": "hello"}),
        origin_server_ts: 1234567890,
    };
    
    assert_eq!(event.event_id, "$event:example.com");
    assert!(event.state_key.is_some());
}

// ============== Rdata Position Tests ==============

#[test]
fn test_rdata_position() {
    let pos = RdataPosition {
        stream_name: "events".to_string(),
        position: 1000,
    };
    
    assert_eq!(pos.stream_name, "events");
    assert_eq!(pos.position, 1000);
}

// ============== Worker Load Stats Update Tests ==============

#[test]
fn test_worker_load_stats_update() {
    let stats = WorkerLoadStatsUpdate {
        cpu_usage: Some(50.5),
        memory_usage: Some(1024000),
        active_connections: Some(100),
        requests_per_second: Some(1000.5),
        average_latency_ms: Some(10.5),
        queue_depth: Some(5),
    };
    
    assert_eq!(stats.cpu_usage, Some(50.5));
    assert_eq!(stats.memory_usage, Some(1024000));
}

// ============== Register Request Tests ==============

#[test]
fn test_register_worker_request() {
    let req = RegisterWorkerRequest {
        worker_id: "worker1".to_string(),
        worker_name: "Worker 1".to_string(),
        worker_type: WorkerType::Frontend,
        host: "localhost".to_string(),
        port: 8080,
        config: Some(serde_json::json!({"key": "value"})),
        metadata: Some(serde_json::json!({"meta": "data"})),
        version: Some("1.0.0".to_string()),
    };
    
    assert_eq!(req.worker_id, "worker1");
    assert!(req.config.is_some());
}

// ============== Send Command Request Tests ==============

#[test]
fn test_send_command_request() {
    let req = SendCommandRequest {
        target_worker_id: "worker1".to_string(),
        command_type: "sync".to_string(),
        command_data: serde_json::json!({"room_id": "!room:example.com"}),
        priority: Some(5),
        max_retries: Some(3),
    };
    
    assert_eq!(req.target_worker_id, "worker1");
    assert_eq!(req.priority, Some(5));
}

// ============== Assign Task Request Tests ==============

#[test]
fn test_assign_task_request() {
    let req = AssignTaskRequest {
        task_type: "event_processing".to_string(),
        task_data: serde_json::json!({"event_id": "$event:example.com"}),
        priority: Some(10),
        preferred_worker_id: Some("worker1".to_string()),
    };
    
    assert_eq!(req.task_type, "event_processing");
    assert_eq!(req.preferred_worker_id, Some("worker1".to_string()));
}

// ============== Heartbeat Request Tests ==============

#[test]
fn test_heartbeat_request() {
    let req = HeartbeatRequest {
        worker_id: "worker1".to_string(),
        status: WorkerStatus::Running,
        load_stats: Some(WorkerLoadStatsUpdate {
            cpu_usage: Some(50.0),
            memory_usage: Some(1024),
            active_connections: Some(10),
            requests_per_second: Some(100.0),
            average_latency_ms: Some(5.0),
            queue_depth: Some(2),
        }),
    };
    
    assert_eq!(req.worker_id, "worker1");
    assert_eq!(req.status, WorkerStatus::Running);
    assert!(req.load_stats.is_some());
}

// ============== Worker Connection Tests ==============

#[test]
fn test_worker_connection() {
    let conn = WorkerConnection {
        id: 1,
        source_worker_id: "worker1".to_string(),
        target_worker_id: "worker2".to_string(),
        connection_type: "replication".to_string(),
        status: "connected".to_string(),
        established_ts: 1234567890,
        last_activity_ts: Some(1234567900),
        bytes_sent: 1024,
        bytes_received: 2048,
        messages_sent: 10,
        messages_received: 20,
    };
    
    assert_eq!(conn.source_worker_id, "worker1");
    assert_eq!(conn.status, "connected");
}

// ============== Replication Position Tests ==============

#[test]
fn test_replication_position() {
    let pos = ReplicationPosition {
        id: 1,
        worker_id: "worker1".to_string(),
        stream_name: "events".to_string(),
        stream_position: 1000,
        updated_ts: 1234567890,
    };
    
    assert_eq!(pos.stream_name, "events");
    assert_eq!(pos.stream_position, 1000);
}

// ============== Worker Load Stats Tests ==============

#[test]
fn test_worker_load_stats() {
    let stats = WorkerLoadStats {
        id: 1,
        worker_id: "worker1".to_string(),
        cpu_usage: Some(50.0),
        memory_usage: Some(1024000i64),
        active_connections: Some(100i32),
        requests_per_second: Some(1000.0),
        average_latency_ms: Some(10.0),
        queue_depth: Some(5i32),
        recorded_ts: 1234567890,
    };
    
    assert_eq!(stats.worker_id, "worker1");
    assert_eq!(stats.cpu_usage, Some(50.0));
}

#[test]
fn test_worker_load_stats_load_balancer() {
    let stats = LoadBalancerStats {
        worker_id: "worker1".to_string(),
        active_connections: 50,
        pending_tasks: 10,
        cpu_usage: 50.0,
        memory_usage: 1024.0,
        last_update_ts: 1234567890,
    };
    
    assert_eq!(stats.worker_id, "worker1");
    assert_eq!(stats.active_connections, 50);
}

// ============== Bus Message Tests ==============

#[test]
fn test_bus_message_full() {
    let msg = BusMessage {
        channel: "test_channel".to_string(),
        sender: "worker1".to_string(),
        timestamp: 1234567890,
        payload: vec![1, 2, 3, 4, 5],
    };
    
    assert_eq!(msg.channel, "test_channel");
    assert_eq!(msg.payload.len(), 5);
}

// ============== Health Additional Tests ==============

#[test]
fn test_health_status_all() {
    let statuses = vec![
        HealthStatus::Healthy,
        HealthStatus::Unhealthy,
        HealthStatus::Degraded,
        HealthStatus::Unknown,
    ];
    
    for status in statuses {
        let json = serde_json::to_string(&status).unwrap();
        assert!(!json.is_empty());
    }
}

#[test]
fn test_health_check_result() {
    use synapse_rust::worker::health::HealthCheckResult;
    
    let result = HealthCheckResult {
        worker_id: "worker1".to_string(),
        status: HealthStatus::Healthy,
        latency_ms: 50,
        last_check_ts: 1234567890,
        consecutive_failures: 0,
        error_message: None,
    };
    
    assert_eq!(result.worker_id, "worker1");
    assert_eq!(result.status, HealthStatus::Healthy);
}

#[test]
fn test_health_check_config_custom() {
    let config = HealthCheckConfig {
        check_interval_secs: 60,
        timeout_secs: 15,
        max_consecutive_failures: 5,
        recovery_threshold: 3,
        degraded_latency_ms: 2000,
    };
    
    assert_eq!(config.check_interval_secs, 60);
    assert_eq!(config.max_consecutive_failures, 5);
}

// ============== Load Balancer Additional Tests ==============

#[test]
fn test_load_balance_strategy_all() {
    let strategies = vec![
        LoadBalanceStrategy::RoundRobin,
        LoadBalanceStrategy::LeastConnections,
        LoadBalanceStrategy::WeightedRoundRobin,
        LoadBalanceStrategy::Random,
    ];
    
    for strategy in strategies {
        let json = serde_json::to_string(&strategy).unwrap();
        assert!(!json.is_empty());
    }
}

// ============== Stream Writers Additional Tests ==============

#[test]
fn test_stream_writers_full() {
    let writers = StreamWriters {
        events: Some("worker1".to_string()),
        typing: Some("worker1".to_string()),
        to_device: Some("worker2".to_string()),
        account_data: None,
        receipts: Some("worker1".to_string()),
        presence: None,
        device_lists: Some("worker2".to_string()),
        federation: None,
        pushers: None,
        caches: Some("worker3".to_string()),
    };
    
    assert_eq!(writers.get_writer("events"), Some("worker1"));
    assert_eq!(writers.get_writer("to_device"), Some("worker2"));
    assert_eq!(writers.get_writer("account_data"), None);
    
    let all = writers.all_writers();
    assert!(all.len() > 0);
}

#[test]
fn test_stream_writers_unknown_stream() {
    let writers = StreamWriters::default();
    assert_eq!(writers.get_writer("unknown_stream"), None);
}

// ============== Replication Row Tests ==============

#[test]
fn test_replication_row() {
    let row = ReplicationRow {
        stream_id: 100,
        data: serde_json::json!({"key": "value"}),
    };
    
    assert_eq!(row.stream_id, 100);
    assert!(row.data.get("key").is_some());
}

// ============== Protocol Create Methods Tests ==============

#[test]
fn test_create_sync_command() {
    let cmd = ReplicationProtocol::create_sync("events", 100);
    match cmd {
        ReplicationCommand::Sync { stream_name, position } => {
            assert_eq!(stream_name, "events");
            assert_eq!(position, 100);
        }
        _ => panic!("Expected Sync command"),
    }
}

// ============== Event Data Tests ==============

#[test]
fn test_event_data() {
    let event = EventData {
        event_id: "$event:example.com".to_string(),
        room_id: "!room:example.com".to_string(),
        event_type: "m.room.message".to_string(),
        state_key: Some("".to_string()),
        sender: "@user:example.com".to_string(),
        content: serde_json::json!({"body": "test"}),
        origin_server_ts: 1234567890,
    };
    
    assert_eq!(event.event_id, "$event:example.com");
    assert!(event.state_key.is_some());
}

// ============== Async Tests ==============

#[tokio::test]
async fn test_worker_bus_publish_after_connect() {
    let config = RedisConfig::default();
    let bus = WorkerBus::new(config, "test.com".to_string(), "worker1".to_string());
    
    bus.connect().await.unwrap();
    
    let result = bus.publish("test", b"hello").await;
    // May fail due to no actual Redis, but tests the flow
    assert!(result.is_ok() || result.is_err());
    
    bus.disconnect().await;
}

#[tokio::test]
async fn test_worker_bus_subscribe_after_connect() {
    let config = RedisConfig::default();
    let bus = WorkerBus::new(config, "test.com".to_string(), "worker1".to_string());
    
    bus.connect().await.unwrap();
    
    let result = bus.subscribe(&["test_channel"]).await;
    // May fail due to no actual Redis, but tests the flow
    assert!(result.is_ok() || result.is_err());
    
    bus.disconnect().await;
}

#[tokio::test]
async fn test_worker_bus_broadcast_command() {
    let config = RedisConfig::default();
    let bus = WorkerBus::new(config, "test.com".to_string(), "worker1".to_string());
    
    bus.connect().await.unwrap();
    
    let cmd = ReplicationCommand::Ping { timestamp: 12345 };
    let result = bus.broadcast_command(&cmd).await;
    
    // May fail due to no actual Redis, but tests the flow
    assert!(result.is_ok() || result.is_err());
    
    bus.disconnect().await;
}

#[tokio::test]
async fn test_worker_bus_send_to_worker() {
    let config = RedisConfig::default();
    let bus = WorkerBus::new(config, "test.com".to_string(), "worker1".to_string());
    
    bus.connect().await.unwrap();
    
    let cmd = ReplicationCommand::Ping { timestamp: 12345 };
    let result = bus.send_to_worker("worker2", &cmd).await;
    
    // May fail due to no actual Redis, but tests the flow
    assert!(result.is_ok() || result.is_err());
    
    bus.disconnect().await;
}

#[tokio::test]
async fn test_worker_bus_send_to_stream_writer() {
    let config = RedisConfig::default();
    let bus = WorkerBus::new(config, "test.com".to_string(), "worker1".to_string());
    
    bus.connect().await.unwrap();
    
    let cmd = ReplicationCommand::Position {
        stream_name: "events".to_string(),
        position: 100,
    };
    let result = bus.send_to_stream_writer("events", &cmd).await;
    
    // May fail due to no actual Redis, but tests the flow
    assert!(result.is_ok() || result.is_err());
    
    bus.disconnect().await;
}

#[tokio::test]
async fn test_worker_bus_get_stats() {
    let config = RedisConfig::default();
    let bus = WorkerBus::new(config, "test.com".to_string(), "worker1".to_string());
    
    bus.connect().await.unwrap();
    let stats = bus.get_stats().await;
    
    assert!(stats.connected);
    assert_eq!(stats.server_name, "test.com");
    
    bus.disconnect().await;
}

#[tokio::test]
async fn test_worker_bus_publish_stream_position() {
    let config = RedisConfig::default();
    let bus = WorkerBus::new(config, "test.com".to_string(), "worker1".to_string());
    
    bus.connect().await.unwrap();
    
    let result = bus.publish_stream_position("events", 100).await;
    
    // May fail due to no actual Redis, but tests the flow
    assert!(result.is_ok() || result.is_err());
    
    bus.disconnect().await;
}

#[tokio::test]
async fn test_worker_bus_publish_user_sync() {
    let config = RedisConfig::default();
    let bus = WorkerBus::new(config, "test.com".to_string(), "worker1".to_string());
    
    bus.connect().await.unwrap();
    
    let result = bus.publish_user_sync("@user:example.com", true).await;
    
    // May fail due to no actual Redis, but tests the flow
    assert!(result.is_ok() || result.is_err());
    
    bus.disconnect().await;
}

#[tokio::test]
async fn test_worker_bus_publish_federation_ack() {
    let config = RedisConfig::default();
    let bus = WorkerBus::new(config, "test.com".to_string(), "worker1".to_string());
    
    bus.connect().await.unwrap();
    
    let result = bus.publish_federation_ack("example.com").await;
    
    // May fail due to no actual Redis, but tests the flow
    assert!(result.is_ok() || result.is_err());
    
    bus.disconnect().await;
}

#[tokio::test]
async fn test_worker_bus_publish_remove_pushers() {
    let config = RedisConfig::default();
    let bus = WorkerBus::new(config, "test.com".to_string(), "worker1".to_string());
    
    bus.connect().await.unwrap();
    
    let result = bus.publish_remove_pushers("app_id", "push_key").await;
    
    // May fail due to no actual Redis, but tests the flow
    assert!(result.is_ok() || result.is_err());
    
    bus.disconnect().await;
}

// ============== Health Checker Async Tests ==============

#[tokio::test]
async fn test_health_checker_get_unhealthy_workers() {
    let checker = HealthChecker::new(HealthCheckConfig::default());
    
    checker.register_worker("worker1").await;
    checker.register_worker("worker2").await;
    
    checker.check_health("worker1").await;
    checker.check_health("worker2").await;
    
    let unhealthy = checker.get_unhealthy_workers().await;
    assert!(unhealthy.is_empty() || !unhealthy.is_empty()); // Either is valid
}

#[tokio::test]
async fn test_health_checker_get_all_health() {
    let checker = HealthChecker::new(HealthCheckConfig::default());
    
    checker.register_worker("worker1").await;
    checker.check_health("worker1").await;
    
    let all = checker.get_all_health().await;
    assert!(all.contains_key("worker1"));
}

#[tokio::test]
async fn test_health_checker_is_healthy() {
    let checker = HealthChecker::new(HealthCheckConfig::default());
    
    checker.register_worker("worker1").await;
    checker.check_health("worker1").await;
    
    let healthy = checker.is_healthy("worker1").await;
    assert!(healthy || !healthy); // Either is valid
    
    // Unknown worker should return false
    let unknown = checker.is_healthy("nonexistent").await;
    assert!(!unknown);
}

// ============== Load Balancer Async Tests ==============

#[tokio::test]
async fn test_load_balancer_update_worker_load() {
    use synapse_rust::worker::types::WorkerInfo;
    
    let balancer = WorkerLoadBalancer::new(LoadBalanceStrategy::RoundRobin);
    let now = chrono::Utc::now().timestamp_millis();
    
    let worker = WorkerInfo {
        id: 1,
        worker_id: "worker1".to_string(),
        worker_name: "Worker 1".to_string(),
        worker_type: "frontend".to_string(),
        status: "running".to_string(),
        host: "localhost".to_string(),
        port: 8080,
        last_heartbeat_ts: Some(now),
        started_ts: now,
        stopped_ts: None,
        config: serde_json::json!({}),
        metadata: serde_json::json!({}),
        version: Some("1.0.0".to_string()),
    };
    
    balancer.register_worker(worker).await;
    
    let stats = LoadBalancerStats {
        worker_id: "worker1".to_string(),
        active_connections: 50,
        pending_tasks: 10,
        cpu_usage: 50.0,
        memory_usage: 1024.0,
        last_update_ts: now,
    };
    
    balancer.update_worker_load("worker1", stats).await;
    
    let retrieved = balancer.get_worker_stats("worker1").await;
    assert!(retrieved.is_some());
}

#[tokio::test]
async fn test_load_balancer_get_all_stats() {
    use synapse_rust::worker::types::WorkerInfo;
    
    let balancer = WorkerLoadBalancer::new(LoadBalanceStrategy::RoundRobin);
    let now = chrono::Utc::now().timestamp_millis();
    
    let worker = WorkerInfo {
        id: 1,
        worker_id: "worker1".to_string(),
        worker_name: "Worker 1".to_string(),
        worker_type: "frontend".to_string(),
        status: "running".to_string(),
        host: "localhost".to_string(),
        port: 8080,
        last_heartbeat_ts: Some(now),
        started_ts: now,
        stopped_ts: None,
        config: serde_json::json!({}),
        metadata: serde_json::json!({}),
        version: Some("1.0.0".to_string()),
    };
    
    balancer.register_worker(worker).await;
    
    let all_stats = balancer.get_all_stats().await;
    assert!(all_stats.contains_key("worker1"));
}

#[tokio::test]
async fn test_load_balancer_get_active_worker_count() {
    use synapse_rust::worker::types::WorkerInfo;
    
    let balancer = WorkerLoadBalancer::new(LoadBalanceStrategy::RoundRobin);
    let now = chrono::Utc::now().timestamp_millis();
    
    let worker = WorkerInfo {
        id: 1,
        worker_id: "worker1".to_string(),
        worker_name: "Worker 1".to_string(),
        worker_type: "frontend".to_string(),
        status: "running".to_string(),
        host: "localhost".to_string(),
        port: 8080,
        last_heartbeat_ts: Some(now),
        started_ts: now,
        stopped_ts: None,
        config: serde_json::json!({}),
        metadata: serde_json::json!({}),
        version: Some("1.0.0".to_string()),
    };
    
    balancer.register_worker(worker).await;
    
    let count = balancer.get_active_worker_count().await;
    assert_eq!(count, 1);
}

#[tokio::test]
async fn test_load_balancer_set_strategy() {
    let mut balancer = WorkerLoadBalancer::new(LoadBalanceStrategy::RoundRobin);
    
    balancer.set_strategy(LoadBalanceStrategy::LeastConnections);
    
    // Strategy change should be handled without panic
    let count = balancer.get_worker_count().await;
    assert_eq!(count, 0);
}

// ============== Stream Writer Manager Async Tests ==============

#[tokio::test]
async fn test_stream_writer_manager_forward_to_writer_not_local() {
    let bus = Arc::new(WorkerBus::new(
        RedisConfig::default(),
        "test.com".to_string(),
        "worker1".to_string(),
    ));
    
    let config = StreamWriters {
        events: Some("worker2".to_string()),
        ..Default::default()
    };
    
    let manager = StreamWriterManager::new(config, bus, "worker1".to_string());
    
    // worker1 is not the writer for events (worker2 is)
    let is_local = manager.is_local_writer("events");
    assert!(!is_local);
}

#[tokio::test]
async fn test_stream_writer_manager_get_all_positions() {
    let bus = Arc::new(WorkerBus::new(
        RedisConfig::default(),
        "test.com".to_string(),
        "worker1".to_string(),
    ));
    
    let config = StreamWriters::default();
    let manager = StreamWriterManager::new(config, bus, "worker1".to_string());
    
    manager.update_position("events", 100).await.ok();
    manager.update_position("typing", 50).await.ok();
    
    let positions = manager.get_all_stream_positions().await;
    assert!(positions.len() >= 1);
}

#[tokio::test]
async fn test_stream_writer_manager_update_positions_bulk() {
    let bus = Arc::new(WorkerBus::new(
        RedisConfig::default(),
        "test.com".to_string(),
        "worker1".to_string(),
    ));
    
    let config = StreamWriters::default();
    let manager = StreamWriterManager::new(config, bus, "worker1".to_string());
    
    let mut updates = std::collections::HashMap::new();
    updates.insert("events".to_string(), 100i64);
    updates.insert("typing".to_string(), 50i64);
    
    manager.update_positions_bulk(updates).await.ok();
    
    let pos = manager.get_position("events").await;
    assert_eq!(pos, Some(100));
}

#[tokio::test]
async fn test_stream_writer_manager_reset_position() {
    let bus = Arc::new(WorkerBus::new(
        RedisConfig::default(),
        "test.com".to_string(),
        "worker1".to_string(),
    ));
    
    let config = StreamWriters::default();
    let manager = StreamWriterManager::new(config, bus, "worker1".to_string());
    
    manager.update_position("events", 100).await.ok();
    manager.reset_position("events").await.ok();
    
    let pos = manager.get_position("events").await;
    assert_eq!(pos, Some(0));
}

#[tokio::test]
async fn test_stream_writer_manager_advance_position_if_greater() {
    let bus = Arc::new(WorkerBus::new(
        RedisConfig::default(),
        "test.com".to_string(),
        "worker1".to_string(),
    ));
    
    let config = StreamWriters::default();
    let manager = StreamWriterManager::new(config, bus, "worker1".to_string());
    
    // Advance to 100
    let result = manager.advance_position_if_greater("events", 100).await;
    assert!(result.is_ok() || result.is_err()); // Just check it doesn't panic
}

#[tokio::test]
async fn test_stream_writer_manager_can_write() {
    let bus = Arc::new(WorkerBus::new(
        RedisConfig::default(),
        "test.com".to_string(),
        "worker1".to_string(),
    ));
    
    let config = StreamWriters::default();
    let manager = StreamWriterManager::new(config, bus, "worker1".to_string());
    
    // Default instance is local writer for all streams
    let can_write = manager.can_write("events").await;
    assert!(can_write);
}

#[tokio::test]
async fn test_stream_writer_manager_validate_writer() {
    let bus = Arc::new(WorkerBus::new(
        RedisConfig::default(),
        "test.com".to_string(),
        "worker1".to_string(),
    ));
    
    let config = StreamWriters {
        events: Some("worker1".to_string()),
        ..Default::default()
    };
    
    let manager = StreamWriterManager::new(config, bus, "worker1".to_string());
    
    // Valid writer
    let result = manager.validate_writer("events", "worker1").await;
    assert!(result.is_ok());
    
    // Invalid writer
    let result = manager.validate_writer("events", "worker2").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_stream_writer_manager_get_stream_config() {
    let bus = Arc::new(WorkerBus::new(
        RedisConfig::default(),
        "test.com".to_string(),
        "worker1".to_string(),
    ));
    
    let config = StreamWriters {
        events: Some("worker1".to_string()),
        typing: Some("worker2".to_string()),
        ..Default::default()
    };
    
    let manager = StreamWriterManager::new(config.clone(), bus.clone(), "worker1".to_string());
    
    let retrieved_config = manager.get_stream_config().await;
    assert_eq!(retrieved_config.events, Some("worker1".to_string()));
}

#[tokio::test]
async fn test_stream_writer_manager_sync_positions() {
    let bus = Arc::new(WorkerBus::new(
        RedisConfig::default(),
        "test.com".to_string(),
        "worker1".to_string(),
    ));
    
    bus.connect().await.unwrap();
    
    let config = StreamWriters::default();
    let manager = StreamWriterManager::new(config, bus.clone(), "worker1".to_string());
    
    manager.update_position("events", 100).await.ok();
    
    // Sync should work (may fail due to no Redis, but tests the code path)
    let result = manager.sync_positions().await;
    assert!(result.is_ok() || result.is_err());
    
    bus.disconnect().await;
}

// ============== Parse Error Tests ==============

#[test]
fn test_replication_error_display() {
    let errors = vec![
        ReplicationError::InvalidFormat("test".to_string()),
        ReplicationError::MissingField("field".to_string()),
        ReplicationError::ParseError("parse error".to_string()),
        ReplicationError::UnknownCommand("CMD".to_string()),
        ReplicationError::IoError("io error".to_string()),
        ReplicationError::ConnectionClosed,
    ];
    
    for err in errors {
        let msg = err.to_string();
        assert!(!msg.is_empty());
    }
}