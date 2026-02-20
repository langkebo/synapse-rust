-- =============================================================================
-- 数据库字段规范化回滚脚本 - 第三阶段
-- 版本: 3.0.0
-- 创建日期: 2026-02-20
-- 描述: 回滚时间字段类型和后缀统一
-- =============================================================================

BEGIN;

-- =============================================================================
-- 第一部分: 回滚时间字段后缀 (_ts -> _at)
-- =============================================================================

-- 1. captcha_config 表
ALTER TABLE captcha_config RENAME COLUMN created_ts TO created_at;
ALTER TABLE captcha_config RENAME COLUMN updated_ts TO updated_at;

-- 2. cas_services 表
ALTER TABLE cas_services RENAME COLUMN created_ts TO created_at;
ALTER TABLE cas_services RENAME COLUMN updated_ts TO updated_at;

-- 3. device_keys 表
ALTER TABLE device_keys RENAME COLUMN created_ts TO created_at;
ALTER TABLE device_keys RENAME COLUMN updated_ts TO updated_at;

-- 4. federation_access_stats 表
ALTER TABLE federation_access_stats RENAME COLUMN created_ts TO created_at;
ALTER TABLE federation_access_stats RENAME COLUMN updated_ts TO updated_at;

-- 5. media_quota_alerts 表
ALTER TABLE media_quota_alerts RENAME COLUMN created_ts TO created_at;

-- 6. media_quota_config 表
ALTER TABLE media_quota_config RENAME COLUMN created_ts TO created_at;
ALTER TABLE media_quota_config RENAME COLUMN updated_ts TO updated_at;

-- 7. notification_templates 表
ALTER TABLE notification_templates RENAME COLUMN created_ts TO created_at;
ALTER TABLE notification_templates RENAME COLUMN updated_ts TO updated_at;

-- 8. saml_identity_providers 表
ALTER TABLE saml_identity_providers RENAME COLUMN created_ts TO created_at;
ALTER TABLE saml_identity_providers RENAME COLUMN updated_ts TO updated_at;

-- 9. server_notifications 表
ALTER TABLE server_notifications RENAME COLUMN created_ts TO created_at;
ALTER TABLE server_notifications RENAME COLUMN updated_ts TO updated_at;

-- 10. user_media_quota 表
ALTER TABLE user_media_quota RENAME COLUMN created_ts TO created_at;
ALTER TABLE user_media_quota RENAME COLUMN updated_ts TO updated_at;

-- 11. user_notification_status 表
ALTER TABLE user_notification_status RENAME COLUMN created_ts TO created_at;

-- 12. federation_blacklist 表
ALTER TABLE federation_blacklist RENAME COLUMN created_ts TO created_at;
ALTER TABLE federation_blacklist RENAME COLUMN updated_ts TO updated_at;

-- 13. media_repository 表
ALTER TABLE media_repository RENAME COLUMN created_ts TO created_at;

-- 14. media_thumbnails 表
ALTER TABLE media_thumbnails RENAME COLUMN created_ts TO created_at;

-- 15. account_data 表
ALTER TABLE account_data RENAME COLUMN created_ts TO created_at;
ALTER TABLE account_data RENAME COLUMN updated_ts TO updated_at;

-- 16. room_account_data 表
ALTER TABLE room_account_data RENAME COLUMN created_ts TO created_at;
ALTER TABLE room_account_data RENAME COLUMN updated_ts TO updated_at;

-- 17. devices 表
ALTER TABLE devices RENAME COLUMN created_ts TO created_at;

-- 18. federation_signing_keys 表
ALTER TABLE federation_signing_keys RENAME COLUMN created_ts TO created_at;

-- 19. blocked_rooms 表
ALTER TABLE blocked_rooms RENAME COLUMN created_ts TO created_at;

-- 20. refresh_tokens 表
ALTER TABLE refresh_tokens RENAME COLUMN expires_at TO expires_ts;

-- =============================================================================
-- 第二部分: 回滚时间字段类型 (BIGINT -> TIMESTAMP WITH TIME ZONE)
-- =============================================================================

-- 1. captcha_config 表
ALTER TABLE captcha_config 
  ALTER COLUMN created_at TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(created_at / 1000.0),
  ALTER COLUMN updated_at TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(updated_at / 1000.0);

-- 2. cas_services 表
ALTER TABLE cas_services 
  ALTER COLUMN created_at TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(created_at / 1000.0),
  ALTER COLUMN updated_at TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(updated_at / 1000.0);

-- 3. device_keys 表
ALTER TABLE device_keys 
  ALTER COLUMN created_at TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(created_at / 1000.0),
  ALTER COLUMN updated_at TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(updated_at / 1000.0);

-- 4. federation_access_stats 表
ALTER TABLE federation_access_stats 
  ALTER COLUMN created_at TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(created_at / 1000.0),
  ALTER COLUMN updated_at TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(updated_at / 1000.0);

-- 5. media_quota_alerts 表
ALTER TABLE media_quota_alerts 
  ALTER COLUMN created_at TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(created_at / 1000.0);

-- 6. media_quota_config 表
ALTER TABLE media_quota_config 
  ALTER COLUMN created_at TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(created_at / 1000.0),
  ALTER COLUMN updated_at TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(updated_at / 1000.0);

-- 7. notification_templates 表
ALTER TABLE notification_templates 
  ALTER COLUMN created_at TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(created_at / 1000.0),
  ALTER COLUMN updated_at TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(updated_at / 1000.0);

-- 8. saml_identity_providers 表
ALTER TABLE saml_identity_providers 
  ALTER COLUMN created_at TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(created_at / 1000.0),
  ALTER COLUMN updated_at TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(updated_at / 1000.0);

-- 9. server_notifications 表
ALTER TABLE server_notifications 
  ALTER COLUMN created_at TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(created_at / 1000.0),
  ALTER COLUMN updated_at TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(updated_at / 1000.0);

-- 10. user_media_quota 表
ALTER TABLE user_media_quota 
  ALTER COLUMN created_at TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(created_at / 1000.0),
  ALTER COLUMN updated_at TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(updated_at / 1000.0);

-- 11. user_notification_status 表
ALTER TABLE user_notification_status 
  ALTER COLUMN created_at TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(created_at / 1000.0);

-- 12. federation_blacklist 表
ALTER TABLE federation_blacklist 
  ALTER COLUMN created_at TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(created_at / 1000.0),
  ALTER COLUMN updated_at TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(updated_at / 1000.0);

-- 13. media_repository 表
ALTER TABLE media_repository 
  ALTER COLUMN created_at TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(created_at / 1000.0);

-- 14. media_thumbnails 表
ALTER TABLE media_thumbnails 
  ALTER COLUMN created_at TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(created_at / 1000.0);

-- 15. account_data 表
ALTER TABLE account_data 
  ALTER COLUMN created_at TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(created_at / 1000.0),
  ALTER COLUMN updated_at TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(updated_at / 1000.0);

-- 16. room_account_data 表
ALTER TABLE room_account_data 
  ALTER COLUMN created_at TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(created_at / 1000.0),
  ALTER COLUMN updated_at TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(updated_at / 1000.0);

-- 17. devices 表
ALTER TABLE devices 
  ALTER COLUMN created_at TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(created_at / 1000.0);

-- 18. federation_signing_keys 表
ALTER TABLE federation_signing_keys 
  ALTER COLUMN created_at TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(created_at / 1000.0);

-- 19. blocked_rooms 表
ALTER TABLE blocked_rooms 
  ALTER COLUMN created_at TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(created_at / 1000.0);

-- =============================================================================
-- 更新版本记录
-- =============================================================================

DELETE FROM schema_migrations WHERE version = '3.0.0';

UPDATE db_metadata SET value = '2.0.0', updated_ts = (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT
WHERE key = 'schema_version';

COMMIT;
