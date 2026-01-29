-- Create device_keys table
CREATE TABLE IF NOT EXISTS device_keys (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255) NOT NULL,
    display_name VARCHAR(255),
    algorithm VARCHAR(50) NOT NULL,
    key_id VARCHAR(255) NOT NULL,
    public_key TEXT NOT NULL,
    signatures JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT fk_device_keys_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT fk_device_keys_device FOREIGN KEY (device_id) REFERENCES devices(device_id) ON DELETE CASCADE,
    CONSTRAINT uk_device_keys UNIQUE (user_id, device_id, key_id)
);

CREATE INDEX IF NOT EXISTS idx_device_keys_user ON device_keys(user_id);
CREATE INDEX IF NOT EXISTS idx_device_keys_device ON device_keys(user_id, device_id);
CREATE INDEX IF NOT EXISTS idx_device_keys_algorithm ON device_keys(algorithm);