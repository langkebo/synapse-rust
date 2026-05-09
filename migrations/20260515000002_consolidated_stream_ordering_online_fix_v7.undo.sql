-- ============================================================================
-- Undo: Stream Ordering Online Fix v7
-- Version: 20260515000002.undo
-- Created: 2026-05-15
--
-- Best-effort rollback:
--   - Keeps populated stream_ordering values in place to avoid data loss
--   - Removes only the covering index introduced by the online fix
--   - Re-aligns the sequence with the current max(stream_ordering)
-- ============================================================================

DROP INDEX IF EXISTS idx_events_sync_covering;

SELECT setval(
    'events_stream_ordering_seq',
    COALESCE((SELECT MAX(stream_ordering) FROM events), 0) + 1,
    false
);
