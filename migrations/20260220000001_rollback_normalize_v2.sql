-- =============================================================================
-- 数据库字段规范化回滚脚本 - 第二阶段
-- 版本: 2.0.0
-- 创建日期: 2026-02-20
-- 描述: 回滚布尔字段规范化、索引和外键约束
-- =============================================================================

BEGIN;

-- =============================================================================
-- 第一部分: 回滚布尔字段规范化 (移除 is_ 前缀)
-- =============================================================================

-- 1. registration_tokens 表
ALTER TABLE registration_tokens RENAME COLUMN is_enabled TO is_active;

-- 2. registration_token_batches 表
ALTER TABLE registration_token_batches RENAME COLUMN is_enabled TO is_active;

-- 3. federation_blacklist 表
ALTER TABLE federation_blacklist RENAME COLUMN is_enabled TO is_active;

-- 4. federation_blacklist_rule 表
ALTER TABLE federation_blacklist_rule RENAME COLUMN is_enabled TO enabled;

-- 5. application_service_statistics 表
ALTER TABLE application_service_statistics RENAME COLUMN is_enabled TO is_active;

-- 6. notification_templates 表
ALTER TABLE notification_templates RENAME COLUMN is_enabled TO is_active;

-- 7. account_data_callbacks 表
ALTER TABLE account_data_callbacks RENAME COLUMN is_enabled TO enabled;

-- 8. captcha_template 表
ALTER TABLE captcha_template RENAME COLUMN is_enabled TO enabled;

-- 9. cross_signing_keys 表
ALTER TABLE cross_signing_keys RENAME COLUMN is_blocked TO blocked;

-- 10. device_keys 表
ALTER TABLE device_keys RENAME COLUMN is_blocked TO blocked;

-- 11. media_callbacks 表
ALTER TABLE media_callbacks RENAME COLUMN is_enabled TO enabled;

-- 12. modules 表
ALTER TABLE modules RENAME COLUMN is_enabled TO enabled;

-- 13. password_auth_providers 表
ALTER TABLE password_auth_providers RENAME COLUMN is_enabled TO enabled;

-- 14. presence_routes 表
ALTER TABLE presence_routes RENAME COLUMN is_enabled TO enabled;

-- 15. push_rules 表
ALTER TABLE push_rules RENAME COLUMN is_enabled TO enabled;

-- 16. pushers 表
ALTER TABLE pushers RENAME COLUMN is_enabled TO enabled;

-- 17. rate_limit_callbacks 表
ALTER TABLE rate_limit_callbacks RENAME COLUMN is_enabled TO enabled;

-- 18. saml_identity_providers 表
ALTER TABLE saml_identity_providers RENAME COLUMN is_enabled TO enabled;

-- 19. media_repository 表
ALTER TABLE media_repository RENAME COLUMN is_quarantined TO quarantined;

-- 20. space_children 表
ALTER TABLE space_children RENAME COLUMN is_suggested TO suggested;

-- 21. refresh_tokens 表
ALTER TABLE refresh_tokens RENAME COLUMN is_revoked TO invalidated;

-- =============================================================================
-- 第二部分: 删除添加的索引
-- =============================================================================

DROP INDEX IF EXISTS idx_users_email;
DROP INDEX IF EXISTS idx_users_creation_ts;
DROP INDEX IF EXISTS idx_users_deactivated;
DROP INDEX IF EXISTS idx_access_tokens_user_id;
DROP INDEX IF EXISTS idx_access_tokens_expires_ts;
DROP INDEX IF EXISTS idx_refresh_tokens_user_id;
DROP INDEX IF EXISTS idx_refresh_tokens_expires_at;
DROP INDEX IF EXISTS idx_devices_user_id;
DROP INDEX IF EXISTS idx_devices_last_seen_ts;
DROP INDEX IF EXISTS idx_events_room_id;
DROP INDEX IF EXISTS idx_events_sender;
DROP INDEX IF EXISTS idx_events_origin_server_ts;
DROP INDEX IF EXISTS idx_room_members_user_id;
DROP INDEX IF EXISTS idx_room_members_room_id;
DROP INDEX IF EXISTS idx_pushers_user_id;
DROP INDEX IF EXISTS idx_user_media_quota_user_id;
DROP INDEX IF EXISTS idx_media_quota_config_is_enabled;
DROP INDEX IF EXISTS idx_server_notifications_is_enabled;
DROP INDEX IF EXISTS idx_user_notification_status_user_id;
DROP INDEX IF EXISTS idx_federation_blacklist_server_name;
DROP INDEX IF EXISTS idx_federation_blacklist_is_enabled;

-- =============================================================================
-- 第三部分: 删除外键约束
-- =============================================================================

ALTER TABLE access_tokens DROP CONSTRAINT IF EXISTS fk_access_tokens_user_id;
ALTER TABLE refresh_tokens DROP CONSTRAINT IF EXISTS fk_refresh_tokens_user_id;
ALTER TABLE devices DROP CONSTRAINT IF EXISTS fk_devices_user_id;
ALTER TABLE user_media_quota DROP CONSTRAINT IF EXISTS fk_user_media_quota_user_id;
ALTER TABLE user_media_quota DROP CONSTRAINT IF EXISTS fk_user_media_quota_quota_config_id;
ALTER TABLE user_notification_status DROP CONSTRAINT IF EXISTS fk_user_notification_status_user_id;
ALTER TABLE user_notification_status DROP CONSTRAINT IF EXISTS fk_user_notification_status_notification_id;

-- =============================================================================
-- 第四部分: 恢复冗余字段
-- =============================================================================

-- 恢复 access_tokens.ip 字段
ALTER TABLE access_tokens ADD COLUMN IF NOT EXISTS ip VARCHAR(255);

-- =============================================================================
-- 更新版本记录
-- =============================================================================

DELETE FROM schema_migrations WHERE version = '2.0.0';

UPDATE db_metadata SET value = '1.0.0', updated_ts = (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT
WHERE key = 'schema_version';

COMMIT;
