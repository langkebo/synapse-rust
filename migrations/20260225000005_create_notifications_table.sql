-- Create notifications table
-- This table stores user notifications

CREATE TABLE IF NOT EXISTS notifications (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255),
    event_id VARCHAR(255),
    event_type VARCHAR(100),
    event_content JSONB,
    sender VARCHAR(255),
    ts BIGINT NOT NULL DEFAULT (EXTRACT(epoch FROM now()) * 1000)::bigint,
    read BOOLEAN DEFAULT FALSE,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Create indexes for efficient querying
CREATE INDEX IF NOT EXISTS idx_notifications_user_id ON notifications(user_id);
CREATE INDEX IF NOT EXISTS idx_notifications_room_id ON notifications(room_id);
CREATE INDEX IF NOT EXISTS idx_notifications_ts ON notifications(ts DESC);
CREATE INDEX IF NOT EXISTS idx_notifications_read ON notifications(read) WHERE read = FALSE;
CREATE INDEX IF NOT EXISTS idx_notifications_user_read ON notifications(user_id, read);

-- Add foreign key constraints
ALTER TABLE notifications
    ADD CONSTRAINT fk_notifications_user_id
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;

ALTER TABLE notifications
    ADD CONSTRAINT fk_notifications_room_id
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;

-- Add comments
COMMENT ON TABLE notifications IS 'Stores user notifications for events';
COMMENT ON COLUMN notifications.user_id IS 'The user who received the notification';
COMMENT ON COLUMN notifications.room_id IS 'The room where the event occurred';
COMMENT ON COLUMN notifications.event_id IS 'The event ID that triggered the notification';
COMMENT ON COLUMN notifications.event_type IS 'The type of event (e.g., m.room.message)';
COMMENT ON COLUMN notifications.event_content IS 'The content of the event';
COMMENT ON COLUMN notifications.sender IS 'The user who sent the event';
COMMENT ON COLUMN notifications.ts IS 'Timestamp of the notification in milliseconds';
COMMENT ON COLUMN notifications.read IS 'Whether the notification has been read';

DO $$
BEGIN
    RAISE NOTICE 'Notifications table created successfully';
END $$;
