//! Worker load balancing (W-C): the switch reaches the manager, and an unhealthy
//! candidate is never selected.
//!
//! Why these tests exist: `WorkerManager::enable_load_balancer` had **no
//! production call site** — only its own unit tests and one integration test
//! called it — so the documented "built-in load balancer" was true of the tests
//! and nothing else (`select_worker_for_task` always took the fallback branch).
//! The wiring is a one-line call now, and this file pins that it happens only
//! when asked for, plus that turning it on does not bypass the health filter.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use synapse_common::config::worker::WorkerLoadBalancingConfig;
use synapse_services::wiring::admin::apply_load_balancing;
use synapse_services::worker::health::{HealthCheckConfig, HealthChecker};
use synapse_services::worker::{LoadBalanceStrategy, WorkerManager};
use synapse_storage::test_mocks::InMemoryWorkerStore;
use synapse_storage::worker::{RegisterWorkerRequest, WorkerStoreApi, WorkerType};

/// A manager over an in-memory registry with the given load-balancing policy and
/// a health checker, exactly as `wiring::admin` builds it — except that one failed
/// probe is enough to reach `Unhealthy`.
///
/// The production default needs `max_consecutive_failures` misses before a worker
/// stops counting as healthy (a single failure is only `Degraded`). The hysteresis
/// is deliberate and not what this file is about, so the fixture removes it rather
/// than sleeping or probing in a loop.
fn manager_with(config: &WorkerLoadBalancingConfig) -> (WorkerManager, Arc<HealthChecker>) {
    let store = Arc::new(InMemoryWorkerStore::new());
    let health_config = HealthCheckConfig { max_consecutive_failures: 1, ..HealthCheckConfig::default() };
    let health_checker = Arc::new(HealthChecker::new(health_config));
    let mut manager = WorkerManager::new(store as Arc<dyn WorkerStoreApi>).with_health_checker(health_checker.clone());
    apply_load_balancing(&mut manager, config);
    (manager, health_checker)
}

/// Register an `event_persister`-typed worker through the manager, so the
/// registration reaches the store, the health checker and the balancer exactly as
/// a real `POST /_synapse/worker/v1/register` would.
async fn register_event_persister(manager: &WorkerManager, worker_id: &str) {
    manager
        .register(RegisterWorkerRequest {
            worker_id: worker_id.to_string(),
            worker_name: worker_id.to_string(),
            worker_type: WorkerType::EventPersister,
            host: "127.0.0.1".to_string(),
            port: 9000,
            config: None,
            metadata: None,
            version: None,
        })
        .await
        .expect("registering a worker against the in-memory store must succeed");
}

#[test]
fn the_default_configuration_leaves_load_balancing_off() {
    // The negative control for the whole batch: with the switch untouched, no
    // balancer is attached, so `select_worker_for_task` keeps taking the fallback
    // path it has always taken. Applying the default must not attach one.
    let config = WorkerLoadBalancingConfig::default();
    assert!(!config.enabled, "the switch must default to off");

    let (manager, _health) = manager_with(&config);

    assert!(manager.load_balancer().is_none(), "default must not attach a load balancer");
}

#[test]
fn enabling_the_switch_attaches_a_load_balancer() {
    let config = WorkerLoadBalancingConfig { enabled: true, ..WorkerLoadBalancingConfig::default() };

    let (manager, _health) = manager_with(&config);

    assert!(manager.load_balancer().is_some(), "the switch must actually reach the manager");
}

#[test]
fn every_configured_strategy_maps_to_its_engine_counterpart() {
    // A wrong mapping would be invisible at runtime (both are just "a strategy"),
    // so each variant is pinned to the engine type it must produce. `strategy` is
    // also the YAML spelling, which is why the config enum is `snake_case`.
    use synapse_common::config::worker::WorkerLoadBalanceStrategy as Config;

    assert_eq!(LoadBalanceStrategy::from(Config::RoundRobin), LoadBalanceStrategy::RoundRobin);
    assert_eq!(LoadBalanceStrategy::from(Config::LeastConnections), LoadBalanceStrategy::LeastConnections);
    assert_eq!(LoadBalanceStrategy::from(Config::WeightedRoundRobin), LoadBalanceStrategy::WeightedRoundRobin);
    assert_eq!(LoadBalanceStrategy::from(Config::Random), LoadBalanceStrategy::Random);
}

#[test]
fn the_configuration_spellings_are_the_snake_case_ones() {
    // W-C's contract with operators: `YAML` values are the snake_case names. A
    // rename would otherwise be a silent "unknown variant" only at deploy time.
    for (yaml, expected) in [
        ("\"round_robin\"", synapse_common::config::worker::WorkerLoadBalanceStrategy::RoundRobin),
        ("\"least_connections\"", synapse_common::config::worker::WorkerLoadBalanceStrategy::LeastConnections),
        ("\"weighted_round_robin\"", synapse_common::config::worker::WorkerLoadBalanceStrategy::WeightedRoundRobin),
        ("\"random\"", synapse_common::config::worker::WorkerLoadBalanceStrategy::Random),
    ] {
        let parsed: synapse_common::config::worker::WorkerLoadBalanceStrategy =
            serde_json::from_str(yaml).expect("the documented spelling must deserialize");
        assert_eq!(parsed, expected, "spelling {yaml} must map to {expected:?}");
    }
}

#[tokio::test]
async fn a_candidate_the_health_checker_cannot_vouch_for_is_never_selected() {
    // The positive control: with the balancer attached, the strategy picks a
    // candidate out of the registry — and a candidate the health checker has no
    // reading for must still never come back as the answer.
    //
    // The fixture state is the realistic one: `WorkerManager::update_worker_status`
    // drops a worker from the health checker as soon as it reports
    // `stopping`/`stopped`/`error`, while the registry row stays until it is
    // unregistered. So "in the registry, not vouched for" is a state production
    // reaches, not an artificial one.
    let config = WorkerLoadBalancingConfig { enabled: true, strategy: Default::default() };
    let (manager, health) = manager_with(&config);

    register_event_persister(&manager, "healthy").await;
    register_event_persister(&manager, "reported_stopped").await;

    // The checker has no reading for the stopped worker (`is_healthy` is false for
    // anything it cannot see), and one successful probe for the live one.
    health.unregister_worker("reported_stopped").await;
    health.check_health("healthy").await;

    assert!(health.is_healthy("healthy").await, "fixture setup: the probed worker must be healthy");
    assert!(
        !health.is_healthy("reported_stopped").await,
        "fixture setup: the unvouched worker must not be healthy, got {:?}",
        health.get_health("reported_stopped").await
    );

    let selected = manager.select_worker_for_task("event_persist").await.expect("selection must succeed");

    assert_eq!(selected.as_deref(), Some("healthy"), "an unvouched candidate must never be selected");
}

#[tokio::test]
async fn an_unvouched_worker_is_used_as_a_last_resort_not_a_preference() {
    // With no healthy candidate at all, selection deliberately degrades to "any
    // candidate": `select_fallback_candidate` only filters by health when that
    // filter leaves something (`manager.rs`), because "a worker is better than no
    // worker" is the chosen trade-off. Pinned here so the test above is not read as
    // a guarantee that an unvouched worker is *never* used — it is, when it is the
    // only one, and a caller that cannot tolerate that must check
    // `runtime.stale` / the health metrics (W-A / W-E) rather than rely on this.
    let config = WorkerLoadBalancingConfig { enabled: true, strategy: Default::default() };
    let (manager, health) = manager_with(&config);

    register_event_persister(&manager, "reported_stopped").await;
    health.unregister_worker("reported_stopped").await;
    assert!(!health.is_healthy("reported_stopped").await, "fixture setup: the unvouched worker must not be healthy");

    let selected = manager.select_worker_for_task("event_persist").await.expect("selection must succeed");

    assert_eq!(
        selected.as_deref(),
        Some("reported_stopped"),
        "with nothing healthy left, the unvouched candidate is the last resort"
    );
}
