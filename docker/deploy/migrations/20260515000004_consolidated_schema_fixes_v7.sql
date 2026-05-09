-- ============================================================================
-- Forward Script: 20260515000004_consolidated_schema_fixes_v7.sql
-- Description: Consolidated schema fixes for v7.
--   - Add unique constraint to room_ephemeral
--   - Add backup_keys fields (first_message_index, forwarded_count, is_verified)
--   - Fix room_ephemeral expires_at column (drop generated, add regular BIGINT)
-- Merged from:
--   - 20260509000001_fix_room_ephemeral_unique.sql
--   - 20260509000002_add_backup_key_fields.sql
--   - 20260520000001_fix_room_ephemeral_expires_at.sql
-- Created: 2026-05-09
-- Risk: LOW - All operations use IF NOT EXISTS / DROP IF EXISTS for idempotency.
-- ============================================================================

SET TIME ZONE 'UTC';

-- ============================================================================
-- 1. Add unique constraint to room_ephemeral
-- ============================================================================
ALTER TABLE room_ephemeral
    ADD CONSTRAINT uq_room_ephemeral_room_event_user UNIQUE (room_id, event_type, user_id);

-- ============================================================================
-- 2. Add fields to backup_keys table
-- ============================================================================
ALTER TABLE backup_keys
    ADD COLUMN IF NOT EXISTS first_message_index BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS forwarded_count BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS is_verified BOOLEAN NOT NULL DEFAULT false;

-- ============================================================================
-- 3. Fix room_ephemeral expires_at column type
--    (Drop generated column, replace with regular BIGINT)
-- ============================================================================
ALTER TABLE room_ephemeral DROP COLUMN IF EXISTS expires_at;
ALTER TABLE room_ephemeral DROP COLUMN IF EXISTS expires_ts;
ALTER TABLE room_ephemeral ADD COLUMN IF NOT EXISTS expires_at BIGINT;

-- ============================================================================
-- Migration record
-- ============================================================================
INSERT INTO schema_migrations (version, name, success, description, applied_ts)
VALUES ('20260515000004', 'consolidated_schema_fixes_v7', TRUE, 'Consolidated schema fixes: room_ephemeral unique constraint, backup_keys fields, room_ephemeral expires_at fix', (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT)
ON CONFLICT (version) DO NOTHING;