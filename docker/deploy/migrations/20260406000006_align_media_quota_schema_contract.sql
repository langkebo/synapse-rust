-- ============================================================================
-- Align media quota schema contract
-- Created: 2026-04-06
-- Description: Restore media quota tables/columns required by MediaQuotaStorage
-- and the schema contract migration gate.
-- ============================================================================

SET TIME ZONE 'UTC';

CREATE TABLE IF NOT EXISTS media_usage_log (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    media_id TEXT NOT NULL,
    file_size_bytes BIGINT NOT NULL,
    mime_type TEXT,
    operation TEXT NOT NULL,
    timestamp BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_media_usage_log_user
ON media_usage_log(user_id);

CREATE INDEX IF NOT EXISTS idx_media_usage_log_timestamp
ON media_usage_log(timestamp);

CREATE TABLE IF NOT EXISTS media_quota_alerts (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    alert_type TEXT NOT NULL,
    threshold_percent INTEGER NOT NULL,
    current_usage_bytes BIGINT NOT NULL,
    quota_limit_bytes BIGINT NOT NULL,
    message TEXT,
    is_read BOOLEAN NOT NULL DEFAULT FALSE,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW())::BIGINT * 1000)
);

CREATE INDEX IF NOT EXISTS idx_media_quota_alerts_user
ON media_quota_alerts(user_id)
WHERE is_read = FALSE;

CREATE TABLE IF NOT EXISTS server_media_quota (
    id BIGSERIAL PRIMARY KEY,
    max_storage_bytes BIGINT,
    max_file_size_bytes BIGINT,
    max_files_count INTEGER,
    current_storage_bytes BIGINT NOT NULL DEFAULT 0,
    current_files_count INTEGER NOT NULL DEFAULT 0,
    alert_threshold_percent INTEGER NOT NULL DEFAULT 80,
    updated_ts BIGINT NOT NULL
);

ALTER TABLE media_quota_config
    ADD COLUMN IF NOT EXISTS name TEXT,
    ADD COLUMN IF NOT EXISTS description TEXT,
    ADD COLUMN IF NOT EXISTS max_storage_bytes BIGINT,
    ADD COLUMN IF NOT EXISTS max_file_size_bytes BIGINT,
    ADD COLUMN IF NOT EXISTS max_files_count INTEGER,
    ADD COLUMN IF NOT EXISTS allowed_mime_types JSONB,
    ADD COLUMN IF NOT EXISTS blocked_mime_types JSONB,
    ADD COLUMN IF NOT EXISTS is_default BOOLEAN;

UPDATE media_quota_config
SET name = COALESCE(name, NULLIF(config_name, ''), 'default')
WHERE name IS NULL;

UPDATE media_quota_config
SET max_storage_bytes = COALESCE(max_storage_bytes, 10737418240)
WHERE max_storage_bytes IS NULL;

UPDATE media_quota_config
SET max_file_size_bytes = COALESCE(max_file_size_bytes, max_file_size, 10485760)
WHERE max_file_size_bytes IS NULL;

UPDATE media_quota_config
SET max_files_count = COALESCE(max_files_count, 10000)
WHERE max_files_count IS NULL;

UPDATE media_quota_config
SET allowed_mime_types = COALESCE(allowed_mime_types, to_jsonb(allowed_content_types), '[]'::jsonb)
WHERE allowed_mime_types IS NULL;

UPDATE media_quota_config
SET blocked_mime_types = COALESCE(blocked_mime_types, '[]'::jsonb)
WHERE blocked_mime_types IS NULL;

UPDATE media_quota_config
SET is_default = COALESCE(is_default, FALSE)
WHERE is_default IS NULL;

ALTER TABLE media_quota_config
    ALTER COLUMN config_name SET DEFAULT '',
    ALTER COLUMN created_ts SET DEFAULT (EXTRACT(EPOCH FROM NOW())::BIGINT * 1000),
    ALTER COLUMN name SET DEFAULT 'default',
    ALTER COLUMN max_storage_bytes SET DEFAULT 10737418240,
    ALTER COLUMN max_file_size_bytes SET DEFAULT 10485760,
    ALTER COLUMN max_files_count SET DEFAULT 10000,
    ALTER COLUMN allowed_mime_types SET DEFAULT '[]'::jsonb,
    ALTER COLUMN blocked_mime_types SET DEFAULT '[]'::jsonb,
    ALTER COLUMN is_default SET DEFAULT FALSE;

ALTER TABLE media_quota_config
    ALTER COLUMN name SET NOT NULL,
    ALTER COLUMN max_storage_bytes SET NOT NULL,
    ALTER COLUMN max_file_size_bytes SET NOT NULL,
    ALTER COLUMN max_files_count SET NOT NULL,
    ALTER COLUMN allowed_mime_types SET NOT NULL,
    ALTER COLUMN blocked_mime_types SET NOT NULL,
    ALTER COLUMN is_default SET NOT NULL;

ALTER TABLE user_media_quota
    ADD COLUMN IF NOT EXISTS quota_config_id BIGINT,
    ADD COLUMN IF NOT EXISTS custom_max_storage_bytes BIGINT,
    ADD COLUMN IF NOT EXISTS custom_max_file_size_bytes BIGINT,
    ADD COLUMN IF NOT EXISTS custom_max_files_count INTEGER,
    ADD COLUMN IF NOT EXISTS current_storage_bytes BIGINT,
    ADD COLUMN IF NOT EXISTS current_files_count INTEGER;

UPDATE user_media_quota
SET current_storage_bytes = COALESCE(current_storage_bytes, used_bytes, 0)
WHERE current_storage_bytes IS NULL;

UPDATE user_media_quota
SET current_files_count = COALESCE(current_files_count, file_count, 0)
WHERE current_files_count IS NULL;

ALTER TABLE user_media_quota
    ALTER COLUMN current_storage_bytes SET DEFAULT 0,
    ALTER COLUMN current_files_count SET DEFAULT 0;

ALTER TABLE user_media_quota
    ALTER COLUMN current_storage_bytes SET NOT NULL,
    ALTER COLUMN current_files_count SET NOT NULL;

UPDATE media_quota_alerts
SET is_read = FALSE
WHERE is_read IS NULL;

ALTER TABLE media_quota_alerts
    ALTER COLUMN created_ts SET DEFAULT (EXTRACT(EPOCH FROM NOW())::BIGINT * 1000),
    ALTER COLUMN is_read SET DEFAULT FALSE;

ALTER TABLE media_quota_alerts
    ALTER COLUMN is_read SET NOT NULL;

INSERT INTO server_media_quota (
    id,
    max_storage_bytes,
    max_file_size_bytes,
    max_files_count,
    current_storage_bytes,
    current_files_count,
    alert_threshold_percent,
    updated_ts
)
SELECT
    1,
    10995116277760,
    1073741824,
    1000000,
    0,
    0,
    80,
    EXTRACT(EPOCH FROM NOW())::BIGINT * 1000
WHERE NOT EXISTS (
    SELECT 1 FROM server_media_quota WHERE id = 1
);

UPDATE server_media_quota
SET current_storage_bytes = COALESCE(current_storage_bytes, 0),
    current_files_count = COALESCE(current_files_count, 0),
    alert_threshold_percent = COALESCE(alert_threshold_percent, 80)
WHERE id = 1;

ALTER TABLE server_media_quota
    ALTER COLUMN current_storage_bytes SET DEFAULT 0,
    ALTER COLUMN current_files_count SET DEFAULT 0,
    ALTER COLUMN alert_threshold_percent SET DEFAULT 80;

ALTER TABLE server_media_quota
    ALTER COLUMN current_storage_bytes SET NOT NULL,
    ALTER COLUMN current_files_count SET NOT NULL,
    ALTER COLUMN alert_threshold_percent SET NOT NULL;

INSERT INTO schema_migrations (version, name, success, description, applied_ts)
VALUES (
    '20260406000006',
    'align_media_quota_schema_contract',
    TRUE,
    'Restore media quota schema columns and tables required by MediaQuotaStorage and contract tests',
    EXTRACT(EPOCH FROM NOW())::BIGINT * 1000
)
ON CONFLICT (version) DO NOTHING;
