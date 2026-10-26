use serde::Deserialize;

// ============================================================================
// SECTION: Retention Policy
// ============================================================================

/// Message retention policy configuration.
///
/// Configures policies for automatically deleting old messages.
#[derive(Debug, Clone, Deserialize)]
pub struct RetentionConfig {
    /// Whether to enable continuous data lifecycle cleanup
    #[serde(default = "default_retention_lifecycle_cleanup_enabled")]
    pub lifecycle_cleanup_enabled: bool,

    /// Data lifecycle cleanup execution interval (seconds)
    #[serde(default = "default_retention_lifecycle_interval_secs")]
    pub lifecycle_cleanup_interval_secs: u64,

    /// Audit event retention days
    #[serde(default = "default_retention_audit_retention_days")]
    pub audit_retention_days: u64,

    /// Retention days for queued to-device messages (`to_device_messages`).
    ///
    /// `0` disables the prune. See `synapse_storage::pruning::prune_old_to_device_messages`.
    #[serde(default = "default_retention_to_device_days")]
    pub to_device_retention_days: u64,
}

fn default_retention_lifecycle_cleanup_enabled() -> bool {
    true
}

fn default_retention_lifecycle_interval_secs() -> u64 {
    300
}

fn default_retention_audit_retention_days() -> u64 {
    90
}

fn default_retention_to_device_days() -> u64 {
    30
}

impl Default for RetentionConfig {
    fn default() -> Self {
        Self {
            lifecycle_cleanup_enabled: default_retention_lifecycle_cleanup_enabled(),
            lifecycle_cleanup_interval_secs: default_retention_lifecycle_interval_secs(),
            audit_retention_days: default_retention_audit_retention_days(),
            to_device_retention_days: default_retention_to_device_days(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retention_config_default() {
        let config = RetentionConfig::default();
        assert!(config.lifecycle_cleanup_enabled);
        assert_eq!(config.lifecycle_cleanup_interval_secs, 300);
        assert_eq!(config.audit_retention_days, 90);
        assert_eq!(config.to_device_retention_days, 30);
    }
}
