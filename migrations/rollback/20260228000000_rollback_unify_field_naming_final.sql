-- =============================================================================
-- Synapse-Rust 数据库字段命名统一迁移回滚脚本
-- 版本: 1.0.0
-- 创建日期: 2026-02-28
-- 描述: 回滚字段命名统一迁移
-- =============================================================================

-- =============================================================================
-- 第一部分: 回滚布尔字段规范化
-- =============================================================================

-- users 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'users' AND column_name = 'is_deactivated') THEN
        ALTER TABLE users RENAME COLUMN is_deactivated TO deactivated;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'users' AND column_name = 'is_shadow_banned') THEN
        ALTER TABLE users RENAME COLUMN is_shadow_banned TO shadow_banned;
    END IF;
END $$;

-- application_service_statistics 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'application_service_statistics' AND column_name = 'is_enabled') THEN
        ALTER TABLE application_service_statistics RENAME COLUMN is_enabled TO is_active;
    END IF;
END $$;

-- media_quota_config 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'media_quota_config' AND column_name = 'is_enabled') THEN
        ALTER TABLE media_quota_config RENAME COLUMN is_enabled TO is_active;
    END IF;
END $$;

-- notification_templates 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'notification_templates' AND column_name = 'is_enabled') THEN
        ALTER TABLE notification_templates RENAME COLUMN is_enabled TO is_active;
    END IF;
END $$;

-- =============================================================================
-- 第二部分: 回滚时间字段规范化
-- =============================================================================

-- push_device 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'push_device' AND column_name = 'created_ts' AND data_type = 'bigint') THEN
        ALTER TABLE push_device ALTER COLUMN created_ts TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(created_ts / 1000.0) AT TIME ZONE 'UTC';
        ALTER TABLE push_device ALTER COLUMN created_ts SET DEFAULT NOW();
        ALTER TABLE push_device RENAME COLUMN created_ts TO created_at;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'push_device' AND column_name = 'updated_ts' AND data_type = 'bigint') THEN
        ALTER TABLE push_device ALTER COLUMN updated_ts TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(updated_ts / 1000.0) AT TIME ZONE 'UTC';
        ALTER TABLE push_device ALTER COLUMN updated_ts SET DEFAULT NOW();
        ALTER TABLE push_device RENAME COLUMN updated_ts TO updated_at;
    END IF;
END $$;

-- push_rule 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'push_rule' AND column_name = 'created_ts' AND data_type = 'bigint') THEN
        ALTER TABLE push_rule ALTER COLUMN created_ts TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(created_ts / 1000.0) AT TIME ZONE 'UTC';
        ALTER TABLE push_rule ALTER COLUMN created_ts SET DEFAULT NOW();
        ALTER TABLE push_rule RENAME COLUMN created_ts TO created_at;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'push_rule' AND column_name = 'updated_ts' AND data_type = 'bigint') THEN
        ALTER TABLE push_rule ALTER COLUMN updated_ts TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(updated_ts / 1000.0) AT TIME ZONE 'UTC';
        ALTER TABLE push_rule ALTER COLUMN updated_ts SET DEFAULT NOW();
        ALTER TABLE push_rule RENAME COLUMN updated_ts TO updated_at;
    END IF;
END $$;

-- push_notification_queue 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'push_notification_queue' AND column_name = 'created_ts' AND data_type = 'bigint') THEN
        ALTER TABLE push_notification_queue ALTER COLUMN created_ts TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(created_ts / 1000.0) AT TIME ZONE 'UTC';
        ALTER TABLE push_notification_queue ALTER COLUMN created_ts SET DEFAULT NOW();
        ALTER TABLE push_notification_queue RENAME COLUMN created_ts TO created_at;
    END IF;
END $$;

-- push_config 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'push_config' AND column_name = 'created_ts' AND data_type = 'bigint') THEN
        ALTER TABLE push_config ALTER COLUMN created_ts TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(created_ts / 1000.0) AT TIME ZONE 'UTC';
        ALTER TABLE push_config ALTER COLUMN created_ts SET DEFAULT NOW();
        ALTER TABLE push_config RENAME COLUMN created_ts TO created_at;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'push_config' AND column_name = 'updated_ts' AND data_type = 'bigint') THEN
        ALTER TABLE push_config ALTER COLUMN updated_ts TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(updated_ts / 1000.0) AT TIME ZONE 'UTC';
        ALTER TABLE push_config ALTER COLUMN updated_ts SET DEFAULT NOW();
        ALTER TABLE push_config RENAME COLUMN updated_ts TO updated_at;
    END IF;
END $$;

-- push_stats 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'push_stats' AND column_name = 'created_ts' AND data_type = 'bigint') THEN
        ALTER TABLE push_stats ALTER COLUMN created_ts TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(created_ts / 1000.0) AT TIME ZONE 'UTC';
        ALTER TABLE push_stats ALTER COLUMN created_ts SET DEFAULT NOW();
        ALTER TABLE push_stats RENAME COLUMN created_ts TO created_at;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'push_stats' AND column_name = 'updated_ts' AND data_type = 'bigint') THEN
        ALTER TABLE push_stats ALTER COLUMN updated_ts TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(updated_ts / 1000.0) AT TIME ZONE 'UTC';
        ALTER TABLE push_stats ALTER COLUMN updated_ts SET DEFAULT NOW();
        ALTER TABLE push_stats RENAME COLUMN updated_ts TO updated_at;
    END IF;
END $$;

-- registration_captcha 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'registration_captcha' AND column_name = 'created_ts' AND data_type = 'bigint') THEN
        ALTER TABLE registration_captcha ALTER COLUMN created_ts TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(created_ts / 1000.0) AT TIME ZONE 'UTC';
        ALTER TABLE registration_captcha ALTER COLUMN created_ts SET DEFAULT NOW();
        ALTER TABLE registration_captcha RENAME COLUMN created_ts TO created_at;
    END IF;
END $$;

-- captcha_template 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'captcha_template' AND column_name = 'created_ts' AND data_type = 'bigint') THEN
        ALTER TABLE captcha_template ALTER COLUMN created_ts TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(created_ts / 1000.0) AT TIME ZONE 'UTC';
        ALTER TABLE captcha_template ALTER COLUMN created_ts SET DEFAULT NOW();
        ALTER TABLE captcha_template RENAME COLUMN created_ts TO created_at;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'captcha_template' AND column_name = 'updated_ts' AND data_type = 'bigint') THEN
        ALTER TABLE captcha_template ALTER COLUMN updated_ts TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(updated_ts / 1000.0) AT TIME ZONE 'UTC';
        ALTER TABLE captcha_template ALTER COLUMN updated_ts SET DEFAULT NOW();
        ALTER TABLE captcha_template RENAME COLUMN updated_ts TO updated_at;
    END IF;
END $$;

-- captcha_config 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'captcha_config' AND column_name = 'created_ts' AND data_type = 'bigint') THEN
        ALTER TABLE captcha_config ALTER COLUMN created_ts TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(created_ts / 1000.0) AT TIME ZONE 'UTC';
        ALTER TABLE captcha_config ALTER COLUMN created_ts SET DEFAULT NOW();
        ALTER TABLE captcha_config RENAME COLUMN created_ts TO created_at;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'captcha_config' AND column_name = 'updated_ts' AND data_type = 'bigint') THEN
        ALTER TABLE captcha_config ALTER COLUMN updated_ts TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(updated_ts / 1000.0) AT TIME ZONE 'UTC';
        ALTER TABLE captcha_config ALTER COLUMN updated_ts SET DEFAULT NOW();
        ALTER TABLE captcha_config RENAME COLUMN updated_ts TO updated_at;
    END IF;
END $$;

-- saml_sessions 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'saml_sessions' AND column_name = 'created_ts' AND data_type = 'bigint') THEN
        ALTER TABLE saml_sessions ALTER COLUMN created_ts TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(created_ts / 1000.0) AT TIME ZONE 'UTC';
        ALTER TABLE saml_sessions ALTER COLUMN created_ts SET DEFAULT NOW();
        ALTER TABLE saml_sessions RENAME COLUMN created_ts TO created_at;
    END IF;
END $$;

-- saml_identity_providers 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'saml_identity_providers' AND column_name = 'created_ts' AND data_type = 'bigint') THEN
        ALTER TABLE saml_identity_providers ALTER COLUMN created_ts TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(created_ts / 1000.0) AT TIME ZONE 'UTC';
        ALTER TABLE saml_identity_providers ALTER COLUMN created_ts SET DEFAULT NOW();
        ALTER TABLE saml_identity_providers RENAME COLUMN created_ts TO created_at;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'saml_identity_providers' AND column_name = 'updated_ts' AND data_type = 'bigint') THEN
        ALTER TABLE saml_identity_providers ALTER COLUMN updated_ts TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(updated_ts / 1000.0) AT TIME ZONE 'UTC';
        ALTER TABLE saml_identity_providers ALTER COLUMN updated_ts SET DEFAULT NOW();
        ALTER TABLE saml_identity_providers RENAME COLUMN updated_ts TO updated_at;
    END IF;
END $$;

-- cas_tickets 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'cas_tickets' AND column_name = 'created_ts' AND data_type = 'bigint') THEN
        ALTER TABLE cas_tickets ALTER COLUMN created_ts TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(created_ts / 1000.0) AT TIME ZONE 'UTC';
        ALTER TABLE cas_tickets ALTER COLUMN created_ts SET DEFAULT NOW();
        ALTER TABLE cas_tickets RENAME COLUMN created_ts TO created_at;
    END IF;
END $$;

-- cas_services 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'cas_services' AND column_name = 'created_ts' AND data_type = 'bigint') THEN
        ALTER TABLE cas_services ALTER COLUMN created_ts TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(created_ts / 1000.0) AT TIME ZONE 'UTC';
        ALTER TABLE cas_services ALTER COLUMN created_ts SET DEFAULT NOW();
        ALTER TABLE cas_services RENAME COLUMN created_ts TO created_at;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'cas_services' AND column_name = 'updated_ts' AND data_type = 'bigint') THEN
        ALTER TABLE cas_services ALTER COLUMN updated_ts TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(updated_ts / 1000.0) AT TIME ZONE 'UTC';
        ALTER TABLE cas_services ALTER COLUMN updated_ts SET DEFAULT NOW();
        ALTER TABLE cas_services RENAME COLUMN updated_ts TO updated_at;
    END IF;
END $$;

-- cas_user_attributes 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'cas_user_attributes' AND column_name = 'created_ts' AND data_type = 'bigint') THEN
        ALTER TABLE cas_user_attributes ALTER COLUMN created_ts TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(created_ts / 1000.0) AT TIME ZONE 'UTC';
        ALTER TABLE cas_user_attributes ALTER COLUMN created_ts SET DEFAULT NOW();
        ALTER TABLE cas_user_attributes RENAME COLUMN created_ts TO created_at;
    END IF;
END $$;

-- modules 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'modules' AND column_name = 'created_ts' AND data_type = 'bigint') THEN
        ALTER TABLE modules ALTER COLUMN created_ts TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(created_ts / 1000.0) AT TIME ZONE 'UTC';
        ALTER TABLE modules ALTER COLUMN created_ts SET DEFAULT NOW();
        ALTER TABLE modules RENAME COLUMN created_ts TO created_at;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'modules' AND column_name = 'updated_ts' AND data_type = 'bigint') THEN
        ALTER TABLE modules ALTER COLUMN updated_ts TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(updated_ts / 1000.0) AT TIME ZONE 'UTC';
        ALTER TABLE modules ALTER COLUMN updated_ts SET DEFAULT NOW();
        ALTER TABLE modules RENAME COLUMN updated_ts TO updated_at;
    END IF;
END $$;

-- module_execution_logs 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'module_execution_logs' AND column_name = 'created_ts' AND data_type = 'bigint') THEN
        ALTER TABLE module_execution_logs ALTER COLUMN created_ts TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(created_ts / 1000.0) AT TIME ZONE 'UTC';
        ALTER TABLE module_execution_logs ALTER COLUMN created_ts SET DEFAULT NOW();
        ALTER TABLE module_execution_logs RENAME COLUMN created_ts TO created_at;
    END IF;
END $$;

-- federation_blacklist 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'federation_blacklist' AND column_name = 'blocked_ts' AND data_type = 'bigint') THEN
        ALTER TABLE federation_blacklist ALTER COLUMN blocked_ts TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(blocked_ts / 1000.0) AT TIME ZONE 'UTC';
        ALTER TABLE federation_blacklist ALTER COLUMN blocked_ts SET DEFAULT NOW();
        ALTER TABLE federation_blacklist RENAME COLUMN blocked_ts TO blocked_at;
    END IF;
END $$;

-- federation_blacklist_rule 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'federation_blacklist_rule' AND column_name = 'created_ts' AND data_type = 'bigint') THEN
        ALTER TABLE federation_blacklist_rule ALTER COLUMN created_ts TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(created_ts / 1000.0) AT TIME ZONE 'UTC';
        ALTER TABLE federation_blacklist_rule ALTER COLUMN created_ts SET DEFAULT NOW();
        ALTER TABLE federation_blacklist_rule RENAME COLUMN created_ts TO created_at;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'federation_blacklist_rule' AND column_name = 'updated_ts' AND data_type = 'bigint') THEN
        ALTER TABLE federation_blacklist_rule ALTER COLUMN updated_ts TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(updated_ts / 1000.0) AT TIME ZONE 'UTC';
        ALTER TABLE federation_blacklist_rule ALTER COLUMN updated_ts SET DEFAULT NOW();
        ALTER TABLE federation_blacklist_rule RENAME COLUMN updated_ts TO updated_at;
    END IF;
END $$;

-- federation_blacklist_log 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'federation_blacklist_log' AND column_name = 'created_ts' AND data_type = 'bigint') THEN
        ALTER TABLE federation_blacklist_log ALTER COLUMN created_ts TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(created_ts / 1000.0) AT TIME ZONE 'UTC';
        ALTER TABLE federation_blacklist_log ALTER COLUMN created_ts SET DEFAULT NOW();
        ALTER TABLE federation_blacklist_log RENAME COLUMN created_ts TO created_at;
    END IF;
END $$;

-- federation_blacklist_config 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'federation_blacklist_config' AND column_name = 'created_ts' AND data_type = 'bigint') THEN
        ALTER TABLE federation_blacklist_config ALTER COLUMN created_ts TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(created_ts / 1000.0) AT TIME ZONE 'UTC';
        ALTER TABLE federation_blacklist_config ALTER COLUMN created_ts SET DEFAULT NOW();
        ALTER TABLE federation_blacklist_config RENAME COLUMN created_ts TO created_at;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'federation_blacklist_config' AND column_name = 'updated_ts' AND data_type = 'bigint') THEN
        ALTER TABLE federation_blacklist_config ALTER COLUMN updated_ts TYPE TIMESTAMP WITH TIME ZONE USING to_timestamp(updated_ts / 1000.0) AT TIME ZONE 'UTC';
        ALTER TABLE federation_blacklist_config ALTER COLUMN updated_ts SET DEFAULT NOW();
        ALTER TABLE federation_blacklist_config RENAME COLUMN updated_ts TO updated_at;
    END IF;
END $$;

-- =============================================================================
-- 第三部分: 删除版本记录
-- =============================================================================

DELETE FROM schema_migrations WHERE version = '20260228000000';

DO $$
BEGIN
    RAISE NOTICE '==========================================';
    RAISE NOTICE '字段命名统一迁移回滚完成';
    RAISE NOTICE '==========================================';
END $$;
