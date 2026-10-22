//! The admin surface: which `/_synapse/admin/*` writes reach a managed stream.
//!
//! 156 admin write routes exist and **16** of them touch a managed stream. This module is
//! the whole classification, and it is deliberately separate from the client/vendor/
//! federation rules in the parent module:
//!
//! * `/_synapse/admin/v1/rooms/{id}/ban` has exactly the same segment shape as
//!   `/_matrix/client/v3/rooms/{id}/ban`, but the two reach the DAG through different code
//!   (`admin_*_user_membership` synthesises an `m.room.member` and broadcasts it, the client
//!   route runs the normal membership flow). Classifying admin paths in their own `match`
//!   means the attribution is a decision rather than a coincidence;
//! * the 140 routes that write no managed stream are just as important: they change
//!   server-side tables (users, media, tokens, retention, room directory, audit) without
//!   writing the room DAG or `device_lists_stream`. The guard test freezes that list, so a
//!   new admin write route forces a decision instead of defaulting to "not a stream".
//!
//! Three routes look like they belong here and were verified **not** to (each checked
//! against its handler rather than its path):
//!
//! * `POST /rooms/{id}/shutdown_room` — `shutdown_room_and_remove_members` only runs
//!   `UPDATE rooms SET is_public = false, name = …` plus a member-row purge
//!   (`synapse-storage/src/room/mod.rs:1023`); no event, no federation.
//! * `POST /users/{u}/evict` — `evict_user_from_joined_rooms` calls the *storage*
//!   `remove_member` per room, not `leave_room`, so no `m.room.member` is emitted
//!   (`synapse-services/src/admin_user_service.rs:321`). Upstream Synapse walks the rooms
//!   through the normal leave path; if that is ever fixed here, this arm must move to
//!   `events`.
//! * `PUT|DELETE /rooms/{id}/listings/public` — writes `rooms.is_public` and
//!   `room_directory` (`synapse-storage/src/room/admin.rs:417`), not `m.room.join_rules`.

use super::WriteStream;

/// Classify one admin write. `None` means "this admin route writes no managed stream".
///
/// The parent module calls this for **every** admin path and returns its verdict directly,
/// including `None` — see the module docs for why it must not fall through.
pub(super) fn admin_stream(segments: &[&str]) -> Option<WriteStream> {
    match segments {
        // Synthesised `m.room.member` + `sign_and_broadcast_event`.
        ["rooms", _room_id, "ban" | "kick"] => Some(WriteStream::Events),
        ["rooms", _room_id, "ban" | "kick" | "unban", _user_id] => Some(WriteStream::Events),
        // `PUT` joins (`join_room`), `DELETE` leaves (`leave_room`) — both write
        // `m.room.member` and broadcast it.
        ["rooms", _room_id, "members", _user_id] => Some(WriteStream::Events),
        // `grant_room_admin` → `upsert_power_levels_event`: a real
        // `m.room.power_levels` state event, not just a users-table flag.
        ["rooms", _room_id, "make_admin"] => Some(WriteStream::Events),
        // `backfill_room_history` → `create_event_with_graph` for every fetched PDU,
        // and the peer list comes from `get_joined_servers_in_room`.
        ["rooms", _room_id, "backfill"] => Some(WriteStream::Events),
        ["join", _room_id_or_alias] => Some(WriteStream::Events),
        // Creates a server-notices room, adds the members and sends the notice:
        // `create_room` + `add_member` + `send_message`.
        ["send_server_notice"] => Some(WriteStream::Events),

        // `revoke_device` (both spellings of the same handler) and `logout_all`
        // both write `device_lists_stream` and fan out `m.device_list_update`.
        ["users", _user_id, "devices", _device_id] => Some(WriteStream::DeviceLists),
        ["users", _user_id, "devices", _device_id, "delete"] => Some(WriteStream::DeviceLists),
        ["users", _user_id, "logout"] => Some(WriteStream::DeviceLists),
        ["user_sessions", _user_id, "invalidate"] => Some(WriteStream::DeviceLists),

        _ => None,
    }
}
