-- =============================================================================
-- 数据库字段规范化迁移脚本 - 第二阶段 (修复版)
-- 版本: 2.0.2
-- 创建日期: 2026-02-20
-- 描述: 完成布尔字段规范化、添加索引和外键约束
-- 修复: 移除顶层事务，每个DO块独立执行
-- =============================================================================

-- =============================================================================
-- 第一部分: 布尔字段规范化 (添加 is_ 前缀)
-- 使用 DO 块进行条件检查，避免重复操作
-- =============================================================================

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'registration_tokens' AND column_name = 'is_active') THEN
        ALTER TABLE registration_tokens RENAME COLUMN is_active TO is_enabled;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'registration_token_batches' AND column_name = 'is_active') THEN
        ALTER TABLE registration_token_batches RENAME COLUMN is_active TO is_enabled;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'federation_blacklist' AND column_name = 'is_active') THEN
        ALTER TABLE federation_blacklist RENAME COLUMN is_active TO is_enabled;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'federation_blacklist_rule' AND column_name = 'enabled') THEN
        ALTER TABLE federation_blacklist_rule RENAME COLUMN enabled TO is_enabled;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'notification_templates' AND column_name = 'is_active') THEN
        ALTER TABLE notification_templates RENAME COLUMN is_active TO is_enabled;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'account_data_callbacks' AND column_name = 'enabled') THEN
        ALTER TABLE account_data_callbacks RENAME COLUMN enabled TO is_enabled;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'captcha_template' AND column_name = 'enabled') THEN
        ALTER TABLE captcha_template RENAME COLUMN enabled TO is_enabled;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'cross_signing_keys' AND column_name = 'blocked') THEN
        ALTER TABLE cross_signing_keys RENAME COLUMN blocked TO is_blocked;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'device_keys' AND column_name = 'blocked') THEN
        ALTER TABLE device_keys RENAME COLUMN blocked TO is_blocked;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'media_callbacks' AND column_name = 'enabled') THEN
        ALTER TABLE media_callbacks RENAME COLUMN enabled TO is_enabled;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'modules' AND column_name = 'enabled') THEN
        ALTER TABLE modules RENAME COLUMN enabled TO is_enabled;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'password_auth_providers' AND column_name = 'enabled') THEN
        ALTER TABLE password_auth_providers RENAME COLUMN enabled TO is_enabled;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'presence_routes' AND column_name = 'enabled') THEN
        ALTER TABLE presence_routes RENAME COLUMN enabled TO is_enabled;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'push_rules' AND column_name = 'enabled') THEN
        ALTER TABLE push_rules RENAME COLUMN enabled TO is_enabled;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'pushers' AND column_name = 'enabled') THEN
        ALTER TABLE pushers RENAME COLUMN enabled TO is_enabled;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'rate_limit_callbacks' AND column_name = 'enabled') THEN
        ALTER TABLE rate_limit_callbacks RENAME COLUMN enabled TO is_enabled;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'saml_identity_providers' AND column_name = 'enabled') THEN
        ALTER TABLE saml_identity_providers RENAME COLUMN enabled TO is_enabled;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'cas_services' AND column_name = 'enabled') THEN
        ALTER TABLE cas_services RENAME COLUMN enabled TO is_enabled;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'push_device' AND column_name = 'enabled') THEN
        ALTER TABLE push_device RENAME COLUMN enabled TO is_enabled;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'push_rule' AND column_name = 'enabled') THEN
        ALTER TABLE push_rule RENAME COLUMN enabled TO is_enabled;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'server_notifications' AND column_name = 'is_active') THEN
        ALTER TABLE server_notifications RENAME COLUMN is_active TO is_enabled;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'server_notifications' AND column_name = 'is_dismissible') THEN
        ALTER TABLE server_notifications RENAME COLUMN is_dismissible TO is_dismissable;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'application_services' AND column_name = 'is_active') THEN
        ALTER TABLE application_services RENAME COLUMN is_active TO is_enabled;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'thread_roots' AND column_name = 'is_active') THEN
        ALTER TABLE thread_roots RENAME COLUMN is_active TO is_enabled;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'ip_blocks' AND column_name = 'is_active') THEN
        ALTER TABLE ip_blocks RENAME COLUMN is_active TO is_enabled;
    END IF;
END $$;

-- =============================================================================
-- 第二部分: 创建索引 (使用 IF NOT EXISTS 避免重复)
-- =============================================================================

CREATE INDEX IF NOT EXISTS idx_users_email ON users(email) WHERE email IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_users_creation_ts ON users(creation_ts);
CREATE INDEX IF NOT EXISTS idx_access_tokens_user_id ON access_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_user_id ON refresh_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_expires_at ON refresh_tokens(expires_at);
CREATE INDEX IF NOT EXISTS idx_devices_user_id ON devices(user_id);
CREATE INDEX IF NOT EXISTS idx_events_room_id ON events(room_id);
CREATE INDEX IF NOT EXISTS idx_events_sender ON events(sender);
CREATE INDEX IF NOT EXISTS idx_events_origin_server_ts ON events(origin_server_ts);
CREATE INDEX IF NOT EXISTS idx_room_members_user_id ON room_members(user_id);
CREATE INDEX IF NOT EXISTS idx_room_members_room_id ON room_members(room_id);
CREATE INDEX IF NOT EXISTS idx_pushers_user_id ON pushers(user_id);
CREATE INDEX IF NOT EXISTS idx_user_media_quota_user_id ON user_media_quota(user_id);
CREATE INDEX IF NOT EXISTS idx_media_quota_config_is_enabled ON media_quota_config(is_enabled);
CREATE INDEX IF NOT EXISTS idx_server_notifications_is_enabled ON server_notifications(is_enabled);
CREATE INDEX IF NOT EXISTS idx_user_notification_status_user_id ON user_notification_status(user_id);
CREATE INDEX IF NOT EXISTS idx_federation_blacklist_server_name ON federation_blacklist(server_name);
CREATE INDEX IF NOT EXISTS idx_federation_blacklist_is_enabled ON federation_blacklist(is_enabled);

-- =============================================================================
-- 完成
-- =============================================================================

DO $$
BEGIN
    RAISE NOTICE '==========================================';
    RAISE NOTICE '布尔字段规范化完成';
    RAISE NOTICE '==========================================';
END $$;
