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
use synapse_services::worker::write_streams::{stream_for_write, WriteStream, WriteSurface};
use synapse_web::routes::declared_ledger_all;

const MAP_FILE: &str = "scripts/contract/write_stream_map.txt";

fn is_write_method(method: &str) -> bool {
    matches!(method, "POST" | "PUT" | "DELETE" | "PATCH")
}

/// The namespaces the map is frozen against.
///
/// `/_synapse/admin/…` is here as of W-I stage 5, and it costs nothing to keep: the
/// admin writes are **in** `declared_ledger_all()` (the same derived table the
/// route-contract gate validates), they were simply filtered out before. Once the
/// namespace was filtered out, the 16 admin routes that really write a managed stream
/// (membership, power levels, backfill, server notices, device revocation) were free to
/// regress into multiple writers with no gate noticing.
fn in_mapped_namespace(path: &str) -> bool {
    path.starts_with("/_matrix/client/")
        || path.starts_with("/_matrix/vendor/v1/")
        || path.starts_with("/_matrix/federation/")
        || path.starts_with("/_synapse/admin/")
}

/// Which namespace a path belongs to, mirroring `mapped_path`'s prefix rules.
fn surface_of(path: &str) -> WriteSurface {
    if path.starts_with("/_matrix/federation/") {
        WriteSurface::Federation
    } else if path.starts_with("/_matrix/vendor/v1/") {
        WriteSurface::Vendor
    } else if path.starts_with("/_synapse/admin/") {
        WriteSurface::Admin
    } else {
        WriteSurface::Client
    }
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

/// Every client, vendor and federation write route the server actually registers.
fn ledger_writes() -> BTreeSet<(String, String)> {
    let ledger = declared_ledger_all();
    let writes: BTreeSet<(String, String)> = ledger
        .iter()
        .filter(|entry| is_write_method(entry.method.as_str()))
        .filter(|entry| in_mapped_namespace(entry.path))
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
    // The admin namespace has feature-gated modules of its own. These rows are in the map
    // because the strict `all-extensions` lane does register them; the lean unit lane
    // does not, and deleting them here would hide them from the lane that matters.
    let saml_admin = !cfg!(feature = "saml-sso") && path.starts_with("/_synapse/admin/v1/saml/");
    let cas_admin = !cfg!(feature = "cas-sso") && path.starts_with("/_synapse/admin/v1/cas/");
    let notifications = !cfg!(feature = "server-notifications")
        && (path.starts_with("/_synapse/admin/v1/notifications")
            || path.starts_with("/_synapse/admin/v1/server_notices")
            || path == "/_synapse/admin/v1/send_server_notice"
            || (path.starts_with("/_synapse/admin/v1/users/")
                && (path.ends_with("/notification") || path.contains("/pushers/"))));
    saml || oidc || voice || calls || saml_admin || cas_admin || notifications
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
fn every_mapped_stream_declares_the_namespace_its_rows_come_from() {
    // The complementary half of "the mapping is exhaustive": each stream publishes the
    // namespaces it claims (`mapped_surfaces`), and every row attributed to it must come
    // from one of them. Adding a federation row to a client-only stream — or a stream to
    // a new namespace without saying so — fails here instead of quietly widening the
    // claim the design document makes.
    let mut violations = Vec::new();
    for row in load_map() {
        let Some(declared) = row.stream.as_deref() else {
            continue;
        };
        let surface = surface_of(&row.path);
        let Some(stream) = WriteStream::all().iter().copied().find(|stream| stream.stream_name() == declared) else {
            violations.push(format!("{} {} names unknown stream `{declared}`", row.method, row.path));
            continue;
        };
        if !stream.mapped_surfaces().contains(&surface) {
            violations.push(format!(
                "{} {} is a {} write, but `{declared}` only declares {:?}",
                row.method,
                row.path,
                surface.as_str(),
                stream.mapped_surfaces().iter().map(|s| s.as_str()).collect::<Vec<_>>()
            ));
        }
    }
    assert!(violations.is_empty(), "the map claims a namespace its stream does not:\n  {}", violations.join("\n  "));
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
fn pointing_events_at_a_declared_writer_is_accepted() {
    // `events` and `device_lists` used to be refused outright; they are now forwarded
    // like any other stream, so the configuration they exist for must start. The other
    // six streams stay on `master`, which is declared here so their routes resolve.
    let mut config = forwarding_config("client_reader");
    declare_instance(&mut config, "master");
    declare_instance(&mut config, "event_persister");
    config.stream_writers.events = vec!["event_persister".to_string()];
    config.stream_writers.device_lists = vec!["event_persister".to_string()];

    let validation = validate_worker_config(&config);
    assert!(validation.valid, "{:?}", validation.errors);
}

#[test]
fn pointing_events_at_an_undeclared_instance_is_a_hard_error() {
    let mut config = forwarding_config("client_reader");
    config.stream_writers.events = vec!["event_persister".to_string()];
    config.stream_writers.device_lists = vec!["event_persister".to_string()];

    let validation = validate_worker_config(&config);
    assert!(!validation.valid, "an unresolved writer must not pass validation");
    let joined = validation.errors.join("\n");
    assert!(joined.contains("events") && joined.contains("instance_map"), "{joined}");
}

#[test]
fn splitting_events_from_device_lists_is_a_hard_error() {
    // One request (`PUT /_matrix/federation/v1/send/{txn}`) writes both streams, so they
    // must resolve to the same instance; otherwise `device_lists` silently gets two
    // writers and the `m.device_list_update` broadcast is duplicated — the exact failure
    // this configuration exists to prevent.
    let mut config = forwarding_config("client_reader");
    declare_instance(&mut config, "master");
    declare_instance(&mut config, "event_persister");
    declare_instance(&mut config, "device_writer");
    config.stream_writers.events = vec!["event_persister".to_string()];
    config.stream_writers.device_lists = vec!["device_writer".to_string()];

    let validation = validate_worker_config(&config);
    assert!(!validation.valid, "coupled streams must not name different writers");
    let joined = validation.errors.join("\n");
    assert!(joined.contains("device_lists"), "{joined}");
    assert!(joined.contains("send"), "the error must say which request couples them: {joined}");
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

// ── the admin surface ─────────────────────────────────────────────────────────
//
// The admin namespace is classified explicitly (`WriteSurface::Admin`), and both halves
// of that classification are asserted here: the routes that reach a real writer must be
// mapped, and the ones that merely look like they should be must stay `-`. The second
// half is the one that keeps the map honest — "it has `rooms/{id}` in the path" is not
// evidence of a DAG write.

#[test]
fn admin_routes_that_reach_a_stream_are_mapped() {
    let cases: [(&str, &str, &str); 16] = [
        // Synthesised `m.room.member` + `sign_and_broadcast_event`.
        ("POST", "/_synapse/admin/v1/rooms/!r:example.org/ban", "events"),
        ("POST", "/_synapse/admin/v1/rooms/!r:example.org/ban/@u:example.org", "events"),
        ("POST", "/_synapse/admin/v1/rooms/!r:example.org/kick", "events"),
        ("POST", "/_synapse/admin/v1/rooms/!r:example.org/kick/@u:example.org", "events"),
        ("POST", "/_synapse/admin/v1/rooms/!r:example.org/unban/@u:example.org", "events"),
        // `join_room` / `leave_room`.
        ("PUT", "/_synapse/admin/v1/rooms/!r:example.org/members/@u:example.org", "events"),
        ("DELETE", "/_synapse/admin/v1/rooms/!r:example.org/members/@u:example.org", "events"),
        // `upsert_power_levels_event`.
        ("POST", "/_synapse/admin/v1/rooms/!r:example.org/make_admin", "events"),
        ("PUT", "/_synapse/admin/v1/rooms/!r:example.org/make_admin", "events"),
        // `create_event_with_graph` per fetched PDU.
        ("POST", "/_synapse/admin/v1/rooms/!r:example.org/backfill", "events"),
        // `create_room` + `add_member` + `send_message`.
        ("POST", "/_synapse/admin/v1/send_server_notice", "events"),
        ("POST", "/_synapse/admin/v1/join/!r:example.org", "events"),
        // `revoke_device` (two spellings of one handler) and `logout_all`.
        ("DELETE", "/_synapse/admin/v1/users/@u:example.org/devices/DEV", "device_lists"),
        ("POST", "/_synapse/admin/v1/users/@u:example.org/devices/DEV/delete", "device_lists"),
        ("POST", "/_synapse/admin/v1/users/@u:example.org/logout", "device_lists"),
        ("POST", "/_synapse/admin/v1/user_sessions/@u:example.org/invalidate", "device_lists"),
    ];
    for (method, path, expected) in cases {
        assert_eq!(stream_for_write(method, path).map(WriteStream::stream_name), Some(expected), "{method} {path}");
    }
}

#[test]
fn admin_routes_that_only_touch_server_tables_stay_unmapped() {
    // Each of these was checked against its handler, not inferred from the path:
    //
    // * `shutdown_room` runs `UPDATE rooms SET is_public = false, name = …` plus a
    //   member-row purge — no event, no federation (`synapse-storage/src/room/mod.rs:1023`).
    // * `evict` calls the *storage* `remove_member` per room rather than `leave_room`, so
    //   no `m.room.member` is emitted (`synapse-services/src/admin_user_service.rs:321`).
    // * `listings/public` writes `rooms.is_public` + `room_directory`
    //   (`synapse-storage/src/room/admin.rs:417`) — no `m.room.join_rules`.
    // * `cascade_redact` only back-fills `events.redacted_by`; it does not persist an
    //   `m.room.redaction` PDU.
    for (method, path) in [
        ("POST", "/_synapse/admin/v1/shutdown_room"),
        ("POST", "/_synapse/admin/v1/rooms/!r:example.org/shutdown_room"),
        ("POST", "/_synapse/admin/v1/users/@u:example.org/evict"),
        ("PUT", "/_synapse/admin/v1/rooms/!r:example.org/listings/public"),
        ("DELETE", "/_synapse/admin/v1/rooms/!r:example.org/listings/public"),
        ("POST", "/_synapse/admin/v1/rooms/!r:example.org/cascade_redact"),
        ("DELETE", "/_synapse/admin/v1/rooms/!r:example.org"),
        ("POST", "/_synapse/admin/v1/purge_room"),
        ("POST", "/_synapse/admin/v1/rooms/!r:example.org/block"),
    ] {
        assert_eq!(stream_for_write(method, path), None, "{method} {path} must not be attributed to a stream");
    }
}

#[test]
fn the_admin_section_of_the_map_is_exactly_the_writers() {
    // Counts are frozen on purpose: the file is meant to grow one deliberate row at a
    // time, and a bulk re-generation that flips a column shows up here rather than in a
    // production incident.
    let rows: Vec<Row> = load_map().into_iter().filter(|row| row.path.starts_with("/_synapse/admin/")).collect();
    assert_eq!(rows.len(), 156, "the admin write surface changed; re-decide every new row");
    let attributed: Vec<&Row> = rows.iter().filter(|row| row.stream.is_some()).collect();
    assert_eq!(attributed.len(), 16, "admin rows attributed to a stream: {attributed:?}");
    assert_eq!(attributed.iter().filter(|row| row.stream.as_deref() == Some("events")).count(), 12);
    assert_eq!(attributed.iter().filter(|row| row.stream.as_deref() == Some("device_lists")).count(), 4);
}
