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
