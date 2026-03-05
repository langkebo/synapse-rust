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
            WorkerType::Master => "master",
            WorkerType::Frontend => "frontend",
            WorkerType::Background => "background",
            WorkerType::EventPersister => "event_persister",
            WorkerType::Synchrotron => "synchrotron",
            WorkerType::FederationSender => "federation_sender",
            WorkerType::FederationReader => "federation_reader",
            WorkerType::MediaRepository => "media_repository",
            WorkerType::Pusher => "pusher",
            WorkerType::AppService => "appservice",
        }
    }

    pub fn can_handle_http(&self) -> bool {
        matches!(
            self,
            WorkerType::Master | WorkerType::Frontend | WorkerType::Synchrotron
        )
    }

    pub fn can_handle_federation(&self) -> bool {
        matches!(
            self,
            WorkerType::Master | WorkerType::FederationSender | WorkerType::FederationReader
        )
    }

    pub fn can_persist_events(&self) -> bool {
        matches!(self, WorkerType::Master | WorkerType::EventPersister)
    }
}

impl FromStr for WorkerType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "master" => Ok(WorkerType::Master),
            "frontend" => Ok(WorkerType::Frontend),
            "background" => Ok(WorkerType::Background),
            "event_persister" => Ok(WorkerType::EventPersister),
            "synchrotron" => Ok(WorkerType::Synchrotron),
            "federation_sender" => Ok(WorkerType::FederationSender),
            "federation_reader" => Ok(WorkerType::FederationReader),
            "media_repository" => Ok(WorkerType::MediaRepository),
            "pusher" => Ok(WorkerType::Pusher),
            "appservice" => Ok(WorkerType::AppService),
            _ => Err(format!("Invalid worker type: {}", s)),
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
            WorkerStatus::Starting => "starting",
            WorkerStatus::Running => "running",
            WorkerStatus::Stopping => "stopping",
            WorkerStatus::Stopped => "stopped",
            WorkerStatus::Error => "error",
        }
    }
}

impl FromStr for WorkerStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "starting" => Ok(WorkerStatus::Starting),
            "running" => Ok(WorkerStatus::Running),
            "stopping" => Ok(WorkerStatus::Stopping),
            "stopped" => Ok(WorkerStatus::Stopped),
            "error" => Ok(WorkerStatus::Error),
            _ => Err(format!("Invalid worker status: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerConfig {
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

impl Default for WorkerConfig {
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
    fn test_worker_config_default() {
        let config = WorkerConfig::default();
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
