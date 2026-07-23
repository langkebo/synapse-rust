-- Rename url_preview_cache.expires_ts to expires_at (forbidden field name fix).
-- v10: expires_at BIGINT  ←  old: expires_ts BIGINT

DO $$ BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'url_preview_cache'
          AND column_name = 'expires_ts'
    ) THEN
        ALTER TABLE url_preview_cache RENAME COLUMN expires_ts TO expires_at;
    END IF;
END $$;

-- Rebuild index with new column name
DROP INDEX IF EXISTS idx_url_preview_cache_expires;
CREATE INDEX IF NOT EXISTS idx_url_preview_cache_expires_at
    ON url_preview_cache(expires_at) WHERE expires_at IS NOT NULL;
