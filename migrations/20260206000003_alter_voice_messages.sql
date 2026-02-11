-- Alter voice_messages table structure to match code expectations
-- Execution time: 2026-02-06
-- Strategy: Use ALTER TABLE instead of DROP/CREATE to preserve data

-- Add missing columns if they don't exist
ALTER TABLE voice_messages ADD COLUMN IF NOT EXISTS session_id VARCHAR(255);
ALTER TABLE voice_messages ADD COLUMN IF NOT EXISTS waveform_data TEXT;
ALTER TABLE voice_messages ADD COLUMN IF NOT EXISTS transcribe_text TEXT;

-- Rename columns to match code expectations (if old names exist)
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'voice_messages' AND column_name = 'size_bytes') THEN
        ALTER TABLE voice_messages RENAME COLUMN size_bytes TO file_size;
    END IF;

    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'voice_messages' AND column_name = 'created_at') THEN
        ALTER TABLE voice_messages RENAME COLUMN created_at TO created_ts;
    END IF;
END $$;

-- Modify column types to match code expectations
ALTER TABLE voice_messages ALTER COLUMN content_type TYPE VARCHAR(100);
ALTER TABLE voice_messages ALTER COLUMN duration_ms TYPE INT;

-- Make room_id nullable if it's not already
ALTER TABLE voice_messages ALTER COLUMN room_id DROP NOT NULL;

-- Add comment
COMMENT ON TABLE voice_messages IS '语音消息存储表';
