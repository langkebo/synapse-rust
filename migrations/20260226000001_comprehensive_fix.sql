-- =============================================================================
-- Synapse-Rust 综合修复迁移脚本
-- 版本: 20260226000001 (整合版)
-- 创建日期: 2026-02-26
-- 描述: 整合所有时间戳列标准化、表结构修复和缺失内容补充
-- 
-- 整合来源:
--   - 20260225000006_standardize_timestamp_columns.sql
--   - 20260225000007_fix_federation_blacklist_rule_columns.sql
--   - 20260225000008_standardize_timestamp_columns_phase2.sql
--   - 20260225000009_standardize_timestamp_columns_phase3.sql
--   - 20260225000004_fix_rooms_table_columns.sql
--   - 20260226000000_add_join_rule_column.sql
--   - 20260225000005_create_notifications_table.sql
--   - 20260225000010_fix_events_missing_columns.sql
-- 
-- 特性:
--   - 幂等性: 可重复执行，不会产生副作用
--   - 安全性: 所有操作都有 IF EXISTS/IF NOT EXISTS 检查
--   - 完整性: 包含所有必要的修复和补充
-- =============================================================================

-- =============================================================================
-- 第一部分: 时间戳列命名标准化
-- 规范: _ts 后缀表示毫秒级 Unix 时间戳 (BIGINT)
-- =============================================================================

DO $$
BEGIN
    RAISE NOTICE '==========================================';
    RAISE NOTICE '开始执行综合修复迁移...';
    RAISE NOTICE '==========================================';

    -- -------------------------------------------------------------------------
    -- 1. federation_signing_keys 表
    -- -------------------------------------------------------------------------
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'federation_signing_keys' AND column_name::text = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'federation_signing_keys' AND column_name::text = 'created_ts') THEN
            ALTER TABLE federation_signing_keys RENAME COLUMN created_at TO created_ts;
            RAISE NOTICE 'Renamed federation_signing_keys.created_at to created_ts';
        END IF;
    END IF;

    -- -------------------------------------------------------------------------
    -- 2. presence 表
    -- -------------------------------------------------------------------------
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'presence' AND column_name::text = 'updated_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'presence' AND column_name::text = 'updated_ts') THEN
            ALTER TABLE presence RENAME COLUMN updated_at TO updated_ts;
            RAISE NOTICE 'Renamed presence.updated_at to updated_ts';
        END IF;
    END IF;

    -- -------------------------------------------------------------------------
    -- 3. notifications 表
    -- -------------------------------------------------------------------------
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'notifications' AND column_name::text = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'notifications' AND column_name::text = 'created_ts') THEN
            ALTER TABLE notifications RENAME COLUMN created_at TO created_ts;
            RAISE NOTICE 'Renamed notifications.created_at to created_ts';
        END IF;
    END IF;

    -- -------------------------------------------------------------------------
    -- 4. account_data 表
    -- -------------------------------------------------------------------------
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'account_data' AND column_name::text = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'account_data' AND column_name::text = 'created_ts') THEN
            ALTER TABLE account_data RENAME COLUMN created_at TO created_ts;
            RAISE NOTICE 'Renamed account_data.created_at to created_ts';
        END IF;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'account_data' AND column_name::text = 'updated_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'account_data' AND column_name::text = 'updated_ts') THEN
            ALTER TABLE account_data RENAME COLUMN updated_at TO updated_ts;
            RAISE NOTICE 'Renamed account_data.updated_at to updated_ts';
        END IF;
    END IF;

    -- -------------------------------------------------------------------------
    -- 5. room_account_data 表
    -- -------------------------------------------------------------------------
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'room_account_data' AND column_name::text = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'room_account_data' AND column_name::text = 'created_ts') THEN
            ALTER TABLE room_account_data RENAME COLUMN created_at TO created_ts;
            RAISE NOTICE 'Renamed room_account_data.created_at to created_ts';
        END IF;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'room_account_data' AND column_name::text = 'updated_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'room_account_data' AND column_name::text = 'updated_ts') THEN
            ALTER TABLE room_account_data RENAME COLUMN updated_at TO updated_ts;
            RAISE NOTICE 'Renamed room_account_data.updated_at to updated_ts';
        END IF;
    END IF;

    -- -------------------------------------------------------------------------
    -- 6. federation_blacklist_rule 表
    -- -------------------------------------------------------------------------
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'federation_blacklist_rule' AND column_name::text = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'federation_blacklist_rule' AND column_name::text = 'created_ts') THEN
            ALTER TABLE federation_blacklist_rule RENAME COLUMN created_at TO created_ts;
            RAISE NOTICE 'Renamed federation_blacklist_rule.created_at to created_ts';
        END IF;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'federation_blacklist_rule' AND column_name::text = 'updated_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'federation_blacklist_rule' AND column_name::text = 'updated_ts') THEN
            ALTER TABLE federation_blacklist_rule RENAME COLUMN updated_at TO updated_ts;
            RAISE NOTICE 'Renamed federation_blacklist_rule.updated_at to updated_ts';
        END IF;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'federation_blacklist_rule' AND column_name::text = 'created_by') THEN
        ALTER TABLE federation_blacklist_rule ADD COLUMN created_by VARCHAR(255) DEFAULT 'system';
        RAISE NOTICE 'Added created_by column to federation_blacklist_rule';
    END IF;

    -- -------------------------------------------------------------------------
    -- 7. captcha_config 表
    -- -------------------------------------------------------------------------
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'captcha_config' AND column_name::text = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'captcha_config' AND column_name::text = 'created_ts') THEN
            ALTER TABLE captcha_config RENAME COLUMN created_at TO created_ts;
        END IF;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'captcha_config' AND column_name::text = 'updated_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'captcha_config' AND column_name::text = 'updated_ts') THEN
            ALTER TABLE captcha_config RENAME COLUMN updated_at TO updated_ts;
        END IF;
    END IF;

    -- -------------------------------------------------------------------------
    -- 8. captcha_template 表
    -- -------------------------------------------------------------------------
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'captcha_template' AND column_name::text = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'captcha_template' AND column_name::text = 'created_ts') THEN
            ALTER TABLE captcha_template RENAME COLUMN created_at TO created_ts;
        END IF;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'captcha_template' AND column_name::text = 'updated_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'captcha_template' AND column_name::text = 'updated_ts') THEN
            ALTER TABLE captcha_template RENAME COLUMN updated_at TO updated_ts;
        END IF;
    END IF;

    -- -------------------------------------------------------------------------
    -- 9. cas_services 表
    -- -------------------------------------------------------------------------
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'cas_services' AND column_name::text = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'cas_services' AND column_name::text = 'created_ts') THEN
            ALTER TABLE cas_services RENAME COLUMN created_at TO created_ts;
        END IF;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'cas_services' AND column_name::text = 'updated_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'cas_services' AND column_name::text = 'updated_ts') THEN
            ALTER TABLE cas_services RENAME COLUMN updated_at TO updated_ts;
        END IF;
    END IF;

    -- -------------------------------------------------------------------------
    -- 10. cas_tickets 表
    -- -------------------------------------------------------------------------
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'cas_tickets' AND column_name::text = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'cas_tickets' AND column_name::text = 'created_ts') THEN
            ALTER TABLE cas_tickets RENAME COLUMN created_at TO created_ts;
        END IF;
    END IF;

    -- -------------------------------------------------------------------------
    -- 11. cas_user_attributes 表
    -- -------------------------------------------------------------------------
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'cas_user_attributes' AND column_name::text = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'cas_user_attributes' AND column_name::text = 'created_ts') THEN
            ALTER TABLE cas_user_attributes RENAME COLUMN created_at TO created_ts;
        END IF;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'cas_user_attributes' AND column_name::text = 'updated_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'cas_user_attributes' AND column_name::text = 'updated_ts') THEN
            ALTER TABLE cas_user_attributes RENAME COLUMN updated_at TO updated_ts;
        END IF;
    END IF;

    -- -------------------------------------------------------------------------
    -- 12. cross_signing_keys 表
    -- -------------------------------------------------------------------------
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'cross_signing_keys' AND column_name::text = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'cross_signing_keys' AND column_name::text = 'created_ts') THEN
            ALTER TABLE cross_signing_keys RENAME COLUMN created_at TO created_ts;
        END IF;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'cross_signing_keys' AND column_name::text = 'updated_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'cross_signing_keys' AND column_name::text = 'updated_ts') THEN
            ALTER TABLE cross_signing_keys RENAME COLUMN updated_at TO updated_ts;
        END IF;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'cross_signing_keys' AND column_name::text = 'added_ts') THEN
        ALTER TABLE cross_signing_keys ADD COLUMN added_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT;
        RAISE NOTICE 'Added added_ts column to cross_signing_keys';
    END IF;

    -- -------------------------------------------------------------------------
    -- 13. device_keys 表
    -- -------------------------------------------------------------------------
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'device_keys' AND column_name::text = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'device_keys' AND column_name::text = 'created_ts') THEN
            ALTER TABLE device_keys RENAME COLUMN created_at TO created_ts;
        END IF;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'device_keys' AND column_name::text = 'updated_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'device_keys' AND column_name::text = 'updated_ts') THEN
            ALTER TABLE device_keys RENAME COLUMN updated_at TO updated_ts;
        END IF;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'device_keys' AND column_name::text = 'added_ts') THEN
        ALTER TABLE device_keys ADD COLUMN added_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT;
        RAISE NOTICE 'Added added_ts column to device_keys';
    END IF;

    -- -------------------------------------------------------------------------
    -- 14. email_verification_tokens 表
    -- -------------------------------------------------------------------------
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'email_verification_tokens' AND column_name::text = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'email_verification_tokens' AND column_name::text = 'created_ts') THEN
            ALTER TABLE email_verification_tokens RENAME COLUMN created_at TO created_ts;
        END IF;
    END IF;

    -- -------------------------------------------------------------------------
    -- 15. filters 表
    -- -------------------------------------------------------------------------
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'filters' AND column_name::text = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'filters' AND column_name::text = 'created_ts') THEN
            ALTER TABLE filters RENAME COLUMN created_at TO created_ts;
        END IF;
    END IF;

    -- -------------------------------------------------------------------------
    -- 16. inbound_megolm_sessions 表
    -- -------------------------------------------------------------------------
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'inbound_megolm_sessions' AND column_name::text = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'inbound_megolm_sessions' AND column_name::text = 'created_ts') THEN
            ALTER TABLE inbound_megolm_sessions RENAME COLUMN created_at TO created_ts;
        END IF;
    END IF;

    -- -------------------------------------------------------------------------
    -- 17. megolm_sessions 表
    -- -------------------------------------------------------------------------
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'megolm_sessions' AND column_name::text = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'megolm_sessions' AND column_name::text = 'created_ts') THEN
            ALTER TABLE megolm_sessions RENAME COLUMN created_at TO created_ts;
        END IF;
    END IF;

    -- -------------------------------------------------------------------------
    -- 18. key_backups 表
    -- -------------------------------------------------------------------------
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'key_backups' AND column_name::text = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'key_backups' AND column_name::text = 'created_ts') THEN
            ALTER TABLE key_backups RENAME COLUMN created_at TO created_ts;
        END IF;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'key_backups' AND column_name::text = 'updated_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'key_backups' AND column_name::text = 'updated_ts') THEN
            ALTER TABLE key_backups RENAME COLUMN updated_at TO updated_ts;
        END IF;
    END IF;

    -- -------------------------------------------------------------------------
    -- 19. openid_tokens 表
    -- -------------------------------------------------------------------------
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'openid_tokens' AND column_name::text = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'openid_tokens' AND column_name::text = 'created_ts') THEN
            ALTER TABLE openid_tokens RENAME COLUMN created_at TO created_ts;
        END IF;
    END IF;

    -- -------------------------------------------------------------------------
    -- 20. push_config 表
    -- -------------------------------------------------------------------------
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'push_config' AND column_name::text = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'push_config' AND column_name::text = 'created_ts') THEN
            ALTER TABLE push_config RENAME COLUMN created_at TO created_ts;
        END IF;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'push_config' AND column_name::text = 'updated_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'push_config' AND column_name::text = 'updated_ts') THEN
            ALTER TABLE push_config RENAME COLUMN updated_at TO updated_ts;
        END IF;
    END IF;

    -- -------------------------------------------------------------------------
    -- 21. push_device 表
    -- -------------------------------------------------------------------------
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'push_device' AND column_name::text = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'push_device' AND column_name::text = 'created_ts') THEN
            ALTER TABLE push_device RENAME COLUMN created_at TO created_ts;
        END IF;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'push_device' AND column_name::text = 'updated_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'push_device' AND column_name::text = 'updated_ts') THEN
            ALTER TABLE push_device RENAME COLUMN updated_at TO updated_ts;
        END IF;
    END IF;

    -- -------------------------------------------------------------------------
    -- 22. push_rule 表
    -- -------------------------------------------------------------------------
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'push_rule' AND column_name::text = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'push_rule' AND column_name::text = 'created_ts') THEN
            ALTER TABLE push_rule RENAME COLUMN created_at TO created_ts;
        END IF;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'push_rule' AND column_name::text = 'updated_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'push_rule' AND column_name::text = 'updated_ts') THEN
            ALTER TABLE push_rule RENAME COLUMN updated_at TO updated_ts;
        END IF;
    END IF;

    -- -------------------------------------------------------------------------
    -- 23. push_stats 表
    -- -------------------------------------------------------------------------
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'push_stats' AND column_name::text = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'push_stats' AND column_name::text = 'created_ts') THEN
            ALTER TABLE push_stats RENAME COLUMN created_at TO created_ts;
        END IF;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'push_stats' AND column_name::text = 'updated_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'push_stats' AND column_name::text = 'updated_ts') THEN
            ALTER TABLE push_stats RENAME COLUMN updated_at TO updated_ts;
        END IF;
    END IF;

    -- -------------------------------------------------------------------------
    -- 24. push_notification_queue 表
    -- -------------------------------------------------------------------------
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'push_notification_queue' AND column_name::text = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'push_notification_queue' AND column_name::text = 'created_ts') THEN
            ALTER TABLE push_notification_queue RENAME COLUMN created_at TO created_ts;
        END IF;
    END IF;

    -- -------------------------------------------------------------------------
    -- 25. backup_keys 表
    -- -------------------------------------------------------------------------
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'backup_keys' AND column_name::text = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'backup_keys' AND column_name::text = 'created_ts') THEN
            ALTER TABLE backup_keys RENAME COLUMN created_at TO created_ts;
        END IF;
    END IF;

    -- -------------------------------------------------------------------------
    -- 26. federation_blacklist_config 表
    -- -------------------------------------------------------------------------
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'federation_blacklist_config' AND column_name::text = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'federation_blacklist_config' AND column_name::text = 'created_ts') THEN
            ALTER TABLE federation_blacklist_config RENAME COLUMN created_at TO created_ts;
        END IF;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'federation_blacklist_config' AND column_name::text = 'updated_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'federation_blacklist_config' AND column_name::text = 'updated_ts') THEN
            ALTER TABLE federation_blacklist_config RENAME COLUMN updated_at TO updated_ts;
        END IF;
    END IF;

    -- -------------------------------------------------------------------------
    -- 27. federation_blacklist_log 表
    -- -------------------------------------------------------------------------
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'federation_blacklist_log' AND column_name::text = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'federation_blacklist_log' AND column_name::text = 'created_ts') THEN
            ALTER TABLE federation_blacklist_log RENAME COLUMN created_at TO created_ts;
        END IF;
    END IF;

    -- -------------------------------------------------------------------------
    -- 28. media_quota_alerts 表
    -- -------------------------------------------------------------------------
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'media_quota_alerts' AND column_name::text = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'media_quota_alerts' AND column_name::text = 'created_ts') THEN
            ALTER TABLE media_quota_alerts RENAME COLUMN created_at TO created_ts;
        END IF;
    END IF;

    -- -------------------------------------------------------------------------
    -- 29. module_execution_logs 表
    -- -------------------------------------------------------------------------
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'module_execution_logs' AND column_name::text = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'module_execution_logs' AND column_name::text = 'created_ts') THEN
            ALTER TABLE module_execution_logs RENAME COLUMN created_at TO created_ts;
        END IF;
    END IF;

    -- -------------------------------------------------------------------------
    -- 30. modules 表
    -- -------------------------------------------------------------------------
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'modules' AND column_name::text = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'modules' AND column_name::text = 'created_ts') THEN
            ALTER TABLE modules RENAME COLUMN created_at TO created_ts;
        END IF;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'modules' AND column_name::text = 'updated_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'modules' AND column_name::text = 'updated_ts') THEN
            ALTER TABLE modules RENAME COLUMN updated_at TO updated_ts;
        END IF;
    END IF;

    -- -------------------------------------------------------------------------
    -- 31. registration_captcha 表
    -- -------------------------------------------------------------------------
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'registration_captcha' AND column_name::text = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'registration_captcha' AND column_name::text = 'created_ts') THEN
            ALTER TABLE registration_captcha RENAME COLUMN created_at TO created_ts;
        END IF;
    END IF;

    -- -------------------------------------------------------------------------
    -- 32. retention_policies 表
    -- -------------------------------------------------------------------------
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'retention_policies' AND column_name::text = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'retention_policies' AND column_name::text = 'created_ts') THEN
            ALTER TABLE retention_policies RENAME COLUMN created_at TO created_ts;
        END IF;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'retention_policies' AND column_name::text = 'updated_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'retention_policies' AND column_name::text = 'updated_ts') THEN
            ALTER TABLE retention_policies RENAME COLUMN updated_at TO updated_ts;
        END IF;
    END IF;

    -- -------------------------------------------------------------------------
    -- 33. saml_identity_providers 表
    -- -------------------------------------------------------------------------
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'saml_identity_providers' AND column_name::text = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'saml_identity_providers' AND column_name::text = 'created_ts') THEN
            ALTER TABLE saml_identity_providers RENAME COLUMN created_at TO created_ts;
        END IF;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'saml_identity_providers' AND column_name::text = 'updated_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'saml_identity_providers' AND column_name::text = 'updated_ts') THEN
            ALTER TABLE saml_identity_providers RENAME COLUMN updated_at TO updated_ts;
        END IF;
    END IF;

    -- -------------------------------------------------------------------------
    -- 34. saml_sessions 表
    -- -------------------------------------------------------------------------
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'saml_sessions' AND column_name::text = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'saml_sessions' AND column_name::text = 'created_ts') THEN
            ALTER TABLE saml_sessions RENAME COLUMN created_at TO created_ts;
        END IF;
    END IF;

    -- -------------------------------------------------------------------------
    -- 35. scheduled_notifications 表
    -- -------------------------------------------------------------------------
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'scheduled_notifications' AND column_name::text = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'scheduled_notifications' AND column_name::text = 'created_ts') THEN
            ALTER TABLE scheduled_notifications RENAME COLUMN created_at TO created_ts;
        END IF;
    END IF;

    -- -------------------------------------------------------------------------
    -- 36. security_events 表
    -- -------------------------------------------------------------------------
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'security_events' AND column_name::text = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'security_events' AND column_name::text = 'created_ts') THEN
            ALTER TABLE security_events RENAME COLUMN created_at TO created_ts;
        END IF;
    END IF;

    -- -------------------------------------------------------------------------
    -- 37. server_media_quota 表
    -- -------------------------------------------------------------------------
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'server_media_quota' AND column_name::text = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'server_media_quota' AND column_name::text = 'created_ts') THEN
            ALTER TABLE server_media_quota RENAME COLUMN created_at TO created_ts;
        END IF;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'server_media_quota' AND column_name::text = 'updated_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'server_media_quota' AND column_name::text = 'updated_ts') THEN
            ALTER TABLE server_media_quota RENAME COLUMN updated_at TO updated_ts;
        END IF;
    END IF;

    -- -------------------------------------------------------------------------
    -- 38. server_notifications 表
    -- -------------------------------------------------------------------------
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'server_notifications' AND column_name::text = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'server_notifications' AND column_name::text = 'created_ts') THEN
            ALTER TABLE server_notifications RENAME COLUMN created_at TO created_ts;
        END IF;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'server_notifications' AND column_name::text = 'updated_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'server_notifications' AND column_name::text = 'updated_ts') THEN
            ALTER TABLE server_notifications RENAME COLUMN updated_at TO updated_ts;
        END IF;
    END IF;

    -- -------------------------------------------------------------------------
    -- 39. user_media_quota 表
    -- -------------------------------------------------------------------------
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'user_media_quota' AND column_name::text = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'user_media_quota' AND column_name::text = 'created_ts') THEN
            ALTER TABLE user_media_quota RENAME COLUMN created_at TO created_ts;
        END IF;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'user_media_quota' AND column_name::text = 'updated_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'user_media_quota' AND column_name::text = 'updated_ts') THEN
            ALTER TABLE user_media_quota RENAME COLUMN updated_at TO updated_ts;
        END IF;
    END IF;

    -- -------------------------------------------------------------------------
    -- 40. user_profiles 表
    -- -------------------------------------------------------------------------
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'user_profiles' AND column_name::text = 'updated_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'user_profiles' AND column_name::text = 'updated_ts') THEN
            ALTER TABLE user_profiles RENAME COLUMN updated_at TO updated_ts;
        END IF;
    END IF;

    RAISE NOTICE '第一部分完成: 时间戳列命名标准化';
END $$;

-- =============================================================================
-- 第二部分: rooms 表修复
-- =============================================================================

DO $$
BEGIN
    -- -------------------------------------------------------------------------
    -- 添加缺失的列
    -- -------------------------------------------------------------------------
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'rooms' AND column_name::text = 'join_rule') THEN
        ALTER TABLE rooms ADD COLUMN join_rule VARCHAR(50) DEFAULT 'invite';
        RAISE NOTICE 'Added join_rule column to rooms';
    END IF;

    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'rooms' AND column_name::text = 'created_ts') THEN
        ALTER TABLE rooms ADD COLUMN created_ts BIGINT;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'rooms' AND column_name::text = 'updated_ts') THEN
        ALTER TABLE rooms ADD COLUMN updated_ts BIGINT;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'rooms' AND column_name::text = 'name') THEN
        ALTER TABLE rooms ADD COLUMN name VARCHAR(255);
    END IF;

    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'rooms' AND column_name::text = 'topic') THEN
        ALTER TABLE rooms ADD COLUMN topic TEXT;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'rooms' AND column_name::text = 'avatar_url') THEN
        ALTER TABLE rooms ADD COLUMN avatar_url VARCHAR(512);
    END IF;

    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'rooms' AND column_name::text = 'canonical_alias') THEN
        ALTER TABLE rooms ADD COLUMN canonical_alias VARCHAR(255);
    END IF;

    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'rooms' AND column_name::text = 'member_count') THEN
        ALTER TABLE rooms ADD COLUMN member_count BIGINT DEFAULT 0;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'rooms' AND column_name::text = 'history_visibility') THEN
        ALTER TABLE rooms ADD COLUMN history_visibility VARCHAR(50) DEFAULT 'joined';
    END IF;

    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'rooms' AND column_name::text = 'encryption') THEN
        ALTER TABLE rooms ADD COLUMN encryption VARCHAR(50);
    END IF;

    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'rooms' AND column_name::text = 'last_activity_ts') THEN
        ALTER TABLE rooms ADD COLUMN last_activity_ts BIGINT;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'rooms' AND column_name::text = 'guest_access') THEN
        ALTER TABLE rooms ADD COLUMN guest_access VARCHAR(50) DEFAULT 'forbidden';
    END IF;

    -- -------------------------------------------------------------------------
    -- 处理 room_version 列
    -- 代码使用 room_version，需要确保该列存在
    -- -------------------------------------------------------------------------
    -- 如果 room_version 不存在但 version 存在，添加 room_version 列
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'rooms' AND column_name::text = 'room_version') THEN
        ALTER TABLE rooms ADD COLUMN room_version VARCHAR(50) DEFAULT '6';
        -- 从 version 复制数据
        UPDATE rooms SET room_version = version WHERE version IS NOT NULL;
        RAISE NOTICE 'Added room_version column to rooms table';
    END IF;
    
    -- 如果 version 不存在但 room_version 存在，添加 version 列
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'rooms' AND column_name::text = 'version') THEN
        ALTER TABLE rooms ADD COLUMN version VARCHAR(50) DEFAULT '6';
        UPDATE rooms SET version = room_version WHERE room_version IS NOT NULL;
        RAISE NOTICE 'Added version column to rooms table';
    END IF;
    
    -- 同步两列的数据
    UPDATE rooms SET room_version = version WHERE version IS NOT NULL AND (room_version IS NULL OR room_version != version);
    UPDATE rooms SET version = room_version WHERE room_version IS NOT NULL AND (version IS NULL OR version != room_version);

    -- -------------------------------------------------------------------------
    -- 从 join_rules 复制数据到 join_rule
    -- -------------------------------------------------------------------------
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'rooms' AND column_name::text = 'join_rules') THEN
        UPDATE rooms SET join_rule = join_rules WHERE join_rules IS NOT NULL AND (join_rule IS NULL OR join_rule = 'invite');
        RAISE NOTICE 'Copied data from join_rules to join_rule';
    END IF;

    -- -------------------------------------------------------------------------
    -- 处理 rooms 表的时间戳列
    -- 代码使用 creation_ts，需要确保该列存在
    -- -------------------------------------------------------------------------
    -- 添加 creation_ts 列（如果不存在）
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'rooms' AND column_name::text = 'creation_ts') THEN
        ALTER TABLE rooms ADD COLUMN creation_ts BIGINT;
        -- 从 created_ts 复制数据
        UPDATE rooms SET creation_ts = created_ts WHERE created_ts IS NOT NULL;
        RAISE NOTICE 'Added creation_ts column to rooms table';
    END IF;
    
    -- 确保 created_ts 列存在
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'rooms' AND column_name::text = 'created_ts') THEN
        ALTER TABLE rooms ADD COLUMN created_ts BIGINT;
        UPDATE rooms SET created_ts = creation_ts WHERE creation_ts IS NOT NULL;
        RAISE NOTICE 'Added created_ts column to rooms table';
    END IF;
    
    -- 同步两列的数据
    UPDATE rooms SET creation_ts = created_ts WHERE created_ts IS NOT NULL AND creation_ts IS NULL;
    UPDATE rooms SET created_ts = creation_ts WHERE creation_ts IS NOT NULL AND created_ts IS NULL;
    
    -- 设置默认值
    UPDATE rooms SET creation_ts = (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT WHERE creation_ts IS NULL;
    UPDATE rooms SET created_ts = (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT WHERE created_ts IS NULL;

    RAISE NOTICE '第二部分完成: rooms 表修复';
END $$;

-- =============================================================================
-- 第三部分: 创建 notifications 表
-- =============================================================================

CREATE TABLE IF NOT EXISTS notifications (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    event_id VARCHAR(255),
    room_id VARCHAR(255),
    notification_type VARCHAR(50) NOT NULL,
    content JSONB DEFAULT '{}',
    read BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    updated_ts BIGINT
);

CREATE INDEX IF NOT EXISTS idx_notifications_user ON notifications(user_id);
CREATE INDEX IF NOT EXISTS idx_notifications_room ON notifications(room_id);
CREATE INDEX IF NOT EXISTS idx_notifications_user_read ON notifications(user_id, read);
CREATE INDEX IF NOT EXISTS idx_notifications_created ON notifications(created_ts DESC);

-- =============================================================================
-- 第四部分: events 表缺失列修复
-- =============================================================================

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'events' AND column_name::text = 'user_id') THEN
        ALTER TABLE events ADD COLUMN user_id VARCHAR(255);
    END IF;

    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'events' AND column_name::text = 'processed_ts') THEN
        ALTER TABLE events ADD COLUMN processed_ts BIGINT;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'events' AND column_name::text = 'unsigned') THEN
        ALTER TABLE events ADD COLUMN unsigned JSONB DEFAULT '{}';
    END IF;

    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'events' AND column_name::text = 'not_before') THEN
        ALTER TABLE events ADD COLUMN not_before BIGINT DEFAULT 0;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'events' AND column_name::text = 'status') THEN
        ALTER TABLE events ADD COLUMN status VARCHAR(50);
    END IF;

    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'events' AND column_name::text = 'reference_image') THEN
        ALTER TABLE events ADD COLUMN reference_image VARCHAR(255);
    END IF;

    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'events' AND column_name::text = 'origin') THEN
        ALTER TABLE events ADD COLUMN origin VARCHAR(255) DEFAULT 'self';
    END IF;

    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'events' AND column_name::text = 'redacted') THEN
        ALTER TABLE events ADD COLUMN redacted BOOLEAN DEFAULT FALSE;
    END IF;

    RAISE NOTICE '第四部分完成: events 表修复';
END $$;

-- =============================================================================
-- 第五部分: pushers 表修复
-- =============================================================================

DO $$
BEGIN
    -- 添加 device_id 列到 pushers 表
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'pushers' AND column_name::text = 'device_id') THEN
        ALTER TABLE pushers ADD COLUMN device_id VARCHAR(255);
        RAISE NOTICE 'Added device_id column to pushers table';
    END IF;
    
    RAISE NOTICE '第五部分完成: pushers 表修复';
END $$;

-- =============================================================================
-- 第六部分: push_rules 表修复
-- =============================================================================

DO $$
BEGIN
    -- 添加 pattern 列到 push_rules 表
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name::text = 'push_rules' AND column_name::text = 'pattern') THEN
        ALTER TABLE push_rules ADD COLUMN pattern VARCHAR(255);
        RAISE NOTICE 'Added pattern column to push_rules table';
    END IF;
    
    RAISE NOTICE '第六部分完成: push_rules 表修复';
END $$;

-- =============================================================================
-- 第七部分: 创建索引
-- =============================================================================

CREATE INDEX IF NOT EXISTS idx_rooms_name ON rooms(name);
CREATE INDEX IF NOT EXISTS idx_rooms_member_count ON rooms(member_count);

-- =============================================================================
-- 第六部分: 创建默认管理员用户
-- 密码: Test@123456 (Argon2id hash)
-- =============================================================================

INSERT INTO users (user_id, username, password_hash, creation_ts, is_admin)
VALUES ('@sysadmin:cjystx.top', 'sysadmin', '$argon2id$v=19$m=65536,t=3,p=1$CmTPwxw07ME/7xRiNoG5NA$9JJUqz3ndJ9eVMikCu3tA6ZfCluwfZ4bGUREKtC4zQc', (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT, TRUE)
ON CONFLICT (user_id) DO UPDATE SET password_hash = EXCLUDED.password_hash, is_admin = EXCLUDED.is_admin;

-- =============================================================================
-- 第七部分: 记录迁移
-- =============================================================================

INSERT INTO schema_migrations (version, description, success)
VALUES ('20260226000001', 'Comprehensive fix - consolidated migration', TRUE)
ON CONFLICT (version) DO UPDATE SET success = TRUE, executed_at = NOW();

DO $$
BEGIN
    RAISE NOTICE '==========================================';
    RAISE NOTICE '综合修复迁移完成!';
    RAISE NOTICE '==========================================';
END $$;
