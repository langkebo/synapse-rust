CREATE TABLE IF NOT EXISTS key_rotation_pending (
    room_id TEXT NOT NULL,
    reason TEXT NOT NULL,
    triggered_by_user_id TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    PRIMARY KEY (room_id, triggered_by_user_id)
);

CREATE INDEX IF NOT EXISTS idx_key_rotation_pending_room
ON key_rotation_pending(room_id);

CREATE TABLE IF NOT EXISTS key_rotation_state (
    user_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    is_rotated BOOLEAN NOT NULL DEFAULT FALSE,
    rotated_at TIMESTAMPTZ,
    PRIMARY KEY (user_id, room_id)
);

CREATE INDEX IF NOT EXISTS idx_key_rotation_state_user
ON key_rotation_state(user_id);

CREATE TABLE IF NOT EXISTS megolm_key_shares (
    room_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    share_reason TEXT NOT NULL,
    shared_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (room_id, session_id)
);

CREATE INDEX IF NOT EXISTS idx_megolm_key_shares_room
ON megolm_key_shares(room_id);
