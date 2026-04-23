-- ============================================================================
-- Consolidated Migration: Feature Additions & Indexes
-- Created: 2026-04-22 (consolidated from 7 migrations dated 2026-04-07 ~ 2026-04-18)
--
-- Merged source files:
--   1. 20260407000001_add_ai_connections.sql
--   2. 20260409090000_to_device_stream_id_seq.sql
--   3. 20260413000001_align_report_rate_limits_schema_contract.sql
--   4. 20260413000002_add_lazy_loaded_members.sql
--   5. 20260414000001_add_application_service_webhook_auth.sql
--   6. 20260414000002_hash_access_tokens.sql
--   7. 20260418010100_add_users_created_ts_index.sql
--
-- All statements use IF NOT EXISTS / IF EXISTS guards for idempotent execution.
-- ============================================================================


-- ===== Merged from: 20260407000001_add_ai_connections.sql =====

-- Migration: add ai_connections table
-- Created at: 2026-04-09
-- Description: AI connection configuration table for MCP tool integrations

CREATE TABLE IF NOT EXISTS ai_connections (
    id VARCHAR(36) PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    provider VARCHAR(50) NOT NULL,  -- 'openclaw', 'trendradar', 'hula'
    config JSONB,                   -- 连接配置（如 mcp_url: http://127.0.0.1:3333）
    is_active BOOLEAN DEFAULT true,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT
);

CREATE INDEX IF NOT EXISTS idx_ai_connections_user_id ON ai_connections(user_id);
CREATE INDEX IF NOT EXISTS idx_ai_connections_provider ON ai_connections(provider);

-- ===== Merged from: 20260409090000_to_device_stream_id_seq.sql =====

DO $$
DECLARE
    target_schema TEXT := current_schema();
    max_stream_id BIGINT := 0;
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE c.relkind = 'S'
          AND n.nspname = target_schema
          AND c.relname = 'to_device_stream_id_seq'
    ) THEN
        EXECUTE format('CREATE SEQUENCE %I.to_device_stream_id_seq', target_schema);
    END IF;

    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = target_schema
          AND table_name = 'to_device_messages'
          AND column_name = 'stream_id'
    ) THEN
        EXECUTE format(
            'SELECT COALESCE(MAX(stream_id), 0) FROM %I.to_device_messages',
            target_schema
        )
        INTO max_stream_id;

        PERFORM setval(
            format('%I.to_device_stream_id_seq', target_schema)::regclass,
            GREATEST(max_stream_id, 1),
            max_stream_id > 0
        );
    END IF;
END $$;

-- ===== Merged from: 20260413000001_align_report_rate_limits_schema_contract.sql =====

SET TIME ZONE 'UTC';

CREATE TABLE IF NOT EXISTS report_rate_limits (
    id BIGSERIAL,
    user_id TEXT NOT NULL,
    report_count INTEGER DEFAULT 0,
    is_blocked BOOLEAN DEFAULT FALSE,
    blocked_until_at BIGINT,
    block_reason TEXT,
    last_report_at BIGINT,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    CONSTRAINT pk_report_rate_limits PRIMARY KEY (id),
    CONSTRAINT uq_report_rate_limits_user UNIQUE (user_id)
);

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = current_schema()
          AND table_name = 'report_rate_limits'
          AND column_name = 'blocked_until'
    ) AND NOT EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = current_schema()
          AND table_name = 'report_rate_limits'
          AND column_name = 'blocked_until_at'
    ) THEN
        ALTER TABLE report_rate_limits RENAME COLUMN blocked_until TO blocked_until_at;
    END IF;

    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = current_schema()
          AND table_name = 'report_rate_limits'
          AND column_name = 'blocked_until_ts'
    ) AND NOT EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = current_schema()
          AND table_name = 'report_rate_limits'
          AND column_name = 'blocked_until_at'
    ) THEN
        ALTER TABLE report_rate_limits RENAME COLUMN blocked_until_ts TO blocked_until_at;
    END IF;

    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = current_schema()
          AND table_name = 'report_rate_limits'
          AND column_name = 'last_report_ts'
    ) AND NOT EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = current_schema()
          AND table_name = 'report_rate_limits'
          AND column_name = 'last_report_at'
    ) THEN
        ALTER TABLE report_rate_limits RENAME COLUMN last_report_ts TO last_report_at;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = current_schema()
          AND table_name = 'report_rate_limits'
          AND column_name = 'blocked_until_at'
    ) THEN
        ALTER TABLE report_rate_limits ADD COLUMN blocked_until_at BIGINT;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = current_schema()
          AND table_name = 'report_rate_limits'
          AND column_name = 'block_reason'
    ) THEN
        ALTER TABLE report_rate_limits ADD COLUMN block_reason TEXT;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = current_schema()
          AND table_name = 'report_rate_limits'
          AND column_name = 'last_report_at'
    ) THEN
        ALTER TABLE report_rate_limits ADD COLUMN last_report_at BIGINT;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = current_schema()
          AND table_name = 'report_rate_limits'
          AND column_name = 'updated_ts'
    ) THEN
        ALTER TABLE report_rate_limits ADD COLUMN updated_ts BIGINT;
    END IF;
END $$;

UPDATE report_rate_limits
SET updated_ts = COALESCE(updated_ts, created_ts)
WHERE updated_ts IS NULL;

CREATE INDEX IF NOT EXISTS idx_report_rate_limits_user
ON report_rate_limits(user_id);

INSERT INTO schema_migrations (version, name, success, description, applied_ts)
VALUES (
    '20260413000001',
    'align_report_rate_limits_schema_contract',
    TRUE,
    'Align report_rate_limits columns with canonical _at naming and add missing block_reason',
    EXTRACT(EPOCH FROM NOW())::BIGINT * 1000
)
ON CONFLICT (version) DO NOTHING;

-- ===== Merged from: 20260413000002_add_lazy_loaded_members.sql =====

SET TIME ZONE 'UTC';

CREATE TABLE IF NOT EXISTS lazy_loaded_members (
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    member_user_id TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT pk_lazy_loaded_members PRIMARY KEY (user_id, device_id, room_id, member_user_id)
);

CREATE INDEX IF NOT EXISTS idx_lazy_loaded_members_lookup
ON lazy_loaded_members(user_id, device_id, room_id);

INSERT INTO schema_migrations (version, name, success, description, applied_ts)
VALUES (
    '20260413000002',
    'add_lazy_loaded_members',
    TRUE,
    'Persist /sync lazy-loaded member cache by user_id, device_id, room_id, member_user_id',
    EXTRACT(EPOCH FROM NOW())::BIGINT * 1000
)
ON CONFLICT (version) DO NOTHING;

-- ===== Merged from: 20260414000001_add_application_service_webhook_auth.sql =====

ALTER TABLE application_services
ADD COLUMN IF NOT EXISTS api_key TEXT;

ALTER TABLE application_services
ADD COLUMN IF NOT EXISTS config JSONB NOT NULL DEFAULT '{}'::jsonb;

-- ===== Merged from: 20260414000002_hash_access_tokens.sql =====

CREATE EXTENSION IF NOT EXISTS pgcrypto;

ALTER TABLE access_tokens
ADD COLUMN IF NOT EXISTS token_hash TEXT;

UPDATE access_tokens
SET token_hash = replace(
        replace(
            trim(trailing '=' from encode(digest(token, 'sha256'), 'base64')),
            '+',
            '-'
        ),
        '/',
        '_'
    )
WHERE token_hash IS NULL
  AND token IS NOT NULL;

ALTER TABLE access_tokens
ALTER COLUMN token DROP NOT NULL;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'uq_access_tokens_token_hash'
    ) THEN
        ALTER TABLE access_tokens
        ADD CONSTRAINT uq_access_tokens_token_hash UNIQUE (token_hash);
    END IF;
END $$;

ALTER TABLE access_tokens
DROP CONSTRAINT IF EXISTS uq_access_tokens_token;

CREATE INDEX IF NOT EXISTS idx_access_tokens_token_hash
ON access_tokens(token_hash);

UPDATE access_tokens
SET token = NULL
WHERE token IS NOT NULL;

ALTER TABLE access_tokens
ALTER COLUMN token_hash SET NOT NULL;

-- ===== Merged from: 20260418010100_add_users_created_ts_index.sql =====

CREATE INDEX IF NOT EXISTS idx_users_created_ts
ON users(created_ts DESC);
