-- Beacon Info Table (MSC3672)
-- Stores beacon metadata and live status
CREATE TABLE IF NOT EXISTS beacon_info (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    event_id TEXT NOT NULL UNIQUE,
    state_key TEXT NOT NULL,
    sender TEXT NOT NULL,
    description TEXT,
    timeout BIGINT NOT NULL DEFAULT 3600000,
    is_live BOOLEAN NOT NULL DEFAULT true,
    asset_type TEXT NOT NULL DEFAULT 'm.self',
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    expires_ts BIGINT,
    
    CONSTRAINT beacon_info_room_id_fkey 
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    CONSTRAINT beacon_info_sender_fkey 
        FOREIGN KEY (sender) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_beacon_info_room ON beacon_info(room_id);
CREATE INDEX IF NOT EXISTS idx_beacon_info_sender ON beacon_info(sender);
CREATE INDEX IF NOT EXISTS idx_beacon_info_state_key ON beacon_info(state_key);
CREATE INDEX IF NOT EXISTS idx_beacon_info_live ON beacon_info(is_live);
CREATE INDEX IF NOT EXISTS idx_beacon_info_expires ON beacon_info(expires_ts);

COMMENT ON TABLE beacon_info IS 'Beacon info events for MSC3672: Live Location Sharing';

-- Beacon Locations Table
-- Stores individual location updates for each beacon
CREATE TABLE IF NOT EXISTS beacon_locations (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    event_id TEXT NOT NULL UNIQUE,
    beacon_info_id TEXT NOT NULL,
    sender TEXT NOT NULL,
    uri TEXT NOT NULL,
    description TEXT,
    timestamp BIGINT NOT NULL,
    accuracy BIGINT,
    created_ts BIGINT NOT NULL,
    
    CONSTRAINT beacon_locations_room_id_fkey 
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    CONSTRAINT beacon_locations_beacon_info_fkey 
        FOREIGN KEY (beacon_info_id) REFERENCES beacon_info(event_id) ON DELETE CASCADE,
    CONSTRAINT beacon_locations_sender_fkey 
        FOREIGN KEY (sender) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_beacon_locations_room ON beacon_locations(room_id);
CREATE INDEX IF NOT EXISTS idx_beacon_locations_beacon_info ON beacon_locations(beacon_info_id);
CREATE INDEX IF NOT EXISTS idx_beacon_locations_sender ON beacon_locations(sender);
CREATE INDEX IF NOT EXISTS idx_beacon_locations_timestamp ON beacon_locations(timestamp DESC);

COMMENT ON TABLE beacon_locations IS 'Beacon location events for MSC3672: Live Location Sharing';
