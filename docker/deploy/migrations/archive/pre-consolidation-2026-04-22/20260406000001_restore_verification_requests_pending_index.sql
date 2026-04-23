-- ============================================================================
-- Restore verification_requests pending lookup index
-- Created: 2026-04-06
-- Description: Re-create a critical verification_requests index that was
-- accidentally dropped during schema alignment consolidation.
-- ============================================================================

SET TIME ZONE 'UTC';

CREATE INDEX IF NOT EXISTS idx_verification_requests_to_user_state
ON verification_requests(to_user, state, updated_ts DESC);

INSERT INTO schema_migrations (version, name, success, description, applied_ts)
VALUES (
    '20260406000001',
    'restore_verification_requests_pending_index',
    TRUE,
    'Restore idx_verification_requests_to_user_state dropped by consolidated schema alignment',
    EXTRACT(EPOCH FROM NOW())::BIGINT * 1000
)
ON CONFLICT (version) DO NOTHING;
