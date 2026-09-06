-- B-8: Add soft_failed column to events table.
--
-- This column replaces the hard-delete pattern in txn_dedup.rs where losing
-- duplicate events (from concurrent client txn_id races) were physically
-- deleted via `DELETE FROM events WHERE event_id = $1`.  Physical deletion
-- risks:
--   1. FK violations (events has FK to event_json, event_edges, etc.)
--   2. Broken event DAG chains (prev_events references dangling IDs)
--   3. Compliance/audit violations (events must be retained for N days)
--
-- The new soft-delete pattern:
--   1. A losing duplicate event is marked `soft_failed = TRUE` instead of deleted.
--   2. All consumer read paths (sync, pagination, sliding sync) filter
--      `WHERE soft_failed = FALSE` so soft-failed events are invisible to clients.
--   3. The events row and all its FK children remain intact for audit/compliance.
--   4. A background purge job can eventually `DELETE FROM events WHERE soft_failed
--      = TRUE AND created_ts < NOW() - INTERVAL 'N days'` once retention allows.
--
-- Existing INSERT statements in create_event / create_event_with_graph do NOT
-- need to change — `DEFAULT FALSE` on the column means every existing INSERT
-- implicitly sets `soft_failed = FALSE`.

ALTER TABLE events ADD COLUMN soft_failed BOOLEAN NOT NULL DEFAULT FALSE;

-- Index to speed up the common consumer read paths:
--   `SELECT ... FROM events WHERE room_id = $1 AND soft_failed = FALSE ORDER BY ...`
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_events_room_soft_failed
    ON events (room_id, soft_failed)
    WHERE soft_failed = FALSE;

-- Backward-compatible partial index for the most critical hot path (sync / paginate):
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_events_stream_ordering
    ON events (room_id, stream_ordering DESC)
    WHERE soft_failed = FALSE;
