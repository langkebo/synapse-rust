-- Fix media_quota_config table - add missing is_default column
-- This fixes the Quota API error: column "is_default" does not exist

-- Add is_default column to media_quota_config
ALTER TABLE media_quota_config ADD COLUMN IF NOT EXISTS is_default BOOLEAN DEFAULT FALSE;

-- Add quota_config_id column to user_media_quota if not exists
ALTER TABLE user_media_quota ADD COLUMN IF NOT EXISTS quota_config_id BIGINT;

-- Create index for is_default
CREATE INDEX IF NOT EXISTS idx_media_quota_config_is_default ON media_quota_config(is_default) WHERE is_default = TRUE;

-- Insert default quota config if not exists
INSERT INTO media_quota_config (config_name, max_file_size, retention_days, is_enabled, is_default, created_ts)
SELECT 'default', 104857600, 90, TRUE, TRUE, EXTRACT(EPOCH FROM NOW()) * 1000
WHERE NOT EXISTS (SELECT 1 FROM media_quota_config WHERE is_default = TRUE);
