use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct PerformanceConfig {
    #[serde(default = "default_sync_event_limit")]
    pub sync_event_limit: u32,
    #[serde(default = "default_sync_poll_interval_ms")]
    pub sync_poll_interval_ms: u64,
    #[serde(default = "default_sync_slow_request_threshold_ms")]
    pub sync_slow_request_threshold_ms: u64,
    #[serde(default = "default_sync_to_device_limit")]
    pub sync_to_device_limit: u32,
    #[serde(default = "default_sync_ephemeral_limit")]
    pub sync_ephemeral_limit: u32,
    /// Sliding sync response latency threshold in milliseconds. When a
    /// sliding sync response takes longer than this, a warning is logged
    /// and the request is counted in the slow-request metrics. This acts
    /// as a performance rollback gate inspired by Synapse v1.153.0rc3,
    /// which reverted a sliding-sync optimisation after performance
    /// regressions went unnoticed.
    #[serde(default = "default_sliding_sync_latency_threshold_ms")]
    pub sliding_sync_latency_threshold_ms: u64,
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            sync_event_limit: default_sync_event_limit(),
            sync_poll_interval_ms: default_sync_poll_interval_ms(),
            sync_slow_request_threshold_ms: default_sync_slow_request_threshold_ms(),
            sync_to_device_limit: default_sync_to_device_limit(),
            sync_ephemeral_limit: default_sync_ephemeral_limit(),
            sliding_sync_latency_threshold_ms: default_sliding_sync_latency_threshold_ms(),
        }
    }
}

fn default_sync_event_limit() -> u32 {
    100
}

fn default_sync_poll_interval_ms() -> u64 {
    250
}

fn default_sync_slow_request_threshold_ms() -> u64 {
    750
}

fn default_sync_to_device_limit() -> u32 {
    200
}

fn default_sync_ephemeral_limit() -> u32 {
    100
}

fn default_sliding_sync_latency_threshold_ms() -> u64 {
    5000
}
