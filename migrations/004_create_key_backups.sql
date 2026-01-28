-- Create key_backups table
CREATE TABLE IF NOT EXISTS key_backups (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    version VARCHAR(255) NOT NULL,
    algorithm VARCHAR(50) NOT NULL,
    auth_data JSONB NOT NULL,
    encrypted_data JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT fk_key_backups_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT uk_key_backups UNIQUE (user_id, version)
);

CREATE INDEX IF NOT EXISTS idx_key_backups_user ON key_backups(user_id);
CREATE INDEX IF NOT EXISTS idx_key_backups_version ON key_backups(user_id, version);