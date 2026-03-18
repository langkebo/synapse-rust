-- Fix user_media_quota table structure to match code expectations

-- Add missing columns
ALTER TABLE user_media_quota ADD COLUMN IF NOT EXISTS custom_max_file_size_bytes BIGINT;
ALTER TABLE user_media_quota ADD COLUMN IF NOT EXISTS custom_max_files_count INTEGER;
ALTER TABLE user_media_quota ADD COLUMN IF NOT EXISTS current_storage_bytes BIGINT DEFAULT 0;
ALTER TABLE user_media_quota ADD COLUMN IF NOT EXISTS current_files_count INTEGER DEFAULT 0;

-- Rename columns to match code
ALTER TABLE user_media_quota RENAME COLUMN used_bytes TO current_storage_bytes_temp;
ALTER TABLE user_media_quota RENAME COLUMN file_count TO current_files_count_temp;

-- Update data
UPDATE user_media_quota 
SET current_storage_bytes = COALESCE(current_storage_bytes_temp, current_storage_bytes, 0),
    current_files_count = COALESCE(current_files_count_temp, current_files_count, 0);

-- Drop temp columns
ALTER TABLE user_media_quota DROP COLUMN IF EXISTS current_storage_bytes_temp;
ALTER TABLE user_media_quota DROP COLUMN IF EXISTS current_files_count_temp;

-- Set default values
UPDATE user_media_quota 
SET current_storage_bytes = 0 WHERE current_storage_bytes IS NULL;
UPDATE user_media_quota 
SET current_files_count = 0 WHERE current_files_count IS NULL;
