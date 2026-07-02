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

    pub async fn get_statistics(&self, limit: i64) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let rows = sqlx::query(
            r"SELECT id, worker_id, worker_name, worker_type, status,
                      host, port, last_heartbeat_ts, started_ts,
                      cpu_usage, memory_usage, active_connections,
                      requests_per_second, average_latency_ms,
                      queue_depth, pending_commands, active_tasks
               FROM worker_statistics
               ORDER BY id DESC
               LIMIT $1",
        )
        .bind(limit)
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

#[cfg(test)]
mod tests {
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
            &[
                "/_matrix/client/*",
                "/_matrix/federation/*",
                "/_matrix/media/*",
                "/_synapse/admin/*",
                "/_synapse/worker/*"
            ]
        );
        assert_eq!(WorkerType::Frontend.owned_route_prefixes(), &["/_matrix/client/*"]);
        assert_eq!(WorkerType::Background.owned_route_prefixes(), &["/_synapse/worker/*"]);
        assert_eq!(WorkerType::EventPersister.owned_route_prefixes(), &["/_synapse/worker/v1/replication/*"]);
        assert_eq!(
            WorkerType::Synchrotron.owned_route_prefixes(),
            &["/_matrix/client/*/sync", "/_matrix/client/v3/sync"]
        );
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
}
