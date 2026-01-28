-- Create event_signatures table
CREATE TABLE IF NOT EXISTS event_signatures (
    id BIGSERIAL PRIMARY KEY,
    event_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255) NOT NULL,
    signature TEXT NOT NULL,
    key_id VARCHAR(255) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT fk_event_signatures_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT fk_event_signatures_device FOREIGN KEY (device_id) REFERENCES devices(device_id) ON DELETE CASCADE,
    CONSTRAINT uk_event_signatures UNIQUE (event_id, user_id, device_id, key_id)
);

CREATE INDEX IF NOT EXISTS idx_event_signatures_event ON event_signatures(event_id);
CREATE INDEX IF NOT EXISTS idx_event_signatures_user ON event_signatures(user_id);
CREATE INDEX IF NOT EXISTS idx_event_signatures_device ON event_signatures(user_id, device_id);