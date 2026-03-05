-- Migration: Fix medium-priority schema gaps from architecture quality review round 2
-- Version: 20260304000002
-- Date: 2026-03-04

-- ============================================================================
-- 1. room_summary_members 补齐 last_active_ts
-- ============================================================================
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_name = 'room_summary_members'
    ) THEN
        ALTER TABLE room_summary_members
        ADD COLUMN IF NOT EXISTS last_active_ts BIGINT;
        ALTER TABLE room_summary_members
        ADD COLUMN IF NOT EXISTS created_ts BIGINT;

        UPDATE room_summary_members
        SET created_ts = COALESCE(created_ts, updated_ts, EXTRACT(EPOCH FROM NOW())::BIGINT * 1000)
        WHERE created_ts IS NULL;
    END IF;
END $$;

-- ============================================================================
-- 2. room_summary_stats 补齐 total_media
-- ============================================================================
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_name = 'room_summary_stats'
    ) THEN
        ALTER TABLE room_summary_stats
        ADD COLUMN IF NOT EXISTS total_media BIGINT NOT NULL DEFAULT 0;

        ALTER TABLE room_summary_stats
        ADD COLUMN IF NOT EXISTS total_state_events BIGINT NOT NULL DEFAULT 0;

        ALTER TABLE room_summary_stats
        ADD COLUMN IF NOT EXISTS storage_size BIGINT NOT NULL DEFAULT 0;

        ALTER TABLE room_summary_stats
        ADD COLUMN IF NOT EXISTS last_updated_ts BIGINT;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_name = 'room_summary_stats'
    ) THEN
        IF EXISTS (
            SELECT 1 FROM information_schema.columns
            WHERE table_name = 'room_summary_stats' AND column_name = 'updated_ts'
        ) THEN
            UPDATE room_summary_stats
            SET last_updated_ts = COALESCE(last_updated_ts, updated_ts, EXTRACT(EPOCH FROM NOW())::BIGINT * 1000)
            WHERE last_updated_ts IS NULL;
        ELSE
            UPDATE room_summary_stats
            SET last_updated_ts = COALESCE(last_updated_ts, EXTRACT(EPOCH FROM NOW())::BIGINT * 1000)
            WHERE last_updated_ts IS NULL;
        END IF;
    END IF;
END $$;

-- ============================================================================
-- 3. registration_tokens 补齐 token_type
-- ============================================================================
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_name = 'registration_tokens'
    ) THEN
        ALTER TABLE registration_tokens
        ADD COLUMN IF NOT EXISTS token_type TEXT;
        ALTER TABLE registration_tokens
        ADD COLUMN IF NOT EXISTS allowed_user_ids JSONB;
        ALTER TABLE registration_tokens
        ADD COLUMN IF NOT EXISTS display_name TEXT;
        ALTER TABLE registration_tokens
        ADD COLUMN IF NOT EXISTS email TEXT;
        ALTER TABLE registration_tokens
        ADD COLUMN IF NOT EXISTS is_used BOOLEAN;
        ALTER TABLE registration_tokens
        ADD COLUMN IF NOT EXISTS last_used_ts BIGINT;

        UPDATE registration_tokens
        SET token_type = COALESCE(token_type, 'single_use')
        WHERE token_type IS NULL;

        UPDATE registration_tokens
        SET is_used = COALESCE(is_used, FALSE)
        WHERE is_used IS NULL;

        IF EXISTS (
            SELECT 1 FROM information_schema.columns
            WHERE table_name = 'registration_tokens'
              AND column_name = 'allowed_user_ids'
              AND data_type = 'jsonb'
        ) THEN
            ALTER TABLE registration_tokens
            DROP COLUMN allowed_user_ids;
            ALTER TABLE registration_tokens
            ADD COLUMN allowed_user_ids TEXT[] DEFAULT '{}'::TEXT[];
        END IF;

        ALTER TABLE registration_tokens
        ALTER COLUMN token_type SET DEFAULT 'single_use';

        ALTER TABLE registration_tokens
        ALTER COLUMN token_type SET NOT NULL;

        UPDATE registration_tokens
        SET allowed_user_ids = COALESCE(allowed_user_ids, '{}'::TEXT[])
        WHERE allowed_user_ids IS NULL;

        ALTER TABLE registration_tokens
        ALTER COLUMN allowed_user_ids SET DEFAULT '{}'::TEXT[];

        ALTER TABLE registration_tokens
        ALTER COLUMN is_used SET DEFAULT FALSE;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_name = 'registration_tokens'
    ) THEN
        CREATE INDEX IF NOT EXISTS idx_registration_tokens_type
        ON registration_tokens(token_type);
    END IF;
END $$;

-- ============================================================================
-- 4. application_services 兼容旧写入路径（sender/as_id默认值）
-- ============================================================================
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_name = 'application_services'
    ) THEN
        ALTER TABLE application_services
        ALTER COLUMN sender SET DEFAULT '';
        ALTER TABLE application_services
        ALTER COLUMN as_id SET DEFAULT ('as_' || SUBSTRING(MD5(RANDOM()::TEXT || CLOCK_TIMESTAMP()::TEXT), 1, 16));
    END IF;
END $$;
