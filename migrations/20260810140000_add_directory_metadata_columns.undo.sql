-- Undo: remove metadata columns added by 20260810140000_add_directory_metadata_columns.sql

DROP INDEX IF EXISTS idx_room_directory_join_rule;

ALTER TABLE room_directory DROP COLUMN IF EXISTS updated_ts;
ALTER TABLE room_directory DROP COLUMN IF EXISTS member_count;
ALTER TABLE room_directory DROP COLUMN IF EXISTS guest_can_join;
ALTER TABLE room_directory DROP COLUMN IF EXISTS world_readable;
ALTER TABLE room_directory DROP COLUMN IF EXISTS join_rule;
ALTER TABLE room_directory DROP COLUMN IF EXISTS canonical_alias;
ALTER TABLE room_directory DROP COLUMN IF EXISTS avatar_url;
ALTER TABLE room_directory DROP COLUMN IF EXISTS topic;
ALTER TABLE room_directory DROP COLUMN IF EXISTS name;
