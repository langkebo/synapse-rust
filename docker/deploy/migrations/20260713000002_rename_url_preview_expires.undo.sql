-- Undo: revert url_preview_cache.expires_at back to expires_ts

DROP INDEX IF EXISTS idx_url_preview_cache_expires_at;
CREATE INDEX IF NOT EXISTS idx_url_preview_cache_expires
    ON url_preview_cache(expires_ts) WHERE expires_ts IS NOT NULL;

DO $$ BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'url_preview_cache'
          AND column_name = 'expires_at'
    ) THEN
        ALTER TABLE url_preview_cache RENAME COLUMN expires_at TO expires_ts;
    END IF;
END $$;
