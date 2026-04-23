-- ============================================================================
-- Restore public schema-contract foreign keys
-- Created: 2026-04-06
-- Description: Re-create room summary and retention foreign keys in the public
-- schema. Constraint existence checks are schema-scoped to avoid false
-- positives from isolated test schemas.
-- ============================================================================

SET TIME ZONE 'UTC';

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM information_schema.table_constraints
        WHERE table_schema = 'public'
          AND table_name = 'room_summary_state'
          AND constraint_name = 'fk_room_summary_state_room'
    ) THEN
        ALTER TABLE room_summary_state
        ADD CONSTRAINT fk_room_summary_state_room
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM information_schema.table_constraints
        WHERE table_schema = 'public'
          AND table_name = 'room_summary_stats'
          AND constraint_name = 'fk_room_summary_stats_room'
    ) THEN
        ALTER TABLE room_summary_stats
        ADD CONSTRAINT fk_room_summary_stats_room
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM information_schema.table_constraints
        WHERE table_schema = 'public'
          AND table_name = 'room_summary_update_queue'
          AND constraint_name = 'fk_room_summary_update_queue_room'
    ) THEN
        ALTER TABLE room_summary_update_queue
        ADD CONSTRAINT fk_room_summary_update_queue_room
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM information_schema.table_constraints
        WHERE table_schema = 'public'
          AND table_name = 'room_children'
          AND constraint_name = 'fk_room_children_parent'
    ) THEN
        ALTER TABLE room_children
        ADD CONSTRAINT fk_room_children_parent
        FOREIGN KEY (parent_room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM information_schema.table_constraints
        WHERE table_schema = 'public'
          AND table_name = 'room_children'
          AND constraint_name = 'fk_room_children_child'
    ) THEN
        ALTER TABLE room_children
        ADD CONSTRAINT fk_room_children_child
        FOREIGN KEY (child_room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM information_schema.table_constraints
        WHERE table_schema = 'public'
          AND table_name = 'retention_cleanup_queue'
          AND constraint_name = 'fk_retention_cleanup_queue_room'
    ) THEN
        ALTER TABLE retention_cleanup_queue
        ADD CONSTRAINT fk_retention_cleanup_queue_room
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM information_schema.table_constraints
        WHERE table_schema = 'public'
          AND table_name = 'retention_cleanup_logs'
          AND constraint_name = 'fk_retention_cleanup_logs_room'
    ) THEN
        ALTER TABLE retention_cleanup_logs
        ADD CONSTRAINT fk_retention_cleanup_logs_room
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM information_schema.table_constraints
        WHERE table_schema = 'public'
          AND table_name = 'retention_stats'
          AND constraint_name = 'fk_retention_stats_room'
    ) THEN
        ALTER TABLE retention_stats
        ADD CONSTRAINT fk_retention_stats_room
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM information_schema.table_constraints
        WHERE table_schema = 'public'
          AND table_name = 'deleted_events_index'
          AND constraint_name = 'fk_deleted_events_index_room'
    ) THEN
        ALTER TABLE deleted_events_index
        ADD CONSTRAINT fk_deleted_events_index_room
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;
END $$;

INSERT INTO schema_migrations (version, name, success, description, applied_ts)
VALUES (
    '20260406000003',
    'restore_public_schema_contract_foreign_keys',
    TRUE,
    'Restore public schema room summary and retention foreign keys with schema-scoped existence checks',
    EXTRACT(EPOCH FROM NOW())::BIGINT * 1000
)
ON CONFLICT (version) DO NOTHING;
