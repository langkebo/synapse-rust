-- Fix media_quota_config table structure to match code expectations
-- This fixes the Quota API errors

-- Rename existing columns to match code
ALTER TABLE media_quota_config RENAME COLUMN config_name TO name;
ALTER TABLE media_quota_config RENAME COLUMN max_file_size TO max_file_size_bytes;

-- Add missing columns
ALTER TABLE media_quota_config ADD COLUMN IF NOT EXISTS description TEXT;
ALTER TABLE media_quota_config ADD COLUMN IF NOT EXISTS max_storage_bytes BIGINT DEFAULT 1073741824;
ALTER TABLE media_quota_config ADD COLUMN IF NOT EXISTS max_files_count INTEGER DEFAULT 1000;
ALTER TABLE media_quota_config ADD COLUMN IF NOT EXISTS allowed_mime_types JSONB DEFAULT '[]'::jsonb;
ALTER TABLE media_quota_config ADD COLUMN IF NOT EXISTS blocked_mime_types JSONB DEFAULT '[]'::jsonb;

-- Drop old content types column if exists
ALTER TABLE media_quota_config DROP COLUMN IF EXISTS allowed_content_types;
ALTER TABLE media_quota_config DROP COLUMN IF EXISTS max_upload_rate;
ALTER TABLE media_quota_config DROP COLUMN IF EXISTS retention_days;

-- Update default config
UPDATE media_quota_config 
SET 
    max_storage_bytes = 1073741824,
    max_files_count = 1000,
    allowed_mime_types = '["image/jpeg", "image/png", "image/gif", "video/mp4", "audio/ogg", "audio/mp3", "application/pdf"]'::jsonb,
    blocked_mime_types = '[]'::jsonb
WHERE is_default = TRUE;

-- Fix user_media_quota table
ALTER TABLE user_media_quota ADD COLUMN IF NOT EXISTS quota_config_id BIGINT;
ALTER TABLE user_media_quota RENAME COLUMN max_bytes TO custom_max_storage_bytes;
ALTER TABLE user_media_quota ADD COLUMN IF NOT EXISTS max_storage_bytes BIGINT DEFAULT 1073741824;
