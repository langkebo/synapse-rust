-- =============================================================================
-- Synapse-Rust 数据库字段规范化迁移脚本
-- 版本: 1.2.0
-- 创建日期: 2026-02-20
-- 描述: 根据 DATABASE_FIELD_STANDARDS.md 规范，规范化字段命名
-- =============================================================================

-- =============================================================================
-- 第一部分: 布尔字段规范化 (添加 is_ 前缀)
-- 使用 DO 块确保列存在才执行重命名
-- =============================================================================

-- 1. users 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'users' AND column_name = 'deactivated') THEN
        ALTER TABLE users RENAME COLUMN deactivated TO is_deactivated;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'users' AND column_name = 'shadow_banned') THEN
        ALTER TABLE users RENAME COLUMN shadow_banned TO is_shadow_banned;
    END IF;
END $$;

-- 2. access_tokens 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'access_tokens' AND column_name = 'invalidated_ts') THEN
        ALTER TABLE access_tokens RENAME COLUMN invalidated_ts TO revoked_ts;
    END IF;
END $$;

-- 3. events 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'events' AND column_name = 'processed') THEN
        ALTER TABLE events RENAME COLUMN processed TO is_processed;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'events' AND column_name = 'outlier') THEN
        ALTER TABLE events RENAME COLUMN outlier TO is_outlier;
    END IF;
END $$;

-- 4. voice_messages 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'voice_messages' AND column_name = 'processed') THEN
        ALTER TABLE voice_messages RENAME COLUMN processed TO is_processed;
    END IF;
END $$;

-- 5. media_repository 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'media_repository' AND column_name = 'quarantine_media') THEN
        ALTER TABLE media_repository RENAME COLUMN quarantine_media TO is_quarantined;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'media_repository' AND column_name = 'safe_from_quarantine') THEN
        ALTER TABLE media_repository RENAME COLUMN safe_from_quarantine TO is_safe_from_quarantine;
    END IF;
END $$;

-- 6. space_children 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'space_children' AND column_name = 'suggested') THEN
        ALTER TABLE space_children RENAME COLUMN suggested TO is_suggested;
    END IF;
END $$;

-- 7. ip_reputation 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'ip_reputation' AND column_name = 'blocked') THEN
        ALTER TABLE ip_reputation RENAME COLUMN blocked TO is_blocked;
    END IF;
END $$;

-- 8-23. 其他表的布尔字段规范化
DO $$
BEGIN
    -- federation_blacklist
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'federation_blacklist' AND column_name = 'is_active') THEN
        ALTER TABLE federation_blacklist RENAME COLUMN is_active TO is_enabled;
    END IF;
    -- federation_blacklist_rule
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'federation_blacklist_rule' AND column_name = 'enabled') THEN
        ALTER TABLE federation_blacklist_rule RENAME COLUMN enabled TO is_enabled;
    END IF;
    -- modules
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'modules' AND column_name = 'enabled') THEN
        ALTER TABLE modules RENAME COLUMN enabled TO is_enabled;
    END IF;
    -- saml_identity_providers
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'saml_identity_providers' AND column_name = 'enabled') THEN
        ALTER TABLE saml_identity_providers RENAME COLUMN enabled TO is_enabled;
    END IF;
    -- cas_services
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'cas_services' AND column_name = 'enabled') THEN
        ALTER TABLE cas_services RENAME COLUMN enabled TO is_enabled;
    END IF;
    -- captcha_template
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'captcha_template' AND column_name = 'enabled') THEN
        ALTER TABLE captcha_template RENAME COLUMN enabled TO is_enabled;
    END IF;
    -- push_device
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'push_device' AND column_name = 'enabled') THEN
        ALTER TABLE push_device RENAME COLUMN enabled TO is_enabled;
    END IF;
    -- push_rule
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'push_rule' AND column_name = 'enabled') THEN
        ALTER TABLE push_rule RENAME COLUMN enabled TO is_enabled;
    END IF;
    -- pushers
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'pushers' AND column_name = 'enabled') THEN
        ALTER TABLE pushers RENAME COLUMN enabled TO is_enabled;
    END IF;
    -- push_rules
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'push_rules' AND column_name = 'enabled') THEN
        ALTER TABLE push_rules RENAME COLUMN enabled TO is_enabled;
    END IF;
    -- server_notifications
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'server_notifications' AND column_name = 'is_active') THEN
        ALTER TABLE server_notifications RENAME COLUMN is_active TO is_enabled;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'server_notifications' AND column_name = 'is_dismissible') THEN
        ALTER TABLE server_notifications RENAME COLUMN is_dismissible TO is_dismissable;
    END IF;
    -- application_services
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'application_services' AND column_name = 'is_active') THEN
        ALTER TABLE application_services RENAME COLUMN is_active TO is_enabled;
    END IF;
    -- registration_tokens
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'registration_tokens' AND column_name = 'is_active') THEN
        ALTER TABLE registration_tokens RENAME COLUMN is_active TO is_enabled;
    END IF;
    -- registration_token_batches
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'registration_token_batches' AND column_name = 'is_active') THEN
        ALTER TABLE registration_token_batches RENAME COLUMN is_active TO is_enabled;
    END IF;
    -- thread_roots
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'thread_roots' AND column_name = 'is_active') THEN
        ALTER TABLE thread_roots RENAME COLUMN is_active TO is_enabled;
    END IF;
    -- ip_blocks
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'ip_blocks' AND column_name = 'is_active') THEN
        ALTER TABLE ip_blocks RENAME COLUMN is_active TO is_enabled;
    END IF;
END $$;

-- =============================================================================
-- 第二部分: 时间字段规范化 - 统一使用 _ts 后缀
-- =============================================================================

-- devices 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'devices' AND column_name = 'created_at') THEN
        ALTER TABLE devices RENAME COLUMN created_at TO created_ts;
    END IF;
END $$;

-- federation_signing_keys 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'federation_signing_keys' AND column_name = 'created_at') THEN
        ALTER TABLE federation_signing_keys RENAME COLUMN created_at TO created_ts;
    END IF;
END $$;

-- ip_reputation 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'ip_reputation' AND column_name = 'last_failed_at') THEN
        ALTER TABLE ip_reputation RENAME COLUMN last_failed_at TO last_failed_ts;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'ip_reputation' AND column_name = 'last_success_at') THEN
        ALTER TABLE ip_reputation RENAME COLUMN last_success_at TO last_success_ts;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'ip_reputation' AND column_name = 'blocked_at') THEN
        ALTER TABLE ip_reputation RENAME COLUMN blocked_at TO blocked_ts;
    END IF;
END $$;

-- =============================================================================
-- 第三部分: 创建索引
-- =============================================================================

-- 用户表索引
CREATE INDEX IF NOT EXISTS idx_users_email ON users(email) WHERE email IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_users_creation_ts ON users(creation_ts);
CREATE INDEX IF NOT EXISTS idx_users_deactivated ON users(is_deactivated) WHERE is_deactivated = TRUE;

-- 访问令牌索引
CREATE INDEX IF NOT EXISTS idx_access_tokens_user_id ON access_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_access_tokens_expires_ts ON access_tokens(expires_ts);

-- 刷新令牌索引
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_user_id ON refresh_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_expires_at ON refresh_tokens(expires_at);

-- 设备索引
CREATE INDEX IF NOT EXISTS idx_devices_user_id ON devices(user_id);

-- 事件索引
CREATE INDEX IF NOT EXISTS idx_events_room_id ON events(room_id);
CREATE INDEX IF NOT EXISTS idx_events_sender ON events(sender);
CREATE INDEX IF NOT EXISTS idx_events_origin_server_ts ON events(origin_server_ts);

-- 房间成员索引
CREATE INDEX IF NOT EXISTS idx_room_members_user_id ON room_members(user_id);
CREATE INDEX IF NOT EXISTS idx_room_members_room_id ON room_members(room_id);

-- 推送器索引
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
-- 完成
-- =============================================================================

DO $$
BEGIN
    RAISE NOTICE '==========================================';
    RAISE NOTICE 'Field name normalization completed';
    RAISE NOTICE '==========================================';
END $$;
