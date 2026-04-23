ALTER TABLE access_tokens ALTER COLUMN token SET NOT NULL;

ALTER TABLE access_tokens DROP CONSTRAINT IF EXISTS uq_access_tokens_token_hash;
ALTER TABLE refresh_tokens DROP CONSTRAINT IF EXISTS uq_refresh_tokens_token_hash;
ALTER TABLE token_blacklist DROP CONSTRAINT IF EXISTS uq_token_blacklist_token_hash;
