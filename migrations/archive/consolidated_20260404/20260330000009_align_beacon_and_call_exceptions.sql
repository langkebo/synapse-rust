-- 1. beacon_info
CREATE TABLE IF NOT EXISTS beacon_info (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    event_id TEXT NOT NULL UNIQUE,
    state_key TEXT NOT NULL,
    sender TEXT NOT NULL,
    description TEXT,
    timeout BIGINT NOT NULL,
    is_live BOOLEAN NOT NULL DEFAULT TRUE,
    asset_type TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    expires_at BIGINT
);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_beacon_info_room_active ON beacon_info(room_id, is_live) WHERE is_live = TRUE;
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_beacon_info_room_state ON beacon_info(room_id, state_key, created_ts DESC);
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_beacon_info_expires ON beacon_info(expires_at) WHERE expires_at IS NOT NULL;

-- 2. beacon_locations
CREATE TABLE IF NOT EXISTS beacon_locations (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    beacon_info_id TEXT NOT NULL,
    sender TEXT NOT NULL,
    uri TEXT NOT NULL,
    description TEXT,
    timestamp BIGINT NOT NULL,
    accuracy BIGINT,
    created_ts BIGINT NOT NULL
);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_beacon_locations_info_ts ON beacon_locations(beacon_info_id, timestamp DESC);

-- 3. call_sessions
CREATE TABLE IF NOT EXISTS call_sessions (
    id BIGSERIAL PRIMARY KEY,
    call_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    caller_id TEXT NOT NULL,
    callee_id TEXT,
    state TEXT NOT NULL,
    offer_sdp TEXT,
    answer_sdp TEXT,
    lifetime BIGINT NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    ended_ts BIGINT
);

CREATE UNIQUE INDEX CONCURRENTLY IF NOT EXISTS idx_call_sessions_call_room ON call_sessions(call_id, room_id);
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_call_sessions_active ON call_sessions(state) WHERE state != 'ended';

-- 4. call_candidates
CREATE TABLE IF NOT EXISTS call_candidates (
    id BIGSERIAL PRIMARY KEY,
    call_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    sender_id TEXT NOT NULL,
    candidate JSONB NOT NULL,
    created_ts BIGINT NOT NULL
);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_call_candidates_session ON call_candidates(call_id, room_id, created_ts ASC);

-- 5. matrixrtc_sessions
CREATE TABLE IF NOT EXISTS matrixrtc_sessions (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    application TEXT NOT NULL,
    call_id TEXT,
    creator TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    config JSONB NOT NULL
);

CREATE UNIQUE INDEX CONCURRENTLY IF NOT EXISTS idx_matrixrtc_sessions_unique ON matrixrtc_sessions(room_id, session_id);
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_matrixrtc_sessions_active ON matrixrtc_sessions(room_id, is_active, created_ts DESC) WHERE is_active = TRUE;

-- 6. matrixrtc_memberships
CREATE TABLE IF NOT EXISTS matrixrtc_memberships (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    membership_id TEXT NOT NULL,
    application TEXT NOT NULL,
    call_id TEXT,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    expires_at BIGINT,
    foci_active TEXT,
    foci_preferred JSONB,
    application_data JSONB,
    is_active BOOLEAN NOT NULL DEFAULT TRUE
);

CREATE UNIQUE INDEX CONCURRENTLY IF NOT EXISTS idx_matrixrtc_memberships_unique ON matrixrtc_memberships(room_id, session_id, user_id, device_id);
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_matrixrtc_memberships_active ON matrixrtc_memberships(room_id, is_active) WHERE is_active = TRUE;

-- 7. matrixrtc_encryption_keys
CREATE TABLE IF NOT EXISTS matrixrtc_encryption_keys (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    key_index INTEGER NOT NULL,
    key TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    expires_at BIGINT,
    sender_user_id TEXT NOT NULL,
    sender_device_id TEXT NOT NULL
);

CREATE UNIQUE INDEX CONCURRENTLY IF NOT EXISTS idx_matrixrtc_encryption_keys_unique ON matrixrtc_encryption_keys(room_id, session_id, key_index);
