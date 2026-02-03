-- Fix voice_messages table - add transcribe_text column
-- Version: 20260204000001
-- Description: Add missing transcribe_text column to voice_messages table
-- Dependencies: 20260202000003_fix_voice_nullable.sql

-- Add transcribe_text column for storing transcribed text from voice messages
ALTER TABLE voice_messages ADD COLUMN IF NOT EXISTS transcribe_text TEXT;

-- Add index for transcribe_text to support searching
CREATE INDEX IF NOT EXISTS idx_voice_messages_transcribe_text ON voice_messages(transcribe_text);

-- Add comment to document the column
COMMENT ON COLUMN voice_messages.transcribe_text IS 'Transcribed text content from speech-to-text processing';
