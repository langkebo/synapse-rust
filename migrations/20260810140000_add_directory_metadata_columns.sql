-- Add metadata columns to room_directory for DirectoryService persistence (ARCH-06).
--
-- The room_directory table already exists (unified_schema_v10) with columns:
--   id, room_id, is_public, is_searchable, app_service_id, added_ts
--
-- This migration adds room-metadata columns so DirectoryService can persist
-- public-room directory entries (name, topic, avatar_url, etc.) instead of
-- storing them in memory. All new columns are either nullable or have defaults
-- so existing INSERT statements that only set room_id/is_public/added_ts
-- continue to work without modification.

-- name / topic / avatar_url / canonical_alias: optional display metadata
ALTER TABLE room_directory ADD COLUMN IF NOT EXISTS name TEXT;
ALTER TABLE room_directory ADD COLUMN IF NOT EXISTS topic TEXT;
ALTER TABLE room_directory ADD COLUMN IF NOT EXISTS avatar_url TEXT;
ALTER TABLE room_directory ADD COLUMN IF NOT EXISTS canonical_alias TEXT;

-- join_rule: defaults to 'public' so rows inserted by legacy code (which only
-- sets is_public = true) are treated as public rooms by DirectoryService.
ALTER TABLE room_directory ADD COLUMN IF NOT EXISTS join_rule TEXT NOT NULL DEFAULT 'public';

-- world_readable / guest_can_join: directory visibility flags
ALTER TABLE room_directory ADD COLUMN IF NOT EXISTS world_readable BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE room_directory ADD COLUMN IF NOT EXISTS guest_can_join BOOLEAN NOT NULL DEFAULT FALSE;

-- member_count: cached member count for directory listing display.
-- Uses BIGINT to match the Rust `i64` type in RoomDirectoryEntryRow; sqlx maps
-- PostgreSQL INTEGER -> i32 and BIGINT -> i64, so a mismatch here causes a
-- runtime ColumnDecode error when querying the directory.
ALTER TABLE room_directory ADD COLUMN IF NOT EXISTS member_count BIGINT NOT NULL DEFAULT 0;

-- updated_ts: last metadata update timestamp (nullable, set on upsert)
ALTER TABLE room_directory ADD COLUMN IF NOT EXISTS updated_ts BIGINT;
