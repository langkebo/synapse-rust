-- ============================================================================
-- Rollback: restore_verification_requests_pending_index
-- Created: 2026-04-06
-- Description: Drops the verification_requests pending lookup index restored by
-- 20260406000001.
-- ============================================================================

SET TIME ZONE 'UTC';

DROP INDEX IF EXISTS idx_verification_requests_to_user_state;
