//! Worker module facade.
//!
//! Re-exports the canonical implementation from `synapse_services::worker`.

pub mod bus;
pub mod health;
pub mod load_balancer;
pub mod manager;
pub mod protocol;
pub mod storage;
pub mod stream;
pub mod tcp;
pub mod topology_validator;
pub mod types;

pub use synapse_services::worker::{
    BusMessage, HealthCheckConfig, HealthCheckResult, HealthChecker, HealthStatus, LoadBalanceStrategy,
    RedisBusConfig, ReplicationCommand, ReplicationEvent, ReplicationProtocol, StreamWriterManager, WorkerBus,
    WorkerLoadBalancer, WorkerLoadStats, WorkerManager, WorkerStorage,
};
pub use synapse_services::worker::topology_validator::{
    current_instance_worker_type, expected_route_owner_for_probe, global_maintenance_owner,
    resolved_current_instance_name, should_run_global_maintenance, validate_topology, validate_worker_config,
    RouteOwnerProbe, TopologyValidation,
};
pub use synapse_services::worker::types::{
    AssignTaskRequest, HeartbeatRequest, RdataEvent, RdataPosition, RegisterWorkerRequest, ReplicationPosition,
    SendCommandRequest, StreamPosition, UpdateConnectionStatsRequest, WorkerCapabilities, WorkerCommand,
    WorkerCommandRow, WorkerConnection, WorkerEvent, WorkerEventRow, WorkerInfo, WorkerLoadStatsUpdate,
    WorkerResponsibilitySummary, WorkerRow, WorkerRuntimeConfig, WorkerStatus, WorkerTaskAssignment,
    WorkerTopologyEntry, WorkerTopologyPreset, WorkerTopologyPresetInstance, WorkerTopologySummary, WorkerType,
};
