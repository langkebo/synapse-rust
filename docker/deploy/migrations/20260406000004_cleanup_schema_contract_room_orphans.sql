-- ============================================================================
-- Cleanup schema-contract room orphans
-- Created: 2026-04-06
-- Description: Remove orphan rows from derived room summary and retention
-- tables so room foreign keys can be restored safely.
-- ============================================================================

SET TIME ZONE 'UTC';

DELETE FROM room_summary_state rss
WHERE NOT EXISTS (
    SELECT 1 FROM rooms r WHERE r.room_id = rss.room_id
);

DELETE FROM room_summary_stats rs
WHERE NOT EXISTS (
    SELECT 1 FROM rooms r WHERE r.room_id = rs.room_id
);

DELETE FROM room_summary_update_queue rsuq
WHERE NOT EXISTS (
    SELECT 1 FROM rooms r WHERE r.room_id = rsuq.room_id
);

DELETE FROM room_children rc
WHERE NOT EXISTS (
    SELECT 1 FROM rooms parent WHERE parent.room_id = rc.parent_room_id
)
   OR NOT EXISTS (
    SELECT 1 FROM rooms child WHERE child.room_id = rc.child_room_id
);

DELETE FROM retention_cleanup_queue rcq
WHERE NOT EXISTS (
    SELECT 1 FROM rooms r WHERE r.room_id = rcq.room_id
);

DELETE FROM retention_cleanup_logs rcl
WHERE NOT EXISTS (
    SELECT 1 FROM rooms r WHERE r.room_id = rcl.room_id
);

DELETE FROM retention_stats rs
WHERE NOT EXISTS (
    SELECT 1 FROM rooms r WHERE r.room_id = rs.room_id
);

DELETE FROM deleted_events_index dei
WHERE NOT EXISTS (
    SELECT 1 FROM rooms r WHERE r.room_id = dei.room_id
);

INSERT INTO schema_migrations (version, name, success, description, applied_ts)
VALUES (
    '20260406000004',
    'cleanup_schema_contract_room_orphans',
    TRUE,
    'Delete orphan rows from derived room summary and retention tables before restoring foreign keys',
    EXTRACT(EPOCH FROM NOW())::BIGINT * 1000
)
ON CONFLICT (version) DO NOTHING;
