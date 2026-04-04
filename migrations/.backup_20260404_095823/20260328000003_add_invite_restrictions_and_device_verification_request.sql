CREATE TABLE IF NOT EXISTS room_invite_blocklist (
    id BIGSERIAL,
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_room_invite_blocklist PRIMARY KEY (id),
    CONSTRAINT uq_room_invite_blocklist_room_user UNIQUE (room_id, user_id),
    CONSTRAINT fk_room_invite_blocklist_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    CONSTRAINT fk_room_invite_blocklist_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_room_invite_blocklist_room ON room_invite_blocklist(room_id);
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_room_invite_blocklist_user ON room_invite_blocklist(user_id);

CREATE TABLE IF NOT EXISTS room_invite_allowlist (
    id BIGSERIAL,
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_room_invite_allowlist PRIMARY KEY (id),
    CONSTRAINT uq_room_invite_allowlist_room_user UNIQUE (room_id, user_id),
    CONSTRAINT fk_room_invite_allowlist_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    CONSTRAINT fk_room_invite_allowlist_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_room_invite_allowlist_room ON room_invite_allowlist(room_id);
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_room_invite_allowlist_user ON room_invite_allowlist(user_id);

CREATE TABLE IF NOT EXISTS device_verification_request (
    id BIGSERIAL,
    user_id TEXT NOT NULL,
    new_device_id TEXT NOT NULL,
    requesting_device_id TEXT,
    verification_method TEXT NOT NULL,
    status TEXT NOT NULL,
    request_token TEXT NOT NULL,
    commitment TEXT,
    pubkey TEXT,
    created_ts BIGINT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ,
    CONSTRAINT pk_device_verification_request PRIMARY KEY (id),
    CONSTRAINT uq_device_verification_request_token UNIQUE (request_token),
    CONSTRAINT fk_device_verification_request_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_device_verification_request_user_device_pending
ON device_verification_request(user_id, new_device_id)
WHERE status = 'pending';

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_device_verification_request_expires_pending
ON device_verification_request(expires_at)
WHERE status = 'pending';
