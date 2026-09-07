use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::collections::HashMap;
use std::str::FromStr;

/// The `WorkerType` enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerType {
    /// The `Master` variant.
    Master,
    /// The `Frontend` variant.
    Frontend,
    /// The `Background` variant.
    Background,
    /// The `EventPersister` variant.
    EventPersister,
    /// The `Synchrotron` variant.
    Synchrotron,
    /// The `FederationSender` variant.
    FederationSender,
    /// The `FederationReader` variant.
    FederationReader,
    /// The `MediaRepository` variant.
    MediaRepository,
    /// The `Pusher` variant.
    Pusher,
    /// The `AppService` variant.
    AppService,
}

impl WorkerType {
    /// See [`as_str`].
    /// See [`as_str`].
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

    /// See [`can_handle_http`].
    /// See [`can_handle_http`].
    pub fn can_handle_http(&self) -> bool {
        matches!(self, Self::Master | Self::Frontend | Self::Synchrotron)
    }

    /// See [`can_handle_federation`].
    /// See [`can_handle_federation`].
    pub fn can_handle_federation(&self) -> bool {
        matches!(self, Self::Master | Self::FederationSender | Self::FederationReader)
    }

    /// See [`can_persist_events`].
    /// See [`can_persist_events`].
    pub fn can_persist_events(&self) -> bool {
        matches!(self, Self::Master | Self::EventPersister)
    }

    /// See [`responsibility_domains`].
    /// See [`responsibility_domains`].
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

    /// See [`owned_route_prefixes`].
    /// See [`owned_route_prefixes`].
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

    /// See [`replication_streams`].
    /// See [`replication_streams`].
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

    /// See [`instance_map_keys`].
    /// See [`instance_map_keys`].
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

    /// See [`all`].
    /// See [`all`].
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

/// The `WorkerStatus` enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerStatus {
    /// The `Starting` variant.
    Starting,
    /// The `Running` variant.
    Running,
    /// The `Stopping` variant.
    Stopping,
    /// The `Stopped` variant.
    Stopped,
    /// The `Error` variant.
    Error,
}

impl WorkerStatus {
    /// See [`as_str`].
    /// See [`as_str`].
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

/// The `WorkerRuntimeConfig` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerRuntimeConfig {
    /// The `worker_id` field.
    pub worker_id: String,
    /// The `worker_name` field.
    pub worker_name: String,
    /// The `worker_type` field.
    pub worker_type: WorkerType,
    /// The `host` field.
    pub host: String,
    /// The `port` field.
    pub port: u16,
    /// The `master_host` field.
    pub master_host: Option<String>,
    /// The `master_port` field.
    pub master_port: Option<u16>,
    /// The `replication_host` field.
    pub replication_host: Option<String>,
    /// The `replication_port` field.
    pub replication_port: Option<u16>,
    /// The `http_port` field.
    pub http_port: Option<u16>,
    /// The `bind_address` field.
    pub bind_address: Option<String>,
    /// The `max_connections` field.
    pub max_connections: Option<u32>,
    /// The `heartbeat_interval_ms` field.
    pub heartbeat_interval_ms: Option<u64>,
    /// The `command_timeout_ms` field.
    pub command_timeout_ms: Option<u64>,
    /// The `extra_config` field.
    pub extra_config: HashMap<String, serde_json::Value>,
}

impl Default for WorkerRuntimeConfig {
    fn default() -> Self {
        Self {
            worker_id: uuid::Uuid::new_v4().simple().to_string(),
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

/// The `WorkerInfo` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerInfo {
    /// The `id` field.
    pub id: i64,
    /// The `worker_id` field.
    pub worker_id: String,
    /// The `worker_name` field.
    pub worker_name: String,
    /// The `worker_type` field.
    pub worker_type: String,
    /// The `host` field.
    pub host: String,
    /// The `port` field.
    pub port: i32,
    /// The `status` field.
    pub status: String,
    /// The `last_heartbeat_ts` field.
    pub last_heartbeat_ts: Option<i64>,
    /// The `started_ts` field.
    pub started_ts: i64,
    /// The `stopped_ts` field.
    pub stopped_ts: Option<i64>,
    /// The `config` field.
    pub config: serde_json::Value,
    /// The `metadata` field.
    pub metadata: serde_json::Value,
    /// The `version` field.
    pub version: Option<String>,
}

/// The `WorkerCommand` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerCommand {
    /// The `id` field.
    pub id: i64,
    /// The `command_id` field.
    pub command_id: String,
    /// The `target_worker_id` field.
    pub target_worker_id: String,
    /// The `source_worker_id` field.
    pub source_worker_id: Option<String>,
    /// The `command_type` field.
    pub command_type: String,
    /// The `command_data` field.
    pub command_data: serde_json::Value,
    /// The `priority` field.
    pub priority: i32,
    /// The `status` field.
    pub status: String,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `sent_ts` field.
    pub sent_ts: Option<i64>,
    /// The `completed_ts` field.
    pub completed_ts: Option<i64>,
    /// The `error_message` field.
    pub error_message: Option<String>,
    /// The `retry_count` field.
    pub retry_count: i32,
    /// The `max_retries` field.
    pub max_retries: i32,
}

/// The `WorkerEvent` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerEvent {
    /// The `id` field.
    pub id: i64,
    /// The `event_id` field.
    pub event_id: String,
    /// The `stream_id` field.
    pub stream_id: i64,
    /// The `event_type` field.
    pub event_type: String,
    /// The `room_id` field.
    pub room_id: Option<String>,
    /// The `sender` field.
    pub sender: Option<String>,
    /// The `event_data` field.
    pub event_data: serde_json::Value,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `processed_by` field.
    pub processed_by: Option<Vec<String>>,
}

/// The `ReplicationPosition` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationPosition {
    /// The `id` field.
    pub id: i64,
    /// The `worker_id` field.
    pub worker_id: String,
    /// The `stream_name` field.
    pub stream_name: String,
    /// The `stream_position` field.
    pub stream_position: i64,
    /// The `updated_ts` field.
    pub updated_ts: i64,
}

/// The `WorkerLoadStats` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerLoadStats {
    /// The `id` field.
    pub id: i64,
    /// The `worker_id` field.
    pub worker_id: String,
    /// The `cpu_usage` field.
    pub cpu_usage: Option<f32>,
    /// The `memory_usage` field.
    pub memory_usage: Option<i64>,
    /// The `active_connections` field.
    pub active_connections: Option<i32>,
    /// The `requests_per_second` field.
    pub requests_per_second: Option<f32>,
    /// The `average_latency_ms` field.
    pub average_latency_ms: Option<f32>,
    /// The `queue_depth` field.
    pub queue_depth: Option<i32>,
    /// The `recorded_ts` field.
    pub recorded_ts: i64,
}

/// The `WorkerTaskAssignment` struct.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct WorkerTaskAssignment {
    /// The `id` field.
    pub id: i64,
    /// The `task_id` field.
    pub task_id: String,
    /// The `task_type` field.
    pub task_type: String,
    /// The `task_data` field.
    pub task_data: serde_json::Value,
    /// The `assigned_worker_id` field.
    pub assigned_worker_id: Option<String>,
    /// The `status` field.
    pub status: String,
    /// The `priority` field.
    pub priority: i32,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `assigned_ts` field.
    pub assigned_ts: Option<i64>,
    /// The `completed_ts` field.
    pub completed_ts: Option<i64>,
    /// The `result` field.
    pub result: Option<serde_json::Value>,
    /// The `error_message` field.
    pub error_message: Option<String>,
}

/// The `WorkerConnection` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerConnection {
    /// The `id` field.
    pub id: i64,
    /// The `source_worker_id` field.
    pub source_worker_id: String,
    /// The `target_worker_id` field.
    pub target_worker_id: String,
    /// The `connection_type` field.
    pub connection_type: String,
    /// The `status` field.
    pub status: String,
    /// The `established_ts` field.
    pub established_ts: i64,
    /// The `last_activity_ts` field.
    pub last_activity_ts: Option<i64>,
    /// The `bytes_sent` field.
    pub bytes_sent: i64,
    /// The `bytes_received` field.
    pub bytes_received: i64,
    /// The `messages_sent` field.
    pub messages_sent: i64,
    /// The `messages_received` field.
    pub messages_received: i64,
}

/// The `RegisterWorkerRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterWorkerRequest {
    /// The `worker_id` field.
    pub worker_id: String,
    /// The `worker_name` field.
    pub worker_name: String,
    /// The `worker_type` field.
    pub worker_type: WorkerType,
    /// The `host` field.
    pub host: String,
    /// The `port` field.
    pub port: u16,
    /// The `config` field.
    pub config: Option<serde_json::Value>,
    /// The `metadata` field.
    pub metadata: Option<serde_json::Value>,
    /// The `version` field.
    pub version: Option<String>,
}

/// The `SendCommandRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendCommandRequest {
    /// The `target_worker_id` field.
    pub target_worker_id: String,
    /// The `command_type` field.
    pub command_type: String,
    /// The `command_data` field.
    pub command_data: serde_json::Value,
    /// The `priority` field.
    pub priority: Option<i32>,
    /// The `max_retries` field.
    pub max_retries: Option<i32>,
}

/// The `AssignTaskRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssignTaskRequest {
    /// The `task_type` field.
    pub task_type: String,
    /// The `task_data` field.
    pub task_data: serde_json::Value,
    /// The `priority` field.
    pub priority: Option<i32>,
    /// The `preferred_worker_id` field.
    pub preferred_worker_id: Option<String>,
}

/// The `HeartbeatRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatRequest {
    /// The `worker_id` field.
    pub worker_id: String,
    /// The `status` field.
    pub status: WorkerStatus,
    /// The `load_stats` field.
    pub load_stats: Option<WorkerLoadStatsUpdate>,
}

/// The `WorkerLoadStatsUpdate` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerLoadStatsUpdate {
    /// The `cpu_usage` field.
    pub cpu_usage: Option<f32>,
    /// The `memory_usage` field.
    pub memory_usage: Option<i64>,
    /// The `active_connections` field.
    pub active_connections: Option<i32>,
    /// The `requests_per_second` field.
    pub requests_per_second: Option<f32>,
    /// The `average_latency_ms` field.
    pub average_latency_ms: Option<f32>,
    /// The `queue_depth` field.
    pub queue_depth: Option<i32>,
}

/// The `StreamPosition` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamPosition {
    /// The `stream_name` field.
    pub stream_name: String,
    /// The `position` field.
    pub position: i64,
}

/// The `RdataEvent` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RdataEvent {
    /// The `stream_id` field.
    pub stream_id: i64,
    /// The `event_id` field.
    pub event_id: String,
    /// The `room_id` field.
    pub room_id: String,
    /// The `event_type` field.
    pub event_type: String,
    /// The `state_key` field.
    pub state_key: Option<String>,
    /// The `sender` field.
    pub sender: String,
    /// The `content` field.
    pub content: serde_json::Value,
    /// The `origin_server_ts` field.
    pub origin_server_ts: i64,
}

/// The `RdataPosition` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RdataPosition {
    /// The `stream_name` field.
    pub stream_name: String,
    /// The `position` field.
    pub position: i64,
}

/// The `WorkerCapabilities` struct.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkerCapabilities {
    /// The `can_handle_http` field.
    pub can_handle_http: bool,
    /// The `can_handle_federation` field.
    pub can_handle_federation: bool,
    /// The `can_persist_events` field.
    pub can_persist_events: bool,
    /// The `can_send_push` field.
    pub can_send_push: bool,
    /// The `can_handle_media` field.
    pub can_handle_media: bool,
    /// The `can_run_background_tasks` field.
    pub can_run_background_tasks: bool,
    /// The `max_concurrent_requests` field.
    pub max_concurrent_requests: u32,
    /// The `supported_protocols` field.
    pub supported_protocols: Vec<String>,
}

impl WorkerCapabilities {
    /// See [`for_type`].
    /// See [`for_type`].
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

/// The `WorkerResponsibilitySummary` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerResponsibilitySummary {
    /// The `worker_type` field.
    pub worker_type: WorkerType,
    /// The `domains` field.
    pub domains: Vec<String>,
    /// The `capabilities` field.
    pub capabilities: WorkerCapabilities,
}

impl WorkerResponsibilitySummary {
    /// See [`for_type`].
    /// See [`for_type`].
    pub fn for_type(worker_type: WorkerType) -> Self {
        Self {
            worker_type,
            domains: worker_type.responsibility_domains().iter().map(|value| (*value).to_string()).collect(),
            capabilities: WorkerCapabilities::for_type(&worker_type),
        }
    }
}

/// The `WorkerTopologyEntry` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerTopologyEntry {
    /// The `worker_type` field.
    pub worker_type: WorkerType,
    /// The `instance_map_keys` field.
    pub instance_map_keys: Vec<String>,
    /// The `domains` field.
    pub domains: Vec<String>,
    /// The `owned_route_prefixes` field.
    pub owned_route_prefixes: Vec<String>,
    /// The `replication_streams` field.
    pub replication_streams: Vec<String>,
    /// The `capabilities` field.
    pub capabilities: WorkerCapabilities,
}

impl WorkerTopologyEntry {
    /// See [`for_type`].
    /// See [`for_type`].
    pub fn for_type(worker_type: WorkerType) -> Self {
        Self {
            worker_type,
            instance_map_keys: worker_type.instance_map_keys().iter().map(|value| (*value).to_string()).collect(),
            domains: worker_type.responsibility_domains().iter().map(|value| (*value).to_string()).collect(),
            owned_route_prefixes: worker_type.owned_route_prefixes().iter().map(|value| (*value).to_string()).collect(),
            replication_streams: worker_type.replication_streams().iter().map(|value| (*value).to_string()).collect(),
            capabilities: WorkerCapabilities::for_type(&worker_type),
        }
    }
}

/// The `WorkerTopologyPresetInstance` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerTopologyPresetInstance {
    /// The `instance_name` field.
    pub instance_name: String,
    /// The `worker_type` field.
    pub worker_type: WorkerType,
    /// The `count` field.
    pub count: u16,
    /// The `purpose` field.
    pub purpose: String,
}

/// The `WorkerTopologyPreset` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerTopologyPreset {
    /// The `name` field.
    pub name: String,
    /// The `description` field.
    pub description: String,
    /// The `instances` field.
    pub instances: Vec<WorkerTopologyPresetInstance>,
}

impl WorkerTopologyPreset {
    /// See [`worker_types`].
    /// See [`worker_types`].
    pub fn worker_types(&self) -> Vec<WorkerType> {
        self.instances.iter().map(|instance| instance.worker_type).collect()
    }
}

/// The `WorkerTopologySummary` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerTopologySummary {
    /// The `workers` field.
    pub workers: Vec<WorkerTopologyEntry>,
    /// The `deployment_presets` field.
    pub deployment_presets: Vec<WorkerTopologyPreset>,
}

impl WorkerTopologySummary {
    /// See [`baseline`].
    /// See [`baseline`].
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

    /// See [`baseline_preset`].
    /// See [`baseline_preset`].
    pub fn baseline_preset(name: &str) -> Option<WorkerTopologyPreset> {
        Self::baseline().deployment_presets.into_iter().find(|preset| preset.name == name)
    }
}

/// The `WorkerRow` struct.
#[derive(Debug, Clone, FromRow)]
pub struct WorkerRow {
    /// The `id` field.
    pub id: i64,
    /// The `worker_id` field.
    pub worker_id: String,
    /// The `worker_name` field.
    pub worker_name: String,
    /// The `worker_type` field.
    pub worker_type: String,
    /// The `host` field.
    pub host: String,
    /// The `port` field.
    pub port: i32,
    /// The `status` field.
    pub status: String,
    /// The `last_heartbeat_ts` field.
    pub last_heartbeat_ts: Option<i64>,
    /// The `started_ts` field.
    pub started_ts: i64,
    /// The `stopped_ts` field.
    pub stopped_ts: Option<i64>,
    /// The `config` field.
    pub config: serde_json::Value,
    /// The `metadata` field.
    pub metadata: serde_json::Value,
    /// The `version` field.
    pub version: Option<String>,
}

impl From<WorkerRow> for WorkerInfo {
    fn from(row: WorkerRow) -> Self {
        Self {
            id: row.id,
            worker_id: row.worker_id,
            worker_name: row.worker_name,
            worker_type: row.worker_type,
            host: row.host,
            port: row.port,
            status: row.status,
            last_heartbeat_ts: row.last_heartbeat_ts,
            started_ts: row.started_ts,
            stopped_ts: row.stopped_ts,
            config: row.config,
            metadata: row.metadata,
            version: row.version,
        }
    }
}

/// The `WorkerCommandRow` struct.
#[derive(Debug, Clone, FromRow)]
pub struct WorkerCommandRow {
    /// The `id` field.
    pub id: i64,
    /// The `command_id` field.
    pub command_id: String,
    /// The `target_worker_id` field.
    pub target_worker_id: String,
    /// The `source_worker_id` field.
    pub source_worker_id: Option<String>,
    /// The `command_type` field.
    pub command_type: String,
    /// The `command_data` field.
    pub command_data: serde_json::Value,
    /// The `priority` field.
    pub priority: i32,
    /// The `status` field.
    pub status: String,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `sent_ts` field.
    pub sent_ts: Option<i64>,
    /// The `completed_ts` field.
    pub completed_ts: Option<i64>,
    /// The `error_message` field.
    pub error_message: Option<String>,
    /// The `retry_count` field.
    pub retry_count: i32,
    /// The `max_retries` field.
    pub max_retries: i32,
}

impl From<WorkerCommandRow> for WorkerCommand {
    fn from(row: WorkerCommandRow) -> Self {
        Self {
            id: row.id,
            command_id: row.command_id,
            target_worker_id: row.target_worker_id,
            source_worker_id: row.source_worker_id,
            command_type: row.command_type,
            command_data: row.command_data,
            priority: row.priority,
            status: row.status,
            created_ts: row.created_ts,
            sent_ts: row.sent_ts,
            completed_ts: row.completed_ts,
            error_message: row.error_message,
            retry_count: row.retry_count,
            max_retries: row.max_retries,
        }
    }
}

/// The `WorkerEventRow` struct.
#[derive(Debug, Clone, FromRow)]
pub struct WorkerEventRow {
    /// The `id` field.
    pub id: i64,
    /// The `event_id` field.
    pub event_id: String,
    /// The `stream_id` field.
    pub stream_id: i64,
    /// The `event_type` field.
    pub event_type: String,
    /// The `room_id` field.
    pub room_id: Option<String>,
    /// The `sender` field.
    pub sender: Option<String>,
    /// The `event_data` field.
    pub event_data: serde_json::Value,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `processed_by` field.
    pub processed_by: Option<sqlx::types::Json<Vec<String>>>,
}

impl From<WorkerEventRow> for WorkerEvent {
    fn from(row: WorkerEventRow) -> Self {
        Self {
            id: row.id,
            event_id: row.event_id,
            stream_id: row.stream_id,
            event_type: row.event_type,
            room_id: row.room_id,
            sender: row.sender,
            event_data: row.event_data,
            created_ts: row.created_ts,
            processed_by: row.processed_by.map(|p| p.0),
        }
    }
}

/// The `UpdateConnectionStatsRequest` struct.
#[derive(Debug, Clone, Default)]
pub struct UpdateConnectionStatsRequest {
    /// The `source_worker_id` field.
    pub source_worker_id: String,
    /// The `target_worker_id` field.
    pub target_worker_id: String,
    /// The `connection_type` field.
    pub connection_type: String,
    /// The `bytes_sent` field.
    pub bytes_sent: i64,
    /// The `bytes_received` field.
    pub bytes_received: i64,
    /// The `messages_sent` field.
    pub messages_sent: i64,
    /// The `messages_received` field.
    pub messages_received: i64,
}

impl UpdateConnectionStatsRequest {
    /// See [`new`].
    pub fn new(
        source_worker_id: impl Into<String>,
        target_worker_id: impl Into<String>,
        connection_type: impl Into<String>,
    ) -> Self {
        Self {
            source_worker_id: source_worker_id.into(),
            target_worker_id: target_worker_id.into(),
            connection_type: connection_type.into(),
            ..Default::default()
        }
    }

    /// See [`bytes_sent`].
    /// See [`bytes_sent`].
    pub fn bytes_sent(mut self, bytes_sent: i64) -> Self {
        self.bytes_sent = bytes_sent;
        self
    }

    /// See [`bytes_received`].
    /// See [`bytes_received`].
    pub fn bytes_received(mut self, bytes_received: i64) -> Self {
        self.bytes_received = bytes_received;
        self
    }

    /// See [`messages_sent`].
    /// See [`messages_sent`].
    pub fn messages_sent(mut self, messages_sent: i64) -> Self {
        self.messages_sent = messages_sent;
        self
    }

    /// See [`messages_received`].
    /// See [`messages_received`].
    pub fn messages_received(mut self, messages_received: i64) -> Self {
        self.messages_received = messages_received;
        self
    }
}
