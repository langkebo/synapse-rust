//! Path parsing: which namespace a write is in, and how its segments are normalised.
//!
//! Split out of the parent module so the classification rules and the path grammar can be
//! read independently — and so the parent stays under the file-size ceiling.

/// The request namespace a write arrives in.
///
/// One stream can be written from more than one namespace — `events` is written by the
/// client API, by federation, *and* by the admin API — so the classifier keeps them
/// apart instead of flattening paths: the same segment shape means different things on
/// either side (`/_matrix/federation/v1/send/{txn}` is a PDU transaction, not a room
/// message; `/_synapse/admin/v1/rooms/{id}/ban` bans through storage plus a synthesised
/// `m.room.member`, while `/_matrix/client/v3/rooms/{id}/ban` is a normal client write).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WriteSurface {
    /// `/_matrix/client/{version}/…` — the client-server API.
    Client,
    /// `/_matrix/vendor/v1/…` — this server's non-standard extensions.
    Vendor,
    /// `/_matrix/federation/{version}/…` — the server-server API.
    Federation,
    /// `/_synapse/admin/{version}/…` — the operator API.
    ///
    /// Added in W-I stage 5: 16 of its 156 write routes really do write a managed
    /// stream, so leaving the namespace out meant "single writer" was false on those
    /// paths. Classifying it explicitly also keeps the admin rules from
    /// *accidentally* matching the client ones — `rooms/{id}/ban` has the same shape on
    /// both sides but reaches the DAG through a completely different code path.
    Admin,
}

impl WriteSurface {
    /// Stable name, used in diagnostics and by the guard test.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Client => "client",
            Self::Vendor => "vendor",
            Self::Federation => "federation",
            Self::Admin => "admin",
        }
    }
}

/// Split a request path into its namespace and version-independent segments.
///
/// `/_matrix/client/v3/rooms/{id}/receipt/x/y` and
/// `/_matrix/vendor/v1/rooms/{id}/receipt/x/y` must produce the same segments: the same
/// handler serves both spellings, and a rule keyed on one of them silently misses the
/// other. Unstable MSC namespaces (`org.matrix.*`, `uk.tcpip.*`, `io.element.*`) are
/// dropped for the same reason.
///
/// **Federation is the exception**: its version segment is kept as the first element,
/// because `v1/invite` (validates the PDU, writes nothing) and `v2/invite` (persists it)
/// have the same shape and different write behaviour. Dropping the version there would
/// claim they are the same write.
///
/// Returns `None` for anything outside these four namespaces. `/_matrix/media/…` stays
/// out of scope (uploads are not stream writes), and so does `/_synapse/worker/…` (the
/// replication surface is the transport the forwarding itself rides on).
pub(super) fn mapped_path(path: &str) -> Option<(WriteSurface, Vec<&str>)> {
    if let Some(rest) = path.strip_prefix("/_matrix/client/") {
        let (version, tail) = rest.split_once('/')?;
        if version == "unstable" {
            let mut segments = split_segments(tail);
            drop_msc_namespace(&mut segments);
            return Some((WriteSurface::Client, segments));
        }
        if matches!(version, "r0" | "v1" | "v2" | "v3" | "v4" | "v5") {
            return Some((WriteSurface::Client, split_segments(tail)));
        }
        return None;
    }
    if let Some(tail) = path.strip_prefix("/_matrix/vendor/v1/") {
        return Some((WriteSurface::Vendor, split_segments(tail)));
    }
    if let Some(rest) = path.strip_prefix("/_matrix/federation/") {
        let (version, tail) = rest.split_once('/')?;
        if !matches!(version, "v1" | "v2" | "unstable") {
            return None;
        }
        let mut segments = vec![version];
        let mut tail_segments = split_segments(tail);
        if version == "unstable" {
            drop_msc_namespace(&mut tail_segments);
        }
        segments.extend(tail_segments);
        return Some((WriteSurface::Federation, segments));
    }
    if let Some(rest) = path.strip_prefix("/_synapse/admin/") {
        // The version segment is dropped: `v1/users/{id}` and `v2/users/{id}` are the
        // same handler (`create_or_update_user_v2` / `delete_user`), unlike federation
        // where `v1/invite` and `v2/invite` differ in whether they persist.
        let (version, tail) = rest.split_once('/')?;
        if !matches!(version, "v1" | "v2") {
            return None;
        }
        return Some((WriteSurface::Admin, split_segments(tail)));
    }
    None
}

fn split_segments(rest: &str) -> Vec<&str> {
    rest.split('/').filter(|segment| !segment.is_empty()).collect()
}

fn drop_msc_namespace(segments: &mut Vec<&str>) {
    if segments.first().is_some_and(|segment| is_msc_namespace(segment)) {
        segments.remove(0);
    }
}

fn is_msc_namespace(segment: &str) -> bool {
    segment.starts_with("org.matrix.") || segment.starts_with("uk.tcpip.") || segment.starts_with("io.element.")
}
