-- Rollback for 20260729120000_read_markers_redundant_origin_server_ts.sql
-- Drops the redundant origin_server_ts column and its fallback index from
-- the read_markers table.

DROP INDEX IF EXISTS idx_read_markers_room_user;
ALTER TABLE read_markers DROP COLUMN IF EXISTS origin_server_ts;
