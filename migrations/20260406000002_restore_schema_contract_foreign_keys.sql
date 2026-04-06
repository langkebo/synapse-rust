-- ============================================================================
-- Restore schema-contract foreign keys
-- Created: 2026-04-06
-- Description: Re-create foreign keys required by schema validator and
-- database integrity tests for room summary and retention tables.
-- ============================================================================

SET TIME ZONE 'UTC';

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_room_summary_state_room'
    ) THEN
        ALTER TABLE room_summary_state
        ADD CONSTRAINT fk_room_summary_state_room
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_room_summary_stats_room'
    ) THEN
        ALTER TABLE room_summary_stats
        ADD CONSTRAINT fk_room_summary_stats_room
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_room_summary_update_queue_room'
    ) THEN
        ALTER TABLE room_summary_update_queue
        ADD CONSTRAINT fk_room_summary_update_queue_room
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_room_children_parent'
    ) THEN
        ALTER TABLE room_children
        ADD CONSTRAINT fk_room_children_parent
        FOREIGN KEY (parent_room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_room_children_child'
    ) THEN
        ALTER TABLE room_children
        ADD CONSTRAINT fk_room_children_child
        FOREIGN KEY (child_room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_retention_cleanup_queue_room'
    ) THEN
        ALTER TABLE retention_cleanup_queue
        ADD CONSTRAINT fk_retention_cleanup_queue_room
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_retention_cleanup_logs_room'
    ) THEN
        ALTER TABLE retention_cleanup_logs
        ADD CONSTRAINT fk_retention_cleanup_logs_room
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_retention_stats_room'
    ) THEN
        ALTER TABLE retention_stats
        ADD CONSTRAINT fk_retention_stats_room
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_deleted_events_index_room'
    ) THEN
        ALTER TABLE deleted_events_index
        ADD CONSTRAINT fk_deleted_events_index_room
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;
END $$;

INSERT INTO schema_migrations (version, name, success, description, applied_ts)
VALUES (
    '20260406000002',
    'restore_schema_contract_foreign_keys',
    TRUE,
    'Restore room summary and retention foreign keys required by schema contract checks',
    EXTRACT(EPOCH FROM NOW())::BIGINT * 1000
)
ON CONFLICT (version) DO NOTHING;
