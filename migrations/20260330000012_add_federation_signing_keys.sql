CREATE TABLE IF NOT EXISTS federation_signing_keys (
    server_name TEXT NOT NULL,
    key_id TEXT NOT NULL,
    secret_key TEXT NOT NULL,
    public_key TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    expires_at BIGINT NOT NULL,
    key_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    ts_added_ms BIGINT NOT NULL,
    ts_valid_until_ms BIGINT NOT NULL,
    PRIMARY KEY (server_name, key_id)
);

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = 'federation_signing_keys'
          AND column_name = 'created_at'
    ) AND NOT EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = 'federation_signing_keys'
          AND column_name = 'created_ts'
    ) THEN
        ALTER TABLE federation_signing_keys RENAME COLUMN created_at TO created_ts;
    END IF;
END $$;

ALTER TABLE federation_signing_keys ADD COLUMN IF NOT EXISTS created_ts BIGINT;
ALTER TABLE federation_signing_keys ADD COLUMN IF NOT EXISTS expires_at BIGINT;
ALTER TABLE federation_signing_keys ADD COLUMN IF NOT EXISTS key_json JSONB DEFAULT '{}'::jsonb;
ALTER TABLE federation_signing_keys ADD COLUMN IF NOT EXISTS ts_added_ms BIGINT;
ALTER TABLE federation_signing_keys ADD COLUMN IF NOT EXISTS ts_valid_until_ms BIGINT;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = 'federation_signing_keys'
          AND column_name = 'key_json'
          AND data_type <> 'jsonb'
    ) THEN
        ALTER TABLE federation_signing_keys
        ALTER COLUMN key_json TYPE JSONB
        USING COALESCE(NULLIF(BTRIM(key_json::text, '"'), ''), '{}')::jsonb;
    END IF;
END $$;

UPDATE federation_signing_keys
SET created_ts = COALESCE(created_ts, ts_added_ms, (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT),
    expires_at = COALESCE(expires_at, ts_valid_until_ms, 0),
    key_json = COALESCE(key_json, '{}'::jsonb),
    ts_added_ms = COALESCE(ts_added_ms, created_ts, (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT),
    ts_valid_until_ms = COALESCE(ts_valid_until_ms, expires_at, 0);

ALTER TABLE federation_signing_keys ALTER COLUMN created_ts SET NOT NULL;
ALTER TABLE federation_signing_keys ALTER COLUMN expires_at SET NOT NULL;
ALTER TABLE federation_signing_keys ALTER COLUMN key_json SET NOT NULL;
ALTER TABLE federation_signing_keys ALTER COLUMN key_json SET DEFAULT '{}'::jsonb;
ALTER TABLE federation_signing_keys ALTER COLUMN ts_added_ms SET NOT NULL;
ALTER TABLE federation_signing_keys ALTER COLUMN ts_valid_until_ms SET NOT NULL;

CREATE INDEX IF NOT EXISTS idx_federation_signing_keys_server_created
ON federation_signing_keys(server_name, created_ts DESC);

CREATE INDEX IF NOT EXISTS idx_federation_signing_keys_key_id
ON federation_signing_keys(key_id);
