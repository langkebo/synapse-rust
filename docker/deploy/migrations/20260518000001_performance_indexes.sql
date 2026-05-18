-- Performance indexes for frequently queried columns
-- These indexes address missing coverage identified during code review

-- state_group_edges: reverse lookup by prev_state_group_id
CREATE INDEX IF NOT EXISTS idx_state_group_edges_prev_state_group
ON state_group_edges(prev_state_group_id);

-- device_lists_stream: efficient streaming queries
CREATE INDEX IF NOT EXISTS idx_device_lists_stream_stream_id
ON device_lists_stream(stream_id);

-- e2ee_audit_log: user key history queries
CREATE INDEX IF NOT EXISTS idx_e2ee_audit_log_user_created
ON e2ee_audit_log(user_id, created_ts DESC);

-- room_invite_blocklist: point lookup optimization
CREATE INDEX IF NOT EXISTS idx_room_invite_blocklist_room_user
ON room_invite_blocklist(room_id, user_id);

-- room_invite_allowlist: point lookup optimization
CREATE INDEX IF NOT EXISTS idx_room_invite_allowlist_room_user
ON room_invite_allowlist(room_id, user_id);

-- event_to_state_groups: reverse lookup by state_group_id
CREATE INDEX IF NOT EXISTS idx_event_to_state_groups_state_group
ON event_to_state_groups(state_group_id);

-- presence: cleanup stale entries
CREATE INDEX IF NOT EXISTS idx_presence_last_active_ts
ON presence(last_active_ts)
WHERE last_active_ts IS NOT NULL;
