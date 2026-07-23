-- Rename url_preview_cache.expires_ts to expires_at (banned-field-name conformance).
-- OPT-020, audit 03 §6 P2.

DO $$ BEGIN
  IF EXISTS (SELECT 1 FROM information_schema.columns
             WHERE table_name='url_preview_cache' AND column_name='expires_ts') THEN
    ALTER TABLE url_preview_cache RENAME COLUMN expires_ts TO expires_at;
  END IF;
END $$;

ALTER INDEX IF EXISTS idx_url_preview_cache_expires RENAME TO idx_url_preview_cache_expires_at;
