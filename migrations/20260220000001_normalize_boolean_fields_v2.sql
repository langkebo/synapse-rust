-- =============================================================================
-- 数据库字段规范化迁移脚本 - 第二阶段
-- 版本: 2.0.0
-- 创建日期: 2026-02-20
-- 描述: 完成布尔字段规范化、添加索引和外键约束
-- =============================================================================

BEGIN;

-- =============================================================================
-- 第一部分: 布尔字段规范化 (添加 is_ 前缀)
-- =============================================================================

-- 1. registration_tokens 表
ALTER TABLE registration_tokens RENAME COLUMN is_active TO is_enabled;

-- 2. registration_token_batches 表
ALTER TABLE registration_token_batches RENAME COLUMN is_active TO is_enabled;

-- 3. federation_blacklist 表
ALTER TABLE federation_blacklist RENAME COLUMN is_active TO is_enabled;

-- 4. federation_blacklist_rule 表
ALTER TABLE federation_blacklist_rule RENAME COLUMN enabled TO is_enabled;

-- 5. application_service_statistics 表
ALTER TABLE application_service_statistics RENAME COLUMN is_active TO is_enabled;

-- 6. notification_templates 表
ALTER TABLE notification_templates RENAME COLUMN is_active TO is_enabled;

-- 7. account_data_callbacks 表
ALTER TABLE account_data_callbacks RENAME COLUMN enabled TO is_enabled;

-- 8. captcha_template 表
ALTER TABLE captcha_template RENAME COLUMN enabled TO is_enabled;

-- 9. cross_signing_keys 表
ALTER TABLE cross_signing_keys RENAME COLUMN blocked TO is_blocked;

-- 10. device_keys 表
ALTER TABLE device_keys RENAME COLUMN blocked TO is_blocked;

-- 11. media_callbacks 表
ALTER TABLE media_callbacks RENAME COLUMN enabled TO is_enabled;

-- 12. modules 表
ALTER TABLE modules RENAME COLUMN enabled TO is_enabled;

-- 13. password_auth_providers 表
ALTER TABLE password_auth_providers RENAME COLUMN enabled TO is_enabled;

-- 14. presence_routes 表
ALTER TABLE presence_routes RENAME COLUMN enabled TO is_enabled;

-- 15. push_rules 表
ALTER TABLE push_rules RENAME COLUMN enabled TO is_enabled;

-- 16. pushers 表
ALTER TABLE pushers RENAME COLUMN enabled TO is_enabled;

-- 17. rate_limit_callbacks 表
ALTER TABLE rate_limit_callbacks RENAME COLUMN enabled TO is_enabled;

-- 18. saml_identity_providers 表
ALTER TABLE saml_identity_providers RENAME COLUMN enabled TO is_enabled;

-- 19. media_repository 表
ALTER TABLE media_repository RENAME COLUMN quarantined TO is_quarantined;

-- 20. space_children 表
ALTER TABLE space_children RENAME COLUMN suggested TO is_suggested;

-- 21. refresh_tokens 表
ALTER TABLE refresh_tokens RENAME COLUMN invalidated TO is_revoked;

-- =============================================================================
-- 第二部分: 添加缺失的索引
-- =============================================================================

-- 用户相关索引
CREATE INDEX IF NOT EXISTS idx_users_email ON users(email);
CREATE INDEX IF NOT EXISTS idx_users_creation_ts ON users(creation_ts);
CREATE INDEX IF NOT EXISTS idx_users_deactivated ON users(is_deactivated) WHERE is_deactivated = TRUE;

-- 令牌相关索引
CREATE INDEX IF NOT EXISTS idx_access_tokens_user_id ON access_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_access_tokens_expires_ts ON access_tokens(expires_ts);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_user_id ON refresh_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_expires_at ON refresh_tokens(expires_at);

-- 设备相关索引
CREATE INDEX IF NOT EXISTS idx_devices_user_id ON devices(user_id);
CREATE INDEX IF NOT EXISTS idx_devices_last_seen_ts ON devices(last_seen_ts DESC);

-- 事件相关索引
CREATE INDEX IF NOT EXISTS idx_events_room_id ON events(room_id);
CREATE INDEX IF NOT EXISTS idx_events_sender ON events(sender);
CREATE INDEX IF NOT EXISTS idx_events_origin_server_ts ON events(origin_server_ts DESC);

-- 房间成员索引
CREATE INDEX IF NOT EXISTS idx_room_members_user_id ON room_members(user_id);
CREATE INDEX IF NOT EXISTS idx_room_members_room_id ON room_members(room_id);

-- 推送相关索引
CREATE INDEX IF NOT EXISTS idx_pushers_user_id ON pushers(user_id);

-- 媒体配额索引
CREATE INDEX IF NOT EXISTS idx_user_media_quota_user_id ON user_media_quota(user_id);
CREATE INDEX IF NOT EXISTS idx_media_quota_config_is_enabled ON media_quota_config(is_enabled);

-- 服务器通知索引
CREATE INDEX IF NOT EXISTS idx_server_notifications_is_enabled ON server_notifications(is_enabled);
CREATE INDEX IF NOT EXISTS idx_user_notification_status_user_id ON user_notification_status(user_id);

-- 联邦黑名单索引
CREATE INDEX IF NOT EXISTS idx_federation_blacklist_server_name ON federation_blacklist(server_name);
CREATE INDEX IF NOT EXISTS idx_federation_blacklist_is_enabled ON federation_blacklist(is_enabled);

-- =============================================================================
-- 第三部分: 添加外键约束
-- =============================================================================

-- 注意: 添加外键约束前需要确保数据一致性
-- 以下约束使用 ON DELETE CASCADE 确保级联删除

-- access_tokens 外键
ALTER TABLE access_tokens 
DROP CONSTRAINT IF EXISTS fk_access_tokens_user_id,
ADD CONSTRAINT fk_access_tokens_user_id 
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;

-- refresh_tokens 外键
ALTER TABLE refresh_tokens 
DROP CONSTRAINT IF EXISTS fk_refresh_tokens_user_id,
ADD CONSTRAINT fk_refresh_tokens_user_id 
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;

-- devices 外键 (如果不存在)
DO $$ 
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'devices_user_id_fkey' 
        AND table_name = 'devices'
    ) THEN
        ALTER TABLE devices 
        ADD CONSTRAINT fk_devices_user_id 
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;
END $$;

-- user_media_quota 外键
ALTER TABLE user_media_quota 
DROP CONSTRAINT IF EXISTS fk_user_media_quota_user_id,
ADD CONSTRAINT fk_user_media_quota_user_id 
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;

ALTER TABLE user_media_quota 
DROP CONSTRAINT IF EXISTS fk_user_media_quota_quota_config_id,
ADD CONSTRAINT fk_user_media_quota_quota_config_id 
    FOREIGN KEY (quota_config_id) REFERENCES media_quota_config(id) ON DELETE SET NULL;

-- user_notification_status 外键
ALTER TABLE user_notification_status 
DROP CONSTRAINT IF EXISTS fk_user_notification_status_user_id,
ADD CONSTRAINT fk_user_notification_status_user_id 
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;

ALTER TABLE user_notification_status 
DROP CONSTRAINT IF EXISTS fk_user_notification_status_notification_id,
ADD CONSTRAINT fk_user_notification_status_notification_id 
    FOREIGN KEY (notification_id) REFERENCES server_notifications(id) ON DELETE CASCADE;

-- =============================================================================
-- 第四部分: 移除冗余字段
-- =============================================================================

-- 移除 access_tokens.ip 字段 (保留 ip_address)
ALTER TABLE access_tokens DROP COLUMN IF EXISTS ip;

-- =============================================================================
-- 更新版本记录
-- =============================================================================

INSERT INTO schema_migrations (version, description, success)
VALUES ('2.0.0', 'Phase 2: Boolean fields normalization, indexes, foreign keys', TRUE)
ON CONFLICT (version) DO UPDATE SET success = TRUE, executed_at = NOW();

UPDATE db_metadata SET value = '2.0.0', updated_ts = (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT
WHERE key = 'schema_version';

COMMIT;

-- =============================================================================
-- 验证脚本
-- =============================================================================

-- 验证索引创建
SELECT indexname, tablename FROM pg_indexes 
WHERE schemaname = 'public' 
AND indexname LIKE 'idx_%'
ORDER BY tablename, indexname;

-- 验证外键约束
SELECT tc.table_name, tc.constraint_name, tc.constraint_type
FROM information_schema.table_constraints tc
WHERE tc.constraint_type = 'FOREIGN KEY'
AND tc.table_schema = 'public'
ORDER BY tc.table_name;
