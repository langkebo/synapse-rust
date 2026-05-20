-- Additional performance indexes for frequently queried tables

CREATE INDEX IF NOT EXISTS idx_state_groups_room_id ON state_groups(room_id);
CREATE INDEX IF NOT EXISTS idx_state_groups_event_id ON state_groups(event_id);
CREATE INDEX IF NOT EXISTS idx_state_group_state_group_type_key ON state_group_state(state_group_id, event_type, state_key);
CREATE INDEX IF NOT EXISTS idx_room_summaries_room_id ON room_summaries(room_id);
CREATE INDEX IF NOT EXISTS idx_room_summary_members_room_user ON room_summary_members(room_id, user_id);
CREATE INDEX IF NOT EXISTS idx_presence_user_id ON presence(user_id);
CREATE INDEX IF NOT EXISTS idx_event_to_state_groups_event_id ON event_to_state_groups(event_id);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_user_id ON refresh_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_device_lists_outbound_pokes_user ON device_lists_outbound_pokes(user_id);
CREATE INDEX IF NOT EXISTS idx_user_filters_user_id ON user_filters(user_id);
CREATE INDEX IF NOT EXISTS idx_room_tags_user_room ON room_tags(user_id, room_id);
CREATE INDEX IF NOT EXISTS idx_pushers_user_id ON pushers(user_id);
CREATE INDEX IF NOT EXISTS idx_user_threepids_medium_address ON user_threepids(medium, address);
CREATE INDEX IF NOT EXISTS idx_account_data_user_type ON account_data(user_id, data_type);
CREATE INDEX IF NOT EXISTS idx_room_memberships_room_user ON room_memberships(room_id, user_id);
