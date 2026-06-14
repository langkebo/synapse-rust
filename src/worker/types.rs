use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerType {
    Master,
    Frontend,
    Background,
    EventPersister,
    Synchrotron,
    FederationSender,
    FederationReader,
    MediaRepository,
    Pusher,
    AppService,
}

impl WorkerType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Master => "master",
            Self::Frontend => "frontend",
            Self::Background => "background",
            Self::EventPersister => "event_persister",
            Self::Synchrotron => "synchrotron",
            Self::FederationSender => "federation_sender",
            Self::FederationReader => "federation_reader",
            Self::MediaRepository => "media_repository",
            Self::Pusher => "pusher",
            Self::AppService => "appservice",
        }
    }

    pub fn can_handle_http(&self) -> bool {
        matches!(self, Self::Master | Self::Frontend | Self::Synchrotron)
    }

    pub fn can_handle_federation(&self) -> bool {
        matches!(self, Self::Master | Self::FederationSender | Self::FederationReader)
    }

    pub fn can_persist_events(&self) -> bool {
        matches!(self, Self::Master | Self::EventPersister)
    }

    pub fn responsibility_domains(&self) -> &'static [&'static str] {
        match self {
            Self::Master => &["client_http", "federation", "event_persistence", "background_jobs", "media", "push"],
            Self::Frontend => &["client_http"],
            Self::Background => &["background_jobs"],
            Self::EventPersister => &["event_persistence"],
            Self::Synchrotron => &["sync_http"],
            Self::FederationSender => &["federation_egress"],
            Self::FederationReader => &["federation_ingress"],
            Self::MediaRepository => &["media_http"],
            Self::Pusher => &["push_delivery"],
            Self::AppService => &["appservice_dispatch"],
        }
    }

    pub fn owned_route_prefixes(&self) -> &'static [&'static str] {
        match self {
            Self::Master => &[
                "/_matrix/client/*",
                "/_matrix/federation/*",
                "/_matrix/media/*",
                "/_synapse/admin/*",
                "/_synapse/worker/*",
            ],
            Self::Frontend => &["/_matrix/client/*"],
            Self::Background => &["/_synapse/worker/*"],
            Self::EventPersister => &["/_synapse/worker/v1/replication/*"],
            Self::Synchrotron => &["/_matrix/client/*/sync", "/_matrix/client/v3/sync"],
            Self::FederationSender => &[],
            Self::FederationReader => &["/_matrix/federation/*"],
            Self::MediaRepository => &["/_matrix/media/*"],
            Self::Pusher => &[],
            Self::AppService => &["/_matrix/app/*"],
        }
    }

    pub fn replication_streams(&self) -> &'static [&'static str] {
        match self {
            Self::Master => &["events", "worker_commands", "worker_tasks"],
            Self::Frontend => &[],
            Self::Background => &["worker_commands", "worker_tasks"],
            Self::EventPersister => &["events"],
            Self::Synchrotron => &["events"],
            Self::FederationSender => &["events"],
            Self::FederationReader => &["events"],
            Self::MediaRepository => &[],
            Self::Pusher => &["worker_tasks"],
            Self::AppService => &["worker_tasks"],
        }
    }

    pub fn instance_map_keys(&self) -> &'static [&'static str] {
        match self {
            Self::Master => &["master"],
            Self::Frontend => &["client_reader"],
            Self::Background => &["background_worker"],
            Self::EventPersister => &["event_persister"],
            Self::Synchrotron => &["sync_worker"],
            Self::FederationSender => &["federation_sender"],
            Self::FederationReader => &["federation_reader"],
            Self::MediaRepository => &["media_repository"],
            Self::Pusher => &["pusher"],
            Self::AppService => &["appservice_worker"],
        }
    }

    /// All worker types, for validation and enumeration.
    pub fn all() -> Vec<Self> {
        vec![
            Self::Master,
            Self::Frontend,
            Self::Background,
            Self::EventPersister,
            Self::Synchrotron,
            Self::FederationSender,
            Self::FederationReader,
            Self::MediaRepository,
            Self::Pusher,
            Self::AppService,
        ]
    }
}

impl FromStr for WorkerType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "master" => Ok(Self::Master),
            "frontend" => Ok(Self::Frontend),
            "background" => Ok(Self::Background),
            "event_persister" => Ok(Self::EventPersister),
            "synchrotron" => Ok(Self::Synchrotron),
            "federation_sender" => Ok(Self::FederationSender),
            "federation_reader" => Ok(Self::FederationReader),
            "media_repository" => Ok(Self::MediaRepository),
            "pusher" => Ok(Self::Pusher),
            "appservice" => Ok(Self::AppService),
            _ => Err(format!("Invalid worker type: {s}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerStatus {
    Starting,
    Running,
    Stopping,
    Stopped,
    Error,
}

impl WorkerStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Stopping => "stopping",
            Self::Stopped => "stopped",
            Self::Error => "error",
        }
    }
}

impl FromStr for WorkerStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "starting" => Ok(Self::Starting),
            "running" => Ok(Self::Running),
            "stopping" => Ok(Self::Stopping),
            "stopped" => Ok(Self::Stopped),
            "error" => Ok(Self::Error),
            _ => Err(format!("Invalid worker status: {s}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerRuntimeConfig {
    pub worker_id: String,
    pub worker_name: String,
    pub worker_type: WorkerType,
    pub host: String,
    pub port: u16,
    pub master_host: Option<String>,
    pub master_port: Option<u16>,
    pub replication_host: Option<String>,
    pub replication_port: Option<u16>,
    pub http_port: Option<u16>,
    pub bind_address: Option<String>,
    pub max_connections: Option<u32>,
    pub heartbeat_interval_ms: Option<u64>,
    pub command_timeout_ms: Option<u64>,
    pub extra_config: HashMap<String, serde_json::Value>,
}

impl Default for WorkerRuntimeConfig {
    fn default() -> Self {
        Self {
            worker_id: uuid::Uuid::new_v4().to_string(),
            worker_name: "worker".to_string(),
            worker_type: WorkerType::Frontend,
            host: "localhost".to_string(),
            port: 8080,
            master_host: None,
            master_port: None,
            replication_host: None,
            replication_port: None,
            http_port: None,
            bind_address: None,
            max_connections: Some(1000),
            heartbeat_interval_ms: Some(5000),
            command_timeout_ms: Some(30000),
            extra_config: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerInfo {
    pub id: i64,
    pub worker_id: String,
    pub worker_name: String,
    pub worker_type: String,
    pub host: String,
    pub port: i32,
    pub status: String,
    pub last_heartbeat_ts: Option<i64>,
    pub started_ts: i64,
    pub stopped_ts: Option<i64>,
    pub config: serde_json::Value,
    pub metadata: serde_json::Value,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerCommand {
    pub id: i64,
    pub command_id: String,
    pub target_worker_id: String,
    pub source_worker_id: Option<String>,
    pub command_type: String,
    pub command_data: serde_json::Value,
    pub priority: i32,
    pub status: String,
    pub created_ts: i64,
    pub sent_ts: Option<i64>,
    pub completed_ts: Option<i64>,
    pub error_message: Option<String>,
    pub retry_count: i32,
    pub max_retries: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerEvent {
    pub id: i64,
    pub event_id: String,
    pub stream_id: i64,
    pub event_type: String,
    pub room_id: Option<String>,
    pub sender: Option<String>,
    pub event_data: serde_json::Value,
    pub created_ts: i64,
    pub processed_by: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationPosition {
    pub id: i64,
    pub worker_id: String,
    pub stream_name: String,
    pub stream_position: i64,
    pub updated_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerLoadStats {
    pub id: i64,
    pub worker_id: String,
    pub cpu_usage: Option<f32>,
    pub memory_usage: Option<i64>,
    pub active_connections: Option<i32>,
    pub requests_per_second: Option<f32>,
    pub average_latency_ms: Option<f32>,
    pub queue_depth: Option<i32>,
    pub recorded_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct WorkerTaskAssignment {
    pub id: i64,
    pub task_id: String,
    pub task_type: String,
    pub task_data: serde_json::Value,
    pub assigned_worker_id: Option<String>,
    pub status: String,
    pub priority: i32,
    pub created_ts: i64,
    pub assigned_ts: Option<i64>,
    pub completed_ts: Option<i64>,
    pub result: Option<serde_json::Value>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerConnection {
    pub id: i64,
    pub source_worker_id: String,
    pub target_worker_id: String,
    pub connection_type: String,
    pub status: String,
    pub established_ts: i64,
    pub last_activity_ts: Option<i64>,
    pub bytes_sent: i64,
    pub bytes_received: i64,
    pub messages_sent: i64,
    pub messages_received: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterWorkerRequest {
    pub worker_id: String,
    pub worker_name: String,
    pub worker_type: WorkerType,
    pub host: String,
    pub port: u16,
    pub config: Option<serde_json::Value>,
    pub metadata: Option<serde_json::Value>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendCommandRequest {
    pub target_worker_id: String,
    pub command_type: String,
    pub command_data: serde_json::Value,
    pub priority: Option<i32>,
    pub max_retries: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssignTaskRequest {
    pub task_type: String,
    pub task_data: serde_json::Value,
    pub priority: Option<i32>,
    pub preferred_worker_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatRequest {
    pub worker_id: String,
    pub status: WorkerStatus,
    pub load_stats: Option<WorkerLoadStatsUpdate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerLoadStatsUpdate {
    pub cpu_usage: Option<f32>,
    pub memory_usage: Option<i64>,
    pub active_connections: Option<i32>,
    pub requests_per_second: Option<f32>,
    pub average_latency_ms: Option<f32>,
    pub queue_depth: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamPosition {
    pub stream_name: String,
    pub position: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RdataEvent {
    pub stream_id: i64,
    pub event_id: String,
    pub room_id: String,
    pub event_type: String,
    pub state_key: Option<String>,
    pub sender: String,
    pub content: serde_json::Value,
    pub origin_server_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RdataPosition {
    pub stream_name: String,
    pub position: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkerCapabilities {
    pub can_handle_http: bool,
    pub can_handle_federation: bool,
    pub can_persist_events: bool,
    pub can_send_push: bool,
    pub can_handle_media: bool,
    pub can_run_background_tasks: bool,
    pub max_concurrent_requests: u32,
    pub supported_protocols: Vec<String>,
}

impl WorkerCapabilities {
    pub fn for_type(worker_type: &WorkerType) -> Self {
        match worker_type {
            WorkerType::Master => Self {
                can_handle_http: true,
                can_handle_federation: true,
                can_persist_events: true,
                can_send_push: true,
                can_handle_media: true,
                can_run_background_tasks: true,
                max_concurrent_requests: 10000,
                supported_protocols: vec!["matrix".to_string(), "federation".to_string()],
            },
            WorkerType::Frontend => Self {
                can_handle_http: true,
                can_handle_federation: false,
                can_persist_events: false,
                can_send_push: false,
                can_handle_media: false,
                can_run_background_tasks: false,
                max_concurrent_requests: 5000,
                supported_protocols: vec!["matrix".to_string()],
            },
            WorkerType::Synchrotron => Self {
                can_handle_http: true,
                can_handle_federation: false,
                can_persist_events: false,
                can_send_push: false,
                can_handle_media: false,
                can_run_background_tasks: false,
                max_concurrent_requests: 3000,
                supported_protocols: vec!["matrix".to_string()],
            },
            WorkerType::EventPersister => Self {
                can_handle_http: false,
                can_handle_federation: false,
                can_persist_events: true,
                can_send_push: false,
                can_handle_media: false,
                can_run_background_tasks: false,
                max_concurrent_requests: 1000,
                supported_protocols: vec![],
            },
            WorkerType::FederationSender => Self {
                can_handle_http: false,
                can_handle_federation: true,
                can_persist_events: false,
                can_send_push: false,
                can_handle_media: false,
                can_run_background_tasks: false,
                max_concurrent_requests: 2000,
                supported_protocols: vec!["federation".to_string()],
            },
            WorkerType::FederationReader => Self {
                can_handle_http: false,
                can_handle_federation: true,
                can_persist_events: false,
                can_send_push: false,
                can_handle_media: false,
                can_run_background_tasks: false,
                max_concurrent_requests: 2000,
                supported_protocols: vec!["federation".to_string()],
            },
            WorkerType::MediaRepository => Self {
                can_handle_http: true,
                can_handle_federation: false,
                can_persist_events: false,
                can_send_push: false,
                can_handle_media: true,
                can_run_background_tasks: false,
                max_concurrent_requests: 1000,
                supported_protocols: vec!["matrix".to_string()],
            },
            WorkerType::Pusher => Self {
                can_handle_http: false,
                can_handle_federation: false,
                can_persist_events: false,
                can_send_push: true,
                can_handle_media: false,
                can_run_background_tasks: false,
                max_concurrent_requests: 500,
                supported_protocols: vec![],
            },
            WorkerType::Background => Self {
                can_handle_http: false,
                can_handle_federation: false,
                can_persist_events: false,
                can_send_push: false,
                can_handle_media: false,
                can_run_background_tasks: true,
                max_concurrent_requests: 100,
                supported_protocols: vec![],
            },
            WorkerType::AppService => Self {
                can_handle_http: false,
                can_handle_federation: false,
                can_persist_events: false,
                can_send_push: false,
                can_handle_media: false,
                can_run_background_tasks: true,
                max_concurrent_requests: 500,
                supported_protocols: vec![],
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerResponsibilitySummary {
    pub worker_type: WorkerType,
    pub domains: Vec<String>,
    pub capabilities: WorkerCapabilities,
}

impl WorkerResponsibilitySummary {
    pub fn for_type(worker_type: WorkerType) -> Self {
        Self {
            worker_type,
            domains: worker_type.responsibility_domains().iter().map(|value| (*value).to_string()).collect(),
            capabilities: WorkerCapabilities::for_type(&worker_type),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerTopologyEntry {
    pub worker_type: WorkerType,
    pub instance_map_keys: Vec<String>,
    pub domains: Vec<String>,
    pub owned_route_prefixes: Vec<String>,
    pub replication_streams: Vec<String>,
    pub capabilities: WorkerCapabilities,
}

impl WorkerTopologyEntry {
    pub fn for_type(worker_type: WorkerType) -> Self {
        Self {
            worker_type,
            instance_map_keys: worker_type.instance_map_keys().iter().map(|value| (*value).to_string()).collect(),
            domains: worker_type.responsibility_domains().iter().map(|value| (*value).to_string()).collect(),
            owned_route_prefixes: worker_type
                .owned_route_prefixes()
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            replication_streams: worker_type.replication_streams().iter().map(|value| (*value).to_string()).collect(),
            capabilities: WorkerCapabilities::for_type(&worker_type),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerTopologyPresetInstance {
    pub instance_name: String,
    pub worker_type: WorkerType,
    pub count: u16,
    pub purpose: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerTopologyPreset {
    pub name: String,
    pub description: String,
    pub instances: Vec<WorkerTopologyPresetInstance>,
}

impl WorkerTopologyPreset {
    pub fn worker_types(&self) -> Vec<WorkerType> {
        self.instances.iter().map(|instance| instance.worker_type).collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerTopologySummary {
    pub workers: Vec<WorkerTopologyEntry>,
    pub deployment_presets: Vec<WorkerTopologyPreset>,
}

impl WorkerTopologySummary {
    pub fn baseline() -> Self {
        let workers = [
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
        .into_iter()
        .map(WorkerTopologyEntry::for_type)
        .collect();

        let deployment_presets = vec![
            WorkerTopologyPreset {
                name: "monolith".to_string(),
                description: "Single process baseline where one master owns all client, federation, media, and background domains."
                    .to_string(),
                instances: vec![WorkerTopologyPresetInstance {
                    instance_name: "master".to_string(),
                    worker_type: WorkerType::Master,
                    count: 1,
                    purpose: "Owns all domains in a single-node deployment.".to_string(),
                }],
            },
            WorkerTopologyPreset {
                name: "split_minimal".to_string(),
                description:
                    "Small multi-worker baseline that separates client ingress, sync, event persistence, federation, media, push, and background tasks."
                        .to_string(),
                instances: vec![
                    WorkerTopologyPresetInstance {
                        instance_name: "master".to_string(),
                        worker_type: WorkerType::Master,
                        count: 1,
                        purpose: "Control plane, admin, and fallback ownership.".to_string(),
                    },
                    WorkerTopologyPresetInstance {
                        instance_name: "client_reader".to_string(),
                        worker_type: WorkerType::Frontend,
                        count: 2,
                        purpose: "Serve client HTTP traffic behind the reverse proxy.".to_string(),
                    },
                    WorkerTopologyPresetInstance {
                        instance_name: "sync_worker".to_string(),
                        worker_type: WorkerType::Synchrotron,
                        count: 1,
                        purpose: "Own sync-heavy endpoints.".to_string(),
                    },
                    WorkerTopologyPresetInstance {
                        instance_name: "event_persister".to_string(),
                        worker_type: WorkerType::EventPersister,
                        count: 1,
                        purpose: "Own event write path and replication event stream.".to_string(),
                    },
                    WorkerTopologyPresetInstance {
                        instance_name: "federation_reader".to_string(),
                        worker_type: WorkerType::FederationReader,
                        count: 1,
                        purpose: "Serve inbound federation traffic.".to_string(),
                    },
                    WorkerTopologyPresetInstance {
                        instance_name: "federation_sender".to_string(),
                        worker_type: WorkerType::FederationSender,
                        count: 1,
                        purpose: "Handle outbound federation delivery.".to_string(),
                    },
                    WorkerTopologyPresetInstance {
                        instance_name: "media_repository".to_string(),
                        worker_type: WorkerType::MediaRepository,
                        count: 1,
                        purpose: "Serve media endpoints.".to_string(),
                    },
                    WorkerTopologyPresetInstance {
                        instance_name: "background_worker".to_string(),
                        worker_type: WorkerType::Background,
                        count: 1,
                        purpose: "Run background jobs and generic worker commands.".to_string(),
                    },
                    WorkerTopologyPresetInstance {
                        instance_name: "pusher".to_string(),
                        worker_type: WorkerType::Pusher,
                        count: 1,
                        purpose: "Deliver push notifications.".to_string(),
                    },
                ],
            },
        ];

        Self { workers, deployment_presets }
    }

    pub fn baseline_preset(name: &str) -> Option<WorkerTopologyPreset> {
        Self::baseline()
            .deployment_presets
            .into_iter()
            .find(|preset| preset.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worker_type_as_str() {
        assert_eq!(WorkerType::Master.as_str(), "master");
        assert_eq!(WorkerType::Frontend.as_str(), "frontend");
        assert_eq!(WorkerType::EventPersister.as_str(), "event_persister");
    }

    #[test]
    fn test_worker_type_from_str() {
        assert_eq!(WorkerType::from_str("master"), Ok(WorkerType::Master));
        assert_eq!(WorkerType::from_str("frontend"), Ok(WorkerType::Frontend));
        assert!(WorkerType::from_str("invalid").is_err());
    }

    #[test]
    fn test_worker_type_capabilities() {
        let master_caps = WorkerCapabilities::for_type(&WorkerType::Master);
        assert!(master_caps.can_handle_http);
        assert!(master_caps.can_persist_events);

        let frontend_caps = WorkerCapabilities::for_type(&WorkerType::Frontend);
        assert!(frontend_caps.can_handle_http);
        assert!(!frontend_caps.can_persist_events);

        let event_persister_caps = WorkerCapabilities::for_type(&WorkerType::EventPersister);
        assert!(!event_persister_caps.can_handle_http);
        assert!(event_persister_caps.can_persist_events);
    }

    #[test]
    fn test_worker_type_responsibility_domains() {
        assert_eq!(WorkerType::AppService.responsibility_domains(), &["appservice_dispatch"]);
        assert_eq!(WorkerType::Frontend.responsibility_domains(), &["client_http"]);
    }

    #[test]
    fn test_worker_type_route_prefixes_and_instance_map_keys() {
        assert_eq!(WorkerType::Synchrotron.owned_route_prefixes(), &["/_matrix/client/*/sync", "/_matrix/client/v3/sync"]);
        assert_eq!(WorkerType::EventPersister.instance_map_keys(), &["event_persister"]);
    }

    #[test]
    fn test_worker_responsibility_summary_for_type() {
        let summary = WorkerResponsibilitySummary::for_type(WorkerType::FederationSender);
        assert_eq!(summary.worker_type, WorkerType::FederationSender);
        assert_eq!(summary.domains, vec!["federation_egress".to_string()]);
        assert!(summary.capabilities.can_handle_federation);
    }

    #[test]
    fn test_worker_topology_summary_baseline_contains_split_minimal_preset() {
        let summary = WorkerTopologySummary::baseline();
        assert!(summary.workers.iter().any(|entry| entry.worker_type == WorkerType::MediaRepository));
        assert!(summary.deployment_presets.iter().any(|preset| preset.name == "split_minimal"));
    }

    #[test]
    fn test_worker_topology_preset_worker_types() {
        let preset = WorkerTopologySummary::baseline_preset("monolith").expect("monolith preset");
        assert_eq!(preset.worker_types(), vec![WorkerType::Master]);
    }

    #[test]
    fn test_worker_config_default() {
        let config = WorkerRuntimeConfig::default();
        assert!(!config.worker_id.is_empty());
        assert_eq!(config.worker_type, WorkerType::Frontend);
        assert_eq!(config.host, "localhost");
    }

    #[test]
    fn test_worker_status_as_str() {
        assert_eq!(WorkerStatus::Running.as_str(), "running");
        assert_eq!(WorkerStatus::Starting.as_str(), "starting");
    }

    #[test]
    fn test_worker_status_from_str() {
        assert_eq!(WorkerStatus::from_str("running"), Ok(WorkerStatus::Running));
        assert_eq!(WorkerStatus::from_str("stopped"), Ok(WorkerStatus::Stopped));
    }
}
