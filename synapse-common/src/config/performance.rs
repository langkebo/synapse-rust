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
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            sync_event_limit: default_sync_event_limit(),
            sync_poll_interval_ms: default_sync_poll_interval_ms(),
            sync_slow_request_threshold_ms: default_sync_slow_request_threshold_ms(),
            sync_to_device_limit: default_sync_to_device_limit(),
            sync_ephemeral_limit: default_sync_ephemeral_limit(),
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