-- Modify voice_messages table to allow NULL room_id
ALTER TABLE voice_messages ALTER COLUMN room_id DROP NOT NULL;
ALTER TABLE voice_messages DROP CONSTRAINT IF EXISTS fk_voice_messages_room;

-- Add session_id column if not exists
ALTER TABLE voice_messages ADD COLUMN IF NOT EXISTS session_id VARCHAR(255);
