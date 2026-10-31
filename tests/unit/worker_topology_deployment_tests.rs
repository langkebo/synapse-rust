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

use std::collections::{BTreeMap, BTreeSet};
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

/// The `worker.stream_writers` mapping in the homeserver config, as
/// `stream name -> declared owners`.
///
/// Same indentation-aware scan as [`config_declared_instances`]: `stream_writers:`
/// sits at two spaces, each stream key at four, each list item (`- value`) at six.
fn config_stream_writers(yaml: &str) -> BTreeMap<String, Vec<String>> {
    let mut streams: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut inside = false;
    let mut current: Option<String> = None;
    for line in yaml.lines() {
        if line.trim_start().starts_with('#') {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed == "stream_writers:" {
            assert_eq!(indent, 2, "{HOMESERVER}: worker.stream_writers must sit at two spaces");
            inside = true;
            continue;
        }
        if !inside {
            continue;
        }
        if indent <= 2 {
            break;
        }
        if indent == 4 {
            let stream = trimmed.trim_end_matches(':').to_string();
            assert!(!stream.is_empty(), "{HOMESERVER}: empty stream key in worker.stream_writers");
            streams.entry(stream.clone()).or_default();
            current = Some(stream);
            continue;
        }
        if indent == 6 {
            let item = trimmed
                .strip_prefix("- ")
                .unwrap_or_else(|| panic!("{HOMESERVER}: unexpected line in worker.stream_writers: {trimmed:?}"));
            let stream = current.clone().expect("a list item cannot precede its stream key");
            streams.entry(stream).or_default().push(item.trim().trim_matches('"').to_string());
        }
    }
    assert!(!streams.is_empty(), "{HOMESERVER}: worker.stream_writers is missing or empty");
    streams
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

/// The single env variable that names the writer for both `events` and
/// `device_lists`.
///
/// One variable for two streams is deliberate: `PUT /_matrix/federation/v1/send/{txn}`
/// writes both in a single request, so the validator rejects a split
/// (`events` and `device_lists` must name the same instance).
const EVENT_WRITER_PLACEHOLDER: &str = "${WORKER_EVENT_WRITER:-master}";

#[test]
fn stream_writers_default_to_master_so_a_single_process_deploy_is_silent() {
    // `homeserver.yaml` serves two topologies: the single-process deploy and the
    // multi-process stack. Written as a literal `event_persister`, every single-process
    // start (write_forwarding: false) hits the validator's "stream_writers deviates from
    // the default but has no effect" notice — the very signal meant to catch a real
    // config/runtime mismatch. The placeholder resolves to `master` (the default) when
    // the variable is unset, so the check goes back to being meaningful.
    let writers = config_stream_writers(&read(HOMESERVER));
    for (stream, owners) in &writers {
        assert_eq!(
            owners,
            &vec![EVENT_WRITER_PLACEHOLDER.to_string()],
            "{HOMESERVER}: worker.stream_writers.{stream} must be `{EVENT_WRITER_PLACEHOLDER}` — a literal writer makes \
             the single-process deploy warn on every start, drowning out the mismatch signal it is supposed to raise"
        );
    }
    for required in ["events", "device_lists"] {
        assert!(writers.contains_key(required), "{HOMESERVER}: worker.stream_writers must declare `{required}`");
    }
}

#[test]
fn the_worker_stack_tells_every_process_the_same_writer() {
    // The writer is resolved by each process from its own environment. Master validates
    // the topology while the readers forward to the writer; if the two sides disagree the
    // declaration and the forwarding target are different instances, with no error.
    let compose = read(COMPOSE);
    let declared = config_declared_instances(&read(HOMESERVER));
    let mut assignments = 0;
    for line in compose.lines() {
        let Some(value) = line.trim().strip_prefix("WORKER_EVENT_WRITER:") else { continue };
        assignments += 1;
        let writer = value.trim();
        assert_eq!(writer, "event_persister", "{COMPOSE}: the worker stack must name `event_persister` as the writer");
        assert!(
            declared.contains(writer),
            "{COMPOSE} names `{writer}` as the writer but {HOMESERVER}: worker.instance_map does not declare it"
        );
    }
    assert!(
        assignments >= 2,
        "{COMPOSE} must set WORKER_EVENT_WRITER for the master (which validates the topology) and for the worker \
         services via the shared environment anchor; found {assignments} assignment(s)"
    );
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

/// The text of one service block in the compose override.
///
/// Blocks start at two-space indentation (`  service_name:`) and run until the next
/// two-space key or EOF. A YAML parser would be nicer, but this crate deliberately
/// avoids a YAML dependency (same reason as `config_declared_instances`).
/// The text of one service block in the compose override.
///
/// Service keys sit at **exactly two** spaces; everything belonging to a service is
/// indented deeper. Scanning for the next `"\n  "` would therefore stop at the
/// service's own first child line (4 spaces start with 2), so this walks lines and
/// only treats a two-space key as a block boundary.
fn service_block(compose: &str, service: &str) -> String {
    let mut block = String::new();
    let mut inside = false;
    let mut found = false;
    for line in compose.lines() {
        let is_service_key = line.starts_with("  ") && !line.starts_with("   ") && line.trim_end().ends_with(':');
        if is_service_key {
            inside = line.trim().trim_end_matches(':') == service;
            found |= inside;
        }
        if inside {
            block.push_str(line);
            block.push('\n');
        }
    }
    assert!(found, "{COMPOSE}: service `{service}` not found");
    block
}

#[test]
fn http_role_instances_run_the_root_binary_with_the_surface_gate_on() {
    // W-H: a role instance must serve *its* slice of the Matrix API, which needs two
    // things that were previously missing: the root binary (the only one with those
    // routes) and `surface_gate: true` (otherwise it serves the whole API and the
    // proxy split buys nothing but extra copies of every endpoint).
    let compose = read(COMPOSE);
    for service in ["client_reader", "sync_worker", "federation_reader", "media_repository"] {
        let block = service_block(&compose, service);
        // 比较时去掉空白：compose 允许 `command: [ "/app/synapse-rust" ]`，
        // 这类纯格式差异不该让守卫失败（格式化工具会加这个空格）。
        assert!(
            block.replace(' ', "").contains("command:[\"/app/synapse-rust\"]"),
            "{service} must run the root binary; `synapse_worker` has no `/_matrix/*` listener"
        );
        assert!(
            block.contains("SYNAPSE__WORKER__SURFACE_GATE: \"true\""),
            "{service} must enable the surface gate, otherwise it answers the whole API and the split is pointless"
        );
    }
}

#[test]
fn exactly_one_service_remains_the_redis_queue_consumer() {
    // The root binary does **not** run the `BackgroundJob` loop (email/push). If every
    // service switched to the root binary, those jobs would stop being consumed with
    // no error anywhere — the queue would just grow. Keep exactly one `synapse_worker`.
    let compose = read(COMPOSE);
    let worker_binary_services =
        ["client_reader", "event_persister", "sync_worker", "federation_reader", "media_repository"]
            .into_iter()
            .filter(|service| service_block(&compose, service).contains("/app/synapse_worker"))
            .collect::<Vec<_>>();
    assert_eq!(
        worker_binary_services,
        vec!["event_persister"],
        "{COMPOSE}: exactly one service must stay on `synapse_worker` (the only Redis task consumer)"
    );
}

#[test]
fn the_proxy_reference_routes_every_role_upstream_and_never_the_queue_consumer() {
    // `docker/nginx/worker-routing.conf` is the only place that documents the split.
    // Two failure modes it must not drift into: pointing an upstream at the queue
    // consumer (only `/metrics`), or leaving the role upstreams commented out so the
    // file silently degrades back to "everything to master".
    let conf = read("docker/nginx/worker-routing.conf");
    for upstream in [
        "upstream synapse_sync_worker",
        "upstream synapse_client_reader",
        "upstream synapse_federation_reader",
        "upstream synapse_media_repository",
    ] {
        assert!(conf.contains(upstream), "worker-routing.conf must define `{upstream}`");
    }
    assert!(
        !conf.contains("synapse_worker:8008"),
        "worker-routing.conf must never proxy to a `synapse_worker` container: it only serves /metrics"
    );
    assert!(
        conf.contains("location ~ ^/_matrix/client/.*/sync$"),
        "the sync location must use a multi-segment wildcard, or room-scoped sync (`/v3/rooms/{{id}}/sync`) bypasses the worker"
    );
}
