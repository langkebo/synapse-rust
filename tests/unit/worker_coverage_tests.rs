#![allow(clippy::unwrap_used, clippy::expect_used)]
// Worker Module Additional Coverage Tests
// Purpose: Increase test coverage for worker module components

use std::str::FromStr;
use synapse_services::worker::protocol::*;
use synapse_services::worker::types::*;

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
    let cmd = ReplicationCommand::Rdata { stream_name: "events".to_string(), token: "100".to_string(), rows: vec![] };
    assert_eq!(cmd.to_string(), "RDATA events 100");
}

#[test]
fn test_replication_command_sync() {
    let cmd = ReplicationCommand::Sync { stream_name: "events".to_string(), position: 100 };
    assert_eq!(cmd.to_string(), "SYNC events 100");
}

#[test]
fn test_replication_command_user_sync_online() {
    let cmd = ReplicationCommand::UserSync { user_id: "@user:example.com".to_string(), state: UserSyncState::Online };
    assert_eq!(cmd.to_string(), "USER_SYNC @user:example.com Online");
}

#[test]
fn test_replication_command_user_sync_offline() {
    let cmd = ReplicationCommand::UserSync { user_id: "@user:example.com".to_string(), state: UserSyncState::Offline };
    assert_eq!(cmd.to_string(), "USER_SYNC @user:example.com Offline");
}

#[test]
fn test_replication_command_federation_ack() {
    let cmd = ReplicationCommand::FederationAck { origin: "example.com".to_string() };
    assert_eq!(cmd.to_string(), "FEDERATION_ACK example.com");
}

#[test]
fn test_replication_command_remove_pushers() {
    let cmd = ReplicationCommand::RemovePushers { app_id: "app_id".to_string(), push_key: "push_key".to_string() };
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
        ReplicationCommand::Rdata { stream_name: "events".to_string(), token: "100".to_string(), rows: vec![] }
    );
}

#[test]
fn test_parse_sync() {
    let cmd = ReplicationCommand::parse("SYNC events 100").unwrap();
    assert_eq!(cmd, ReplicationCommand::Sync { stream_name: "events".to_string(), position: 100 });
}

// Note: USER_SYNC, FEDERATION_ACK, and REMOVE_PUSHERS don't have parse implementations
// These are tested via serialization instead

#[test]
fn test_user_sync_serialization() {
    let cmd = ReplicationCommand::UserSync { user_id: "@user:example.com".to_string(), state: UserSyncState::Online };
    let json = serde_json::to_string(&cmd).unwrap();
    assert!(json.contains("user_sync"));
}

#[test]
fn test_federation_ack_serialization() {
    let cmd = ReplicationCommand::FederationAck { origin: "example.com".to_string() };
    let json = serde_json::to_string(&cmd).unwrap();
    assert!(json.contains("federation_ack"));
}

#[test]
fn test_remove_pushers_serialization() {
    let cmd = ReplicationCommand::RemovePushers { app_id: "app_id".to_string(), push_key: "push_key".to_string() };
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
    let event = ReplicationEvent::Federation { stream_id: 1, origin: "example.com".to_string(), events: vec![] };

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
    let event = ReplicationEvent::Backfill { stream_id: 1, room_id: "!room:example.com".to_string(), events: vec![] };

    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("backfill"));
}

// ============== Presence State Tests ==============

#[test]
fn test_presence_state_serialization() {
    let states = vec![PresenceState::Online, PresenceState::Unavailable, PresenceState::Offline, PresenceState::Busy];

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
        // `can_handle_http` / `can_handle_federation` and
        // `supported_protocols` are two views of the same fact (the worker
        // models expose both, so consumers may read either). Assert they agree
        // instead of the `P || !P` tautology that used to stand here, which no
        // change to `for_type` could ever falsify.
        assert_eq!(
            caps.can_handle_http,
            caps.supported_protocols.iter().any(|p| p == "matrix"),
            "{wt:?}: can_handle_http must mirror the advertised matrix protocol"
        );
        assert_eq!(
            caps.can_handle_federation,
            caps.supported_protocols.iter().any(|p| p == "federation"),
            "{wt:?}: can_handle_federation must mirror the advertised federation protocol"
        );
        assert!(caps.max_concurrent_requests > 0, "{wt:?}: a concrete worker type must accept some concurrency");
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
