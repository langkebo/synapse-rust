CREATE EXTENSION IF NOT EXISTS pgcrypto;

ALTER TABLE access_tokens
ADD COLUMN IF NOT EXISTS token_hash TEXT;

UPDATE access_tokens
SET token_hash = replace(
        replace(
            trim(trailing '=' from encode(digest(token, 'sha256'), 'base64')),
            '+',
            '-'
        ),
        '/',
        '_'
    )
WHERE token_hash IS NULL
  AND token IS NOT NULL;

ALTER TABLE access_tokens
ALTER COLUMN token DROP NOT NULL;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'uq_access_tokens_token_hash'
    ) THEN
        ALTER TABLE access_tokens
        ADD CONSTRAINT uq_access_tokens_token_hash UNIQUE (token_hash);
    END IF;
END $$;

ALTER TABLE access_tokens
DROP CONSTRAINT IF EXISTS uq_access_tokens_token;

CREATE INDEX IF NOT EXISTS idx_access_tokens_token_hash
ON access_tokens(token_hash);

UPDATE access_tokens
SET token = NULL
WHERE token IS NOT NULL;

ALTER TABLE access_tokens
ALTER COLUMN token_hash SET NOT NULL;
