-- P1-7: Add redundant origin_server_ts column to read_markers table.
--
-- Problem: get_unread_counts uses LEFT JOIN events e ON e.event_id = rm.event_id
-- to compute last_read_ts. When purge_history deletes the event referenced by
-- read_markers.event_id, the JOIN returns NULL and COALESCE(MAX(NULL), 0) = 0,
-- causing ALL remaining events (including already-read ones that survived the
-- purge as local events) to be counted as unread — a notification count bloat.
--
-- Fix (mirrors Element Synapse approach): cache the marker event's
-- origin_server_ts in read_markers at write time. get_unread_counts then uses
-- COALESCE(e.origin_server_ts, rm.origin_server_ts, 0) so the cached value
-- survives event deletion.
--
-- Safety: idempotent — uses DO $$ ... ADD COLUMN IF NOT EXISTS.
-- Backfill: best-effort — existing markers get origin_server_ts from a JOIN
-- to events; rows whose event_id is already purged remain NULL (acceptable:
-- they were already producing last_read_ts=0 before this migration).

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = current_schema() AND table_name = 'read_markers' AND column_name = 'origin_server_ts'
    ) THEN
        ALTER TABLE read_markers ADD COLUMN origin_server_ts BIGINT;
    END IF;
END $$;

-- Backfill existing rows from the events table (NULL for already-purged markers).
UPDATE read_markers rm
SET origin_server_ts = e.origin_server_ts
FROM events e
WHERE rm.event_id = e.event_id
  AND rm.origin_server_ts IS NULL;

-- Index to support fallback lookups in get_unread_counts when needed.
CREATE INDEX IF NOT EXISTS idx_read_markers_room_user
ON read_markers(room_id, user_id);
