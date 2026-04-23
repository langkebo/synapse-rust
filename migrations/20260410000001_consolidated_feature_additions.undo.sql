-- Undo: Consolidated Feature Additions (reverse order)

-- ===== From: 20260418010100_add_users_created_ts_index.undo.sql =====
DROP INDEX IF EXISTS idx_users_created_ts;

-- ===== From: 20260414000002_hash_access_tokens.undo.sql =====
-- Best-effort rollback only: original plaintext access tokens are intentionally discarded
-- by the forward migration and cannot be reconstructed.

UPDATE access_tokens
SET token = COALESCE(token, token_hash)
WHERE token IS NULL;

DROP INDEX IF EXISTS idx_access_tokens_token_hash;

ALTER TABLE access_tokens
DROP CONSTRAINT IF EXISTS uq_access_tokens_token_hash;

ALTER TABLE access_tokens
ALTER COLUMN token SET NOT NULL;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'uq_access_tokens_token'
    ) THEN
        ALTER TABLE access_tokens
        ADD CONSTRAINT uq_access_tokens_token UNIQUE (token);
    END IF;
END $$;

ALTER TABLE access_tokens
DROP COLUMN IF EXISTS token_hash;

-- ===== From: 20260414000001_add_application_service_webhook_auth.undo.sql =====
ALTER TABLE application_services
DROP COLUMN IF EXISTS config;

ALTER TABLE application_services
DROP COLUMN IF EXISTS api_key;

-- ===== From: 20260413000002_add_lazy_loaded_members.undo.sql =====
SET TIME ZONE 'UTC';

DROP INDEX IF EXISTS idx_lazy_loaded_members_lookup;

DROP TABLE IF EXISTS lazy_loaded_members;

-- ===== From: 20260413000001_align_report_rate_limits_schema_contract.undo.sql =====
SET TIME ZONE 'UTC';

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = 'report_rate_limits'
          AND column_name = 'blocked_until_at'
    ) AND NOT EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = 'report_rate_limits'
          AND column_name = 'blocked_until'
    ) THEN
        ALTER TABLE report_rate_limits RENAME COLUMN blocked_until_at TO blocked_until;
    END IF;

    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = 'report_rate_limits'
          AND column_name = 'last_report_at'
    ) AND NOT EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = 'report_rate_limits'
          AND column_name = 'last_report_ts'
    ) THEN
        ALTER TABLE report_rate_limits RENAME COLUMN last_report_at TO last_report_ts;
    END IF;
END $$;

ALTER TABLE IF EXISTS report_rate_limits
    DROP COLUMN IF EXISTS block_reason;

-- ===== From: 20260407000001_add_ai_connections.undo.sql =====
-- Undo migration: drop ai_connections table

DROP INDEX IF EXISTS idx_ai_connections_user_id;
DROP INDEX IF EXISTS idx_ai_connections_provider;
DROP TABLE IF EXISTS ai_connections;

