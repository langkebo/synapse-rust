DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_name = 'events' AND column_name = 'sender'
    ) AND EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_name = 'events' AND column_name = 'user_id'
    ) THEN
        UPDATE events
        SET user_id = sender
        WHERE user_id IS NULL AND sender IS NOT NULL;
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_events_user_id ON events(user_id);

CREATE TABLE IF NOT EXISTS filters (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    filter_id TEXT NOT NULL,
    content JSONB NOT NULL,
    created_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    UNIQUE(user_id, filter_id)
);

CREATE INDEX IF NOT EXISTS idx_filters_user_id ON filters(user_id);

CREATE TABLE IF NOT EXISTS openid_tokens (
    id BIGSERIAL PRIMARY KEY,
    token TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL,
    device_id TEXT,
    created_ts BIGINT NOT NULL,
    expires_ts BIGINT NOT NULL,
    is_valid BOOLEAN DEFAULT TRUE
);

CREATE INDEX IF NOT EXISTS idx_openid_tokens_user_id ON openid_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_openid_tokens_token ON openid_tokens(token);
