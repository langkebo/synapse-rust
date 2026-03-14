-- Invite Blocklist and Allowlist - MSC4380
-- Allows room admins to control who can be invited to a room
-- Following project field naming standards

-- Blocklist: Users that CANNOT be invited
CREATE TABLE IF NOT EXISTS room_invite_blocklist (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    UNIQUE(room_id, user_id)
);

-- Allowlist: Users that CAN ONLY be invited (when set, only these users can be invited)
CREATE TABLE IF NOT EXISTS room_invite_allowlist (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    UNIQUE(room_id, user_id)
);

-- Index for fast lookups
CREATE INDEX IF NOT EXISTS idx_room_invite_blocklist_room 
    ON room_invite_blocklist(room_id);

CREATE INDEX IF NOT EXISTS idx_room_invite_allowlist_room 
    ON room_invite_allowlist(room_id);

-- Add comments
COMMENT ON TABLE room_invite_blocklist IS 'Room Invite Blocklist - MSC4380';
COMMENT ON TABLE room_invite_allowlist IS 'Room Invite Allowlist - MSC4380';
