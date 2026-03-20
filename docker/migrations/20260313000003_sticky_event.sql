-- Sticky Event Storage - MSC4354
-- Stores sticky event metadata for rooms
-- Following project field naming standards

CREATE TABLE IF NOT EXISTS room_sticky_events (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    sticky BOOLEAN NOT NULL DEFAULT true,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    UNIQUE(room_id, user_id, event_type)
);

-- Index for fast lookups
CREATE INDEX IF NOT EXISTS idx_room_sticky_events_room_user 
    ON room_sticky_events(room_id, user_id);

CREATE INDEX IF NOT EXISTS idx_room_sticky_events_user 
    ON room_sticky_events(user_id) WHERE sticky = true;

-- Add comments
COMMENT ON TABLE room_sticky_events IS 'Room Sticky Events - MSC4354';
COMMENT ON COLUMN room_sticky_events.room_id IS 'Room identifier';
COMMENT ON COLUMN room_sticky_events.user_id IS 'User who set the sticky event';
COMMENT ON COLUMN room_sticky_events.event_id IS 'The sticky event ID';
COMMENT ON COLUMN room_sticky_events.event_type IS 'Type of the sticky event';
COMMENT ON COLUMN room_sticky_events.sticky IS 'Whether the event is still sticky';
