-- Create to_device messages table
CREATE TABLE IF NOT EXISTS to_device_messages (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255) NOT NULL,
    message_type VARCHAR(255) NOT NULL,
    content JSONB NOT NULL,
    created_ts BIGINT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_to_device_messages_user_device ON to_device_messages(user_id, device_id);
CREATE INDEX IF NOT EXISTS idx_to_device_messages_created ON to_device_messages(created_ts);
