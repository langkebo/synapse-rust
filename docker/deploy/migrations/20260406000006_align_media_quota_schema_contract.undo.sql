-- ============================================================================
-- Rollback: align_media_quota_schema_contract
-- Created: 2026-04-06
-- Description: Removes media quota schema contract alignment artifacts.
-- ============================================================================

SET TIME ZONE 'UTC';

DROP INDEX IF EXISTS idx_media_quota_alerts_user;
DROP INDEX IF EXISTS idx_media_usage_log_timestamp;
DROP INDEX IF EXISTS idx_media_usage_log_user;

DROP TABLE IF EXISTS media_quota_alerts;
DROP TABLE IF EXISTS media_usage_log;
DROP TABLE IF EXISTS server_media_quota;

ALTER TABLE IF EXISTS user_media_quota
    DROP COLUMN IF EXISTS quota_config_id,
    DROP COLUMN IF EXISTS custom_max_storage_bytes,
    DROP COLUMN IF EXISTS custom_max_file_size_bytes,
    DROP COLUMN IF EXISTS custom_max_files_count,
    DROP COLUMN IF EXISTS current_storage_bytes,
    DROP COLUMN IF EXISTS current_files_count;

ALTER TABLE IF EXISTS media_quota_config
    DROP COLUMN IF EXISTS name,
    DROP COLUMN IF EXISTS description,
    DROP COLUMN IF EXISTS max_storage_bytes,
    DROP COLUMN IF EXISTS max_file_size_bytes,
    DROP COLUMN IF EXISTS max_files_count,
    DROP COLUMN IF EXISTS allowed_mime_types,
    DROP COLUMN IF EXISTS blocked_mime_types,
    DROP COLUMN IF EXISTS is_default;
