-- ============================================================================
-- Consolidated Minor Features Rollback
-- Created: 2026-04-04
-- Description: Rolls back consolidated minor features migration
-- Replaces: 20260328000002 rollback, 20260330000010 undo, 20260330000011 undo
-- ============================================================================

SET TIME ZONE 'UTC';

DROP INDEX IF EXISTS idx_feature_flag_targets_lookup;
DROP INDEX IF EXISTS idx_feature_flags_expires_at;
DROP INDEX IF EXISTS idx_feature_flags_scope_status;
DROP TABLE IF EXISTS feature_flag_targets;
DROP TABLE IF EXISTS feature_flags;

DROP INDEX IF EXISTS idx_federation_cache_expiry;
DROP INDEX IF EXISTS idx_federation_cache_key;
DROP TABLE IF EXISTS federation_cache;
