use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

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
            owned_route_prefixes: worker_type.owned_route_prefixes().iter().map(|value| (*value).to_string()).collect(),
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
        Self::baseline().deployment_presets.into_iter().find(|preset| preset.name == name)
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct WorkerRow {
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

#[derive(Debug, Clone, FromRow)]
pub struct WorkerCommandRow {
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

#[derive(Debug, Clone, FromRow)]
pub struct WorkerEventRow {
    pub id: i64,
    pub event_id: String,
    pub stream_id: i64,
    pub event_type: String,
    pub room_id: Option<String>,
    pub sender: Option<String>,
    pub event_data: serde_json::Value,
    pub created_ts: i64,
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

#[derive(Debug, Clone, Default)]
pub struct UpdateConnectionStatsRequest {
    pub source_worker_id: String,
    pub target_worker_id: String,
    pub connection_type: String,
    pub bytes_sent: i64,
    pub bytes_received: i64,
    pub messages_sent: i64,
    pub messages_received: i64,
}

impl UpdateConnectionStatsRequest {
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

    pub fn bytes_sent(mut self, bytes_sent: i64) -> Self {
        self.bytes_sent = bytes_sent;
        self
    }

    pub fn bytes_received(mut self, bytes_received: i64) -> Self {
        self.bytes_received = bytes_received;
        self
    }

    pub fn messages_sent(mut self, messages_sent: i64) -> Self {
        self.messages_sent = messages_sent;
        self
    }

    pub fn messages_received(mut self, messages_received: i64) -> Self {
        self.messages_received = messages_received;
        self
    }
}

#[derive(Clone)]
pub struct WorkerStorage {
    pool: Arc<PgPool>,
}

impl WorkerStorage {
    fn status_releases_in_flight_work(status: &str) -> bool {
        matches!(status, "stopped" | "error")
    }

    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn register_worker(&self, request: RegisterWorkerRequest) -> Result<WorkerInfo, sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        let config = request.config.unwrap_or(serde_json::json!({}));
        let metadata = request.metadata.unwrap_or(serde_json::json!({}));

        let row: WorkerRow = sqlx::query_as::<_, WorkerRow>(
            r#"
            INSERT INTO workers (
                worker_id, worker_name, worker_type, host, port, status, started_ts, config, metadata, version
            )
            VALUES ($1, $2, $3, $4, $5, 'starting', $6, $7, $8, $9)
            RETURNING id, worker_id, worker_name,
                      worker_type, host, port,
                      status, last_heartbeat_ts,
                      started_ts, stopped_ts,
                      COALESCE(config, '{}'::jsonb) as config,
                      COALESCE(metadata, '{}'::jsonb) as metadata,
                      version
            "#,
        )
        .bind(&request.worker_id)
        .bind(&request.worker_name)
        .bind(request.worker_type.as_str())
        .bind(&request.host)
        .bind(request.port as i32)
        .bind(now)
        .bind(&config)
        .bind(&metadata)
        .bind(request.version.as_deref())
        .fetch_one(&*self.pool)
        .await?;

        Ok(row.into())
    }

    pub async fn get_worker(&self, worker_id: &str) -> Result<Option<WorkerInfo>, sqlx::Error> {
        let row: Option<WorkerRow> = sqlx::query_as::<_, WorkerRow>(
            r#"SELECT id, worker_id, worker_name,
                      worker_type, host, port,
                      status, last_heartbeat_ts,
                      started_ts, stopped_ts,
                      COALESCE(config, '{}'::jsonb) as config,
                      COALESCE(metadata, '{}'::jsonb) as metadata,
                      version
               FROM workers WHERE worker_id = $1"#,
        )
        .bind(worker_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row.map(|r| r.into()))
    }

    pub async fn get_workers_by_type(&self, worker_type: &str) -> Result<Vec<WorkerInfo>, sqlx::Error> {
        let rows: Vec<WorkerRow> = sqlx::query_as::<_, WorkerRow>(
            r#"SELECT id, worker_id, worker_name,
                      worker_type, host, port,
                      status, last_heartbeat_ts,
                      started_ts, stopped_ts,
                      COALESCE(config, '{}'::jsonb) as config,
                      COALESCE(metadata, '{}'::jsonb) as metadata,
                      version
               FROM workers WHERE worker_type = $1 ORDER BY started_ts DESC"#,
        )
        .bind(worker_type)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    pub async fn get_active_workers(&self) -> Result<Vec<WorkerInfo>, sqlx::Error> {
        let rows: Vec<WorkerRow> = sqlx::query_as::<_, WorkerRow>(
            r#"
            SELECT id, worker_id, worker_name,
                   worker_type, host, port,
                   status, last_heartbeat_ts,
                   started_ts, stopped_ts,
                   COALESCE(config, '{}'::jsonb) as config,
                   COALESCE(metadata, '{}'::jsonb) as metadata,
                   version
            FROM workers
            WHERE status IN ('running', 'starting')
            ORDER BY started_ts DESC
            "#,
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    pub async fn update_worker_status(&self, worker_id: &str, status: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        let mut tx = self.pool.begin().await?;

        if Self::status_releases_in_flight_work(status) {
            sqlx::query(
                r"
                UPDATE worker_task_assignments
                SET status = 'pending',
                    assigned_worker_id = NULL,
                    assigned_ts = NULL
                WHERE assigned_worker_id = $1
                  AND status IN ('pending', 'running')
                ",
            )
            .bind(worker_id)
            .execute(&mut *tx)
            .await?;
        }

        sqlx::query(
            r"
            UPDATE workers
            SET status = $2,
                last_heartbeat_ts = $3,
                stopped_ts = CASE WHEN $2 IN ('stopped', 'error') THEN $3 ELSE NULL END
            WHERE worker_id = $1
            ",
        )
        .bind(worker_id)
        .bind(status)
        .bind(now)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(())
    }

    pub async fn update_heartbeat(&self, worker_id: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query(r"UPDATE workers SET last_heartbeat_ts = $2, status = 'running' WHERE worker_id = $1")
            .bind(worker_id)
            .bind(now)
            .execute(&*self.pool)
            .await?;

        Ok(())
    }

    pub async fn unregister_worker(&self, worker_id: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            r"
            UPDATE worker_task_assignments
            SET status = 'pending',
                assigned_worker_id = NULL,
                assigned_ts = NULL
            WHERE assigned_worker_id = $1
              AND status IN ('pending', 'running')
            ",
        )
        .bind(worker_id)
        .execute(&mut *tx)
        .await?;

        sqlx::query(r"UPDATE workers SET status = 'stopped', stopped_ts = $2 WHERE worker_id = $1")
            .bind(worker_id)
            .bind(now)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        Ok(())
    }

    pub async fn create_command(&self, request: SendCommandRequest) -> Result<WorkerCommand, sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        let command_id = uuid::Uuid::new_v4().to_string();

        let row: WorkerCommandRow = sqlx::query_as::<_, WorkerCommandRow>(
            r#"
            INSERT INTO worker_commands (
                command_id, target_worker_id, command_type, command_data, priority, status, created_ts, max_retries
            )
            VALUES ($1, $2, $3, $4, $5, 'pending', $6, $7)
            RETURNING id, command_id, target_worker_id,
                      source_worker_id, command_type,
                      COALESCE(command_data, '{}'::jsonb) as command_data,
                      priority, status, created_ts,
                      sent_ts, completed_ts,
                      error_message, retry_count, max_retries
            "#,
        )
        .bind(&command_id)
        .bind(&request.target_worker_id)
        .bind(&request.command_type)
        .bind(&request.command_data)
        .bind(request.priority.unwrap_or(0))
        .bind(now)
        .bind(request.max_retries.unwrap_or(3))
        .fetch_one(&*self.pool)
        .await?;

        Ok(row.into())
    }

    pub async fn get_pending_commands(&self, worker_id: &str, limit: i64) -> Result<Vec<WorkerCommand>, sqlx::Error> {
        let rows: Vec<WorkerCommandRow> = sqlx::query_as::<_, WorkerCommandRow>(
            r#"
            SELECT id, command_id, target_worker_id,
                      source_worker_id, command_type,
                      COALESCE(command_data, '{}'::jsonb) as command_data,
                      priority, status, created_ts,
                      sent_ts, completed_ts,
                      error_message, retry_count, max_retries
            FROM worker_commands
            WHERE target_worker_id = $1 AND status = 'pending'
            ORDER BY priority DESC, created_ts ASC
            LIMIT $2
            "#,
        )
        .bind(worker_id)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    pub async fn mark_command_sent(&self, command_id: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query(r"UPDATE worker_commands SET status = 'sent', sent_ts = $2 WHERE command_id = $1")
            .bind(command_id)
            .bind(now)
            .execute(&*self.pool)
            .await?;

        Ok(())
    }

    pub async fn complete_command(&self, command_id: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query(r"UPDATE worker_commands SET status = 'completed', completed_ts = $2 WHERE command_id = $1")
            .bind(command_id)
            .bind(now)
            .execute(&*self.pool)
            .await?;

        Ok(())
    }

    pub async fn fail_command(&self, command_id: &str, error: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query(
            r"
            UPDATE worker_commands SET
                status = CASE WHEN retry_count >= max_retries THEN 'failed' ELSE 'pending' END,
                retry_count = retry_count + 1,
                error_message = $2,
                completed_ts = CASE WHEN retry_count >= max_retries THEN $3::BIGINT ELSE NULL END
            WHERE command_id = $1
            ",
        )
        .bind(command_id)
        .bind(error)
        .bind(Some(now))
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn add_event(
        &self,
        event_id: &str,
        event_type: &str,
        room_id: Option<&str>,
        sender: Option<&str>,
        event_data: serde_json::Value,
    ) -> Result<WorkerEvent, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let row = sqlx::query_as::<_, WorkerEventRow>(
            r"
            INSERT INTO worker_events (
                event_id, event_type, room_id, sender, event_data, created_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id, event_id, stream_id, event_type, room_id,
                      sender, event_data, created_ts, processed_by
            ",
        )
        .bind(event_id)
        .bind(event_type)
        .bind(room_id)
        .bind(sender)
        .bind(&event_data)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row.into())
    }

    pub async fn get_events_since(&self, stream_id: i64, limit: i64) -> Result<Vec<WorkerEvent>, sqlx::Error> {
        let rows = sqlx::query_as::<_, WorkerEventRow>(
            r"SELECT id, event_id, stream_id, event_type, room_id,
                      sender, event_data, created_ts, processed_by
               FROM worker_events WHERE stream_id > $1 ORDER BY stream_id ASC LIMIT $2",
        )
        .bind(stream_id)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    pub async fn mark_event_processed(&self, event_id: &str, worker_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r"
            UPDATE worker_events SET processed_by = array_append(COALESCE(processed_by, '{}'), $2)
            WHERE event_id = $1
            ",
        )
        .bind(event_id)
        .bind(worker_id)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn update_replication_position(
        &self,
        worker_id: &str,
        stream_name: &str,
        position: i64,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query(
            r"
            INSERT INTO replication_positions (worker_id, stream_name, stream_position, updated_ts)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (worker_id, stream_name) DO UPDATE SET
                stream_position = EXCLUDED.stream_position,
                updated_ts = EXCLUDED.updated_ts
            ",
        )
        .bind(worker_id)
        .bind(stream_name)
        .bind(position)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_replication_position(
        &self,
        worker_id: &str,
        stream_name: &str,
    ) -> Result<Option<i64>, sqlx::Error> {
        let result = sqlx::query_scalar::<_, i64>(
            r#"SELECT stream_position FROM replication_positions WHERE worker_id = $1 AND stream_name = $2"#,
        )
        .bind(worker_id)
        .bind(stream_name)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(result)
    }

    pub fn record_load_stats(&self, worker_id: &str, stats: &WorkerLoadStatsUpdate) -> Result<(), sqlx::Error> {
        tracing::debug!(
            worker_id = worker_id,
            cpu = ?stats.cpu_usage,
            memory = ?stats.memory_usage,
            connections = ?stats.active_connections,
            rps = ?stats.requests_per_second,
            latency_ms = ?stats.average_latency_ms,
            queue = ?stats.queue_depth,
            "worker load stats"
        );
        Ok(())
    }

    pub async fn assign_task(&self, request: AssignTaskRequest) -> Result<WorkerTaskAssignment, sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        let task_id = uuid::Uuid::new_v4().to_string();

        let row: WorkerTaskAssignment = sqlx::query_as::<_, WorkerTaskAssignment>(
            r#"
            INSERT INTO worker_task_assignments (
                task_id, task_type, task_data, priority, status, created_ts
            )
            VALUES ($1, $2, $3, $4, 'pending', $5)
            RETURNING id, task_id, task_type,
                      COALESCE(task_data, '{}'::jsonb) as task_data,
                      assigned_worker_id,
                      status, priority,
                      created_ts, assigned_ts,
                      completed_ts, result,
                      error_message
            "#,
        )
        .bind(&task_id)
        .bind(&request.task_type)
        .bind(&request.task_data)
        .bind(request.priority.unwrap_or(0))
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_pending_tasks(&self, limit: i64) -> Result<Vec<WorkerTaskAssignment>, sqlx::Error> {
        let rows: Vec<WorkerTaskAssignment> = sqlx::query_as::<_, WorkerTaskAssignment>(
            r#"
            SELECT id, task_id, task_type,
                      COALESCE(task_data, '{}'::jsonb) as task_data,
                      assigned_worker_id,
                      status, priority,
                      created_ts, assigned_ts,
                      completed_ts, result,
                      error_message
            FROM worker_task_assignments
            WHERE status = 'pending'
            ORDER BY priority DESC, created_ts ASC
            LIMIT $1
            "#,
        )
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn claim_next_pending_task(&self, worker_id: &str) -> Result<Option<WorkerTaskAssignment>, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query_as::<_, WorkerTaskAssignment>(
            r#"
            UPDATE worker_task_assignments
            SET assigned_worker_id = $1, assigned_ts = $2, status = 'running'
            WHERE id = (
                SELECT id
                FROM worker_task_assignments
                WHERE status = 'pending'
                  AND assigned_worker_id IS NULL
                ORDER BY priority DESC, created_ts ASC
                LIMIT 1
                FOR UPDATE SKIP LOCKED
            )
            RETURNING id, task_id, task_type,
                      COALESCE(task_data, '{}'::jsonb) as task_data,
                      assigned_worker_id,
                      status, priority,
                      created_ts, assigned_ts,
                      completed_ts, result,
                      error_message
            "#,
        )
        .bind(worker_id)
        .bind(now)
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn claim_next_pending_task_for_types(
        &self,
        worker_id: &str,
        allowed_task_types: &[String],
    ) -> Result<Option<WorkerTaskAssignment>, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query_as::<_, WorkerTaskAssignment>(
            r#"
            UPDATE worker_task_assignments
            SET assigned_worker_id = $1, assigned_ts = $2, status = 'running'
            WHERE id = (
                SELECT id
                FROM worker_task_assignments
                WHERE status = 'pending'
                  AND assigned_worker_id IS NULL
                  AND task_type = ANY($3)
                ORDER BY priority DESC, created_ts ASC
                LIMIT 1
                FOR UPDATE SKIP LOCKED
            )
            RETURNING id, task_id, task_type,
                      COALESCE(task_data, '{}'::jsonb) as task_data,
                      assigned_worker_id,
                      status, priority,
                      created_ts, assigned_ts,
                      completed_ts, result,
                      error_message
            "#,
        )
        .bind(worker_id)
        .bind(now)
        .bind(allowed_task_types)
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn assign_task_to_worker(&self, task_id: &str, worker_id: &str) -> Result<bool, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let result: sqlx::postgres::PgQueryResult = sqlx::query(
            r"
            UPDATE worker_task_assignments
            SET assigned_worker_id = $2, assigned_ts = $3, status = 'running'
            WHERE task_id = $1
              AND status = 'pending'
              AND assigned_worker_id IS NULL
            ",
        )
        .bind(task_id)
        .bind(worker_id)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected() == 1)
    }

    pub async fn complete_task(&self, task_id: &str, result: Option<serde_json::Value>) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query(
            r"UPDATE worker_task_assignments SET status = 'completed', completed_ts = $2, result = $3 WHERE task_id = $1",
        )
        .bind(task_id)
        .bind(now)
        .bind(result.as_ref())
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn fail_task(&self, task_id: &str, error: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query(
            r"UPDATE worker_task_assignments SET status = 'failed', completed_ts = $2, error_message = $3 WHERE task_id = $1",
        )
        .bind(task_id)
        .bind(now)
        .bind(error)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub fn record_connection(
        &self,
        source_worker_id: &str,
        target_worker_id: &str,
        connection_type: &str,
    ) -> Result<(), sqlx::Error> {
        tracing::info!(
            source = source_worker_id,
            target = target_worker_id,
            conn_type = connection_type,
            "worker connection established"
        );
        Ok(())
    }

    pub fn update_connection_stats(&self, request: &UpdateConnectionStatsRequest) -> Result<(), sqlx::Error> {
        tracing::debug!(
            source = %request.source_worker_id,
            target = %request.target_worker_id,
            conn_type = %request.connection_type,
            bytes_sent = request.bytes_sent,
            bytes_received = request.bytes_received,
            "worker connection stats"
        );
        Ok(())
    }

    pub async fn get_statistics(&self) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let rows = sqlx::query(
            r"SELECT id, worker_id, worker_name, worker_type, status,
                      host, port, last_heartbeat_ts, started_ts,
                      cpu_usage, memory_usage, active_connections,
                      requests_per_second, average_latency_ms,
                      queue_depth, pending_commands, active_tasks
               FROM worker_statistics",
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                use sqlx::Row;
                serde_json::json!({
                    "id": row.get::<i64, _>("id"),
                    "worker_id": row.get::<String, _>("worker_id"),
                    "worker_name": row.get::<Option<String>, _>("worker_name"),
                    "worker_type": row.get::<Option<String>, _>("worker_type"),
                    "status": row.get::<Option<String>, _>("status"),
                    "host": row.get::<Option<String>, _>("host"),
                    "port": row.get::<Option<i32>, _>("port"),
                    "last_heartbeat_ts": row.get::<Option<i64>, _>("last_heartbeat_ts"),
                    "started_ts": row.get::<Option<i64>, _>("started_ts"),
                    "cpu_usage": row.get::<Option<f64>, _>("cpu_usage"),
                    "memory_usage": row.get::<Option<f64>, _>("memory_usage"),
                    "active_connections": row.get::<Option<i32>, _>("active_connections"),
                    "requests_per_second": row.get::<Option<f64>, _>("requests_per_second"),
                    "average_latency_ms": row.get::<Option<f64>, _>("average_latency_ms"),
                    "queue_depth": row.get::<Option<i32>, _>("queue_depth"),
                    "pending_commands": row.get::<Option<i32>, _>("pending_commands"),
                    "active_tasks": row.get::<Option<i32>, _>("active_tasks"),
                })
            })
            .collect())
    }

    pub async fn get_type_statistics(&self) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let rows = sqlx::query(
            r"
            SELECT worker_type, total_count, running_count, starting_count,
                   stopping_count, stopped_count, avg_cpu_usage, avg_memory_usage,
                   total_connections
            FROM worker_type_statistics
            ",
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                use sqlx::Row;
                serde_json::json!({
                    "worker_type": row.get::<String, _>("worker_type"),
                    "total_count": row.get::<i64, _>("total_count"),
                    "running_count": row.get::<i64, _>("running_count"),
                    "starting_count": row.get::<i64, _>("starting_count"),
                    "stopping_count": row.get::<i64, _>("stopping_count"),
                    "stopped_count": row.get::<i64, _>("stopped_count"),
                    "avg_cpu_usage": row.get::<Option<f64>, _>("avg_cpu_usage"),
                    "avg_memory_usage": row.get::<Option<f64>, _>("avg_memory_usage"),
                    "total_connections": row.get::<Option<i64>, _>("total_connections"),
                })
            })
            .collect())
    }
}
