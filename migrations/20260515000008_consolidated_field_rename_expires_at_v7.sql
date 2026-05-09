-- ============================================================================
-- Forward Script: 20260515000008_consolidated_field_rename_expires_at_v7.sql
-- Description: Rename all `expires_ts` columns to `expires_at` to comply with
--              project field naming standards.
--              - expires_ts → expires_at (可选时间戳应使用 _at 后缀)
--              - verification_expires_ts → verification_expires_at
-- Tables affected: saml_sessions, access_tokens, refresh_tokens,
--                  registration_tokens, invite_tokens, rendezvous_session,
--                  users (user_threepids), qr_login_sessions, registration_captcha
-- Created: 2026-05-09
-- Risk: HIGH — Column renames across multiple tables. Must be tested in staging.
-- Rollback: Use corresponding .undo.sql
-- ============================================================================

SET TIME ZONE 'UTC';

-- ============================================================================
-- 1. saml_sessions: expires_ts → expires_at
-- ============================================================================
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'saml_sessions'
          AND column_name = 'expires_ts'
    ) THEN
        ALTER TABLE saml_sessions RENAME COLUMN expires_ts TO expires_at;
    END IF;
END $$;

-- ============================================================================
-- 2. access_tokens: expires_ts → expires_at
-- ============================================================================
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'access_tokens'
          AND column_name = 'expires_ts'
    ) THEN
        ALTER TABLE access_tokens RENAME COLUMN expires_ts TO expires_at;
    END IF;
END $$;

-- ============================================================================
-- 3. refresh_tokens: expires_ts → expires_at
-- ============================================================================
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'refresh_tokens'
          AND column_name = 'expires_ts'
    ) THEN
        ALTER TABLE refresh_tokens RENAME COLUMN expires_ts TO expires_at;
    END IF;
END $$;

-- ============================================================================
-- 4. registration_tokens: expires_ts → expires_at
-- ============================================================================
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'registration_tokens'
          AND column_name = 'expires_ts'
    ) THEN
        ALTER TABLE registration_tokens RENAME COLUMN expires_ts TO expires_at;
    END IF;
END $$;

-- ============================================================================
-- 5. invite_tokens: expires_ts → expires_at
-- ============================================================================
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'invite_tokens'
          AND column_name = 'expires_ts'
    ) THEN
        ALTER TABLE invite_tokens RENAME COLUMN expires_ts TO expires_at;
    END IF;
END $$;

-- ============================================================================
-- 6. rendezvous_session: expires_ts → expires_at
-- ============================================================================
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'rendezvous_session'
          AND column_name = 'expires_ts'
    ) THEN
        ALTER TABLE rendezvous_session RENAME COLUMN expires_ts TO expires_at;
    END IF;
END $$;

-- ============================================================================
-- 7. user_threepids: verification_expires_ts → verification_expires_at
-- ============================================================================
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'user_threepids'
          AND column_name = 'verification_expires_ts'
    ) THEN
        ALTER TABLE user_threepids RENAME COLUMN verification_expires_ts TO verification_expires_at;
    END IF;
END $$;

-- ============================================================================
-- 8. qr_login_sessions: expires_ts → expires_at (extension table)
-- ============================================================================
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_schema = 'public' AND table_name = 'qr_login_sessions'
    ) AND EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'qr_login_sessions'
          AND column_name = 'expires_ts'
    ) THEN
        ALTER TABLE qr_login_sessions RENAME COLUMN expires_ts TO expires_at;
    END IF;
END $$;

-- ============================================================================
-- 9. registration_captcha: expires_ts → expires_at
-- ============================================================================
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'registration_captcha'
          AND column_name = 'expires_ts'
    ) THEN
        ALTER TABLE registration_captcha RENAME COLUMN expires_ts TO expires_at;
    END IF;
END $$;

-- ============================================================================
-- Migration record
-- ============================================================================
INSERT INTO schema_migrations (version, name, success, description, applied_ts)
VALUES (
    '20260515000008',
    'consolidated_field_rename_expires_at_v7',
    TRUE,
    'Rename all expires_ts columns to expires_at across 9 tables per field naming standards',
    (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT
)
ON CONFLICT (version) DO NOTHING;