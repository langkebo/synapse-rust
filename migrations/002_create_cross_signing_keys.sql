-- Create cross_signing_keys table
CREATE TABLE IF NOT EXISTS cross_signing_keys (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    key_type VARCHAR(50) NOT NULL,
    public_key TEXT NOT NULL,
    usage JSONB NOT NULL,
    signatures JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT fk_cross_signing_keys_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT uk_cross_signing_keys UNIQUE (user_id, key_type)
);

CREATE INDEX IF NOT EXISTS idx_cross_signing_keys_user ON cross_signing_keys(user_id);
CREATE INDEX IF NOT EXISTS idx_cross_signing_keys_type ON cross_signing_keys(key_type);