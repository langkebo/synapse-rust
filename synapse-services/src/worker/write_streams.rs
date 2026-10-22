//! `worker.stream_writers` made executable (W-I).
//!
//! `StreamWriters` used to be a declaration with no consumer: `events: ["event_persister"]`
//! was parsed, validated, reported by `/_synapse/worker/v1/topology/validate` — and
//! changed nothing, because every instance wrote every stream itself. This module
//! answers the question that declaration was always trying to state:
//!
//! > for *this* write, on *this* instance, do I execute it or hand it to the
//! > configured writer?
//!
//! The answer is a pure function of the configuration ([`write_route`]), so the
//! routing table can be tested without a cluster, and the HTTP layer
//! (`synapse_web::middleware::write_forwarding`) only has to *obey* it.
//!
//! # Why single-writer, if not for stream ordering
//!
//! Upstream Synapse needs one writer per stream because stream positions are
//! allocated in-process. **This repository does not**: `events.stream_ordering`
//! defaults to `nextval('events_stream_ordering_seq')`, so concurrent writers still
//! get globally increasing positions. What a second writer *does* duplicate is the
//! post-commit fan-out (`run_post_commit_fan_out`: federation broadcast, appservice
//! dispatch, room-summary refresh). That — not ordering — is what these routes
//! protect. See `docs/audit/WORKER_STREAM_WRITE_FORWARDING_DESIGN_2026-10-22.md` §2.
//!
//! # Why only six of the eight streams
//!
//! [`WriteStream::is_forwardable`] is deliberately narrower than the config surface.
//! A stream may only be forwarded when its **route mapping is exhaustive**; `events`
//! also has a federation write surface (`/_matrix/federation/v1/send/...`) and a
//! background delayed-event dispatcher that bypasses HTTP, and `device_lists` is a
//! *side effect* of the whole device-management surface. Forwarding half of either
//! would produce a mixed state that looks enforced and is not, so the validator
//! refuses those configurations outright rather than letting them be ignored.

use synapse_common::config::worker::WorkerConfig;

use crate::worker::topology_validator::resolved_current_instance_name;

/// One of the eight streams `worker.stream_writers` can name.
///
/// The names must stay in step with [`synapse_common::config::worker::StreamWriters`]
/// field names; `stream_writer_sets()` is the other half of that mapping and a guard
/// test compares the two so a new field cannot be half-wired.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WriteStream {
    /// The room DAG (`events`).
    Events,
    /// Ephemeral typing state (`typing`).
    Typing,
    /// Per-device message queues (`to_device`).
    ToDevice,
    /// Per-user and per-room account data, including tags (`account_data`).
    AccountData,
    /// Read receipts and read markers (`receipts`).
    Receipts,
    /// Presence state (`presence`).
    Presence,
    /// Push rules (`push_rules`).
    PushRules,
    /// Outbound device-list changes (`device_lists`).
    DeviceLists,
}

impl WriteStream {
    /// The `worker.stream_writers` key this stream corresponds to.
    pub fn stream_name(self) -> &'static str {
        match self {
            Self::Events => "events",
            Self::Typing => "typing",
            Self::ToDevice => "to_device",
            Self::AccountData => "account_data",
            Self::Receipts => "receipts",
            Self::Presence => "presence",
            Self::PushRules => "push_rules",
            Self::DeviceLists => "device_lists",
        }
    }

    /// Every stream, in the order they appear in `StreamWriters`.
    pub fn all() -> &'static [WriteStream] {
        &[
            Self::Events,
            Self::Typing,
            Self::ToDevice,
            Self::AccountData,
            Self::Receipts,
            Self::Presence,
            Self::PushRules,
            Self::DeviceLists,
        ]
    }

    /// May this stream's writes be forwarded in this build?
    ///
    /// `false` means the configuration surface exists but is refused by
    /// [`crate::worker::topology_validator::validate_worker_config`] — see the module
    /// documentation for why a half-covered stream is worse than an unsupported one.
    pub fn is_forwardable(self) -> bool {
        !matches!(self, Self::Events | Self::DeviceLists)
    }

    /// The configured owner instances of this stream.
    pub fn owners(self, config: &WorkerConfig) -> &[String] {
        let writers = &config.stream_writers;
        match self {
            Self::Events => &writers.events,
            Self::Typing => &writers.typing,
            Self::ToDevice => &writers.to_device,
            Self::AccountData => &writers.account_data,
            Self::Receipts => &writers.receipts,
            Self::Presence => &writers.presence,
            Self::PushRules => &writers.push_rules,
            Self::DeviceLists => &writers.device_lists,
        }
        .as_slice()
    }

    /// Why this stream cannot be forwarded yet — quoted in the validation error so an
    /// operator does not have to go looking.
    pub fn forwardable_rationale(self) -> &'static str {
        match self {
            Self::Events => {
                "events 的写入还有联邦面（/_matrix/federation/v1/send 等）与 delayed-event 后台派发，\
                 只转客户端那一半会得到\"客户端单写者 + 联邦多写者\"的混合态 —— 而重复的联邦广播正是要防的东西"
            }
            Self::DeviceLists => {
                "device_lists 不是端点而是一批设备写操作的副作用（keys/upload、devices/*、logout*、\
                 delete_devices），把设备管理面整体搬到 writer 上属独立改动"
            }
            _ => "",
        }
    }
}

/// Where a write for one stream must be executed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteRoute {
    /// Execute in this process.
    Local,
    /// Hand the request to `instance` at `base_url` (no trailing slash).
    Forward {
        /// The configured writer instance name.
        instance: String,
        /// `scheme://host:port` from `worker.instance_map`.
        base_url: String,
    },
    /// The configuration says another instance owns the stream, but it cannot be
    /// reached or authenticated against. Never silently fall back to writing locally:
    /// that would restore the multi-writer state the configuration exists to remove.
    Unavailable {
        /// The instance the configuration names.
        instance: String,
        /// Human-readable cause, also surfaced by the topology endpoint.
        reason: String,
    },
}

/// Header naming the stream a forwarded write belongs to.
///
/// Only honoured together with a valid [`WRITE_FORWARD_SECRET_HEADER`]: the pair means
/// "this request was already routed here by another instance, execute it locally".
pub const WRITE_FORWARD_STREAM_HEADER: &str = "x-synapse-write-stream";

/// Shared-secret header, the same one `/_synapse/worker/v1/*` authenticates with.
pub const WRITE_FORWARD_SECRET_HEADER: &str = "x-synapse-worker-secret";

/// Why a forward target is unusable, or `None` when it is fine.
///
/// Kept separate from [`write_route`] so the validator and the request path agree on
/// what "usable" means without duplicating the checks.
fn unusable_reason(config: &WorkerConfig, instance: &str) -> Option<String> {
    if !config.replication.http.enabled {
        return Some(
            "worker.replication.http.enabled 为 false：转发需要共享密钥来防止回路，拿不到密钥就不能转发".to_string(),
        );
    }
    if config.replication.http.secret.as_deref().unwrap_or_default().is_empty()
        && config.replication.http.secret_path.is_none()
    {
        return Some(
            "worker.replication.http.secret / secret_path 未配置：转发无法携带可信标记，会与普通请求无法区分"
                .to_string(),
        );
    }
    if !config.instance_map.contains_key(instance) {
        return Some(format!("worker.instance_map 里没有 '{instance}'，无法解析 writer 的地址"));
    }
    None
}

/// Build the `scheme://host:port` of an `instance_map` entry.
///
/// `tls` defaults to **true** in the config, so a plain-HTTP container deployment has
/// to set it to `false` explicitly — getting this backwards shows up as a TLS
/// handshake failure against a plain listener, which reads like "the writer is down".
pub fn instance_base_url(config: &WorkerConfig, instance: &str) -> Option<String> {
    let location = config.instance_map.get(instance)?;
    let scheme = if location.tls { "https" } else { "http" };
    Some(format!("{scheme}://{}:{}", location.host, location.port))
}

/// Decide where a write for `stream` executes on this instance.
///
/// Rules, in order:
///
/// 1. `worker.write_forwarding = false` (or `worker.enabled = false`) ⇒
///    [`WriteRoute::Local`]. The switch exists so that turning it on is the only way
///    any deployment changes behaviour.
/// 2. This instance is one of the configured owners ⇒ `Local`. For the
///    `AtLeastOne` streams that is the correct reading: any owner may write.
/// 3. Otherwise the **first** configured owner is the target. Owners are tried in
///    configuration order because that is the order the operator wrote them in, which
///    is the only signal available about preference.
pub fn write_route(config: &WorkerConfig, stream: WriteStream) -> WriteRoute {
    // Two switches, both required. `write_forwarding` is the operator's opt-in;
    // `enabled` is a structural guard — a process that was not started in worker mode
    // is a monolith and has no peer to forward to, so it must never forward even if a
    // stray configuration says otherwise (the validator reports that combination as an
    // error as well).
    if !config.write_forwarding || !config.enabled {
        return WriteRoute::Local;
    }

    let current = resolved_current_instance_name(config);
    let owners = stream.owners(config);
    if owners.iter().any(|owner| owner == &current) {
        return WriteRoute::Local;
    }

    let Some(instance) = owners.first() else {
        // An empty owner list is already a cardinality violation reported by the
        // validator; treating it as "local" keeps the request path from inventing a
        // second, differently-worded failure.
        return WriteRoute::Local;
    };

    if let Some(reason) = unusable_reason(config, instance) {
        return WriteRoute::Unavailable { instance: instance.clone(), reason };
    }
    match instance_base_url(config, instance) {
        Some(base_url) => WriteRoute::Forward { instance: instance.clone(), base_url },
        // Unreachable in practice: `unusable_reason` already required the map entry.
        None => WriteRoute::Unavailable {
            instance: instance.clone(),
            reason: format!("worker.instance_map 里没有 '{instance}'"),
        },
    }
}

/// All streams whose write route is not `Local`, precomputed for the topology report
/// and for the startup log line.
pub fn non_local_routes(config: &WorkerConfig) -> Vec<(WriteStream, WriteRoute)> {
    WriteStream::all()
        .iter()
        .copied()
        .map(|stream| (stream, write_route(config, stream)))
        .filter(|(_, route)| !matches!(route, WriteRoute::Local))
        .collect()
}

// ── (method, path) → stream ────────────────────────────────────────────────────

/// Which stream does this write touch? `None` means "not a stream-owned write".
///
/// Only `POST`/`PUT`/`DELETE`/`PATCH` are considered: every routable write in the
/// client API uses one of them, and treating `GET` as a write would forward reads.
///
/// A wrong answer here is **not** corrupting — the writer runs the identical handler,
/// so a misclassified request merely takes an extra hop. What it *does* cost is the
/// single-writer guarantee for the stream whose route was missed, which is why
/// `tests/unit/worker_write_stream_tests.rs` freezes the mapping against the real
/// route ledger instead of trusting this table's completeness.
pub fn stream_for_write(method: &str, path: &str) -> Option<WriteStream> {
    if !matches!(method, "POST" | "PUT" | "DELETE" | "PATCH") {
        return None;
    }
    let segments = normalized_segments(path)?;

    match segments.as_slice() {
        ["sendToDevice", _event_type, _txn_id] => Some(WriteStream::ToDevice),

        ["presence", "list"] => Some(WriteStream::Presence),
        ["presence", _user_id, "status"] => Some(WriteStream::Presence),

        ["pushrules", ..] => Some(WriteStream::PushRules),

        ["user", _user_id, "account_data", ..] => Some(WriteStream::AccountData),
        ["user", _user_id, "rooms", _room_id, "account_data", ..] => Some(WriteStream::AccountData),
        // Tags are account data per the spec; clients read them back through
        // `/sync`'s `account_data` section, so they belong to the same writer.
        ["user", _user_id, "rooms", _room_id, "tags", ..] => Some(WriteStream::AccountData),
        ["rooms", _room_id, "account_data", ..] => Some(WriteStream::AccountData),

        ["rooms", _room_id, "receipt", ..] => Some(WriteStream::Receipts),
        ["rooms", _room_id, "read_markers"] => Some(WriteStream::Receipts),
        ["rooms", _room_id, "threads", _thread_id, "read"] => Some(WriteStream::Receipts),

        ["rooms", _room_id, "typing", _user_id] => Some(WriteStream::Typing),
        ["rooms", "typing"] => Some(WriteStream::Typing),

        _ => None,
    }
}

/// Split a client-vendor path into its version-independent segments.
///
/// `/_matrix/client/v3/rooms/{id}/receipt/x/y` and
/// `/_matrix/vendor/v1/rooms/{id}/receipt/x/y` must produce the same answer: the same
/// handler serves both spellings, and a rule keyed on one of them silently misses the
/// other. Unstable MSC namespaces (`org.matrix.*`, `uk.tcpip.*`, `io.element.*`) are
/// dropped for the same reason.
///
/// Returns `None` for anything outside the client-facing namespaces: federation and
/// the admin API are deliberately not part of this mapping (see the `events` note in
/// the module documentation).
fn normalized_segments(path: &str) -> Option<Vec<&str>> {
    let (rest, drop_msc_namespace) = if let Some(rest) = path.strip_prefix("/_matrix/client/") {
        let (version, tail) = rest.split_once('/')?;
        if version == "unstable" {
            // `/_matrix/client/unstable/<namespace>/…` — the namespace segment may or
            // may not be an MSC identifier, and only the MSC one is dropped.
            (tail, true)
        } else if matches!(version, "r0" | "v1" | "v2" | "v3" | "v4" | "v5") {
            (tail, false)
        } else {
            return None;
        }
    } else if let Some(rest) = path.strip_prefix("/_matrix/vendor/v1/") {
        (rest, false)
    } else {
        return None;
    };

    let mut segments: Vec<&str> = rest.split('/').filter(|segment| !segment.is_empty()).collect();
    if drop_msc_namespace && segments.first().is_some_and(|segment| is_msc_namespace(segment)) {
        segments.remove(0);
    }
    Some(segments)
}

fn is_msc_namespace(segment: &str) -> bool {
    segment.starts_with("org.matrix.") || segment.starts_with("uk.tcpip.") || segment.starts_with("io.element.")
}

#[cfg(test)]
mod tests {
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
    fn only_six_streams_declare_themselves_forwardable() {
        let forwardable: Vec<&str> =
            WriteStream::all().iter().filter(|s| s.is_forwardable()).map(|s| s.stream_name()).collect();
        assert_eq!(forwardable, ["typing", "to_device", "account_data", "receipts", "presence", "push_rules"]);
        assert!(!WriteStream::Events.forwardable_rationale().is_empty());
        assert!(!WriteStream::DeviceLists.forwardable_rationale().is_empty());
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
    fn the_unshipped_streams_are_not_mapped_even_though_their_routes_look_obvious() {
        // If these ever start returning a stream, the mapping would claim single-writer
        // coverage the design explicitly does not provide (see the module docs).
        for path in [
            "/_matrix/client/v3/rooms/!r/send/m.room.message/t1",
            "/_matrix/client/v3/createRoom",
            "/_matrix/client/v3/keys/upload",
            "/_matrix/client/v3/devices/$d",
        ] {
            assert_eq!(stream_for_write("PUT", path), None, "{path}");
        }
    }

    #[test]
    fn non_client_namespaces_are_out_of_scope() {
        for path in [
            "/_matrix/federation/v1/send/1",
            "/_synapse/admin/v1/rooms/x/redact/y",
            "/_matrix/media/v3/upload",
            "/_matrix/app/v1/transactions/1",
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
}
