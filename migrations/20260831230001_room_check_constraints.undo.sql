-- Undo: drop all rooms / room_summaries check constraints added by
-- 20260831230001_room_check_constraints.sql
--
-- Note: constraints were also declared in
-- 00000000_unified_schema_v11.sql baseline. After running this undo,
-- those baseline constraints will still exist, which is the desired
-- end-state when the migration was only needed to back-fill v10 -> v11
-- environments.

ALTER TABLE rooms DROP CONSTRAINT IF EXISTS ck_rooms_join_rules_valid;
ALTER TABLE rooms DROP CONSTRAINT IF EXISTS ck_rooms_history_visibility_valid;
ALTER TABLE rooms DROP CONSTRAINT IF EXISTS ck_rooms_visibility_valid;
ALTER TABLE rooms DROP CONSTRAINT IF EXISTS ck_rooms_room_version_valid;
ALTER TABLE rooms DROP CONSTRAINT IF EXISTS ck_rooms_timestamps_nonneg;

ALTER TABLE room_summaries DROP CONSTRAINT IF EXISTS ck_room_summaries_join_rules_valid;
ALTER TABLE room_summaries DROP CONSTRAINT IF EXISTS ck_room_summaries_history_visibility_valid;
ALTER TABLE room_summaries DROP CONSTRAINT IF EXISTS ck_room_summaries_guest_access_valid;
ALTER TABLE room_summaries DROP CONSTRAINT IF EXISTS ck_room_summaries_member_count_nonneg;
ALTER TABLE room_summaries DROP CONSTRAINT IF EXISTS ck_room_summaries_unread_nonneg;
