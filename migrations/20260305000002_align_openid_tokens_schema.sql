DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM information_schema.tables
        WHERE table_schema = 'public' AND table_name = 'openid_tokens'
    ) THEN
        IF NOT EXISTS (
            SELECT 1
            FROM information_schema.columns
            WHERE table_schema = 'public' AND table_name = 'openid_tokens' AND column_name = 'device_id'
        ) THEN
            ALTER TABLE openid_tokens ADD COLUMN device_id TEXT;
        END IF;

        IF NOT EXISTS (
            SELECT 1
            FROM information_schema.columns
            WHERE table_schema = 'public' AND table_name = 'openid_tokens' AND column_name = 'expires_ts'
        ) THEN
            ALTER TABLE openid_tokens ADD COLUMN expires_ts BIGINT;
        END IF;

        IF EXISTS (
            SELECT 1
            FROM information_schema.columns
            WHERE table_schema = 'public' AND table_name = 'openid_tokens' AND column_name = 'expires_at'
        ) THEN
            UPDATE openid_tokens
            SET expires_ts = expires_at
            WHERE expires_ts IS NULL;
        END IF;

        IF NOT EXISTS (
            SELECT 1
            FROM information_schema.columns
            WHERE table_schema = 'public' AND table_name = 'openid_tokens' AND column_name = 'is_valid'
        ) THEN
            ALTER TABLE openid_tokens ADD COLUMN is_valid BOOLEAN NOT NULL DEFAULT TRUE;
        END IF;

        ALTER TABLE openid_tokens ALTER COLUMN expires_ts SET NOT NULL;
    END IF;
END $$;
