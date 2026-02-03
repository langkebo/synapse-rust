-- Fix voice_messages room_id to be nullable
-- Version: 20260202000003

ALTER TABLE voice_messages ALTER COLUMN room_id DROP NOT NULL;
