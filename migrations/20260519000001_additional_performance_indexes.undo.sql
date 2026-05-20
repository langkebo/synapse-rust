-- Undo additional performance indexes

DROP INDEX IF EXISTS idx_state_groups_room_id;
DROP INDEX IF EXISTS idx_state_groups_event_id;
DROP INDEX IF EXISTS idx_state_group_state_group_type_key;
DROP INDEX IF EXISTS idx_room_summaries_room_id;
DROP INDEX IF EXISTS idx_room_summary_members_room_user;
DROP INDEX IF EXISTS idx_presence_user_id;
DROP INDEX IF EXISTS idx_event_to_state_groups_event_id;
DROP INDEX IF EXISTS idx_refresh_tokens_user_id;
DROP INDEX IF EXISTS idx_device_lists_outbound_pokes_user;
DROP INDEX IF EXISTS idx_user_filters_user_id;
DROP INDEX IF EXISTS idx_room_tags_user_room;
DROP INDEX IF EXISTS idx_pushers_user_id;
DROP INDEX IF EXISTS idx_threepids_medium_address;
DROP INDEX IF EXISTS idx_account_data_user_type;
DROP INDEX IF EXISTS idx_room_memberships_room_user;
