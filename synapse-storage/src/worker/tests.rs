use super::*;

fn all_worker_types() -> Vec<WorkerType> {
    vec![
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
    ]
}

fn all_worker_statuses() -> Vec<WorkerStatus> {
    vec![
        WorkerStatus::Starting,
        WorkerStatus::Running,
        WorkerStatus::Stopping,
        WorkerStatus::Stopped,
        WorkerStatus::Error,
    ]
}

// ------------------------------------------------------------------
// WorkerType tests
// ------------------------------------------------------------------

#[test]
fn test_worker_type_as_str() {
    assert_eq!(WorkerType::Master.as_str(), "master");
    assert_eq!(WorkerType::Frontend.as_str(), "frontend");
    assert_eq!(WorkerType::Background.as_str(), "background");
    assert_eq!(WorkerType::EventPersister.as_str(), "event_persister");
    assert_eq!(WorkerType::Synchrotron.as_str(), "synchrotron");
    assert_eq!(WorkerType::FederationSender.as_str(), "federation_sender");
    assert_eq!(WorkerType::FederationReader.as_str(), "federation_reader");
    assert_eq!(WorkerType::MediaRepository.as_str(), "media_repository");
    assert_eq!(WorkerType::Pusher.as_str(), "pusher");
    assert_eq!(WorkerType::AppService.as_str(), "appservice");
}

#[test]
fn test_worker_type_can_handle_http() {
    assert!(WorkerType::Master.can_handle_http());
    assert!(WorkerType::Frontend.can_handle_http());
    assert!(WorkerType::Synchrotron.can_handle_http());
    assert!(!WorkerType::Background.can_handle_http());
    assert!(!WorkerType::EventPersister.can_handle_http());
    assert!(!WorkerType::FederationSender.can_handle_http());
    assert!(!WorkerType::FederationReader.can_handle_http());
    assert!(!WorkerType::MediaRepository.can_handle_http());
    assert!(!WorkerType::Pusher.can_handle_http());
    assert!(!WorkerType::AppService.can_handle_http());
}

#[test]
fn test_worker_type_can_handle_federation() {
    assert!(WorkerType::Master.can_handle_federation());
    assert!(WorkerType::FederationSender.can_handle_federation());
    assert!(WorkerType::FederationReader.can_handle_federation());
    assert!(!WorkerType::Frontend.can_handle_federation());
    assert!(!WorkerType::Background.can_handle_federation());
    assert!(!WorkerType::EventPersister.can_handle_federation());
    assert!(!WorkerType::Synchrotron.can_handle_federation());
    assert!(!WorkerType::MediaRepository.can_handle_federation());
    assert!(!WorkerType::Pusher.can_handle_federation());
    assert!(!WorkerType::AppService.can_handle_federation());
}

#[test]
fn test_worker_type_can_persist_events() {
    assert!(WorkerType::Master.can_persist_events());
    assert!(WorkerType::EventPersister.can_persist_events());
    assert!(!WorkerType::Frontend.can_persist_events());
    assert!(!WorkerType::Background.can_persist_events());
    assert!(!WorkerType::Synchrotron.can_persist_events());
    assert!(!WorkerType::FederationSender.can_persist_events());
    assert!(!WorkerType::FederationReader.can_persist_events());
    assert!(!WorkerType::MediaRepository.can_persist_events());
    assert!(!WorkerType::Pusher.can_persist_events());
    assert!(!WorkerType::AppService.can_persist_events());
}

#[test]
fn test_worker_type_responsibility_domains() {
    assert_eq!(
        WorkerType::Master.responsibility_domains(),
        &["client_http", "federation", "event_persistence", "background_jobs", "media", "push"]
    );
    assert_eq!(WorkerType::Frontend.responsibility_domains(), &["client_http"]);
    assert_eq!(WorkerType::Background.responsibility_domains(), &["background_jobs"]);
    assert_eq!(WorkerType::EventPersister.responsibility_domains(), &["event_persistence"]);
    assert_eq!(WorkerType::Synchrotron.responsibility_domains(), &["sync_http"]);
    assert_eq!(WorkerType::FederationSender.responsibility_domains(), &["federation_egress"]);
    assert_eq!(WorkerType::FederationReader.responsibility_domains(), &["federation_ingress"]);
    assert_eq!(WorkerType::MediaRepository.responsibility_domains(), &["media_http"]);
    assert_eq!(WorkerType::Pusher.responsibility_domains(), &["push_delivery"]);
    assert_eq!(WorkerType::AppService.responsibility_domains(), &["appservice_dispatch"]);
}

#[test]
fn test_worker_type_owned_route_prefixes() {
    assert_eq!(
        WorkerType::Master.owned_route_prefixes(),
        &["/_matrix/client/*", "/_matrix/federation/*", "/_matrix/media/*", "/_synapse/admin/*", "/_synapse/worker/*"]
    );
    assert_eq!(WorkerType::Frontend.owned_route_prefixes(), &["/_matrix/client/*"]);
    assert_eq!(WorkerType::Background.owned_route_prefixes(), &["/_synapse/worker/*"]);
    assert_eq!(WorkerType::EventPersister.owned_route_prefixes(), &["/_synapse/worker/v1/replication/*"]);
    assert_eq!(WorkerType::Synchrotron.owned_route_prefixes(), &["/_matrix/client/*/sync", "/_matrix/client/v3/sync"]);
    assert_eq!(WorkerType::FederationSender.owned_route_prefixes(), &[] as &[&str]);
    assert_eq!(WorkerType::FederationReader.owned_route_prefixes(), &["/_matrix/federation/*"]);
    assert_eq!(WorkerType::MediaRepository.owned_route_prefixes(), &["/_matrix/media/*"]);
    assert_eq!(WorkerType::Pusher.owned_route_prefixes(), &[] as &[&str]);
    assert_eq!(WorkerType::AppService.owned_route_prefixes(), &["/_matrix/app/*"]);
}

#[test]
fn test_worker_type_replication_streams() {
    assert_eq!(WorkerType::Master.replication_streams(), &["events", "worker_commands", "worker_tasks"]);
    assert_eq!(WorkerType::Frontend.replication_streams(), &[] as &[&str]);
    assert_eq!(WorkerType::Background.replication_streams(), &["worker_commands", "worker_tasks"]);
    assert_eq!(WorkerType::EventPersister.replication_streams(), &["events"]);
    assert_eq!(WorkerType::Synchrotron.replication_streams(), &["events"]);
    assert_eq!(WorkerType::FederationSender.replication_streams(), &["events"]);
    assert_eq!(WorkerType::FederationReader.replication_streams(), &["events"]);
    assert_eq!(WorkerType::MediaRepository.replication_streams(), &[] as &[&str]);
    assert_eq!(WorkerType::Pusher.replication_streams(), &["worker_tasks"]);
    assert_eq!(WorkerType::AppService.replication_streams(), &["worker_tasks"]);
}

#[test]
fn test_worker_type_instance_map_keys() {
    assert_eq!(WorkerType::Master.instance_map_keys(), &["master"]);
    assert_eq!(WorkerType::Frontend.instance_map_keys(), &["client_reader"]);
    assert_eq!(WorkerType::Background.instance_map_keys(), &["background_worker"]);
    assert_eq!(WorkerType::EventPersister.instance_map_keys(), &["event_persister"]);
    assert_eq!(WorkerType::Synchrotron.instance_map_keys(), &["sync_worker"]);
    assert_eq!(WorkerType::FederationSender.instance_map_keys(), &["federation_sender"]);
    assert_eq!(WorkerType::FederationReader.instance_map_keys(), &["federation_reader"]);
    assert_eq!(WorkerType::MediaRepository.instance_map_keys(), &["media_repository"]);
    assert_eq!(WorkerType::Pusher.instance_map_keys(), &["pusher"]);
    assert_eq!(WorkerType::AppService.instance_map_keys(), &["appservice_worker"]);
}

#[test]
fn test_worker_type_all_returns_all_variants() {
    let all = WorkerType::all();
    let expected = all_worker_types();
    assert_eq!(all.len(), expected.len());
    for variant in &expected {
        assert!(all.contains(variant), "Missing variant: {variant:?}");
    }
}

#[test]
fn test_worker_type_from_str_roundtrip() {
    for variant in all_worker_types() {
        let s = variant.as_str();
        match s.parse::<WorkerType>() {
            Ok(parsed) => assert_eq!(parsed, variant),
            Err(e) => assert!(false, "Failed to parse '{s}' back to WorkerType: {e}"),
        }
    }
}

#[test]
fn test_worker_type_from_str_error() {
    match "invalid_worker".parse::<WorkerType>() {
        Err(msg) => assert!(msg.contains("Invalid worker type")),
        Ok(_) => assert!(false, "Expected error for invalid worker type"),
    }
    match "".parse::<WorkerType>() {
        Err(msg) => assert!(msg.contains("Invalid worker type")),
        Ok(_) => assert!(false, "Expected error for empty string"),
    }
}

#[test]
fn test_worker_type_serde_roundtrip() {
    for variant in all_worker_types() {
        let json = serde_json::to_string(&variant).expect("serialize");
        let deserialized: WorkerType = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized, variant);
    }
}

#[test]
fn test_worker_type_debug_format() {
    let s = format!("{:?}", WorkerType::Master);
    assert_eq!(s, "Master");
}

// ------------------------------------------------------------------
// WorkerStatus tests
// ------------------------------------------------------------------

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
    for variant in all_worker_statuses() {
        let s = variant.as_str();
        match s.parse::<WorkerStatus>() {
            Ok(parsed) => assert_eq!(parsed, variant),
            Err(e) => assert!(false, "Failed to parse '{s}' back to WorkerStatus: {e}"),
        }
    }
}

#[test]
fn test_worker_status_from_str_error() {
    match "unknown".parse::<WorkerStatus>() {
        Err(msg) => assert!(msg.contains("Invalid worker status")),
        Ok(_) => assert!(false, "Expected error for invalid status"),
    }
}

#[test]
fn test_worker_status_serde_roundtrip() {
    for variant in all_worker_statuses() {
        let json = serde_json::to_string(&variant).expect("serialize");
        let deserialized: WorkerStatus = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized, variant);
    }
}

// ------------------------------------------------------------------
// WorkerRuntimeConfig tests
// ------------------------------------------------------------------

#[test]
fn test_worker_runtime_config_default() {
    let config = WorkerRuntimeConfig::default();
    assert_eq!(config.worker_name, "worker");
    assert_eq!(config.worker_type, WorkerType::Frontend);
    assert_eq!(config.host, "localhost");
    assert_eq!(config.port, 8080);
    assert!(config.master_host.is_none());
    assert!(config.master_port.is_none());
    assert!(config.replication_host.is_none());
    assert!(config.replication_port.is_none());
    assert!(config.http_port.is_none());
    assert!(config.bind_address.is_none());
    assert_eq!(config.max_connections, Some(1000));
    assert_eq!(config.heartbeat_interval_ms, Some(5000));
    assert_eq!(config.command_timeout_ms, Some(30000));
    assert!(config.extra_config.is_empty());
    // worker_id should be a non-empty UUID string
    assert!(!config.worker_id.is_empty());
}

#[test]
fn test_worker_runtime_config_default_worker_id_unique() {
    let c1 = WorkerRuntimeConfig::default();
    let c2 = WorkerRuntimeConfig::default();
    assert_ne!(c1.worker_id, c2.worker_id, "each default should generate a unique worker_id");
}

// ------------------------------------------------------------------
// WorkerCapabilities tests
// ------------------------------------------------------------------

#[test]
fn test_worker_capabilities_for_master() {
    let caps = WorkerCapabilities::for_type(&WorkerType::Master);
    assert!(caps.can_handle_http);
    assert!(caps.can_handle_federation);
    assert!(caps.can_persist_events);
    assert!(caps.can_send_push);
    assert!(caps.can_handle_media);
    assert!(caps.can_run_background_tasks);
    assert_eq!(caps.max_concurrent_requests, 10000);
    assert_eq!(caps.supported_protocols, vec!["matrix", "federation"]);
}

#[test]
fn test_worker_capabilities_for_frontend() {
    let caps = WorkerCapabilities::for_type(&WorkerType::Frontend);
    assert!(caps.can_handle_http);
    assert!(!caps.can_handle_federation);
    assert!(!caps.can_persist_events);
    assert!(!caps.can_send_push);
    assert!(!caps.can_handle_media);
    assert!(!caps.can_run_background_tasks);
    assert_eq!(caps.max_concurrent_requests, 5000);
    assert_eq!(caps.supported_protocols, vec!["matrix"]);
}

#[test]
fn test_worker_capabilities_for_synchrotron() {
    let caps = WorkerCapabilities::for_type(&WorkerType::Synchrotron);
    assert!(caps.can_handle_http);
    assert!(!caps.can_handle_federation);
    assert!(!caps.can_persist_events);
    assert!(!caps.can_send_push);
    assert!(!caps.can_handle_media);
    assert!(!caps.can_run_background_tasks);
    assert_eq!(caps.max_concurrent_requests, 3000);
    assert_eq!(caps.supported_protocols, vec!["matrix"]);
}

#[test]
fn test_worker_capabilities_for_event_persister() {
    let caps = WorkerCapabilities::for_type(&WorkerType::EventPersister);
    assert!(!caps.can_handle_http);
    assert!(!caps.can_handle_federation);
    assert!(caps.can_persist_events);
    assert!(!caps.can_send_push);
    assert!(!caps.can_handle_media);
    assert!(!caps.can_run_background_tasks);
    assert_eq!(caps.max_concurrent_requests, 1000);
    assert!(caps.supported_protocols.is_empty());
}

#[test]
fn test_worker_capabilities_for_federation_sender() {
    let caps = WorkerCapabilities::for_type(&WorkerType::FederationSender);
    assert!(!caps.can_handle_http);
    assert!(caps.can_handle_federation);
    assert!(!caps.can_persist_events);
    assert!(!caps.can_send_push);
    assert!(!caps.can_handle_media);
    assert!(!caps.can_run_background_tasks);
    assert_eq!(caps.max_concurrent_requests, 2000);
    assert_eq!(caps.supported_protocols, vec!["federation"]);
}

#[test]
fn test_worker_capabilities_for_federation_reader() {
    let caps = WorkerCapabilities::for_type(&WorkerType::FederationReader);
    assert!(!caps.can_handle_http);
    assert!(caps.can_handle_federation);
    assert!(!caps.can_persist_events);
    assert!(!caps.can_send_push);
    assert!(!caps.can_handle_media);
    assert!(!caps.can_run_background_tasks);
    assert_eq!(caps.max_concurrent_requests, 2000);
    assert_eq!(caps.supported_protocols, vec!["federation"]);
}

#[test]
fn test_worker_capabilities_for_media_repository() {
    let caps = WorkerCapabilities::for_type(&WorkerType::MediaRepository);
    assert!(caps.can_handle_http);
    assert!(!caps.can_handle_federation);
    assert!(!caps.can_persist_events);
    assert!(!caps.can_send_push);
    assert!(caps.can_handle_media);
    assert!(!caps.can_run_background_tasks);
    assert_eq!(caps.max_concurrent_requests, 1000);
    assert_eq!(caps.supported_protocols, vec!["matrix"]);
}

#[test]
fn test_worker_capabilities_for_pusher() {
    let caps = WorkerCapabilities::for_type(&WorkerType::Pusher);
    assert!(!caps.can_handle_http);
    assert!(!caps.can_handle_federation);
    assert!(!caps.can_persist_events);
    assert!(caps.can_send_push);
    assert!(!caps.can_handle_media);
    assert!(!caps.can_run_background_tasks);
    assert_eq!(caps.max_concurrent_requests, 500);
    assert!(caps.supported_protocols.is_empty());
}

#[test]
fn test_worker_capabilities_for_background() {
    let caps = WorkerCapabilities::for_type(&WorkerType::Background);
    assert!(!caps.can_handle_http);
    assert!(!caps.can_handle_federation);
    assert!(!caps.can_persist_events);
    assert!(!caps.can_send_push);
    assert!(!caps.can_handle_media);
    assert!(caps.can_run_background_tasks);
    assert_eq!(caps.max_concurrent_requests, 100);
    assert!(caps.supported_protocols.is_empty());
}

#[test]
fn test_worker_capabilities_for_appservice() {
    let caps = WorkerCapabilities::for_type(&WorkerType::AppService);
    assert!(!caps.can_handle_http);
    assert!(!caps.can_handle_federation);
    assert!(!caps.can_persist_events);
    assert!(!caps.can_send_push);
    assert!(!caps.can_handle_media);
    assert!(caps.can_run_background_tasks);
    assert_eq!(caps.max_concurrent_requests, 500);
    assert!(caps.supported_protocols.is_empty());
}

#[test]
fn test_worker_capabilities_all_types_have_some_capability() {
    for variant in all_worker_types() {
        let caps = WorkerCapabilities::for_type(&variant);
        let has_some = caps.can_handle_http
            || caps.can_handle_federation
            || caps.can_persist_events
            || caps.can_send_push
            || caps.can_handle_media
            || caps.can_run_background_tasks;
        assert!(has_some, "WorkerType {variant:?} has zero capabilities");
        assert!(caps.max_concurrent_requests > 0, "WorkerType {variant:?} has zero max_concurrent_requests");
    }
}

// ------------------------------------------------------------------
// WorkerResponsibilitySummary tests
// ------------------------------------------------------------------

#[test]
fn test_worker_responsibility_summary_for_type() {
    for variant in all_worker_types() {
        let summary = WorkerResponsibilitySummary::for_type(variant);
        assert_eq!(summary.worker_type, variant);
        assert!(!summary.domains.is_empty(), "WorkerType {variant:?} has empty domains");
        assert_eq!(summary.capabilities.can_handle_http, WorkerCapabilities::for_type(&variant).can_handle_http);
    }
}

#[test]
fn test_worker_responsibility_summary_domain_count() {
    for variant in all_worker_types() {
        let summary = WorkerResponsibilitySummary::for_type(variant);
        assert_eq!(
            summary.domains.len(),
            variant.responsibility_domains().len(),
            "WorkerType {variant:?} domain count mismatch"
        );
        for domain in variant.responsibility_domains() {
            assert!(summary.domains.contains(&domain.to_string()), "Missing domain {domain}");
        }
    }
}

// ------------------------------------------------------------------
// WorkerTopologyEntry tests
// ------------------------------------------------------------------

#[test]
fn test_worker_topology_entry_for_type() {
    for variant in all_worker_types() {
        let entry = WorkerTopologyEntry::for_type(variant);
        assert_eq!(entry.worker_type, variant);
        assert!(!entry.instance_map_keys.is_empty());
        assert!(!entry.domains.is_empty());
        assert_eq!(entry.domains.len(), variant.responsibility_domains().len());
        assert_eq!(entry.instance_map_keys.len(), variant.instance_map_keys().len());
        assert_eq!(entry.owned_route_prefixes.len(), variant.owned_route_prefixes().len());
        assert_eq!(entry.replication_streams.len(), variant.replication_streams().len());
    }
}

#[test]
fn test_worker_topology_entry_master_has_most_routes() {
    let master = WorkerTopologyEntry::for_type(WorkerType::Master);
    let frontend = WorkerTopologyEntry::for_type(WorkerType::Frontend);
    assert!(master.owned_route_prefixes.len() > frontend.owned_route_prefixes.len());
}

// ------------------------------------------------------------------
// WorkerTopologyPreset tests
// ------------------------------------------------------------------

#[test]
fn test_worker_topology_preset_worker_types() {
    let preset = WorkerTopologyPreset {
        name: "test".to_string(),
        description: "test preset".to_string(),
        instances: vec![
            WorkerTopologyPresetInstance {
                instance_name: "a".to_string(),
                worker_type: WorkerType::Frontend,
                count: 2,
                purpose: "serve".to_string(),
            },
            WorkerTopologyPresetInstance {
                instance_name: "b".to_string(),
                worker_type: WorkerType::Master,
                count: 1,
                purpose: "control".to_string(),
            },
        ],
    };
    let types = preset.worker_types();
    assert_eq!(types, vec![WorkerType::Frontend, WorkerType::Master]);
}

// ------------------------------------------------------------------
// WorkerTopologySummary tests
// ------------------------------------------------------------------

#[test]
fn test_worker_topology_summary_baseline_has_all_workers() {
    let summary = WorkerTopologySummary::baseline();
    assert_eq!(summary.workers.len(), 10);
    for variant in all_worker_types() {
        let found = summary.workers.iter().any(|w| w.worker_type == variant);
        assert!(found, "Missing {variant:?} in baseline workers");
    }
}

#[test]
fn test_worker_topology_summary_baseline_has_presets() {
    let summary = WorkerTopologySummary::baseline();
    assert_eq!(summary.deployment_presets.len(), 2);
    assert_eq!(summary.deployment_presets[0].name, "monolith");
    assert_eq!(summary.deployment_presets[1].name, "split_minimal");
}

#[test]
fn test_worker_topology_summary_monolith_preset() {
    let preset = WorkerTopologySummary::baseline_preset("monolith");
    match preset {
        Some(p) => {
            assert_eq!(p.instances.len(), 1);
            assert_eq!(p.instances[0].worker_type, WorkerType::Master);
            assert_eq!(p.instances[0].count, 1);
        }
        None => assert!(false, "monolith preset not found"),
    }
}

#[test]
fn test_worker_topology_summary_split_minimal_preset() {
    let preset = WorkerTopologySummary::baseline_preset("split_minimal");
    match preset {
        Some(p) => {
            assert_eq!(p.instances.len(), 9);
            let types: Vec<WorkerType> = p.instances.iter().map(|i| i.worker_type).collect();
            assert!(types.contains(&WorkerType::Master));
            assert!(types.contains(&WorkerType::Frontend));
            assert!(types.contains(&WorkerType::Synchrotron));
            assert!(types.contains(&WorkerType::EventPersister));
            assert!(types.contains(&WorkerType::FederationReader));
            assert!(types.contains(&WorkerType::FederationSender));
            assert!(types.contains(&WorkerType::MediaRepository));
            assert!(types.contains(&WorkerType::Background));
            assert!(types.contains(&WorkerType::Pusher));
        }
        None => assert!(false, "split_minimal preset not found"),
    }
}

#[test]
fn test_worker_topology_summary_baseline_preset_not_found() {
    let preset = WorkerTopologySummary::baseline_preset("nonexistent");
    assert!(preset.is_none());
}

// ------------------------------------------------------------------
// UpdateConnectionStatsRequest tests
// ------------------------------------------------------------------

#[test]
fn test_update_connection_stats_request_new() {
    let req = UpdateConnectionStatsRequest::new("source1", "target1", "direct");
    assert_eq!(req.source_worker_id, "source1");
    assert_eq!(req.target_worker_id, "target1");
    assert_eq!(req.connection_type, "direct");
    assert_eq!(req.bytes_sent, 0);
    assert_eq!(req.bytes_received, 0);
    assert_eq!(req.messages_sent, 0);
    assert_eq!(req.messages_received, 0);
}

#[test]
fn test_update_connection_stats_request_builder() {
    let req = UpdateConnectionStatsRequest::new("src", "tgt", "relay")
        .bytes_sent(100)
        .bytes_received(200)
        .messages_sent(10)
        .messages_received(20);
    assert_eq!(req.source_worker_id, "src");
    assert_eq!(req.target_worker_id, "tgt");
    assert_eq!(req.connection_type, "relay");
    assert_eq!(req.bytes_sent, 100);
    assert_eq!(req.bytes_received, 200);
    assert_eq!(req.messages_sent, 10);
    assert_eq!(req.messages_received, 20);
}

#[test]
fn test_update_connection_stats_request_default_zeros() {
    let req = UpdateConnectionStatsRequest::default();
    assert_eq!(req.bytes_sent, 0);
    assert_eq!(req.bytes_received, 0);
    assert_eq!(req.messages_sent, 0);
    assert_eq!(req.messages_received, 0);
    assert_eq!(req.source_worker_id, "");
    assert_eq!(req.target_worker_id, "");
    assert_eq!(req.connection_type, "");
}

// ------------------------------------------------------------------
// WorkerStorage pure static method tests
// ------------------------------------------------------------------

#[test]
fn test_status_releases_in_flight_work() {
    assert!(WorkerStorage::status_releases_in_flight_work("stopped"));
    assert!(WorkerStorage::status_releases_in_flight_work("error"));
    assert!(!WorkerStorage::status_releases_in_flight_work("running"));
    assert!(!WorkerStorage::status_releases_in_flight_work("starting"));
    assert!(!WorkerStorage::status_releases_in_flight_work("stopping"));
    assert!(!WorkerStorage::status_releases_in_flight_work("pending"));
    assert!(!WorkerStorage::status_releases_in_flight_work(""));
}

// ------------------------------------------------------------------
// From impl tests (row -> domain type conversions)
// ------------------------------------------------------------------

#[test]
fn test_from_worker_row_to_worker_info() {
    let row = WorkerRow {
        id: 42,
        worker_id: "w-001".to_string(),
        worker_name: "test-worker".to_string(),
        worker_type: "frontend".to_string(),
        host: "10.0.0.1".to_string(),
        port: 8080,
        status: "running".to_string(),
        last_heartbeat_ts: Some(1700000000000),
        started_ts: 1699999000000,
        stopped_ts: Some(1700000000000),
        config: serde_json::json!({"key": "value"}),
        metadata: serde_json::json!({"version": "1.0"}),
        version: Some("1.0.0".to_string()),
    };
    let info: WorkerInfo = row.into();
    assert_eq!(info.id, 42);
    assert_eq!(info.worker_id, "w-001");
    assert_eq!(info.worker_name, "test-worker");
    assert_eq!(info.worker_type, "frontend");
    assert_eq!(info.host, "10.0.0.1");
    assert_eq!(info.port, 8080);
    assert_eq!(info.status, "running");
    assert_eq!(info.last_heartbeat_ts, Some(1700000000000));
    assert_eq!(info.started_ts, 1699999000000);
    assert_eq!(info.stopped_ts, Some(1700000000000));
    assert_eq!(info.config, serde_json::json!({"key": "value"}));
    assert_eq!(info.metadata, serde_json::json!({"version": "1.0"}));
    assert_eq!(info.version, Some("1.0.0".to_string()));
}

#[test]
fn test_from_worker_row_to_worker_info_with_nulls() {
    let row = WorkerRow {
        id: 7,
        worker_id: "w-null".to_string(),
        worker_name: "null-test".to_string(),
        worker_type: "master".to_string(),
        host: "localhost".to_string(),
        port: 8008,
        status: "starting".to_string(),
        last_heartbeat_ts: None,
        started_ts: 1699999000000,
        stopped_ts: None,
        config: serde_json::json!({}),
        metadata: serde_json::json!({}),
        version: None,
    };
    let info: WorkerInfo = row.into();
    assert!(info.last_heartbeat_ts.is_none());
    assert!(info.stopped_ts.is_none());
    assert!(info.version.is_none());
}

#[test]
fn test_from_worker_command_row_to_worker_command() {
    let row = WorkerCommandRow {
        id: 1,
        command_id: "cmd-001".to_string(),
        target_worker_id: "w-target".to_string(),
        source_worker_id: Some("w-source".to_string()),
        command_type: "reload".to_string(),
        command_data: serde_json::json!({"param": true}),
        priority: 10,
        status: "pending".to_string(),
        created_ts: 1700000000000,
        sent_ts: None,
        completed_ts: None,
        error_message: None,
        retry_count: 0,
        max_retries: 3,
    };
    let cmd: WorkerCommand = row.into();
    assert_eq!(cmd.id, 1);
    assert_eq!(cmd.command_id, "cmd-001");
    assert_eq!(cmd.target_worker_id, "w-target");
    assert_eq!(cmd.source_worker_id, Some("w-source".to_string()));
    assert_eq!(cmd.command_type, "reload");
    assert_eq!(cmd.command_data, serde_json::json!({"param": true}));
    assert_eq!(cmd.priority, 10);
    assert_eq!(cmd.status, "pending");
    assert_eq!(cmd.created_ts, 1700000000000);
    assert!(cmd.sent_ts.is_none());
    assert!(cmd.completed_ts.is_none());
    assert!(cmd.error_message.is_none());
    assert_eq!(cmd.retry_count, 0);
    assert_eq!(cmd.max_retries, 3);
}

#[test]
fn test_from_worker_event_row_to_worker_event() {
    let row = WorkerEventRow {
        id: 99,
        event_id: "evt-001".to_string(),
        stream_id: 500,
        event_type: "m.room.message".to_string(),
        room_id: Some("!room:test".to_string()),
        sender: Some("@user:test".to_string()),
        event_data: serde_json::json!({"body": "hello"}),
        created_ts: 1700000000000,
        processed_by: Some(sqlx::types::Json(vec!["worker1".to_string(), "worker2".to_string()])),
    };
    let evt: WorkerEvent = row.into();
    assert_eq!(evt.id, 99);
    assert_eq!(evt.event_id, "evt-001");
    assert_eq!(evt.stream_id, 500);
    assert_eq!(evt.event_type, "m.room.message");
    assert_eq!(evt.room_id, Some("!room:test".to_string()));
    assert_eq!(evt.sender, Some("@user:test".to_string()));
    assert_eq!(evt.event_data, serde_json::json!({"body": "hello"}));
    assert_eq!(evt.created_ts, 1700000000000);
    match evt.processed_by {
        Some(ref workers) => {
            assert_eq!(workers.len(), 2);
            assert_eq!(workers[0], "worker1");
            assert_eq!(workers[1], "worker2");
        }
        None => assert!(false, "expected Some processed_by"),
    }
}

#[test]
fn test_from_worker_event_row_to_worker_event_no_processed_by() {
    let row = WorkerEventRow {
        id: 100,
        event_id: "evt-002".to_string(),
        stream_id: 501,
        event_type: "m.room.member".to_string(),
        room_id: None,
        sender: None,
        event_data: serde_json::json!({}),
        created_ts: 1700000000000,
        processed_by: None,
    };
    let evt: WorkerEvent = row.into();
    assert!(evt.room_id.is_none());
    assert!(evt.sender.is_none());
    assert!(evt.processed_by.is_none());
}

// ------------------------------------------------------------------
// Worker struct field initialization tests
// ------------------------------------------------------------------

#[test]
fn test_worker_info_field_defaults() {
    let info = WorkerInfo {
        id: 0,
        worker_id: String::new(),
        worker_name: String::new(),
        worker_type: String::new(),
        host: String::new(),
        port: 0,
        status: String::new(),
        last_heartbeat_ts: None,
        started_ts: 0,
        stopped_ts: None,
        config: serde_json::json!({}),
        metadata: serde_json::json!({}),
        version: None,
    };
    assert_eq!(info.id, 0);
    assert!(info.worker_id.is_empty());
    assert!(info.version.is_none());
}

#[test]
fn test_replication_position_fields() {
    let pos = ReplicationPosition {
        id: 1,
        worker_id: "w-1".to_string(),
        stream_name: "events".to_string(),
        stream_position: 5000,
        updated_ts: 1700000000000,
    };
    assert_eq!(pos.stream_name, "events");
    assert_eq!(pos.stream_position, 5000);
}

#[test]
fn test_worker_load_stats_fields() {
    let stats = WorkerLoadStats {
        id: 1,
        worker_id: "w-1".to_string(),
        cpu_usage: Some(45.5),
        memory_usage: Some(256_000_000),
        active_connections: Some(128),
        requests_per_second: Some(250.0),
        average_latency_ms: Some(12.3),
        queue_depth: Some(5),
        recorded_ts: 1700000000000,
    };
    assert_eq!(stats.cpu_usage, Some(45.5));
    assert_eq!(stats.memory_usage, Some(256_000_000));
    assert_eq!(stats.active_connections, Some(128));
    assert_eq!(stats.requests_per_second, Some(250.0));
    assert_eq!(stats.average_latency_ms, Some(12.3));
    assert_eq!(stats.queue_depth, Some(5));
}

#[test]
fn test_worker_load_stats_all_none() {
    let stats = WorkerLoadStats {
        id: 2,
        worker_id: "w-2".to_string(),
        cpu_usage: None,
        memory_usage: None,
        active_connections: None,
        requests_per_second: None,
        average_latency_ms: None,
        queue_depth: None,
        recorded_ts: 1700000000000,
    };
    assert!(stats.cpu_usage.is_none());
    assert!(stats.memory_usage.is_none());
    assert!(stats.active_connections.is_none());
    assert!(stats.requests_per_second.is_none());
    assert!(stats.average_latency_ms.is_none());
    assert!(stats.queue_depth.is_none());
}

#[test]
fn test_worker_topology_preset_instance_fields() {
    let instance = WorkerTopologyPresetInstance {
        instance_name: "sync_worker".to_string(),
        worker_type: WorkerType::Synchrotron,
        count: 3,
        purpose: "handle sync requests".to_string(),
    };
    assert_eq!(instance.instance_name, "sync_worker");
    assert_eq!(instance.worker_type, WorkerType::Synchrotron);
    assert_eq!(instance.count, 3);
    assert_eq!(instance.purpose, "handle sync requests");
}

#[test]
fn test_register_worker_request_fields() {
    let req = RegisterWorkerRequest {
        worker_id: "w-001".to_string(),
        worker_name: "test-worker".to_string(),
        worker_type: WorkerType::Frontend,
        host: "10.0.0.1".to_string(),
        port: 8080,
        config: Some(serde_json::json!({"max_conn": 100})),
        metadata: Some(serde_json::json!({"region": "us-east"})),
        version: Some("2.0.0".to_string()),
    };
    assert_eq!(req.worker_id, "w-001");
    assert_eq!(req.worker_type, WorkerType::Frontend);
    assert_eq!(req.port, 8080);
}

#[test]
fn test_register_worker_request_minimal() {
    let req = RegisterWorkerRequest {
        worker_id: "w-min".to_string(),
        worker_name: "min".to_string(),
        worker_type: WorkerType::Background,
        host: "localhost".to_string(),
        port: 0,
        config: None,
        metadata: None,
        version: None,
    };
    assert!(req.config.is_none());
    assert!(req.metadata.is_none());
    assert!(req.version.is_none());
}

#[test]
fn test_heartbeat_request_fields() {
    let req = HeartbeatRequest {
        worker_id: "w-001".to_string(),
        status: WorkerStatus::Running,
        load_stats: Some(WorkerLoadStatsUpdate {
            cpu_usage: Some(30.0),
            memory_usage: Some(512_000_000),
            active_connections: Some(42),
            requests_per_second: Some(100.0),
            average_latency_ms: Some(5.0),
            queue_depth: Some(2),
        }),
    };
    assert_eq!(req.worker_id, "w-001");
    assert_eq!(req.status, WorkerStatus::Running);
    match req.load_stats {
        Some(stats) => {
            assert_eq!(stats.cpu_usage, Some(30.0));
        }
        None => assert!(false, "expected load_stats"),
    }
}

#[test]
fn test_heartbeat_request_no_load_stats() {
    let req = HeartbeatRequest { worker_id: "w-002".to_string(), status: WorkerStatus::Stopping, load_stats: None };
    assert!(req.load_stats.is_none());
}

#[test]
fn test_worker_connection_fields() {
    let conn = WorkerConnection {
        id: 1,
        source_worker_id: "src".to_string(),
        target_worker_id: "tgt".to_string(),
        connection_type: "direct".to_string(),
        status: "active".to_string(),
        established_ts: 1700000000000,
        last_activity_ts: Some(1700000100000),
        bytes_sent: 1024,
        bytes_received: 2048,
        messages_sent: 50,
        messages_received: 100,
    };
    assert_eq!(conn.source_worker_id, "src");
    assert_eq!(conn.target_worker_id, "tgt");
    assert_eq!(conn.connection_type, "direct");
    assert_eq!(conn.bytes_sent, 1024);
    assert_eq!(conn.bytes_received, 2048);
    assert_eq!(conn.messages_sent, 50);
    assert_eq!(conn.messages_received, 100);
}

#[test]
fn test_send_command_request_fields() {
    let req = SendCommandRequest {
        target_worker_id: "w-target".to_string(),
        command_type: "reload_config".to_string(),
        command_data: serde_json::json!({"section": "logging"}),
        priority: Some(5),
        max_retries: Some(2),
    };
    assert_eq!(req.target_worker_id, "w-target");
    assert_eq!(req.command_type, "reload_config");
    assert_eq!(req.priority, Some(5));
    assert_eq!(req.max_retries, Some(2));
}

#[test]
fn test_send_command_request_default_fields() {
    let req = SendCommandRequest {
        target_worker_id: "w-target".to_string(),
        command_type: "ping".to_string(),
        command_data: serde_json::json!({}),
        priority: None,
        max_retries: None,
    };
    assert!(req.priority.is_none());
    assert!(req.max_retries.is_none());
}

#[test]
fn test_assign_task_request_fields() {
    let req = AssignTaskRequest {
        task_type: "process_media".to_string(),
        task_data: serde_json::json!({"media_id": "abc123"}),
        priority: Some(10),
        preferred_worker_id: Some("w-media".to_string()),
    };
    assert_eq!(req.task_type, "process_media");
    assert_eq!(req.priority, Some(10));
    assert_eq!(req.preferred_worker_id, Some("w-media".to_string()));
}

#[test]
fn test_stream_position_fields() {
    let pos = StreamPosition { stream_name: "events".to_string(), position: 1000 };
    assert_eq!(pos.stream_name, "events");
    assert_eq!(pos.position, 1000);
}

#[test]
fn test_rdata_event_fields() {
    let evt = RdataEvent {
        stream_id: 500,
        event_id: "$evt001".to_string(),
        room_id: "!room:test".to_string(),
        event_type: "m.room.message".to_string(),
        state_key: Some("".to_string()),
        sender: "@user:test".to_string(),
        content: serde_json::json!({"body": "hi"}),
        origin_server_ts: 1700000000000,
    };
    assert_eq!(evt.stream_id, 500);
    assert_eq!(evt.event_id, "$evt001");
    assert_eq!(evt.state_key, Some("".to_string()));
}

#[test]
fn test_rdata_position_fields() {
    let pos = RdataPosition { stream_name: "events".to_string(), position: 999 };
    assert_eq!(pos.stream_name, "events");
    assert_eq!(pos.position, 999);
}
