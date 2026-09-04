pub(crate) mod basic;
pub mod batch;
pub(crate) mod create;
pub(crate) mod dag;
pub(crate) mod ephemeral;
pub(crate) mod models;
pub(crate) mod pagination;
pub mod reader;
pub(crate) mod redaction;
pub(crate) mod search;
pub(crate) mod signature;
pub mod state;
pub(crate) mod txn_dedup;
pub(crate) mod unread;
pub(crate) mod writer;

pub use models::*;
pub use reader::EventReader;
pub use writer::EventWriter;

/// Canonical 14-column SELECT list for `RoomEvent` deserialization.
///
/// Used by `event/basic.rs` and `event/batch.rs` to avoid hand-rolling the same
/// column list across 15+ query methods. Mirrors the pattern already in use
/// for `StateEvent` via `STATE_EVENT_OUTER_COLS` / `STATE_EVENT_INNER_COLS`
/// in `event/state.rs`.
///
/// The `COALESCE(...)` expressions preserve backward-compatible null handling:
/// - `user_id` falls back to `sender` for legacy events
/// - `depth` / `origin_server_ts` / `not_before` default to 0
/// - `origin` normalizes empty/`undefined` strings to `'self'`
///
/// `reference_image` removed in v11 cleanup (migration
/// `20260904040000_schema_cleanup_dedup_and_dead_code.sql`) since no INSERT
/// ever populated it. See `.scratch/db-schema-audit-2026-09-04.md` §4.3.
///
/// Ref: TDD落地执行清单 §8.2 ARC-1..5 (Problem #2 SQL Column Boilerplate)
pub(crate) const ROOM_EVENT_COLS: &str = "\
    event_id, room_id, COALESCE(user_id, sender) as user_id, event_type, content, state_key, \
    COALESCE(depth, 0) as depth, COALESCE(origin_server_ts, 0) as origin_server_ts, \
    COALESCE(origin_server_ts, 0) as processed_at, COALESCE(not_before, 0) as not_before, \
    status, COALESCE(NULLIF(NULLIF(BTRIM(origin), ''), 'undefined'), 'self') as origin, \
    stream_ordering, redacts";

#[cfg(test)]
mod tests;

#[cfg(test)]
mod db_tests;
