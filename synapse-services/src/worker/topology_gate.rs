//! Startup enforcement of the static worker-topology verdict (W-B).
//!
//! [`crate::worker::topology_validator::validate_worker_config`] reports; this
//! module decides what the **startup path** does with that report. Until now the
//! answer was hard-coded: log it and start anyway, which is how a
//! `stream_writers.events` pointing at a non-existent instance turned into "events
//! are not being persisted" hours later instead of a failed boot.
//!
//! The decision lives here rather than inline in `src/server/mod.rs` because a
//! refusal to start is otherwise only observable by booting a real server against
//! a deliberately broken configuration (database and all). As a plain function
//! over `WorkerConfig`, both polarities are ordinary unit tests.
//!
//! Scope note: this is the **static** half only. Whether the declared instances
//! are actually *running* cannot be decided at startup — nothing has registered
//! yet — and is answered later by
//! [`crate::worker::runtime_topology::evaluate_runtime_topology`].

use synapse_common::config::worker::WorkerConfig;

use crate::worker::topology_validator::{validate_worker_config, TopologyValidation};

/// The startup topology check: the report, plus what the caller must do about it.
#[derive(Debug, Clone)]
pub struct StartupTopologyCheck {
    /// What the static validator found.
    pub validation: TopologyValidation,
    /// `Some(reason)` when the caller must refuse to start.
    ///
    /// Two tiers:
    /// * **hard** — a notify-bound stream forwarded without Redis. Always fatal:
    ///   nothing is lost, but every `/sync` long-poll parked on a reader waits for
    ///   the client's own timeout (typically 30 s), and no correctness test can
    ///   observe that. `worker.topology.strict` must not be able to silence it.
    /// * **soft** — every other validator error. Fatal only under
    ///   `worker.topology.strict`; otherwise it is logged and the server starts.
    pub fatal: Option<String>,
}

/// Run the static validator and apply the configured enforcement policy.
///
/// `worker.topology.strict` escalates **soft** errors only: warnings stay advisory
/// even under `strict`, matching [`TopologyValidation::valid`] — which is false
/// exactly when there is an error — so the pre-deploy check
/// (`--validate-topology`) and the startup path cannot disagree about whether a
/// given configuration is deployable.
///
/// The one exception is the notify-fan-out finding, which is **always** fatal: it
/// is a deployment precondition (forwarding a notify-bound stream requires Redis),
/// not a latent configuration smell to be tolerated. See [`StartupTopologyCheck`].
///
/// `redis_enabled` is `redis.enabled`, which lives outside [`WorkerConfig`] and is
/// therefore passed in. It participates in one check only: a stream that reaches
/// `/sync` through the notifier fan-out cannot be forwarded without Redis, because the
/// fan-out half is the only thing that crosses instances
/// ([`crate::worker::write_streams::missing_notify_fan_out`], design doc §4.5). Callers
/// that only have the worker block pass `true`, which keeps the pre-W-I verdict.
pub fn check_startup_topology(config: &WorkerConfig, redis_enabled: bool) -> StartupTopologyCheck {
    let mut validation = validate_worker_config(config);

    // Collected separately so the verdict below can tell a *precondition* failure
    // (this one) from an ordinary validator error (the rest).
    let mut hard: Vec<String> = Vec::new();
    for message in crate::worker::write_streams::missing_notify_fan_out(config, redis_enabled) {
        validation.valid = false;
        validation.errors.push(message.clone());
        hard.push(message);
    }

    let fatal = if !hard.is_empty() {
        // Unconditional: `strict` governs advisory findings, not deployment
        // preconditions. A silent 30 s latency on every push is exactly the class
        // of failure that must not depend on someone remembering a switch.
        Some(hard.join("; "))
    } else if !validation.valid && config.topology.strict {
        Some(validation.errors.join("; "))
    } else {
        None
    };

    StartupTopologyCheck { validation, fatal }
}

#[cfg(test)]
mod tests {
    use super::*;
    use synapse_common::config::worker::{ReplicationConfig, StreamWriters};

    /// A worker-enabled configuration that the static validator rejects:
    /// `instance_map` is populated while replication is off, which is the
    /// multi-worker topology's most common missing switch.
    fn invalid_config(strict: bool) -> WorkerConfig {
        let mut config = WorkerConfig { enabled: true, instance_name: "master".to_string(), ..WorkerConfig::default() };
        config.instance_map.insert(
            "event_persister".to_string(),
            synapse_common::config::worker::InstanceLocationConfig {
                host: "127.0.0.1".to_string(),
                port: 9102,
                tls: false,
            },
        );
        config.replication = ReplicationConfig::default();
        config.topology.strict = strict;
        config
    }

    /// A worker-enabled configuration the validator accepts, with replication on
    /// and a real secret so the presence check passes.
    fn valid_config(strict: bool) -> WorkerConfig {
        let mut config = WorkerConfig { enabled: true, instance_name: "master".to_string(), ..WorkerConfig::default() };
        config.instance_map.insert(
            "event_persister".to_string(),
            synapse_common::config::worker::InstanceLocationConfig {
                host: "127.0.0.1".to_string(),
                port: 9102,
                tls: false,
            },
        );
        config.replication.enabled = true;
        config.replication.http.enabled = true;
        config.replication.http.secret = Some("a-sufficiently-long-test-secret-value".to_string());
        config.stream_writers =
            StreamWriters { events: vec!["event_persister".to_string()], ..StreamWriters::default() };
        config.topology.strict = strict;
        config
    }

    #[test]
    fn an_invalid_configuration_is_fatal_under_strict() {
        // The positive control for W-B: this is the behaviour the switch exists for.
        let check = check_startup_topology(&invalid_config(true), true);

        assert!(!check.validation.valid, "the fixture must actually be invalid");
        let reason = check.fatal.expect("strict must refuse to start on an invalid topology");
        assert!(reason.contains("replication"), "the reason must carry the validator's message, got: {reason}");
    }

    #[test]
    fn an_invalid_configuration_only_warns_by_default() {
        // The negative control: without the switch, behaviour is exactly what it
        // is today — the report is produced, and startup continues.
        let check = check_startup_topology(&invalid_config(false), true);

        assert!(!check.validation.valid);
        assert!(check.fatal.is_none(), "strict is off, so startup must not be refused");
    }

    #[test]
    fn a_valid_configuration_is_never_fatal_even_under_strict() {
        let check = check_startup_topology(&valid_config(true), true);

        assert!(check.validation.valid, "fixture must be valid, errors: {:?}", check.validation.errors);
        assert!(check.fatal.is_none());
    }

    #[test]
    fn warnings_alone_are_not_escalated_by_strict() {
        // An instance name that maps to no known worker type yields a warning, not
        // an error. `strict` escalates errors only, so this must still start: the
        // two verdicts (pre-deploy `--validate-topology` and startup) share one
        // definition of "deployable", which is `valid`.
        let mut config = valid_config(true);
        config.instance_map.insert(
            "not_a_known_worker_type".to_string(),
            synapse_common::config::worker::InstanceLocationConfig {
                host: "127.0.0.1".to_string(),
                port: 9103,
                tls: false,
            },
        );

        let check = check_startup_topology(&config, true);

        assert!(check.validation.valid, "warnings must not make the report invalid");
        assert!(!check.validation.warnings.is_empty(), "the fixture must produce a warning");
        assert!(check.fatal.is_none());
    }

    #[test]
    fn forwarding_without_redis_is_always_fatal() {
        // `events` cannot be forwarded without Redis (design doc §4.5): the write lands on
        // the writer and the only cross-instance wake-up for `/sync` is unavailable. It is
        // an *error* — not a warning — because the deployment looks healthy otherwise, and
        // a deployment precondition must not be silenceable by `worker.topology.strict`.
        let mut config = valid_config(false);
        config.write_forwarding = true;
        // `events` 与 `device_lists` 必须同实例（联邦 `send` 一个请求写两路），否则
        // `coupled_stream_conflicts` 会先报错，这条测试就测不到 Redis 那一条。
        config.stream_writers.device_lists = vec!["event_persister".to_string()];

        for strict in [true, false] {
            config.topology.strict = strict;
            let check = check_startup_topology(&config, false);
            assert!(!check.validation.valid, "strict={strict}: the report must be invalid");
            assert!(
                check.validation.errors.iter().any(|error| error.contains("redis.enabled")),
                "strict={strict}: the error must name the fix, got {:?}",
                check.validation.errors
            );
            let reason =
                check.fatal.unwrap_or_else(|| panic!("strict={strict}: forwarding without Redis must refuse to start"));
            assert!(reason.contains("redis.enabled"), "the reason must name the fix: {reason}");
        }

        // Negative control: with Redis the same configuration is valid.
        config.topology.strict = true;
        let check = check_startup_topology(&config, true);
        assert!(check.validation.valid, "{:?}", check.validation.errors);
        assert!(check.fatal.is_none());
    }

    #[test]
    fn the_monolith_default_never_needs_redis_even_though_forwarding_is_off() {
        // Guards against the new check leaking into the default deployment: forwarding is
        // off, so no stream is routed anywhere and Redis is irrelevant.
        let mut config = WorkerConfig::default();
        config.topology.strict = true;
        let check = check_startup_topology(&config, false);
        assert!(check.validation.valid);
        assert!(check.fatal.is_none());
    }

    #[test]
    fn a_disabled_worker_block_is_valid_regardless_of_strictness() {
        // Monolith deployments must not be affected by the switch at all.
        let mut config = WorkerConfig::default();
        config.topology.strict = true;

        let check = check_startup_topology(&config, true);

        assert!(check.validation.valid);
        assert!(check.fatal.is_none());
    }
}
