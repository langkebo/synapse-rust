//! Offline command-line entry points that run *before* the daemon (or a worker
//! process) starts.
//!
//! `--validate-topology` answers one question — "is this worker configuration
//! deployable?" — with an exit code and nothing else (W-B). It exists because the
//! startup check has always been advisory (see `worker.topology.strict`): a bad
//! `stream_writers` / `instance_map` block was logged and the process started
//! anyway, so the mistake surfaced later as "no events are being persisted"
//! instead of as a failed deploy.
//!
//! The verdict is deliberately the *same* [`validate_worker_config`] the startup
//! path uses, so a pre-deploy check and the running instance cannot disagree
//! about whether a configuration is valid. Nothing is started, no database or
//! Redis connection is opened — only the configuration is read, which is what
//! makes this usable from CI.
//!
//! The runtime half of the topology picture (declared vs *registered*) cannot be
//! answered offline by construction: it needs a live registry. That is
//! `GET /_synapse/worker/v1/topology/validate`'s `runtime` field.

use synapse_common::config::worker::WorkerConfig;
use synapse_common::config::Config;
use synapse_services::worker::topology_validator::validate_worker_config;

/// Command-line flag that selects the offline topology check.
pub const VALIDATE_TOPOLOGY_FLAG: &str = "--validate-topology";

/// The configured topology is valid.
pub const TOPOLOGY_OK: i32 = 0;

/// The configured topology has errors: do not deploy this configuration.
pub const TOPOLOGY_INVALID: i32 = 1;

/// The configuration itself could not be loaded or validated.
pub const TOPOLOGY_CONFIG_ERROR: i32 = 2;

/// Whether `args` asks for the offline topology check.
///
/// Generic over the item type so both `std::env::args()` (the real caller) and a
/// literal slice (tests) can drive it.
pub fn wants_topology_validation<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter().any(|argument| argument.as_ref() == VALIDATE_TOPOLOGY_FLAG)
}

/// The exit code for an already-loaded configuration.
///
/// Reports on stderr as well as through `tracing`: `main` calls this *before*
/// telemetry is initialised, so the log subscriber is frequently not wired yet —
/// the same reason `main` falls back to `eprintln!` for configuration failures.
///
/// A configuration problem that only produces *warnings* is reported as valid
/// (`TOPOLOGY_OK`), matching the startup path, which also treats warnings as
/// non-fatal.
pub fn topology_exit_code(worker: &WorkerConfig, redis_enabled: bool) -> i32 {
    let mut validation = validate_worker_config(worker);
    // Same extra check as the startup path (`check_startup_topology`): a notify-bound
    // stream cannot be forwarded without Redis, and the two verdicts must not disagree
    // about whether a configuration is deployable (design doc §4.5).
    for message in synapse_services::worker::write_streams::missing_notify_fan_out(worker, redis_enabled) {
        validation.valid = false;
        validation.errors.push(message);
    }
    validation.log();

    for warning in &validation.warnings {
        eprintln!("warning: {warning}");
    }

    if validation.valid {
        println!("worker topology: OK ({} warning(s))", validation.warnings.len());
        return TOPOLOGY_OK;
    }

    for error in &validation.errors {
        eprintln!("error: {error}");
    }
    eprintln!(
        "worker topology: {} error(s) — this configuration is not deployable. \
         Fix the worker block, or (to keep starting anyway) drop the check.",
        validation.errors.len()
    );
    TOPOLOGY_INVALID
}

/// Load the configuration the same way the daemon would, then run the offline
/// check on it.
///
/// Split from [`topology_exit_code`] so the verdict is unit-testable without a
/// configuration file: `Config::load` resolves `SYNAPSE_CONFIG_PATH` and the
/// environment, which a test should not have to fake.
pub fn run_offline_topology_check() -> i32 {
    match Config::load() {
        Ok(config) => topology_exit_code(&config.worker, config.redis.enabled),
        Err(error) => {
            eprintln!("Failed to load configuration: {error}");
            TOPOLOGY_CONFIG_ERROR
        }
    }
}
