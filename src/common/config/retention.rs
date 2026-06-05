use serde::Deserialize;

// ============================================================================
// SECTION: Retention Policy
// ============================================================================

/// Message retention policy configuration.
///
/// Configures policies for automatically deleting old messages.
#[derive(Debug, Clone, Deserialize)]
pub struct RetentionConfig {
    /// Whether to enable message retention
    #[serde(default)]
    pub enabled: bool,

    /// Default retention policy
    #[serde(default)]
    pub default_policy: Option<RetentionPolicy>,

    /// Minimum allowed retention period (seconds)
    #[serde(default)]
    pub allowed_lifetime_min: Option<u64>,

    /// Maximum allowed retention period (seconds)
    #[serde(default)]
    pub allowed_lifetime_max: Option<u64>,

    /// Whether to purge deleted messages
    #[serde(default = "default_retention_purge_jobs")]
    pub purge_jobs: Vec<RetentionPurgeJob>,

    /// Whether to enable continuous data lifecycle cleanup
    #[serde(default = "default_retention_lifecycle_cleanup_enabled")]
    pub lifecycle_cleanup_enabled: bool,

    /// Data lifecycle cleanup execution interval (seconds)
    #[serde(default = "default_retention_lifecycle_interval_secs")]
    pub lifecycle_cleanup_interval_secs: u64,

    /// Lifecycle cleanup batch size
    #[serde(default = "default_retention_cleanup_batch_size")]
    pub cleanup_batch_size: u32,

    /// Audit event retention days
    #[serde(default = "default_retention_audit_retention_days")]
    pub audit_retention_days: u64,

    /// Completed cleanup queue record retention days
    #[serde(default = "default_retention_queue_retention_days")]
    pub queue_retention_days: u64,
}

/// Retention policy
#[derive(Debug, Clone, Deserialize)]
pub struct RetentionPolicy {
    /// Minimum retention period (seconds)
    #[serde(default)]
    pub min_lifetime: Option<u64>,

    /// Maximum retention period (seconds)
    #[serde(default)]
    pub max_lifetime: Option<u64>,
}

/// Retention purge job
#[derive(Debug, Clone, Deserialize)]
pub struct RetentionPurgeJob {
    /// Purge interval (seconds)
    #[serde(default = "default_purge_job_interval")]
    pub interval: u64,

    /// Maximum number of rooms per purge
    #[serde(default = "default_purge_job_batch_size")]
    pub batch_size: u32,
}

fn default_retention_purge_jobs() -> Vec<RetentionPurgeJob> {
    vec![RetentionPurgeJob { interval: default_purge_job_interval(), batch_size: default_purge_job_batch_size() }]
}

fn default_purge_job_interval() -> u64 {
    86400
}

fn default_purge_job_batch_size() -> u32 {
    100
}

fn default_retention_lifecycle_cleanup_enabled() -> bool {
    true
}

fn default_retention_lifecycle_interval_secs() -> u64 {
    300
}

fn default_retention_cleanup_batch_size() -> u32 {
    100
}

fn default_retention_audit_retention_days() -> u64 {
    90
}

fn default_retention_queue_retention_days() -> u64 {
    30
}

impl Default for RetentionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            default_policy: None,
            allowed_lifetime_min: None,
            allowed_lifetime_max: None,
            purge_jobs: default_retention_purge_jobs(),
            lifecycle_cleanup_enabled: default_retention_lifecycle_cleanup_enabled(),
            lifecycle_cleanup_interval_secs: default_retention_lifecycle_interval_secs(),
            cleanup_batch_size: default_retention_cleanup_batch_size(),
            audit_retention_days: default_retention_audit_retention_days(),
            queue_retention_days: default_retention_queue_retention_days(),
        }
    }
}