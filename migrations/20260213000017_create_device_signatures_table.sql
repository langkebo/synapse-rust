CREATE TABLE IF NOT EXISTS device_signatures (
    id SERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL DEFAULT '',
    signing_key_id TEXT NOT NULL,
    target_user_id TEXT NOT NULL,
    target_device_id TEXT NOT NULL DEFAULT '',
    target_key_id TEXT NOT NULL,
    signature TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(user_id, signing_key_id, target_user_id, target_key_id)
);

CREATE INDEX IF NOT EXISTS idx_device_signatures_user ON device_signatures(user_id);
CREATE INDEX IF NOT EXISTS idx_device_signatures_target_device ON device_signatures(user_id, target_device_id);
CREATE INDEX IF NOT EXISTS idx_device_signatures_target_key ON device_signatures(user_id, target_key_id);
