-- ============================================================================
-- Rollback: align_presence_schema_contract
-- Created: 2026-04-06
-- Description: Restores nullable/default behavior for legacy presence columns
-- if a rollback to the pre-contract shape is required.
-- ============================================================================

SET TIME ZONE 'UTC';

ALTER TABLE IF EXISTS presence
    ALTER COLUMN presence DROP NOT NULL;

ALTER TABLE IF EXISTS presence
    ALTER COLUMN presence DROP DEFAULT;

ALTER TABLE IF EXISTS presence
    ALTER COLUMN last_active_ts DROP NOT NULL;

ALTER TABLE IF EXISTS presence
    ALTER COLUMN last_active_ts DROP DEFAULT;

DROP INDEX IF EXISTS idx_presence_user_status;
