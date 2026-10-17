//! Deployment-consistency guard for the reference multi-process stack (W-D).
//!
//! The stack spans three files that must agree or it silently degrades:
//!
//! * `docker/docker-compose.workers.yml` — which processes exist and what instance
//!   name each declares via `WORKER_INSTANCE_NAME`;
//! * `docker/config/homeserver.yaml` — `worker.instance_map`, i.e. which instances
//!   the master considers *declared*;
//! * `scripts/test/worker_topology_smoke.sh` — the list it asserts against.
//!
//! Get them out of step and nothing announces it: a name in the compose file but not
//! in `instance_map` is an undeclared instance (a warning, not an error), while a
//! name in `instance_map` with no process behind it is reported as `runtime.missing`
//! by the topology endpoint — which is correct behaviour but useless as a deploy
//! template. These tests are the offline half of W-D: the container-run half is
//! `scripts/test/worker_topology_smoke.sh`.
//!
//! Deliberately **not** covered here (needs a real stack): that the end-to-end stack
//! actually comes up, that workers register, and that `/sync` returns the message.
//! That is what the smoke script is for, and it is not run by `nextest`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use synapse_storage::worker::WorkerType;

const COMPOSE: &str = "docker/docker-compose.workers.yml";
const HOMESERVER: &str = "docker/config/homeserver.yaml";
const SMOKE: &str = "scripts/test/worker_topology_smoke.sh";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(rel: &str) -> String {
    let path = repo_root().join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("expected {path:?} to be readable: {e}"))
}

/// The instance names the smoke script asserts against.
///
/// Reads the `DECLARED_INSTANCES=( ... )` array rather than duplicating the list:
/// a copy here would make this test agree with itself instead of with the script.
fn smoke_declared_instances(script: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut inside = false;
    for line in script.lines() {
        if line.starts_with("readonly DECLARED_INSTANCES=(") {
            inside = true;
            continue;
        }
        if inside {
            let trimmed = line.trim();
            if trimmed == ")" {
                break;
            }
            if !trimmed.is_empty() && !trimmed.starts_with('#') {
                names.insert(trimmed.to_string());
            }
        }
    }
    assert!(!names.is_empty(), "{SMOKE}: could not read the DECLARED_INSTANCES array");
    names
}

/// The instance name every worker service declares through `WORKER_INSTANCE_NAME`.
fn compose_declared_instances(compose: &str) -> BTreeSet<String> {
    compose
        .lines()
        .filter_map(|line| line.trim().strip_prefix("WORKER_INSTANCE_NAME:"))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect()
}

/// The keys of the `worker.instance_map` mapping in the homeserver config.
///
/// A minimal indentation-aware scan: `instance_map:` sits at two spaces, each
/// instance key at four, and the mapping ends at the next key with indentation two
/// or less. A real YAML parser would be nicer, but the root crate deliberately has
/// no YAML dependency (`config_mount_tests.rs` scans lines for the same reason).
fn config_declared_instances(yaml: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut inside = false;
    for line in yaml.lines() {
        if line.trim_start().starts_with('#') {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        if line.trim() == "instance_map:" {
            assert_eq!(indent, 2, "{HOMESERVER}: worker.instance_map must sit at two spaces");
            inside = true;
            continue;
        }
        if !inside {
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        if indent <= 2 {
            break;
        }
        if indent == 4 {
            let key = line.trim().trim_end_matches(':');
            if !key.is_empty() {
                names.insert(key.to_string());
            }
        }
    }
    assert!(!names.is_empty(), "{HOMESERVER}: worker.instance_map is missing or empty");
    names
}

/// Every instance name the codebase can classify, drawn from the same table the
/// validator uses (`WorkerType::instance_map_keys`).
fn known_instance_names() -> BTreeSet<String> {
    WorkerType::all()
        .iter()
        .flat_map(|worker_type| worker_type.instance_map_keys().iter().map(|key| (*key).to_string()))
        .collect()
}

#[test]
fn the_compose_config_and_smoke_script_declare_the_same_instances() {
    let compose = compose_declared_instances(&read(COMPOSE));
    let config = config_declared_instances(&read(HOMESERVER));
    let smoke = smoke_declared_instances(&read(SMOKE));

    assert_eq!(
        compose, config,
        "{COMPOSE} declares {compose:?} but {HOMESERVER} declares {config:?}: a compose-only name is an undeclared \
         instance, a config-only name is a permanently `missing` one"
    );
    assert_eq!(smoke, config, "{SMOKE} asserts on {smoke:?} but {HOMESERVER} declares {config:?}");
}

#[test]
fn every_declared_instance_is_a_name_the_validator_can_classify() {
    let known = known_instance_names();
    for name in config_declared_instances(&read(HOMESERVER)) {
        assert!(
            known.contains(&name),
            "'{name}' is not one of the names `WorkerType::instance_map_keys` knows ({known:?}); the static validator \
             would emit a warning and route-ownership checks would be incomplete"
        );
    }
}

#[test]
fn the_master_enables_the_worker_surface_so_workers_can_register() {
    // Registration/heartbeat routes are only mounted when `worker.enabled` is true
    // (`WorkerBodyModule::merge_into`). Without this line in the override the stack
    // starts six workers that can never register — they would only log heartbeats
    // failing, and `runtime.missing` would list every one of them.
    let compose = read(COMPOSE);
    assert!(
        compose.contains("SYNAPSE__WORKER__ENABLED"),
        "{COMPOSE} must enable the worker surface on the master, otherwise no worker can register"
    );
}

#[test]
fn every_worker_service_is_pointed_at_the_master_and_carries_the_shared_secret() {
    // Both are silent-failure switches: the default heartbeat target is
    // `127.0.0.1:<server.port>`, which inside a container points at the worker
    // itself, and a missing secret makes the worker skip heartbeats entirely
    // (`synapse_worker.rs` logs a warning and disables the sender) instead of
    // failing loudly.
    let compose = read(COMPOSE);
    assert!(
        compose.contains("SYNAPSE_WORKER_BASE_URL:"),
        "{COMPOSE} must point workers at the master; the default is the local listener"
    );
    assert!(
        compose.contains("WORKER_REPLICATION_SECRET"),
        "{COMPOSE} must give workers the shared secret, otherwise they never heartbeat"
    );
}

#[test]
fn the_image_actually_contains_the_worker_binary() {
    // The multi-process stack was unrunnable for a more basic reason than topology:
    // the Dockerfile only built and installed `synapse-rust` and `healthcheck`, so
    // `command: ["/app/synapse_worker"]` would have failed with "not found".
    let dockerfile = read("docker/Dockerfile");
    assert!(dockerfile.contains("--bin synapse_worker"), "docker/Dockerfile must build the worker binary");
    assert!(
        dockerfile.contains("/out/app/synapse_worker"),
        "docker/Dockerfile must install the worker binary, otherwise the compose stack cannot start it"
    );
}

#[test]
fn background_worker_is_deliberately_not_declared() {
    // Declaring `background_worker` hands global maintenance (pruning, the
    // app-service scheduler) to whichever instance carries that name
    // (`global_maintenance_owner`). A `synapse_worker` process does not run those
    // duties, so the reference stack must not declare it: master would skip the
    // work and the named worker would never do it, with no error anywhere.
    let declared = config_declared_instances(&read(HOMESERVER));
    assert!(
        !declared.contains("background_worker"),
        "declaring `background_worker` without a process that runs those duties silently disables global \
         maintenance — see the note next to `worker.instance_map` in {HOMESERVER} (audit W-7)"
    );
    assert!(
        !compose_declared_instances(&read(COMPOSE)).contains("background_worker"),
        "{COMPOSE} must not start a `background_worker` it cannot honour"
    );
}
