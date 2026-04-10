-- ============================================================================
-- Rollback: restore_public_schema_contract_foreign_keys
-- Created: 2026-04-06
-- Description: Drops the public schema foreign keys restored by
-- 20260406000003.
-- ============================================================================

SET TIME ZONE 'UTC';

ALTER TABLE IF EXISTS deleted_events_index
    DROP CONSTRAINT IF EXISTS fk_deleted_events_index_room;
ALTER TABLE IF EXISTS retention_stats
    DROP CONSTRAINT IF EXISTS fk_retention_stats_room;
ALTER TABLE IF EXISTS retention_cleanup_logs
    DROP CONSTRAINT IF EXISTS fk_retention_cleanup_logs_room;
ALTER TABLE IF EXISTS retention_cleanup_queue
    DROP CONSTRAINT IF EXISTS fk_retention_cleanup_queue_room;
ALTER TABLE IF EXISTS room_children
    DROP CONSTRAINT IF EXISTS fk_room_children_child;
ALTER TABLE IF EXISTS room_children
    DROP CONSTRAINT IF EXISTS fk_room_children_parent;
ALTER TABLE IF EXISTS room_summary_update_queue
    DROP CONSTRAINT IF EXISTS fk_room_summary_update_queue_room;
ALTER TABLE IF EXISTS room_summary_stats
    DROP CONSTRAINT IF EXISTS fk_room_summary_stats_room;
ALTER TABLE IF EXISTS room_summary_state
    DROP CONSTRAINT IF EXISTS fk_room_summary_state_room;
