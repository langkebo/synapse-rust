-- Final fix for missing columns in device_keys and voice_messages
-- Version: 20260202000002

-- device_keys missing public_key, signatures, created_at, updated_at
ALTER TABLE device_keys ADD COLUMN IF NOT EXISTS public_key TEXT;
ALTER TABLE device_keys ADD COLUMN IF NOT EXISTS signatures JSONB;
ALTER TABLE device_keys ADD COLUMN IF NOT EXISTS created_at TIMESTAMPTZ DEFAULT NOW();
ALTER TABLE device_keys ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ DEFAULT NOW();

-- voice_messages missing waveform_data
ALTER TABLE voice_messages ADD COLUMN IF NOT EXISTS waveform_data TEXT;

-- Sync waveform to waveform_data if needed (optional, for existing data)
UPDATE voice_messages SET waveform_data = waveform::text WHERE waveform_data IS NULL AND waveform IS NOT NULL;
