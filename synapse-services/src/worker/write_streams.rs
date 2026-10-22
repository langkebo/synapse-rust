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
//! # All eight streams, three namespaces
//!
//! The mapping in [`stream_for_write`] covers three request namespaces because a
//! stream's writes do not stop at the client API:
//!
//! * **client** (`/_matrix/client/{version}/…`) — the ordinary C-S write surface;
//! * **vendor** (`/_matrix/vendor/v1/…`) — this server's own extensions, several of
//!   which write room events (`create_dm`, `pinned_events`, `relations`,
//!   `anti_screenshot`, sticky events) or create rooms (`rooms/create_private`,
//!   `friends/dm/{user}`, `invites/claim`);
//! * **federation** (`/_matrix/federation/{version}/…`) — `send`, `send_join`,
//!   `send_leave`, `v2/invite`, `knock`, `thirdparty/invite`. Omitting this half
//!   would leave "client writes single-writer, federation writes everywhere", which
//!   is worse than not doing it at all: the duplicated post-commit fan-out in the
//!   table above happens on the federation entry points too.
//!
//! One write deliberately stays outside the mapping because it has no route at all:
//! the **delayed-event dispatcher** (`src/server/mod.rs`) injects due events without
//! an HTTP request, so [`owns_events_dispatch`] — not a route — decides which instance
//! runs it.
//!
//! The **admin** namespace (`/_synapse/admin/…`) *is* mapped (`WriteSurface::Admin`):
//! 16 of its 156 write routes really write a managed stream (membership, power levels,
//! backfill, server notices, device revocation), and leaving it out made "single
//! writer" false on those paths. The other 140 rows are frozen as `-` so a new admin
//! write route forces a decision instead of slipping through.

use synapse_common::config::worker::WorkerConfig;

use crate::worker::topology_validator::resolved_current_instance_name;

mod admin;
mod paths;

pub use paths::WriteSurface;

use admin::admin_stream;
use paths::mapped_path;

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

    /// The request namespaces whose writes for this stream are mapped.
    ///
    /// This is the machine-checkable half of "the mapping is exhaustive": the guard
    /// test derives the surfaces actually used by the frozen map from the rows
    /// themselves, so a row added under a namespace the stream does not declare fails
    /// instead of quietly widening the claim.
    pub fn mapped_surfaces(self) -> &'static [WriteSurface] {
        match self {
            Self::Events => {
                &[WriteSurface::Client, WriteSurface::Vendor, WriteSurface::Federation, WriteSurface::Admin]
            }
            Self::Receipts => &[WriteSurface::Client, WriteSurface::Vendor],
            // `device_lists` is written by the admin surface too: `DELETE
            // /users/{u}/devices/{d}`, `/users/{u}/logout` and
            // `/user_sessions/{u}/invalidate` all reach `revoke_device` / `logout_all`.
            Self::DeviceLists => &[WriteSurface::Client, WriteSurface::Admin],
            Self::Typing | Self::ToDevice | Self::AccountData | Self::Presence | Self::PushRules => {
                &[WriteSurface::Client]
            }
        }
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

    /// Does this stream reach `/sync` through the **notifier fan-out** rather than
    /// through the database alone?
    ///
    /// Two streams do: room events and to-device messages are pushed to the waiter by
    /// `EventNotifier` (`notifying_event_writer.rs` → `notify_room`, and the
    /// `sendToDevice` handler → `notify_user`). That notifier is process-local `Notify`
    /// plus a Redis publish; without Redis it cannot reach another instance, and a
    /// reader's long-poll then waits out its own timeout
    /// ([`missing_notify_fan_out`], design doc §4.5).
    ///
    /// `device_lists` is deliberately **not** in this set even though `/sync` carries a
    /// device-list section: nothing calls `notify_user` on a device-list change, so that
    /// section has always been "picked up at the next wake-up" and forwarding does not
    /// change it.
    pub fn notify_bound(self) -> bool {
        matches!(self, Self::Events | Self::ToDevice)
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

/// Streams whose writes arrive in the **same request**, and so cannot have different
/// writers.
///
/// `PUT /_matrix/federation/v1/send/{txn}` is one request that writes two streams:
/// its PDUs go to `events`, and an `m.signing_key_update` EDU makes it record a
/// `device_lists` change too (and broadcast `m.device_list_update` to every peer that
/// shares a room). The request is attributed to `events`, so it executes on the
/// `events` writer — meaning that instance *also* writes `device_lists`.
///
/// If the two streams named different writers, the resulting state would be worse than
/// either extreme: `device_lists` would have two writers (the events writer, which gets
/// every federation EDU, plus the configured one, which gets every client-side device
/// change) and the `m.device_list_update` broadcast would be duplicated exactly as the
/// configuration was meant to prevent.
pub const COUPLED_STREAM_PAIRS: [(WriteStream, WriteStream); 1] = [(WriteStream::Events, WriteStream::DeviceLists)];

/// Coupled-stream owner lists that disagree, as startup-error messages.
///
/// Only reported when forwarding is actually on: with `write_forwarding = false` the
/// whole block is inert, and the pre-existing warning already says so.
pub fn coupled_stream_conflicts(config: &WorkerConfig) -> Vec<String> {
    if !config.write_forwarding || !config.enabled {
        return Vec::new();
    }
    let mut conflicts = Vec::new();
    for (left, right) in COUPLED_STREAM_PAIRS {
        let mut left_owners: Vec<&str> = left.owners(config).iter().map(String::as_str).collect();
        let mut right_owners: Vec<&str> = right.owners(config).iter().map(String::as_str).collect();
        left_owners.sort_unstable();
        right_owners.sort_unstable();
        if left_owners == right_owners {
            continue;
        }
        conflicts.push(format!(
            "worker.stream_writers.{} = {:?} 与 worker.stream_writers.{} = {:?} 不同：\
             同一个请求（联邦 `PUT /_matrix/federation/v1/send/{{txn}}` 的 PDU 会写 {}，\
             其中的 `m.signing_key_update` EDU 还会写 {}）会同时写这两路流 ⇒ 两路必须指向同一实例，\
             否则 {} 会退化成多写者、`m.device_list_update` 广播会重复。",
            left.stream_name(),
            left.owners(config),
            right.stream_name(),
            right.owners(config),
            left.stream_name(),
            right.stream_name(),
            right.stream_name(),
        ));
    }
    conflicts
}

/// Forwarding configurations that cannot wake a reader's `/sync` long-poll, as
/// startup-error messages.
///
/// # The gap this closes
///
/// [`WriteStream::notify_bound`] streams are pushed to the waiter by `EventNotifier`,
/// which is process-local `Notify` **plus** a Redis publish; the Redis publish is the
/// only half that crosses instances, and `publish_redis` returns early when no pool was
/// configured (`event_notifier.rs`). Meanwhile the `/sync` waiter registers its slots
/// unconditionally — the notifier always exists, Redis or not — so it never falls back
/// to the periodic-poll branch. Result: with `write_forwarding: true` and Redis off, a
/// write executed on the writer wakes that process's waiters only, and a client parked
/// on a reader learns about it when its own `timeout` expires (typically 30 s).
///
/// Nothing is lost — `/sync` still returns the events on the next iteration — so this
/// cannot be detected by a correctness test, which is exactly why it is a **startup
/// error** rather than a warning: the deployment would otherwise look healthy while
/// every push is late.
///
/// `redis_enabled` is `redis.enabled` from the server config, which is not part of
/// [`WorkerConfig`]; callers that only have the worker block must pass `true` (the
/// pre-W-I behaviour, and what the single-process default implies).
pub fn missing_notify_fan_out(config: &WorkerConfig, redis_enabled: bool) -> Vec<String> {
    if redis_enabled || !config.write_forwarding || !config.enabled {
        return Vec::new();
    }
    let mut messages = Vec::new();
    for stream in WriteStream::all().iter().copied().filter(|stream| stream.notify_bound()) {
        // Only a *forwarded* stream is affected: when this instance is the writer it does
        // both halves in-process, and an `Unavailable` target already has its own error
        // (the write fails outright, so there is no silent latency to report).
        if let WriteRoute::Forward { instance, .. } = write_route(config, stream) {
            messages.push(format!(
                "worker.write_forwarding = true 且 worker.stream_writers.{} = {:?}，但 redis.enabled = false：\
                 {} 的写入会在实例 '{instance}' 上执行，而唤醒 /sync 长轮询只靠进程内 Notify + Redis 扇出 ⇒ \
                 落在本实例的长轮询要等客户端自己的 timeout（典型 30 s）才拿到数据（数据不丢，只是变慢）。\
                 要么打开 redis.enabled（跨实例唤醒的唯一通道），要么把 {} 设回 [\"master\"]\
                 （docs/audit/WORKER_STREAM_WRITE_FORWARDING_DESIGN_2026-10-22.md §4.5）。",
                stream.stream_name(),
                stream.owners(config),
                stream.stream_name(),
                stream.stream_name(),
            ));
        }
    }
    messages
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
///
/// # The classification rule
///
/// A route belongs to `events` when its handler ends up **writing the room DAG**
/// (`create_event` / `send_message` / state / redaction) **or broadcasting a room-scoped
/// PDU or EDU to federated peers**. Both halves matter: sticky events
/// (`set_is_sticky_event`) and pinned events write metadata plus an outbound EDU, so two
/// writers would duplicate the broadcast even though only one of them writes `events`.
///
/// A route belongs to `device_lists` when its handler writes `device_lists_stream` /
/// `device_lists_changes` — i.e. it creates, renames or deletes a device, or changes a
/// device's keys/cross-signing material. `broadcast_device_list_update` then sends an
/// `m.device_list_update` EDU to every remote server that shares a room with the user,
/// which is the fan-out that must not happen twice.
///
/// Everything else (receipts, typing, account data, push rules, presence, and the
/// custom space/thread/friend tables this server keeps outside the DAG) is `None`.
pub fn stream_for_write(method: &str, path: &str) -> Option<WriteStream> {
    if !matches!(method, "POST" | "PUT" | "DELETE" | "PATCH") {
        return None;
    }
    let (surface, segments) = mapped_path(path)?;
    let segments = segments.as_slice();

    // Admin writes are classified in `admin.rs`, and that module **returns** for every
    // admin path (including `None`) so shape coincidence with the client routes cannot
    // decide the attribution. See its module docs for the 16-of-156 breakdown.
    if surface == WriteSurface::Admin {
        return admin_stream(segments);
    }

    // ── device_lists: the device-management surface ──────────────────────────
    //
    // Client-only on purpose: federation has no device-management routes, and the
    // side effect that *does* ride on a federation request (`m.signing_key_update`
    // EDUs, handled inside `PUT /_matrix/federation/v1/send/{txn}`) is attributed to
    // `events` below. `validate_worker_config` then requires both streams to name the
    // same writer, because one request writes both.
    if surface == WriteSurface::Client {
        match segments {
            ["devices", _device_id] => return Some(WriteStream::DeviceLists),
            ["delete_devices"] => return Some(WriteStream::DeviceLists),
            ["logout", "all"] => return Some(WriteStream::DeviceLists),
            ["account", "deactivate"] => return Some(WriteStream::DeviceLists),
            ["keys", "upload"] | ["keys", "upload", _] => return Some(WriteStream::DeviceLists),
            ["keys", "device_signing", "upload"] => return Some(WriteStream::DeviceLists),
            _ => {}
        }
    }

    // ── events: room creation, the DAG, membership, and federation PDU intake ─
    match segments {
        // Room creation, however it is spelled.
        ["createRoom"] | ["rooms", "create_private"] | ["create_dm"] => Some(WriteStream::Events),
        ["friends", "dm", _user_id] => Some(WriteStream::Events),
        // One-time invites materialise a deferred room when claimed.
        ["invites", "claim"] => Some(WriteStream::Events),

        // Joining or knocking by id/alias creates a membership event.
        ["join", _room_id_or_alias] | ["knock", _room_id_or_alias] => Some(WriteStream::Events),

        // Every `rooms/{id}/…` write that lands a PDU. `read_markers`, `receipt`,
        // `typing`, `account_data`, `report`, `forget`, `summary/*`, `threads/*` and
        // the vendor space/thread families are matched further down: they either
        // belong to another stream or touch no stream at all.
        //
        // ⚠️ Two `m.call.*` writes are excluded on purpose: `call_hangup` and
        // `call_candidates` only touch the local `call_sessions` table — unlike
        // `call_invite` / `call_answer`, they never call `create_event`, so mapping them
        // would buy no single-writer guarantee and cost a hop per SDP candidate. That is
        // a spec deviation in its own right (a conforming server emits `m.call.*` as room
        // events) and is recorded in the audit document; if the handlers are fixed to
        // emit, this arm must go with them.
        ["rooms", _room_id, "send", "m.call.candidates" | "m.call.hangup", _txn_id] => None,
        ["rooms", _room_id, "send", _event_type, _txn_id] => Some(WriteStream::Events),
        ["rooms", _room_id, "state", _event_type, ..] => Some(WriteStream::Events),
        ["rooms", _room_id, "redact", _event_id, _txn_id] => Some(WriteStream::Events),
        ["rooms", _room_id, "invite" | "join" | "leave" | "kick" | "ban" | "unban" | "upgrade"] => {
            Some(WriteStream::Events)
        }
        // MSC4354 sticky events persist metadata *and* broadcast an EDU to every
        // federated peer in the room.
        ["rooms", _room_id, "sticky_events", ..] => Some(WriteStream::Events),
        // Vendor: these create real room events (`m.room.pinned_events`,
        // `com.hula.*` state, relation events).
        ["rooms", _room_id, "pinned_events", ..] => Some(WriteStream::Events),
        ["rooms", _room_id, "relations", _event_id, _rel_type, _txn_id] => Some(WriteStream::Events),
        ["rooms", _room_id, "anti_screenshot"] => Some(WriteStream::Events),
        ["admin", "room", _room_id, "redact"] => Some(WriteStream::Events),

        // ── federation: PDU intake ───────────────────────────────────────────
        //
        // `send` is the transaction endpoint: PDUs *and* EDUs, so it writes
        // `events` (and, for `m.signing_key_update`, `device_lists` — see the note
        // above).
        ["v1", "send", _txn_id] => Some(WriteStream::Events),
        ["v1" | "v2", "send_join" | "send_leave", _room_id, _event_id] => Some(WriteStream::Events),
        // `v2/invite` persists the invited PDU; `v1/invite` only validates it, so it
        // is *not* mapped here — see the audit's "ratchet blind spot" note.
        ["v2", "invite", _room_id, _event_id] => Some(WriteStream::Events),
        ["v1", "knock", _room_id, _user_id] => Some(WriteStream::Events),
        ["v1", "thirdparty", "invite"] => Some(WriteStream::Events),

        // ── the six light streams ────────────────────────────────────────────
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

/// Does *this* instance run the MSC4140 delayed-event dispatcher?
/// The dispatcher injects due events without an HTTP request, so no route mapping can
/// reach it — ownership has to be decided by process. It returns `true` whenever
/// forwarding is off (the default), so a deployment that never opts in keeps the
/// pre-existing "every process may dispatch, the Redis lock arbitrates" behaviour.
///
/// With forwarding on, only the `events` writer dispatches: every other instance would
/// be a second writer of the room DAG, which is exactly what the configuration exists
/// to remove.
pub fn owns_events_dispatch(config: &WorkerConfig) -> bool {
    matches!(write_route(config, WriteStream::Events), WriteRoute::Local)
}

#[cfg(test)]
mod tests;
