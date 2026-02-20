-- =============================================================================
-- 数据库字段规范化迁移脚本 - 第二阶段 (修复版)
-- 版本: 2.0.1
-- 创建日期: 2026-02-20
-- 描述: 完成布尔字段规范化、添加索引和外键约束
-- 修复: 添加条件检查避免重复操作和表/列不存在导致的错误
-- =============================================================================

BEGIN;

-- =============================================================================
-- 第一部分: 布尔字段规范化 (添加 is_ 前缀)
-- =============================================================================

-- 使用 DO 块进行条件检查，避免重复操作

DO $$
BEGIN
    -- registration_tokens 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'registration_tokens' AND column_name = 'is_active') THEN
        ALTER TABLE registration_tokens RENAME COLUMN is_active TO is_enabled;
    END IF;
    
    -- registration_token_batches 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'registration_token_batches' AND column_name = 'is_active') THEN
        ALTER TABLE registration_token_batches RENAME COLUMN is_active TO is_enabled;
    END IF;
    
    -- federation_blacklist 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'federation_blacklist' AND column_name = 'is_active') THEN
        ALTER TABLE federation_blacklist RENAME COLUMN is_active TO is_enabled;
    END IF;
    
    -- federation_blacklist_rule 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'federation_blacklist_rule' AND column_name = 'enabled') THEN
        ALTER TABLE federation_blacklist_rule RENAME COLUMN enabled TO is_enabled;
    END IF;
    
    -- notification_templates 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'notification_templates' AND column_name = 'is_active') THEN
        ALTER TABLE notification_templates RENAME COLUMN is_active TO is_enabled;
    END IF;
    
    -- account_data_callbacks 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'account_data_callbacks' AND column_name = 'enabled') THEN
        ALTER TABLE account_data_callbacks RENAME COLUMN enabled TO is_enabled;
    END IF;
    
    -- captcha_template 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'captcha_template' AND column_name = 'enabled') THEN
        ALTER TABLE captcha_template RENAME COLUMN enabled TO is_enabled;
    END IF;
    
    -- cross_signing_keys 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'cross_signing_keys' AND column_name = 'blocked') THEN
        ALTER TABLE cross_signing_keys RENAME COLUMN blocked TO is_blocked;
    END IF;
    
    -- device_keys 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'device_keys' AND column_name = 'blocked') THEN
        ALTER TABLE device_keys RENAME COLUMN blocked TO is_blocked;
    END IF;
    
    -- media_callbacks 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'media_callbacks' AND column_name = 'enabled') THEN
        ALTER TABLE media_callbacks RENAME COLUMN enabled TO is_enabled;
    END IF;
    
    -- modules 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'modules' AND column_name = 'enabled') THEN
        ALTER TABLE modules RENAME COLUMN enabled TO is_enabled;
    END IF;
    
    -- password_auth_providers 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'password_auth_providers' AND column_name = 'enabled') THEN
        ALTER TABLE password_auth_providers RENAME COLUMN enabled TO is_enabled;
    END IF;
    
    -- presence_routes 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'presence_routes' AND column_name = 'enabled') THEN
        ALTER TABLE presence_routes RENAME COLUMN enabled TO is_enabled;
    END IF;
    
    -- push_rules 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'push_rules' AND column_name = 'enabled') THEN
        ALTER TABLE push_rules RENAME COLUMN enabled TO is_enabled;
    END IF;
    
    -- pushers 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'pushers' AND column_name = 'enabled') THEN
        ALTER TABLE pushers RENAME COLUMN enabled TO is_enabled;
    END IF;
    
    -- rate_limit_callbacks 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'rate_limit_callbacks' AND column_name = 'enabled') THEN
        ALTER TABLE rate_limit_callbacks RENAME COLUMN enabled TO is_enabled;
    END IF;
    
    -- saml_identity_providers 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'saml_identity_providers' AND column_name = 'enabled') THEN
        ALTER TABLE saml_identity_providers RENAME COLUMN enabled TO is_enabled;
    END IF;
    
    -- media_repository 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'media_repository' AND column_name = 'quarantined') THEN
        ALTER TABLE media_repository RENAME COLUMN quarantined TO is_quarantined;
    END IF;
    
    -- space_children 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'space_children' AND column_name = 'suggested') THEN
        ALTER TABLE space_children RENAME COLUMN suggested TO is_suggested;
    END IF;
    
    -- refresh_tokens 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'refresh_tokens' AND column_name = 'invalidated') THEN
        ALTER TABLE refresh_tokens RENAME COLUMN invalidated TO is_revoked;
    END IF;
    
    -- media_quota_config 表 (需要同时处理 is_active 和 is_enabled 的情况)
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'media_quota_config' AND column_name = 'is_active') THEN
        -- 先检查 is_enabled 是否已存在
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'media_quota_config' AND column_name = 'is_enabled') THEN
            ALTER TABLE media_quota_config RENAME COLUMN is_active TO is_enabled;
        ELSE
            -- 如果 is_enabled 已存在，删除 is_active
            ALTER TABLE media_quota_config DROP COLUMN is_active;
        END IF;
    END IF;
    
    RAISE NOTICE '布尔字段规范化完成';
END $$;

-- =============================================================================
-- 第二部分: 添加缺失的索引
-- =============================================================================

CREATE INDEX IF NOT EXISTS idx_users_email ON users(email);
CREATE INDEX IF NOT EXISTS idx_users_creation_ts ON users(creation_ts);
CREATE INDEX IF NOT EXISTS idx_users_deactivated ON users(is_deactivated) WHERE is_deactivated = TRUE;

CREATE INDEX IF NOT EXISTS idx_access_tokens_user_id ON access_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_access_tokens_expires_ts ON access_tokens(expires_ts);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_user_id ON refresh_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_expires_at ON refresh_tokens(expires_at);

CREATE INDEX IF NOT EXISTS idx_devices_user_id ON devices(user_id);
CREATE INDEX IF NOT EXISTS idx_devices_last_seen_ts ON devices(last_seen_ts DESC);

CREATE INDEX IF NOT EXISTS idx_events_room_id ON events(room_id);
CREATE INDEX IF NOT EXISTS idx_events_sender ON events(sender);
CREATE INDEX IF NOT EXISTS idx_events_origin_server_ts ON events(origin_server_ts DESC);

CREATE INDEX IF NOT EXISTS idx_room_members_user_id ON room_members(user_id);
CREATE INDEX IF NOT EXISTS idx_room_members_room_id ON room_members(room_id);

CREATE INDEX IF NOT EXISTS idx_pushers_user_id ON pushers(user_id);

CREATE INDEX IF NOT EXISTS idx_user_media_quota_user_id ON user_media_quota(user_id);

-- media_quota_config 索引 (使用 is_enabled 而非 is_active)
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'media_quota_config' AND column_name = 'is_enabled') THEN
        CREATE INDEX IF NOT EXISTS idx_media_quota_config_is_enabled ON media_quota_config(is_enabled);
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_server_notifications_is_enabled ON server_notifications(is_enabled);
CREATE INDEX IF NOT EXISTS idx_user_notification_status_user_id ON user_notification_status(user_id);

CREATE INDEX IF NOT EXISTS idx_federation_blacklist_server_name ON federation_blacklist(server_name);
CREATE INDEX IF NOT EXISTS idx_federation_blacklist_is_enabled ON federation_blacklist(is_enabled);

-- =============================================================================
-- 第三部分: 添加外键约束
-- =============================================================================

-- access_tokens 外键
DO $$ 
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_access_tokens_user_id' AND table_name = 'access_tokens'
    ) THEN
        ALTER TABLE access_tokens 
        ADD CONSTRAINT fk_access_tokens_user_id 
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;
END $$;

-- refresh_tokens 外键
DO $$ 
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_refresh_tokens_user_id' AND table_name = 'refresh_tokens'
    ) THEN
        ALTER TABLE refresh_tokens 
        ADD CONSTRAINT fk_refresh_tokens_user_id 
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;
END $$;

-- devices 外键
DO $$ 
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'devices_user_id_fkey' AND table_name = 'devices'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_devices_user_id' AND table_name = 'devices'
    ) THEN
        ALTER TABLE devices 
        ADD CONSTRAINT fk_devices_user_id 
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;
END $$;

-- user_media_quota 外键
DO $$ 
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_user_media_quota_user_id' AND table_name = 'user_media_quota'
    ) THEN
        ALTER TABLE user_media_quota 
        ADD CONSTRAINT fk_user_media_quota_user_id 
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;
END $$;

-- user_media_quota quota_config_id 外键 (仅当列存在时)
DO $$ 
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'user_media_quota' AND column_name = 'quota_config_id') THEN
        IF NOT EXISTS (
            SELECT 1 FROM information_schema.table_constraints 
            WHERE constraint_name = 'fk_user_media_quota_quota_config_id' AND table_name = 'user_media_quota'
        ) THEN
            ALTER TABLE user_media_quota 
            ADD CONSTRAINT fk_user_media_quota_quota_config_id 
                FOREIGN KEY (quota_config_id) REFERENCES media_quota_config(id) ON DELETE SET NULL;
        END IF;
    END IF;
END $$;

-- user_notification_status 外键
DO $$ 
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_user_notification_status_user_id' AND table_name = 'user_notification_status'
    ) THEN
        ALTER TABLE user_notification_status 
        ADD CONSTRAINT fk_user_notification_status_user_id 
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;
END $$;

DO $$ 
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_user_notification_status_notification_id' AND table_name = 'user_notification_status'
    ) THEN
        ALTER TABLE user_notification_status 
        ADD CONSTRAINT fk_user_notification_status_notification_id 
            FOREIGN KEY (notification_id) REFERENCES server_notifications(id) ON DELETE CASCADE;
    END IF;
END $$;

-- =============================================================================
-- 第四部分: 移除冗余字段
-- =============================================================================

ALTER TABLE access_tokens DROP COLUMN IF EXISTS ip;

-- =============================================================================
-- 更新版本记录
-- =============================================================================

INSERT INTO schema_migrations (version, description, success)
VALUES ('2.0.1', 'Phase 2 (Fixed): Boolean fields normalization, indexes, foreign keys', TRUE)
ON CONFLICT (version) DO UPDATE SET success = TRUE, executed_at = NOW();

UPDATE db_metadata SET value = '2.0.1', updated_ts = (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT
WHERE key = 'schema_version';

COMMIT;

-- =============================================================================
-- 验证脚本
-- =============================================================================

SELECT indexname, tablename FROM pg_indexes 
WHERE schemaname = 'public' 
AND indexname LIKE 'idx_%'
ORDER BY tablename, indexname;

SELECT tc.table_name, tc.constraint_name, tc.constraint_type
FROM information_schema.table_constraints tc
WHERE tc.constraint_type = 'FOREIGN KEY'
AND tc.table_schema = 'public'
ORDER BY tc.table_name;
