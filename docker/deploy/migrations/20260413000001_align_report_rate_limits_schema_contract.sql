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
        WHERE table_schema = 'public'
          AND table_name = 'report_rate_limits'
          AND column_name = 'blocked_until'
    ) AND NOT EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = 'report_rate_limits'
          AND column_name = 'blocked_until_at'
    ) THEN
        ALTER TABLE report_rate_limits RENAME COLUMN blocked_until TO blocked_until_at;
    END IF;

    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = 'report_rate_limits'
          AND column_name = 'blocked_until_ts'
    ) AND NOT EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = 'report_rate_limits'
          AND column_name = 'blocked_until_at'
    ) THEN
        ALTER TABLE report_rate_limits RENAME COLUMN blocked_until_ts TO blocked_until_at;
    END IF;

    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = 'report_rate_limits'
          AND column_name = 'last_report_ts'
    ) AND NOT EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = 'report_rate_limits'
          AND column_name = 'last_report_at'
    ) THEN
        ALTER TABLE report_rate_limits RENAME COLUMN last_report_ts TO last_report_at;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = 'report_rate_limits'
          AND column_name = 'blocked_until_at'
    ) THEN
        ALTER TABLE report_rate_limits ADD COLUMN blocked_until_at BIGINT;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = 'report_rate_limits'
          AND column_name = 'block_reason'
    ) THEN
        ALTER TABLE report_rate_limits ADD COLUMN block_reason TEXT;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = 'report_rate_limits'
          AND column_name = 'last_report_at'
    ) THEN
        ALTER TABLE report_rate_limits ADD COLUMN last_report_at BIGINT;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = 'public'
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
