-- Best-effort rollback only: original plaintext access tokens are intentionally discarded
-- by the forward migration and cannot be reconstructed.

UPDATE access_tokens
SET token = COALESCE(token, token_hash)
WHERE token IS NULL;

DROP INDEX IF EXISTS idx_access_tokens_token_hash;

ALTER TABLE access_tokens
DROP CONSTRAINT IF EXISTS uq_access_tokens_token_hash;

ALTER TABLE access_tokens
ALTER COLUMN token SET NOT NULL;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'uq_access_tokens_token'
    ) THEN
        ALTER TABLE access_tokens
        ADD CONSTRAINT uq_access_tokens_token UNIQUE (token);
    END IF;
END $$;

ALTER TABLE access_tokens
DROP COLUMN IF EXISTS token_hash;
