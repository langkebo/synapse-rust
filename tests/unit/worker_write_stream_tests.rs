//! W-I guards: the `stream_writers` write map must match the **real** route table.
//!
//! `stream_for_write()` decides whether a write is handed to another instance. Its
//! failure mode is silent in both directions:
//!
//! * a route that *should* belong to a stream is not matched ⇒ that stream quietly goes
//!   back to having several writers, and the duplicate post-commit fan-out (federation
//!   broadcast, appservice dispatch) is back with no runtime symptom;
//! * a route is matched that does not belong ⇒ an extra network hop for no reason.
//!
//! Neither shows up in a test that only exercises the matcher, because the matcher is
//! what is wrong. So the truth source here is [`declared_ledger_all`] — the same derived
//! ledger the route-contract gate validates — frozen into
//! `scripts/contract/write_stream_map.txt` with the classification spelled out per route.
//!
//! The ratchet is **bidirectional**:
//!
//! 1. the file's `(method, path)` set must equal the ledger's client-surface write set, so
//!    adding a write route forces a decision instead of slipping through;
//! 2. the file's `stream` column must equal what `stream_for_write()` returns, so changing
//!    the matcher without changing the file fails too.

use std::collections::BTreeSet;

use synapse_services::worker::runtime_topology::stream_writer_sets;
use synapse_services::worker::topology_validator::validate_worker_config;
use synapse_services::worker::write_streams::{stream_for_write, WriteStream};
use synapse_web::routes::declared_ledger_all;

const MAP_FILE: &str = "scripts/contract/write_stream_map.txt";

fn is_write_method(method: &str) -> bool {
    matches!(method, "POST" | "PUT" | "DELETE" | "PATCH")
}

/// One row of the frozen map. `stream: None` is the `-` placeholder.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Row {
    method: String,
    path: String,
    stream: Option<String>,
}

fn load_map() -> Vec<Row> {
    let text = std::fs::read_to_string(MAP_FILE)
        .unwrap_or_else(|error| panic!("{MAP_FILE} must exist (it freezes the write map): {error}"));
    let mut rows = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let columns: Vec<&str> = line.split('\t').collect();
        assert_eq!(columns.len(), 3, "{MAP_FILE}:{} must be `METHOD<TAB>path<TAB>stream`, got {line:?}", index + 1);
        let (method, path, stream) = (columns[0], columns[1], columns[2]);
        assert!(is_write_method(method), "{MAP_FILE}:{}: `{method}` is not a write method", index + 1);
        assert!(path.starts_with('/'), "{MAP_FILE}:{}: path must be absolute ({path})", index + 1);
        rows.push(Row {
            method: method.to_string(),
            path: path.to_string(),
            stream: if stream == "-" { None } else { Some(stream.to_string()) },
        });
    }
    assert!(rows.len() > 100, "the frozen map must not look truncated ({} rows)", rows.len());
    rows
}

/// Every client-surface write route the server actually registers.
fn ledger_writes() -> BTreeSet<(String, String)> {
    let ledger = declared_ledger_all();
    let writes: BTreeSet<(String, String)> = ledger
        .iter()
        .filter(|entry| is_write_method(entry.method.as_str()))
        .filter(|entry| entry.path.starts_with("/_matrix/client/") || entry.path.starts_with("/_matrix/vendor/v1/"))
        .map(|entry| (entry.method.as_str().to_string(), entry.path.to_string()))
        .collect();
    assert!(writes.len() > 100, "the client write surface must not look empty ({})", writes.len());
    writes
}

/// Whether this route's registration depends on a cargo feature that is off in this
/// build.
///
/// The map is generated from the `all-extensions` route table, but the unit lane compiles
/// with a leaner feature set, so a gated route is legitimately absent from the ledger
/// here. Without this filter the "stale row" half of the ratchet would fail on every lean
/// build — and the tempting fix (delete those rows) would then hide them from the strict
/// `all-extensions` lane, which is where they matter.
///
/// Each family is listed with the feature that gates it, so turning a feature **on** makes
/// the row mandatory again in the same build.
fn absent_because_feature_is_off(path: &str) -> bool {
    let saml = !cfg!(feature = "saml-sso")
        && (path.starts_with("/_matrix/client/v3/login/saml")
            || path.starts_with("/_matrix/client/v3/login/sso/redirect/saml"));
    let oidc = !cfg!(feature = "builtin-oidc") && path.starts_with("/_matrix/client/v3/oidc/");
    let voice = !cfg!(feature = "voice-extended") && path.starts_with("/_matrix/vendor/v1/voice/");
    let calls = !cfg!(feature = "voip-tracking") && path.contains("/send/m.call.");
    saml || oidc || voice || calls
}

#[test]
fn the_frozen_map_covers_exactly_the_real_client_write_surface() {
    let map: BTreeSet<(String, String)> = load_map().into_iter().map(|row| (row.method, row.path)).collect();
    let ledger = ledger_writes();

    let missing: Vec<_> = ledger.difference(&map).collect();
    let stale: Vec<_> = load_map()
        .into_iter()
        .map(|row| (row.method, row.path))
        .filter(|(_, path)| !absent_because_feature_is_off(path))
        .filter(|key| !ledger.contains(key))
        .collect();

    assert!(
        missing.is_empty(),
        "these client write routes are NOT in {MAP_FILE} ⇒ decide which stream they write \
         (or record `-` if none) and add the row:\n  {}",
        missing.iter().map(|(m, p)| format!("{m}\t{p}")).collect::<Vec<_>>().join("\n  ")
    );
    assert!(
        stale.is_empty(),
        "these rows in {MAP_FILE} no longer correspond to a registered route ⇒ remove them:\n  {}",
        stale.iter().map(|(m, p)| format!("{m}\t{p}")).collect::<Vec<_>>().join("\n  ")
    );
}

#[test]
fn the_frozen_map_agrees_with_the_matcher() {
    let mut mismatches = Vec::new();
    for row in load_map() {
        let actual = stream_for_write(&row.method, &row.path).map(WriteStream::stream_name);
        let declared = row.stream.as_deref();
        if actual != declared {
            mismatches.push(format!(
                "{} {} — map says {:?}, `stream_for_write` says {:?}",
                row.method, row.path, declared, actual
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "the map and the matcher disagree; fix whichever is wrong and keep both in step:\n  {}",
        mismatches.join("\n  ")
    );
}

#[test]
fn no_route_is_mapped_to_a_stream_that_cannot_be_forwarded() {
    // `events` and `device_lists` have no exhaustive route mapping yet. Returning them
    // would claim single-writer coverage the build does not provide, and the middleware
    // would then forward a stream it cannot complete.
    let unforwardable: BTreeSet<&str> = WriteStream::all()
        .iter()
        .filter(|stream| !stream.is_forwardable())
        .map(|stream| stream.stream_name())
        .collect();
    for row in load_map() {
        if let Some(stream) = row.stream {
            assert!(
                !unforwardable.contains(stream.as_str()),
                "{} {} is mapped to `{stream}`, which this build cannot forward",
                row.method,
                row.path
            );
        }
    }
}

#[test]
fn every_stream_field_is_represented_in_the_mapping_table() {
    // `StreamWriters` has eight fields; `WriteStream` has eight variants and
    // `stream_writer_sets()` has eight entries. Three hand-maintained lists of the same
    // thing drift — assert they are the same set, so adding a field cannot be half-wired.
    let from_config: BTreeSet<&str> = stream_writer_sets(&Default::default()).iter().map(|(name, _)| *name).collect();
    let from_enum: BTreeSet<&str> = WriteStream::all().iter().map(|stream| stream.stream_name()).collect();
    assert_eq!(
        from_config, from_enum,
        "`StreamWriters` fields and `WriteStream` variants disagree — a new stream is only half-wired"
    );
    assert_eq!(from_enum.len(), 8);
}

#[test]
fn forwarding_is_off_unless_it_is_switched_on() {
    // The whole feature is opt-in: a deployment that never touches
    // `worker.write_forwarding` must keep the pre-existing "whoever receives it writes"
    // behaviour, and `stream_writers` must stay a declaration there.
    let config = synapse_common::config::WorkerConfig::default();
    assert!(!config.write_forwarding);
    assert!(config.stream_writers.events == vec!["master".to_string()]);
}

// ── startup verdict ────────────────────────────────────────────────────────────
//
// These are the tests that encode the reason this feature exists: a `stream_writers`
// block that cannot be honoured must **fail**, not be logged and ignored.

fn forwarding_config(instance: &str) -> synapse_common::config::WorkerConfig {
    let mut config = synapse_common::config::WorkerConfig {
        enabled: true,
        instance_name: instance.to_string(),
        write_forwarding: true,
        ..Default::default()
    };
    config.replication.enabled = true;
    config.replication.http.enabled = true;
    config.replication.http.secret = Some("k".repeat(32));
    config
}

fn declare_instance(config: &mut synapse_common::config::WorkerConfig, name: &str) {
    config.instance_map.insert(
        name.to_string(),
        synapse_common::config::InstanceLocationConfig { host: name.to_string(), port: 8008, tls: false },
    );
}

#[test]
fn pointing_events_at_another_instance_is_a_hard_error() {
    // `events` cannot be forwarded yet (federation writes and the delayed-event
    // dispatcher bypass the route layer). Accepting the configuration silently is the
    // exact failure mode this change removes.
    let mut config = forwarding_config("client_reader");
    declare_instance(&mut config, "event_persister");
    config.stream_writers.events = vec!["event_persister".to_string()];

    let validation = validate_worker_config(&config);
    assert!(!validation.valid, "a non-default `events` writer must not pass validation");
    let joined = validation.errors.join("\n");
    assert!(joined.contains("events"), "{joined}");
    assert!(joined.contains("不支持"), "the error must say the stream is unsupported: {joined}");
}

#[test]
fn a_forwardable_stream_without_a_declared_target_is_an_error() {
    let mut config = forwarding_config("client_reader");
    config.stream_writers.receipts = vec!["event_persister".to_string()];

    let validation = validate_worker_config(&config);
    assert!(!validation.valid);
    let joined = validation.errors.join("\n");
    assert!(joined.contains("receipts") && joined.contains("instance_map"), "{joined}");
}

#[test]
fn with_forwarding_off_a_non_default_block_is_still_only_a_warning() {
    // Backwards compatibility: existing deployments carry configurations that were
    // documented as harmless declarations, and they must keep starting.
    let mut config = forwarding_config("client_reader");
    config.write_forwarding = false;
    declare_instance(&mut config, "event_persister");
    config.stream_writers.events = vec!["event_persister".to_string()];

    let validation = validate_worker_config(&config);
    assert!(validation.errors.is_empty(), "{:?}", validation.errors);
    let joined = validation.warnings.join("\n");
    assert!(joined.contains("write_forwarding"), "the warning must point at the switch: {joined}");
}

#[test]
fn forwarding_in_a_monolith_is_refused_even_though_workers_are_off() {
    // `worker.enabled: false` short-circuits most validation; this one must survive it,
    // otherwise "forwarding on, workers off" would start and then write locally.
    let mut config = forwarding_config("master");
    config.enabled = false;

    let validation = validate_worker_config(&config);
    assert!(!validation.valid, "the disabled branch must carry the error, not just warnings");
    assert!(validation.errors.iter().any(|message| message.contains("worker.enabled")), "{:?}", validation.errors);
}
