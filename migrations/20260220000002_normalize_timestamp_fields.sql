-- =============================================================================
-- 数据库字段规范化迁移脚本 - 第三阶段
-- 版本: 3.0.0
-- 创建日期: 2026-02-20
-- 描述: 统一时间字段类型 (TIMESTAMP -> BIGINT) 和后缀 (_at -> _ts)
-- =============================================================================

BEGIN;

-- =============================================================================
-- 第一部分: 时间字段类型统一 (TIMESTAMP WITH TIME ZONE -> BIGINT)
-- =============================================================================

-- 1. captcha_config 表
ALTER TABLE captcha_config 
  ALTER COLUMN created_at TYPE BIGINT USING (EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT,
  ALTER COLUMN updated_at TYPE BIGINT USING (EXTRACT(EPOCH FROM updated_at) * 1000)::BIGINT;

-- 2. cas_services 表
ALTER TABLE cas_services 
  ALTER COLUMN created_at TYPE BIGINT USING (EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT,
  ALTER COLUMN updated_at TYPE BIGINT USING (EXTRACT(EPOCH FROM updated_at) * 1000)::BIGINT;

-- 3. device_keys 表
ALTER TABLE device_keys 
  ALTER COLUMN created_at TYPE BIGINT USING (EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT,
  ALTER COLUMN updated_at TYPE BIGINT USING (EXTRACT(EPOCH FROM updated_at) * 1000)::BIGINT;

-- 4. federation_access_stats 表
ALTER TABLE federation_access_stats 
  ALTER COLUMN created_at TYPE BIGINT USING (EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT,
  ALTER COLUMN updated_at TYPE BIGINT USING (EXTRACT(EPOCH FROM updated_at) * 1000)::BIGINT;

-- 5. media_quota_alerts 表
ALTER TABLE media_quota_alerts 
  ALTER COLUMN created_at TYPE BIGINT USING (EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT;

-- 6. media_quota_config 表
ALTER TABLE media_quota_config 
  ALTER COLUMN created_at TYPE BIGINT USING (EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT,
  ALTER COLUMN updated_at TYPE BIGINT USING (EXTRACT(EPOCH FROM updated_at) * 1000)::BIGINT;

-- 7. notification_templates 表
ALTER TABLE notification_templates 
  ALTER COLUMN created_at TYPE BIGINT USING (EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT,
  ALTER COLUMN updated_at TYPE BIGINT USING (EXTRACT(EPOCH FROM updated_at) * 1000)::BIGINT;

-- 8. saml_identity_providers 表
ALTER TABLE saml_identity_providers 
  ALTER COLUMN created_at TYPE BIGINT USING (EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT,
  ALTER COLUMN updated_at TYPE BIGINT USING (EXTRACT(EPOCH FROM updated_at) * 1000)::BIGINT;

-- 9. server_notifications 表
ALTER TABLE server_notifications 
  ALTER COLUMN created_at TYPE BIGINT USING (EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT,
  ALTER COLUMN updated_at TYPE BIGINT USING (EXTRACT(EPOCH FROM updated_at) * 1000)::BIGINT;

-- 10. user_media_quota 表
ALTER TABLE user_media_quota 
  ALTER COLUMN created_at TYPE BIGINT USING (EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT,
  ALTER COLUMN updated_at TYPE BIGINT USING (EXTRACT(EPOCH FROM updated_at) * 1000)::BIGINT;

-- 11. user_notification_status 表
ALTER TABLE user_notification_status 
  ALTER COLUMN created_at TYPE BIGINT USING (EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT;

-- 12. federation_blacklist 表
ALTER TABLE federation_blacklist 
  ALTER COLUMN created_at TYPE BIGINT USING (EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT,
  ALTER COLUMN updated_at TYPE BIGINT USING (EXTRACT(EPOCH FROM updated_at) * 1000)::BIGINT;

-- 13. media_repository 表
ALTER TABLE media_repository 
  ALTER COLUMN created_at TYPE BIGINT USING (EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT;

-- 14. media_thumbnails 表
ALTER TABLE media_thumbnails 
  ALTER COLUMN created_at TYPE BIGINT USING (EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT;

-- 15. account_data 表
ALTER TABLE account_data 
  ALTER COLUMN created_at TYPE BIGINT USING (EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT,
  ALTER COLUMN updated_at TYPE BIGINT USING (EXTRACT(EPOCH FROM updated_at) * 1000)::BIGINT;

-- 16. room_account_data 表
ALTER TABLE room_account_data 
  ALTER COLUMN created_at TYPE BIGINT USING (EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT,
  ALTER COLUMN updated_at TYPE BIGINT USING (EXTRACT(EPOCH FROM updated_at) * 1000)::BIGINT;

-- 17. devices 表
ALTER TABLE devices 
  ALTER COLUMN created_at TYPE BIGINT USING (EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT;

-- 18. federation_signing_keys 表
ALTER TABLE federation_signing_keys 
  ALTER COLUMN created_at TYPE BIGINT USING (EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT;

-- 19. blocked_rooms 表
ALTER TABLE blocked_rooms 
  ALTER COLUMN created_at TYPE BIGINT USING (EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT;

-- =============================================================================
-- 第二部分: 时间字段后缀统一 (_at -> _ts)
-- =============================================================================

-- 1. captcha_config 表
ALTER TABLE captcha_config RENAME COLUMN created_at TO created_ts;
ALTER TABLE captcha_config RENAME COLUMN updated_at TO updated_ts;

-- 2. cas_services 表
ALTER TABLE cas_services RENAME COLUMN created_at TO created_ts;
ALTER TABLE cas_services RENAME COLUMN updated_at TO updated_ts;

-- 3. device_keys 表
ALTER TABLE device_keys RENAME COLUMN created_at TO created_ts;
ALTER TABLE device_keys RENAME COLUMN updated_at TO updated_ts;

-- 4. federation_access_stats 表
ALTER TABLE federation_access_stats RENAME COLUMN created_at TO created_ts;
ALTER TABLE federation_access_stats RENAME COLUMN updated_at TO updated_ts;

-- 5. media_quota_alerts 表
ALTER TABLE media_quota_alerts RENAME COLUMN created_at TO created_ts;

-- 6. media_quota_config 表
ALTER TABLE media_quota_config RENAME COLUMN created_at TO created_ts;
ALTER TABLE media_quota_config RENAME COLUMN updated_at TO updated_ts;

-- 7. notification_templates 表
ALTER TABLE notification_templates RENAME COLUMN created_at TO created_ts;
ALTER TABLE notification_templates RENAME COLUMN updated_at TO updated_ts;

-- 8. saml_identity_providers 表
ALTER TABLE saml_identity_providers RENAME COLUMN created_at TO created_ts;
ALTER TABLE saml_identity_providers RENAME COLUMN updated_at TO updated_ts;

-- 9. server_notifications 表
ALTER TABLE server_notifications RENAME COLUMN created_at TO created_ts;
ALTER TABLE server_notifications RENAME COLUMN updated_at TO updated_ts;

-- 10. user_media_quota 表
ALTER TABLE user_media_quota RENAME COLUMN created_at TO created_ts;
ALTER TABLE user_media_quota RENAME COLUMN updated_at TO updated_ts;

-- 11. user_notification_status 表
ALTER TABLE user_notification_status RENAME COLUMN created_at TO created_ts;

-- 12. federation_blacklist 表
ALTER TABLE federation_blacklist RENAME COLUMN created_at TO created_ts;
ALTER TABLE federation_blacklist RENAME COLUMN updated_at TO updated_ts;

-- 13. media_repository 表
ALTER TABLE media_repository RENAME COLUMN created_at TO created_ts;

-- 14. media_thumbnails 表
ALTER TABLE media_thumbnails RENAME COLUMN created_at TO created_ts;

-- 15. account_data 表
ALTER TABLE account_data RENAME COLUMN created_at TO created_ts;
ALTER TABLE account_data RENAME COLUMN updated_at TO updated_ts;

-- 16. room_account_data 表
ALTER TABLE room_account_data RENAME COLUMN created_at TO created_ts;
ALTER TABLE room_account_data RENAME COLUMN updated_at TO updated_ts;

-- 17. devices 表
ALTER TABLE devices RENAME COLUMN created_at TO created_ts;

-- 18. federation_signing_keys 表
ALTER TABLE federation_signing_keys RENAME COLUMN created_at TO created_ts;

-- 19. blocked_rooms 表
ALTER TABLE blocked_rooms RENAME COLUMN created_at TO created_ts;

-- 20. refresh_tokens 表
ALTER TABLE refresh_tokens RENAME COLUMN expires_ts TO expires_at;

-- =============================================================================
-- 更新版本记录
-- =============================================================================

INSERT INTO schema_migrations (version, description, success)
VALUES ('3.0.0', 'Phase 3: Timestamp type and suffix normalization', TRUE)
ON CONFLICT (version) DO UPDATE SET success = TRUE, executed_at = NOW();

UPDATE db_metadata SET value = '3.0.0', updated_ts = (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT
WHERE key = 'schema_version';

COMMIT;

-- =============================================================================
-- 验证脚本
-- =============================================================================

-- 验证时间字段类型
SELECT table_name, column_name, data_type 
FROM information_schema.columns 
WHERE table_schema = 'public' 
AND column_name LIKE '%_ts'
AND data_type != 'bigint'
ORDER BY table_name, column_name;

-- 验证没有遗留的 _at 后缀时间字段
SELECT table_name, column_name, data_type 
FROM information_schema.columns 
WHERE table_schema = 'public' 
AND column_name LIKE '%_at'
AND data_type = 'bigint'
ORDER BY table_name, column_name;
