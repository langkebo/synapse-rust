ALTER TABLE access_tokens ALTER COLUMN token DROP NOT NULL;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'uq_access_tokens_token_hash'
    ) THEN
        ALTER TABLE access_tokens ADD CONSTRAINT uq_access_tokens_token_hash UNIQUE (token_hash);
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'uq_refresh_tokens_token_hash'
    ) THEN
        ALTER TABLE refresh_tokens ADD CONSTRAINT uq_refresh_tokens_token_hash UNIQUE (token_hash);
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'uq_token_blacklist_token_hash'
    ) THEN
        ALTER TABLE token_blacklist ADD CONSTRAINT uq_token_blacklist_token_hash UNIQUE (token_hash);
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_access_tokens_token_hash ON access_tokens(token_hash);
CREATE INDEX IF NOT EXISTS idx_access_tokens_valid ON access_tokens(is_revoked) WHERE is_revoked = FALSE;
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_user_id ON refresh_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_revoked ON refresh_tokens(is_revoked);
CREATE INDEX IF NOT EXISTS idx_token_blacklist_hash ON token_blacklist(token_hash);
