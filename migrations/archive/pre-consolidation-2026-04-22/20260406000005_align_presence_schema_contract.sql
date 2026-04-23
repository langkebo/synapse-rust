-- ============================================================================
-- Align presence schema contract
-- Created: 2026-04-06
-- Description: Repair legacy presence nullability/default drift so presence
-- schema contract tests match the unified schema baseline.
-- ============================================================================

SET TIME ZONE 'UTC';

UPDATE presence
SET presence = 'offline'
WHERE presence IS NULL;

UPDATE presence
SET last_active_ts = 0
WHERE last_active_ts IS NULL;

ALTER TABLE IF EXISTS presence
    ALTER COLUMN presence SET DEFAULT 'offline';

ALTER TABLE IF EXISTS presence
    ALTER COLUMN last_active_ts SET DEFAULT 0;

ALTER TABLE IF EXISTS presence
    ALTER COLUMN presence SET NOT NULL;

ALTER TABLE IF EXISTS presence
    ALTER COLUMN last_active_ts SET NOT NULL;

CREATE INDEX IF NOT EXISTS idx_presence_user_status
ON presence(user_id, presence);

INSERT INTO schema_migrations (version, name, success, description, applied_ts)
VALUES (
    '20260406000005',
    'align_presence_schema_contract',
    TRUE,
    'Repair legacy presence nullability/default drift and ensure presence schema contract index',
    EXTRACT(EPOCH FROM NOW())::BIGINT * 1000
)
ON CONFLICT (version) DO NOTHING;
