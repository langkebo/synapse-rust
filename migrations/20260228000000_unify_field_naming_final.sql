-- =============================================================================
-- Synapse-Rust 数据库字段命名统一迁移脚本 - 最终版
-- 版本: 1.0.0
-- 创建日期: 2026-02-28
-- 描述: 统一所有字段命名规范，确保与 DATABASE_FIELD_STANDARDS.md 一致
-- 
-- 命名规范:
-- - 使用 snake_case
-- - 布尔字段使用 is_/has_ 前缀
-- - 时间戳字段使用 _ts 后缀（毫秒级）
-- - 可选时间戳使用 _at 后缀
-- =============================================================================

-- =============================================================================
-- 第一部分: 布尔字段规范化 (添加 is_ 前缀)
-- =============================================================================

-- users 表: deactivated -> is_deactivated, shadow_banned -> is_shadow_banned
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'users' AND column_name = 'deactivated' AND column_name NOT IN (SELECT column_name FROM information_schema.columns WHERE table_name = 'users' AND column_name = 'is_deactivated')) THEN
        ALTER TABLE users RENAME COLUMN deactivated TO is_deactivated;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'users' AND column_name = 'shadow_banned' AND column_name NOT IN (SELECT column_name FROM information_schema.columns WHERE table_name = 'users' AND column_name = 'is_shadow_banned')) THEN
        ALTER TABLE users RENAME COLUMN shadow_banned TO is_shadow_banned;
    END IF;
END $$;

-- application_service_statistics 表: is_active -> is_enabled
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'application_service_statistics' AND column_name = 'is_active') THEN
        ALTER TABLE application_service_statistics RENAME COLUMN is_active TO is_enabled;
    END IF;
END $$;

-- media_quota_config 表: is_active -> is_enabled
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'media_quota_config' AND column_name = 'is_active') THEN
        ALTER TABLE media_quota_config RENAME COLUMN is_active TO is_enabled;
    END IF;
END $$;

-- notification_templates 表: is_active -> is_enabled
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'notification_templates' AND column_name = 'is_active') THEN
        ALTER TABLE notification_templates RENAME COLUMN is_active TO is_enabled;
    END IF;
END $$;

-- =============================================================================
-- 第二部分: 时间字段规范化 (created_at/updated_at -> created_ts/updated_ts)
-- =============================================================================

-- push_device 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'push_device' AND column_name = 'created_at' AND data_type = 'timestamp with time zone') THEN
        ALTER TABLE push_device ALTER COLUMN created_at DROP DEFAULT;
        ALTER TABLE push_device ALTER COLUMN created_at TYPE BIGINT USING (EXTRACT(EPOCH FROM COALESCE(created_at, NOW())) * 1000)::BIGINT;
        ALTER TABLE push_device RENAME COLUMN created_at TO created_ts;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'push_device' AND column_name = 'updated_at' AND data_type = 'timestamp with time zone') THEN
        ALTER TABLE push_device ALTER COLUMN updated_at DROP DEFAULT;
        ALTER TABLE push_device ALTER COLUMN updated_at TYPE BIGINT USING (EXTRACT(EPOCH FROM COALESCE(updated_at, NOW())) * 1000)::BIGINT;
        ALTER TABLE push_device RENAME COLUMN updated_at TO updated_ts;
    END IF;
END $$;

-- push_rule 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'push_rule' AND column_name = 'created_at' AND data_type = 'timestamp with time zone') THEN
        ALTER TABLE push_rule ALTER COLUMN created_at DROP DEFAULT;
        ALTER TABLE push_rule ALTER COLUMN created_at TYPE BIGINT USING (EXTRACT(EPOCH FROM COALESCE(created_at, NOW())) * 1000)::BIGINT;
        ALTER TABLE push_rule RENAME COLUMN created_at TO created_ts;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'push_rule' AND column_name = 'updated_at' AND data_type = 'timestamp with time zone') THEN
        ALTER TABLE push_rule ALTER COLUMN updated_at DROP DEFAULT;
        ALTER TABLE push_rule ALTER COLUMN updated_at TYPE BIGINT USING (EXTRACT(EPOCH FROM COALESCE(updated_at, NOW())) * 1000)::BIGINT;
        ALTER TABLE push_rule RENAME COLUMN updated_at TO updated_ts;
    END IF;
END $$;

-- push_notification_queue 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'push_notification_queue' AND column_name = 'created_at' AND data_type = 'timestamp with time zone') THEN
        ALTER TABLE push_notification_queue ALTER COLUMN created_at DROP DEFAULT;
        ALTER TABLE push_notification_queue ALTER COLUMN created_at TYPE BIGINT USING (EXTRACT(EPOCH FROM COALESCE(created_at, NOW())) * 1000)::BIGINT;
        ALTER TABLE push_notification_queue RENAME COLUMN created_at TO created_ts;
    END IF;
END $$;

-- push_config 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'push_config' AND column_name = 'created_at' AND data_type = 'timestamp with time zone') THEN
        ALTER TABLE push_config ALTER COLUMN created_at DROP DEFAULT;
        ALTER TABLE push_config ALTER COLUMN created_at TYPE BIGINT USING (EXTRACT(EPOCH FROM COALESCE(created_at, NOW())) * 1000)::BIGINT;
        ALTER TABLE push_config RENAME COLUMN created_at TO created_ts;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'push_config' AND column_name = 'updated_at' AND data_type = 'timestamp with time zone') THEN
        ALTER TABLE push_config ALTER COLUMN updated_at DROP DEFAULT;
        ALTER TABLE push_config ALTER COLUMN updated_at TYPE BIGINT USING (EXTRACT(EPOCH FROM COALESCE(updated_at, NOW())) * 1000)::BIGINT;
        ALTER TABLE push_config RENAME COLUMN updated_at TO updated_ts;
    END IF;
END $$;

-- push_stats 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'push_stats' AND column_name = 'created_at' AND data_type = 'timestamp with time zone') THEN
        ALTER TABLE push_stats ALTER COLUMN created_at DROP DEFAULT;
        ALTER TABLE push_stats ALTER COLUMN created_at TYPE BIGINT USING (EXTRACT(EPOCH FROM COALESCE(created_at, NOW())) * 1000)::BIGINT;
        ALTER TABLE push_stats RENAME COLUMN created_at TO created_ts;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'push_stats' AND column_name = 'updated_at' AND data_type = 'timestamp with time zone') THEN
        ALTER TABLE push_stats ALTER COLUMN updated_at DROP DEFAULT;
        ALTER TABLE push_stats ALTER COLUMN updated_at TYPE BIGINT USING (EXTRACT(EPOCH FROM COALESCE(updated_at, NOW())) * 1000)::BIGINT;
        ALTER TABLE push_stats RENAME COLUMN updated_at TO updated_ts;
    END IF;
END $$;

-- registration_captcha 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'registration_captcha' AND column_name = 'created_at' AND data_type = 'timestamp with time zone') THEN
        ALTER TABLE registration_captcha ALTER COLUMN created_at DROP DEFAULT;
        ALTER TABLE registration_captcha ALTER COLUMN created_at TYPE BIGINT USING (EXTRACT(EPOCH FROM COALESCE(created_at, NOW())) * 1000)::BIGINT;
        ALTER TABLE registration_captcha RENAME COLUMN created_at TO created_ts;
    END IF;
END $$;

-- captcha_template 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'captcha_template' AND column_name = 'created_at' AND data_type = 'timestamp with time zone') THEN
        ALTER TABLE captcha_template ALTER COLUMN created_at DROP DEFAULT;
        ALTER TABLE captcha_template ALTER COLUMN created_at TYPE BIGINT USING (EXTRACT(EPOCH FROM COALESCE(created_at, NOW())) * 1000)::BIGINT;
        ALTER TABLE captcha_template RENAME COLUMN created_at TO created_ts;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'captcha_template' AND column_name = 'updated_at' AND data_type = 'timestamp with time zone') THEN
        ALTER TABLE captcha_template ALTER COLUMN updated_at DROP DEFAULT;
        ALTER TABLE captcha_template ALTER COLUMN updated_at TYPE BIGINT USING (EXTRACT(EPOCH FROM COALESCE(updated_at, NOW())) * 1000)::BIGINT;
        ALTER TABLE captcha_template RENAME COLUMN updated_at TO updated_ts;
    END IF;
END $$;

-- captcha_config 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'captcha_config' AND column_name = 'created_at' AND data_type = 'timestamp with time zone') THEN
        ALTER TABLE captcha_config ALTER COLUMN created_at DROP DEFAULT;
        ALTER TABLE captcha_config ALTER COLUMN created_at TYPE BIGINT USING (EXTRACT(EPOCH FROM COALESCE(created_at, NOW())) * 1000)::BIGINT;
        ALTER TABLE captcha_config RENAME COLUMN created_at TO created_ts;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'captcha_config' AND column_name = 'updated_at' AND data_type = 'timestamp with time zone') THEN
        ALTER TABLE captcha_config ALTER COLUMN updated_at DROP DEFAULT;
        ALTER TABLE captcha_config ALTER COLUMN updated_at TYPE BIGINT USING (EXTRACT(EPOCH FROM COALESCE(updated_at, NOW())) * 1000)::BIGINT;
        ALTER TABLE captcha_config RENAME COLUMN updated_at TO updated_ts;
    END IF;
END $$;

-- saml_sessions 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'saml_sessions' AND column_name = 'created_at' AND data_type = 'timestamp with time zone') THEN
        ALTER TABLE saml_sessions ALTER COLUMN created_at DROP DEFAULT;
        ALTER TABLE saml_sessions ALTER COLUMN created_at TYPE BIGINT USING (EXTRACT(EPOCH FROM COALESCE(created_at, NOW())) * 1000)::BIGINT;
        ALTER TABLE saml_sessions RENAME COLUMN created_at TO created_ts;
    END IF;
END $$;

-- saml_identity_providers 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'saml_identity_providers' AND column_name = 'created_at' AND data_type = 'timestamp with time zone') THEN
        ALTER TABLE saml_identity_providers ALTER COLUMN created_at DROP DEFAULT;
        ALTER TABLE saml_identity_providers ALTER COLUMN created_at TYPE BIGINT USING (EXTRACT(EPOCH FROM COALESCE(created_at, NOW())) * 1000)::BIGINT;
        ALTER TABLE saml_identity_providers RENAME COLUMN created_at TO created_ts;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'saml_identity_providers' AND column_name = 'updated_at' AND data_type = 'timestamp with time zone') THEN
        ALTER TABLE saml_identity_providers ALTER COLUMN updated_at DROP DEFAULT;
        ALTER TABLE saml_identity_providers ALTER COLUMN updated_at TYPE BIGINT USING (EXTRACT(EPOCH FROM COALESCE(updated_at, NOW())) * 1000)::BIGINT;
        ALTER TABLE saml_identity_providers RENAME COLUMN updated_at TO updated_ts;
    END IF;
END $$;

-- cas_tickets 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'cas_tickets' AND column_name = 'created_at' AND data_type = 'timestamp with time zone') THEN
        ALTER TABLE cas_tickets ALTER COLUMN created_at DROP DEFAULT;
        ALTER TABLE cas_tickets ALTER COLUMN created_at TYPE BIGINT USING (EXTRACT(EPOCH FROM COALESCE(created_at, NOW())) * 1000)::BIGINT;
        ALTER TABLE cas_tickets RENAME COLUMN created_at TO created_ts;
    END IF;
END $$;

-- cas_services 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'cas_services' AND column_name = 'created_at' AND data_type = 'timestamp with time zone') THEN
        ALTER TABLE cas_services ALTER COLUMN created_at DROP DEFAULT;
        ALTER TABLE cas_services ALTER COLUMN created_at TYPE BIGINT USING (EXTRACT(EPOCH FROM COALESCE(created_at, NOW())) * 1000)::BIGINT;
        ALTER TABLE cas_services RENAME COLUMN created_at TO created_ts;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'cas_services' AND column_name = 'updated_at' AND data_type = 'timestamp with time zone') THEN
        ALTER TABLE cas_services ALTER COLUMN updated_at DROP DEFAULT;
        ALTER TABLE cas_services ALTER COLUMN updated_at TYPE BIGINT USING (EXTRACT(EPOCH FROM COALESCE(updated_at, NOW())) * 1000)::BIGINT;
        ALTER TABLE cas_services RENAME COLUMN updated_at TO updated_ts;
    END IF;
END $$;

-- cas_user_attributes 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'cas_user_attributes' AND column_name = 'created_at' AND data_type = 'timestamp with time zone') THEN
        ALTER TABLE cas_user_attributes ALTER COLUMN created_at DROP DEFAULT;
        ALTER TABLE cas_user_attributes ALTER COLUMN created_at TYPE BIGINT USING (EXTRACT(EPOCH FROM COALESCE(created_at, NOW())) * 1000)::BIGINT;
        ALTER TABLE cas_user_attributes RENAME COLUMN created_at TO created_ts;
    END IF;
END $$;

-- modules 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'modules' AND column_name = 'created_at' AND data_type = 'timestamp with time zone') THEN
        ALTER TABLE modules ALTER COLUMN created_at DROP DEFAULT;
        ALTER TABLE modules ALTER COLUMN created_at TYPE BIGINT USING (EXTRACT(EPOCH FROM COALESCE(created_at, NOW())) * 1000)::BIGINT;
        ALTER TABLE modules RENAME COLUMN created_at TO created_ts;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'modules' AND column_name = 'updated_at' AND data_type = 'timestamp with time zone') THEN
        ALTER TABLE modules ALTER COLUMN updated_at DROP DEFAULT;
        ALTER TABLE modules ALTER COLUMN updated_at TYPE BIGINT USING (EXTRACT(EPOCH FROM COALESCE(updated_at, NOW())) * 1000)::BIGINT;
        ALTER TABLE modules RENAME COLUMN updated_at TO updated_ts;
    END IF;
END $$;

-- module_execution_logs 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'module_execution_logs' AND column_name = 'created_at' AND data_type = 'timestamp with time zone') THEN
        ALTER TABLE module_execution_logs ALTER COLUMN created_at DROP DEFAULT;
        ALTER TABLE module_execution_logs ALTER COLUMN created_at TYPE BIGINT USING (EXTRACT(EPOCH FROM COALESCE(created_at, NOW())) * 1000)::BIGINT;
        ALTER TABLE module_execution_logs RENAME COLUMN created_at TO created_ts;
    END IF;
END $$;

-- federation_blacklist 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'federation_blacklist' AND column_name = 'blocked_at' AND data_type = 'timestamp with time zone') THEN
        ALTER TABLE federation_blacklist ALTER COLUMN blocked_at DROP DEFAULT;
        ALTER TABLE federation_blacklist ALTER COLUMN blocked_at TYPE BIGINT USING (EXTRACT(EPOCH FROM COALESCE(blocked_at, NOW())) * 1000)::BIGINT;
        ALTER TABLE federation_blacklist RENAME COLUMN blocked_at TO blocked_ts;
    END IF;
END $$;

-- federation_blacklist_rule 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'federation_blacklist_rule' AND column_name = 'created_at' AND data_type = 'timestamp with time zone') THEN
        ALTER TABLE federation_blacklist_rule ALTER COLUMN created_at DROP DEFAULT;
        ALTER TABLE federation_blacklist_rule ALTER COLUMN created_at TYPE BIGINT USING (EXTRACT(EPOCH FROM COALESCE(created_at, NOW())) * 1000)::BIGINT;
        ALTER TABLE federation_blacklist_rule RENAME COLUMN created_at TO created_ts;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'federation_blacklist_rule' AND column_name = 'updated_at' AND data_type = 'timestamp with time zone') THEN
        ALTER TABLE federation_blacklist_rule ALTER COLUMN updated_at DROP DEFAULT;
        ALTER TABLE federation_blacklist_rule ALTER COLUMN updated_at TYPE BIGINT USING (EXTRACT(EPOCH FROM COALESCE(updated_at, NOW())) * 1000)::BIGINT;
        ALTER TABLE federation_blacklist_rule RENAME COLUMN updated_at TO updated_ts;
    END IF;
END $$;

-- federation_blacklist_log 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'federation_blacklist_log' AND column_name = 'created_at' AND data_type = 'timestamp with time zone') THEN
        ALTER TABLE federation_blacklist_log ALTER COLUMN created_at DROP DEFAULT;
        ALTER TABLE federation_blacklist_log ALTER COLUMN created_at TYPE BIGINT USING (EXTRACT(EPOCH FROM COALESCE(created_at, NOW())) * 1000)::BIGINT;
        ALTER TABLE federation_blacklist_log RENAME COLUMN created_at TO created_ts;
    END IF;
END $$;

-- federation_blacklist_config 表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'federation_blacklist_config' AND column_name = 'created_at' AND data_type = 'timestamp with time zone') THEN
        ALTER TABLE federation_blacklist_config ALTER COLUMN created_at DROP DEFAULT;
        ALTER TABLE federation_blacklist_config ALTER COLUMN created_at TYPE BIGINT USING (EXTRACT(EPOCH FROM COALESCE(created_at, NOW())) * 1000)::BIGINT;
        ALTER TABLE federation_blacklist_config RENAME COLUMN created_at TO created_ts;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'federation_blacklist_config' AND column_name = 'updated_at' AND data_type = 'timestamp with time zone') THEN
        ALTER TABLE federation_blacklist_config ALTER COLUMN updated_at DROP DEFAULT;
        ALTER TABLE federation_blacklist_config ALTER COLUMN updated_at TYPE BIGINT USING (EXTRACT(EPOCH FROM COALESCE(updated_at, NOW())) * 1000)::BIGINT;
        ALTER TABLE federation_blacklist_config RENAME COLUMN updated_at TO updated_ts;
    END IF;
END $$;

-- =============================================================================
-- 第三部分: 更新版本记录
-- =============================================================================

INSERT INTO schema_migrations (version, description, success)
VALUES ('20260228000000', 'Unify field naming final - boolean and timestamp fields', TRUE)
ON CONFLICT (version) DO UPDATE SET success = TRUE, executed_at = NOW();

DO $$
BEGIN
    RAISE NOTICE '==========================================';
    RAISE NOTICE '字段命名统一迁移完成';
    RAISE NOTICE '==========================================';
END $$;
