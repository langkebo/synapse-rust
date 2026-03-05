-- Migration: Finalize field naming standards and NOT NULL audit columns
-- Version: 20260304000004
-- Date: 2026-03-04

-- ============================================================================
-- 1. registration_captcha 对齐 *_ts 字段命名
-- ============================================================================
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_name = 'registration_captcha'
    ) THEN
        ALTER TABLE registration_captcha
        ADD COLUMN IF NOT EXISTS created_ts BIGINT;
        ALTER TABLE registration_captcha
        ADD COLUMN IF NOT EXISTS expires_ts BIGINT;
        ALTER TABLE registration_captcha
        ADD COLUMN IF NOT EXISTS used_ts BIGINT;
        ALTER TABLE registration_captcha
        ADD COLUMN IF NOT EXISTS verified_ts BIGINT;

        IF EXISTS (
            SELECT 1 FROM information_schema.columns
            WHERE table_name = 'registration_captcha' AND column_name = 'created_at'
        ) THEN
            UPDATE registration_captcha
            SET created_ts = COALESCE(created_ts, created_at)
            WHERE created_ts IS NULL;
        END IF;

        IF EXISTS (
            SELECT 1 FROM information_schema.columns
            WHERE table_name = 'registration_captcha' AND column_name = 'expires_at'
        ) THEN
            UPDATE registration_captcha
            SET expires_ts = COALESCE(expires_ts, expires_at)
            WHERE expires_ts IS NULL;
        END IF;

        IF EXISTS (
            SELECT 1 FROM information_schema.columns
            WHERE table_name = 'registration_captcha' AND column_name = 'used_at'
        ) THEN
            UPDATE registration_captcha
            SET used_ts = COALESCE(used_ts, used_at)
            WHERE used_ts IS NULL;
        END IF;

        IF EXISTS (
            SELECT 1 FROM information_schema.columns
            WHERE table_name = 'registration_captcha' AND column_name = 'verified_at'
        ) THEN
            UPDATE registration_captcha
            SET verified_ts = COALESCE(verified_ts, verified_at)
            WHERE verified_ts IS NULL;
        END IF;

        UPDATE registration_captcha
        SET created_ts = COALESCE(created_ts, EXTRACT(EPOCH FROM NOW())::BIGINT * 1000),
            expires_ts = COALESCE(expires_ts, EXTRACT(EPOCH FROM NOW())::BIGINT * 1000 + 600000)
        WHERE created_ts IS NULL OR expires_ts IS NULL;

        ALTER TABLE registration_captcha
        ALTER COLUMN created_ts SET NOT NULL;
        ALTER TABLE registration_captcha
        ALTER COLUMN expires_ts SET NOT NULL;

        ALTER TABLE registration_captcha
        ALTER COLUMN created_ts SET DEFAULT (EXTRACT(EPOCH FROM NOW())::BIGINT * 1000);

        ALTER TABLE registration_captcha
        ALTER COLUMN expires_ts SET DEFAULT (EXTRACT(EPOCH FROM NOW())::BIGINT * 1000 + 600000);

        ALTER TABLE registration_captcha DROP COLUMN IF EXISTS created_at;
        ALTER TABLE registration_captcha DROP COLUMN IF EXISTS expires_at;
        ALTER TABLE registration_captcha DROP COLUMN IF EXISTS used_at;
        ALTER TABLE registration_captcha DROP COLUMN IF EXISTS verified_at;
    END IF;
END $$;

-- ============================================================================
-- 2. 关键汇总表审计字段补齐 NOT NULL
-- ============================================================================
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_name = 'room_summary_state'
    ) THEN
        UPDATE room_summary_state
        SET created_ts = COALESCE(created_ts, updated_ts, EXTRACT(EPOCH FROM NOW())::BIGINT * 1000),
            updated_ts = COALESCE(updated_ts, created_ts, EXTRACT(EPOCH FROM NOW())::BIGINT * 1000)
        WHERE created_ts IS NULL OR updated_ts IS NULL;

        ALTER TABLE room_summary_state ALTER COLUMN created_ts SET NOT NULL;
        ALTER TABLE room_summary_state ALTER COLUMN updated_ts SET NOT NULL;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_name = 'room_summary_stats'
    ) THEN
        UPDATE room_summary_stats
        SET created_ts = COALESCE(created_ts, updated_ts, last_updated_ts, EXTRACT(EPOCH FROM NOW())::BIGINT * 1000),
            updated_ts = COALESCE(updated_ts, created_ts, last_updated_ts, EXTRACT(EPOCH FROM NOW())::BIGINT * 1000)
        WHERE created_ts IS NULL OR updated_ts IS NULL;

        ALTER TABLE room_summary_stats ALTER COLUMN created_ts SET NOT NULL;
        ALTER TABLE room_summary_stats ALTER COLUMN updated_ts SET NOT NULL;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_name = 'room_summary_update_queue'
    ) THEN
        UPDATE room_summary_update_queue
        SET created_ts = COALESCE(created_ts, updated_ts, EXTRACT(EPOCH FROM NOW())::BIGINT * 1000),
            updated_ts = COALESCE(updated_ts, created_ts, EXTRACT(EPOCH FROM NOW())::BIGINT * 1000)
        WHERE created_ts IS NULL OR updated_ts IS NULL;

        ALTER TABLE room_summary_update_queue ALTER COLUMN created_ts SET NOT NULL;
        ALTER TABLE room_summary_update_queue ALTER COLUMN updated_ts SET NOT NULL;
    END IF;
END $$;
