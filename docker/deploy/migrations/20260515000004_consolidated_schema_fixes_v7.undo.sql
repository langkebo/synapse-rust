-- ============================================================================
-- Rollback Script: 20260515000004_consolidated_schema_fixes_v7.undo.sql
-- Forward Script: 20260515000004_consolidated_schema_fixes_v7.sql
-- Created: 2026-05-09
-- Risk: MEDIUM - Drops unique constraint, removes columns (data loss possible).
-- Rollback RTO: < 5 minutes
-- ============================================================================

-- ============================================================================
-- 1. Revert room_ephemeral: recreate generated expires_at if it existed
-- ============================================================================
ALTER TABLE room_ephemeral DROP COLUMN IF EXISTS expires_at;
ALTER TABLE room_ephemeral DROP COLUMN IF EXISTS expires_ts;

-- ============================================================================
-- 2. Revert backup_keys fields
-- ============================================================================
ALTER TABLE backup_keys
    DROP COLUMN IF EXISTS first_message_index,
    DROP COLUMN IF EXISTS forwarded_count,
    DROP COLUMN IF EXISTS is_verified;

-- ============================================================================
-- 3. Revert room_ephemeral unique constraint
-- ============================================================================
ALTER TABLE room_ephemeral
    DROP CONSTRAINT IF EXISTS uq_room_ephemeral_room_event_user;
