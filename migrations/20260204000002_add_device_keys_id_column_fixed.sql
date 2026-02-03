-- Fix device_keys table - add id column (corrected version)
-- Version: 20260204000002
-- Description: Add id column to device_keys table to fix query API
-- Dependencies: 20260202000002_fix_device_keys_and_voice_final.sql

-- Drop existing composite primary key
ALTER TABLE device_keys DROP CONSTRAINT device_keys_pkey;

-- Add id column as UUID primary key
ALTER TABLE device_keys ADD COLUMN IF NOT EXISTS id UUID PRIMARY KEY DEFAULT gen_random_uuid();

-- Add index for id column
CREATE INDEX IF NOT EXISTS idx_device_keys_id ON device_keys(id);

-- Add unique constraint on user_id and device_id
ALTER TABLE device_keys ADD CONSTRAINT device_keys_user_device_unique UNIQUE (user_id, device_id);

-- Add comment to document the column
COMMENT ON COLUMN device_keys.id IS 'Primary key identifier for device keys';
