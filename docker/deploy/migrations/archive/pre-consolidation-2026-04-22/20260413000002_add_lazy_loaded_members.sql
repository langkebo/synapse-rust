SET TIME ZONE 'UTC';

CREATE TABLE IF NOT EXISTS lazy_loaded_members (
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    member_user_id TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT pk_lazy_loaded_members PRIMARY KEY (user_id, device_id, room_id, member_user_id)
);

CREATE INDEX IF NOT EXISTS idx_lazy_loaded_members_lookup
ON lazy_loaded_members(user_id, device_id, room_id);

INSERT INTO schema_migrations (version, name, success, description, applied_ts)
VALUES (
    '20260413000002',
    'add_lazy_loaded_members',
    TRUE,
    'Persist /sync lazy-loaded member cache by user_id, device_id, room_id, member_user_id',
    EXTRACT(EPOCH FROM NOW())::BIGINT * 1000
)
ON CONFLICT (version) DO NOTHING;
