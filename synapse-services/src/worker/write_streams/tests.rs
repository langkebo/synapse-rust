use super::*;

fn config_with(write_forwarding: bool, instance: &str) -> WorkerConfig {
    let mut config =
        WorkerConfig { enabled: true, instance_name: instance.to_string(), write_forwarding, ..Default::default() };
    config.replication.http.enabled = true;
    config.replication.http.secret = Some("s".repeat(32));
    config
}

fn with_instance(mut config: WorkerConfig, name: &str, host: &str, tls: bool) -> WorkerConfig {
    config.instance_map.insert(
        name.to_string(),
        synapse_common::config::worker::InstanceLocationConfig { host: host.to_string(), port: 8008, tls },
    );
    config
}

#[test]
fn switching_forwarding_off_makes_every_stream_local() {
    // The switch is the only thing that may change behaviour: a deployment that
    // never sets it must keep writing locally even with a non-default
    // `stream_writers` block.
    let mut config = config_with(false, "client_reader");
    config.stream_writers.receipts = vec!["event_persister".to_string()];
    assert_eq!(write_route(&config, WriteStream::Receipts), WriteRoute::Local);
}

#[test]
fn a_process_that_is_not_in_worker_mode_never_forwards() {
    // `worker.enabled = false` means "monolith": there is no peer to hand the write
    // to, so a stray `write_forwarding: true` must not turn into outbound traffic.
    let mut config = config_with(true, "client_reader");
    config.enabled = false;
    config.stream_writers.receipts = vec!["event_persister".to_string()];
    let config = with_instance(config, "event_persister", "persister.internal", false);
    assert_eq!(write_route(&config, WriteStream::Receipts), WriteRoute::Local);
}

#[test]
fn the_configured_writer_becomes_the_target() {
    let mut config = config_with(true, "client_reader");
    config.stream_writers.receipts = vec!["event_persister".to_string()];
    let config = with_instance(config, "event_persister", "persister.internal", false);

    assert_eq!(
        write_route(&config, WriteStream::Receipts),
        WriteRoute::Forward {
            instance: "event_persister".to_string(),
            base_url: "http://persister.internal:8008".to_string(),
        }
    );
}

#[test]
fn the_writer_itself_writes_locally_instead_of_forwarding_to_itself() {
    let mut config = config_with(true, "event_persister");
    config.stream_writers.receipts = vec!["event_persister".to_string()];
    let config = with_instance(config, "event_persister", "persister.internal", false);
    assert_eq!(write_route(&config, WriteStream::Receipts), WriteRoute::Local);
}

#[test]
fn any_owner_of_a_multi_writer_stream_writes_locally() {
    // `receipts` is an `AtLeastOne` stream: two owners both legitimately write,
    // so the second one must not forward to the first.
    let mut config = config_with(true, "client_reader");
    config.stream_writers.receipts = vec!["event_persister".to_string(), "client_reader".to_string()];
    let config = with_instance(config, "event_persister", "persister.internal", false);
    assert_eq!(write_route(&config, WriteStream::Receipts), WriteRoute::Local);
}

#[test]
fn tls_defaults_matter_because_the_scheme_comes_from_the_instance_entry() {
    let mut config = config_with(true, "client_reader");
    config.stream_writers.to_device = vec!["event_persister".to_string()];
    let config = with_instance(config, "event_persister", "persister.internal", true);
    match write_route(&config, WriteStream::ToDevice) {
        WriteRoute::Forward { base_url, .. } => assert_eq!(base_url, "https://persister.internal:8008"),
        other => panic!("expected a forward, got {other:?}"),
    }
}

#[test]
fn a_missing_instance_map_entry_is_unavailable_rather_than_local() {
    // Falling back to local would silently restore multi-writer behaviour — the
    // exact failure mode `write_forwarding` exists to remove.
    let mut config = config_with(true, "client_reader");
    config.stream_writers.receipts = vec!["event_persister".to_string()];
    match write_route(&config, WriteStream::Receipts) {
        WriteRoute::Unavailable { instance, reason } => {
            assert_eq!(instance, "event_persister");
            assert!(reason.contains("instance_map"), "{reason}");
        }
        other => panic!("expected unavailable, got {other:?}"),
    }
}

#[test]
fn forwarding_without_a_replication_secret_is_unavailable() {
    // Without a shared secret the forwarded request cannot be told apart from a
    // client request, so the loop-break would not work.
    let mut config = config_with(true, "client_reader");
    config.stream_writers.receipts = vec!["event_persister".to_string()];
    config.replication.http.enabled = false;
    let config = with_instance(config, "event_persister", "persister.internal", false);
    match write_route(&config, WriteStream::Receipts) {
        WriteRoute::Unavailable { reason, .. } => assert!(reason.contains("replication"), "{reason}"),
        other => panic!("expected unavailable, got {other:?}"),
    }
}

#[test]
fn every_stream_declares_the_namespaces_it_is_mapped_in() {
    // The declaration is the machine-checkable half of "the mapping is exhaustive":
    // `tests/unit/worker_write_stream_tests.rs` derives the surfaces actually used by
    // the frozen map and requires each of them to be declared here.
    let events: Vec<&str> = WriteStream::Events.mapped_surfaces().iter().map(|s| s.as_str()).collect();
    assert_eq!(events, ["client", "vendor", "federation"], "events is written from all three namespaces");

    let device_lists: Vec<&str> = WriteStream::DeviceLists.mapped_surfaces().iter().map(|s| s.as_str()).collect();
    assert_eq!(
        device_lists,
        ["client"],
        "device_lists has no federation route of its own — the EDUs ride on `events`' send transaction"
    );

    for stream in WriteStream::all() {
        assert!(!stream.mapped_surfaces().is_empty(), "{} declares no namespace", stream.stream_name());
        assert!(
            stream.mapped_surfaces().contains(&WriteSurface::Client),
            "{} must at least be mapped on the client surface",
            stream.stream_name()
        );
    }
}

#[test]
fn reads_are_never_forwarded() {
    for method in ["GET", "HEAD", "OPTIONS"] {
        assert_eq!(stream_for_write(method, "/_matrix/client/v3/rooms/x/receipt/m.read/y"), None);
    }
}

#[test]
fn version_spellings_of_the_same_endpoint_agree() {
    for path in [
        "/_matrix/client/v3/rooms/!r/receipt/m.read/$e",
        "/_matrix/client/v1/rooms/!r/receipt/m.read/$e",
        "/_matrix/vendor/v1/rooms/!r/receipt/m.read/$e",
    ] {
        assert_eq!(stream_for_write("POST", path), Some(WriteStream::Receipts), "{path}");
    }
}

#[test]
fn the_six_supported_families_map_to_their_streams() {
    let cases = [
        ("PUT", "/_matrix/client/v3/sendToDevice/m.room_key/abc", WriteStream::ToDevice),
        ("PUT", "/_matrix/client/v3/presence/@a:b/status", WriteStream::Presence),
        ("POST", "/_matrix/client/v3/presence/list", WriteStream::Presence),
        ("PUT", "/_matrix/client/v3/pushrules/global/override/x", WriteStream::PushRules),
        ("DELETE", "/_matrix/client/v3/pushrules/global/override/x", WriteStream::PushRules),
        ("PUT", "/_matrix/client/v3/user/@a:b/account_data/m.direct", WriteStream::AccountData),
        ("PUT", "/_matrix/client/v3/user/@a:b/rooms/!r/account_data/m.tag", WriteStream::AccountData),
        ("PUT", "/_matrix/client/v3/user/@a:b/rooms/!r/tags/work", WriteStream::AccountData),
        ("PUT", "/_matrix/client/v3/rooms/!r/account_data/m.fully_read", WriteStream::AccountData),
        ("POST", "/_matrix/client/v3/rooms/!r/read_markers", WriteStream::Receipts),
        ("POST", "/_matrix/client/v3/rooms/!r/threads/$t/read", WriteStream::Receipts),
        ("PUT", "/_matrix/client/v3/rooms/!r/typing/@a:b", WriteStream::Typing),
    ];
    for (method, path, expected) in cases {
        assert_eq!(stream_for_write(method, path), Some(expected), "{method} {path}");
    }
}

#[test]
fn every_room_dag_entry_point_is_attributed_to_events() {
    // The client surface: message, state, redaction, membership, room creation and
    // the vendor extensions that materialise rooms or PDU-carrying state.
    let cases = [
        ("PUT", "/_matrix/client/v3/rooms/!r/send/m.room.message/t1"),
        ("PUT", "/_matrix/client/v3/rooms/!r/state/m.room.topic"),
        ("PUT", "/_matrix/client/v3/rooms/!r/state/m.room.member/@a:b"),
        ("PUT", "/_matrix/client/v3/rooms/!r/redact/$e/t1"),
        ("POST", "/_matrix/client/v3/rooms/!r/leave"),
        ("POST", "/_matrix/client/v3/rooms/!r/kick"),
        ("POST", "/_matrix/client/v3/rooms/!r/ban"),
        ("POST", "/_matrix/client/v3/createRoom"),
        ("POST", "/_matrix/client/v3/join/!r"),
        ("POST", "/_matrix/client/v3/knock/!r"),
        ("POST", "/_matrix/client/v3/rooms/!r/upgrade"),
        ("POST", "/_matrix/client/v3/admin/room/!r/redact"),
        ("POST", "/_matrix/client/unstable/org.matrix.msc4354/rooms/!r/sticky_events"),
        ("DELETE", "/_matrix/client/unstable/org.matrix.msc4354/rooms/!r/sticky_events/m.room.message"),
        ("POST", "/_matrix/vendor/v1/create_dm"),
        ("POST", "/_matrix/vendor/v1/rooms/create_private"),
        ("POST", "/_matrix/vendor/v1/friends/dm/@a:b"),
        ("POST", "/_matrix/vendor/v1/invites/claim"),
        ("PUT", "/_matrix/vendor/v1/rooms/!r/anti_screenshot"),
        ("POST", "/_matrix/vendor/v1/rooms/!r/pinned_events"),
        ("PUT", "/_matrix/vendor/v1/rooms/!r/relations/$e/m.annotation/t1"),
    ];
    for (method, path) in cases {
        assert_eq!(stream_for_write(method, path), Some(WriteStream::Events), "{method} {path}");
    }
}

#[test]
fn the_federation_write_surface_is_attributed_to_events() {
    // Half a mapping would be "client writes single-writer, federation writes
    // everywhere" — and the duplicated fan-out happens on these entry points too.
    let cases = [
        ("PUT", "/_matrix/federation/v1/send/1"),
        ("PUT", "/_matrix/federation/v1/send_join/!r/$e"),
        ("PUT", "/_matrix/federation/v2/send_join/!r/$e"),
        ("PUT", "/_matrix/federation/v1/send_leave/!r/$e"),
        ("PUT", "/_matrix/federation/v2/send_leave/!r/$e"),
        ("PUT", "/_matrix/federation/v2/invite/!r/$e"),
        ("POST", "/_matrix/federation/v1/knock/!r/@a:b"),
        ("POST", "/_matrix/federation/v1/thirdparty/invite"),
    ];
    for (method, path) in cases {
        assert_eq!(stream_for_write(method, path), Some(WriteStream::Events), "{method} {path}");
    }
    // `v1/invite` only validates and co-signs; it persists nothing, so claiming it
    // would cost a needless hop.
    assert_eq!(stream_for_write("PUT", "/_matrix/federation/v1/invite/!r/$e"), None);
    assert_eq!(stream_for_write("POST", "/_matrix/federation/v1/get_missing_events/!r"), None);
}

#[test]
fn the_device_management_surface_is_attributed_to_device_lists() {
    let cases = [
        ("PUT", "/_matrix/client/v3/devices/$d"),
        ("DELETE", "/_matrix/client/v3/devices/$d"),
        ("POST", "/_matrix/client/v3/delete_devices"),
        ("POST", "/_matrix/client/v3/logout/all"),
        ("POST", "/_matrix/client/v3/account/deactivate"),
        ("POST", "/_matrix/client/v3/keys/upload"),
        ("POST", "/_matrix/client/v3/keys/upload/$d"),
        ("POST", "/_matrix/client/v3/keys/device_signing/upload"),
    ];
    for (method, path) in cases {
        assert_eq!(stream_for_write(method, path), Some(WriteStream::DeviceLists), "{method} {path}");
    }
    // `logout` (one device) only revokes tokens here — the device row survives, so
    // it records no device-list change. `keys/query` and `keys/claim` are reads.
    for (method, path) in [
        ("POST", "/_matrix/client/v3/logout"),
        ("POST", "/_matrix/client/v3/keys/query"),
        ("POST", "/_matrix/client/v3/keys/claim"),
        ("POST", "/_matrix/client/v3/keys/device_list/update"),
    ] {
        assert_eq!(stream_for_write(method, path), None, "{method} {path}");
    }
}

#[test]
fn writes_that_touch_no_stream_stay_unmapped() {
    // These are real writes, and mapping them would cost a pointless hop without
    // buying a single-writer guarantee: they touch this server's own tables
    // (threads, spaces, friends, delayed-event queue) or another stream entirely.
    for (method, path) in [
        ("POST", "/_matrix/client/v3/rooms/!r/forget"),
        ("POST", "/_matrix/vendor/v1/rooms/!r/threads"),
        ("POST", "/_matrix/vendor/v1/spaces/!s/join"),
        ("POST", "/_matrix/vendor/v1/rooms/!r/summary"),
        ("POST", "/_matrix/vendor/v1/rooms/!r/convert/$e"),
        ("PUT", "/_matrix/client/unstable/org.matrix.msc4140/rooms/!r/delayed_event/m.room.message/t1"),
        ("POST", "/_matrix/client/v3/keys/signatures/upload"),
    ] {
        assert_eq!(stream_for_write(method, path), None, "{method} {path}");
    }
}

#[test]
fn namespaces_outside_the_ledger_are_never_forwarded() {
    // Admin and media are outside the route ledger this mapping is frozen against;
    // claiming them would be a guarantee nothing checks.
    for path in [
        "/_synapse/admin/v1/rooms/x/redact/y",
        "/_matrix/media/v3/upload",
        "/_matrix/app/v1/transactions/1",
        "/_matrix/key/v2/query",
    ] {
        assert_eq!(stream_for_write("PUT", path), None, "{path}");
    }
}

#[test]
fn unstable_namespaces_are_stripped_before_matching() {
    assert_eq!(
        stream_for_write("PUT", "/_matrix/client/unstable/org.matrix.msc2732/sendToDevice/m.x/t"),
        Some(WriteStream::ToDevice)
    );
}

#[test]
fn coupled_streams_must_name_the_same_writer() {
    // One request writes both: the federation transaction's PDUs land in `events`,
    // and an `m.signing_key_update` EDU in the same transaction writes
    // `device_lists`. Different writers would leave `device_lists` with two of them.
    let mut config = config_with(true, "client_reader");
    config.stream_writers.events = vec!["event_persister".to_string()];
    config.stream_writers.device_lists = vec!["master".to_string()];
    let conflicts = coupled_stream_conflicts(&config);
    assert_eq!(conflicts.len(), 1, "{conflicts:?}");
    assert!(conflicts[0].contains("device_lists"), "{}", conflicts[0]);

    config.stream_writers.device_lists = vec!["event_persister".to_string()];
    assert!(coupled_stream_conflicts(&config).is_empty());
}

#[test]
fn coupled_streams_are_not_policed_while_forwarding_is_off() {
    // The whole block is inert without the switch, and the pre-existing warning
    // already says so — adding an error here would reject configurations that were
    // documented as harmless.
    let mut config = config_with(false, "client_reader");
    config.stream_writers.events = vec!["event_persister".to_string()];
    config.stream_writers.device_lists = vec!["master".to_string()];
    assert!(coupled_stream_conflicts(&config).is_empty());
}

#[test]
fn the_delayed_event_dispatcher_runs_only_on_the_events_writer() {
    // It injects events without an HTTP request, so no route mapping reaches it.
    let mut config = config_with(false, "client_reader");
    config.stream_writers.events = vec!["event_persister".to_string()];
    assert!(owns_events_dispatch(&config), "forwarding off ⇒ every process dispatches, as before");

    let mut config = with_instance(config, "event_persister", "persister.internal", false);
    config.write_forwarding = true;
    assert!(!owns_events_dispatch(&config), "a reader must not dispatch");

    config.instance_name = "event_persister".to_string();
    assert!(owns_events_dispatch(&config), "the configured writer dispatches");
}

#[test]
fn forwarding_a_notify_bound_stream_without_redis_is_reported() {
    // `events` reaches `/sync` through `EventNotifier`, whose only cross-instance half
    // is the Redis publish. Forwarding it with Redis off leaves every long-poll on a
    // reader waiting for its own timeout — a latency defect no correctness test sees,
    // which is why it is surfaced at startup.
    let mut config = with_instance(config_with(true, "client_reader"), "event_persister", "persister.internal", false);
    config.stream_writers.events = vec!["event_persister".to_string()];
    config.stream_writers.device_lists = vec!["event_persister".to_string()];

    let messages = missing_notify_fan_out(&config, false);
    assert_eq!(messages.len(), 1, "{messages:?}");
    assert!(messages[0].contains("events"), "{}", messages[0]);
    assert!(messages[0].contains("redis.enabled"), "the remedy must be named: {}", messages[0]);

    // Negative control: the same configuration with Redis on is fine.
    assert!(missing_notify_fan_out(&config, true).is_empty());
}

#[test]
fn a_notify_bound_stream_written_locally_never_needs_redis() {
    // This instance is the writer: it does both halves in-process.
    let mut config = config_with(true, "event_persister");
    config.stream_writers.events = vec!["event_persister".to_string()];
    assert!(missing_notify_fan_out(&config, false).is_empty());
}

#[test]
fn only_the_notify_bound_streams_are_reported() {
    // `device_lists` has a `/sync` section but no `notify_user` call on change, and the
    // six light streams are pulled from the database; forwarding either without Redis
    // costs nothing extra, so reporting them would be noise.
    let mut config = with_instance(config_with(true, "client_reader"), "event_persister", "persister.internal", false);
    config.stream_writers.device_lists = vec!["event_persister".to_string()];
    config.stream_writers.receipts = vec!["event_persister".to_string()];
    config.stream_writers.typing = vec!["event_persister".to_string()];
    assert!(missing_notify_fan_out(&config, false).is_empty(), "only events/to_device are notify-bound");

    assert!(WriteStream::Events.notify_bound());
    assert!(WriteStream::ToDevice.notify_bound());
    for stream in [
        WriteStream::Typing,
        WriteStream::AccountData,
        WriteStream::Receipts,
        WriteStream::Presence,
        WriteStream::PushRules,
        WriteStream::DeviceLists,
    ] {
        assert!(!stream.notify_bound(), "{} must not be treated as notify-bound", stream.stream_name());
    }
}

#[test]
fn to_device_is_reported_too() {
    let mut config = with_instance(config_with(true, "client_reader"), "event_persister", "persister.internal", false);
    config.stream_writers.to_device = vec!["event_persister".to_string()];
    let messages = missing_notify_fan_out(&config, false);
    assert_eq!(messages.len(), 1, "{messages:?}");
    assert!(messages[0].contains("to_device"), "{}", messages[0]);
}

#[test]
fn an_unusable_writer_is_not_double_reported_as_a_redis_problem() {
    // When the target cannot be resolved the write fails outright, so the notifier gap
    // is moot — that case already has its own error and must not be reported twice.
    let mut config = config_with(true, "client_reader");
    config.stream_writers.events = vec!["event_persister".to_string()];
    config.stream_writers.device_lists = vec!["event_persister".to_string()];
    assert!(missing_notify_fan_out(&config, false).is_empty(), "no instance_map entry ⇒ Unavailable, not Forward");
}
