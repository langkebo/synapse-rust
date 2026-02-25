-- 统一时间戳列命名规范迁移脚本（第二阶段）
-- 规范：
--   - _ts 后缀：毫秒级 Unix 时间戳 (BIGINT) - 项目统一使用
--   - _at 后缀：TIMESTAMPTZ 时间类型 - 避免使用
-- 
-- 本脚本处理剩余的表

DO $$
BEGIN
    -- 1. captcha_config 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'captcha_config' AND column_name = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'captcha_config' AND column_name = 'created_ts') THEN
            ALTER TABLE captcha_config RENAME COLUMN created_at TO created_ts;
            RAISE NOTICE 'Renamed captcha_config.created_at to created_ts';
        END IF;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'captcha_config' AND column_name = 'updated_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'captcha_config' AND column_name = 'updated_ts') THEN
            ALTER TABLE captcha_config RENAME COLUMN updated_at TO updated_ts;
            RAISE NOTICE 'Renamed captcha_config.updated_at to updated_ts';
        END IF;
    END IF;

    -- 2. captcha_template 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'captcha_template' AND column_name = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'captcha_template' AND column_name = 'created_ts') THEN
            ALTER TABLE captcha_template RENAME COLUMN created_at TO created_ts;
            RAISE NOTICE 'Renamed captcha_template.created_at to created_ts';
        END IF;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'captcha_template' AND column_name = 'updated_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'captcha_template' AND column_name = 'updated_ts') THEN
            ALTER TABLE captcha_template RENAME COLUMN updated_at TO updated_ts;
            RAISE NOTICE 'Renamed captcha_template.updated_at to updated_ts';
        END IF;
    END IF;

    -- 3. cas_services 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'cas_services' AND column_name = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'cas_services' AND column_name = 'created_ts') THEN
            ALTER TABLE cas_services RENAME COLUMN created_at TO created_ts;
            RAISE NOTICE 'Renamed cas_services.created_at to created_ts';
        END IF;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'cas_services' AND column_name = 'updated_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'cas_services' AND column_name = 'updated_ts') THEN
            ALTER TABLE cas_services RENAME COLUMN updated_at TO updated_ts;
            RAISE NOTICE 'Renamed cas_services.updated_at to updated_ts';
        END IF;
    END IF;

    -- 4. cas_tickets 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'cas_tickets' AND column_name = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'cas_tickets' AND column_name = 'created_ts') THEN
            ALTER TABLE cas_tickets RENAME COLUMN created_at TO created_ts;
            RAISE NOTICE 'Renamed cas_tickets.created_at to created_ts';
        END IF;
    END IF;

    -- 5. cas_user_attributes 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'cas_user_attributes' AND column_name = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'cas_user_attributes' AND column_name = 'created_ts') THEN
            ALTER TABLE cas_user_attributes RENAME COLUMN created_at TO created_ts;
            RAISE NOTICE 'Renamed cas_user_attributes.created_at to created_ts';
        END IF;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'cas_user_attributes' AND column_name = 'updated_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'cas_user_attributes' AND column_name = 'updated_ts') THEN
            ALTER TABLE cas_user_attributes RENAME COLUMN updated_at TO updated_ts;
            RAISE NOTICE 'Renamed cas_user_attributes.updated_at to updated_ts';
        END IF;
    END IF;

    -- 6. cross_signing_keys 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'cross_signing_keys' AND column_name = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'cross_signing_keys' AND column_name = 'created_ts') THEN
            ALTER TABLE cross_signing_keys RENAME COLUMN created_at TO created_ts;
            RAISE NOTICE 'Renamed cross_signing_keys.created_at to created_ts';
        END IF;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'cross_signing_keys' AND column_name = 'updated_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'cross_signing_keys' AND column_name = 'updated_ts') THEN
            ALTER TABLE cross_signing_keys RENAME COLUMN updated_at TO updated_ts;
            RAISE NOTICE 'Renamed cross_signing_keys.updated_at to updated_ts';
        END IF;
    END IF;

    -- 7. device_keys 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'device_keys' AND column_name = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'device_keys' AND column_name = 'created_ts') THEN
            ALTER TABLE device_keys RENAME COLUMN created_at TO created_ts;
            RAISE NOTICE 'Renamed device_keys.created_at to created_ts';
        END IF;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'device_keys' AND column_name = 'updated_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'device_keys' AND column_name = 'updated_ts') THEN
            ALTER TABLE device_keys RENAME COLUMN updated_at TO updated_ts;
            RAISE NOTICE 'Renamed device_keys.updated_at to updated_ts';
        END IF;
    END IF;

    -- 8. email_verification_tokens 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'email_verification_tokens' AND column_name = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'email_verification_tokens' AND column_name = 'created_ts') THEN
            ALTER TABLE email_verification_tokens RENAME COLUMN created_at TO created_ts;
            RAISE NOTICE 'Renamed email_verification_tokens.created_at to created_ts';
        END IF;
    END IF;

    -- 9. filters 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'filters' AND column_name = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'filters' AND column_name = 'created_ts') THEN
            ALTER TABLE filters RENAME COLUMN created_at TO created_ts;
            RAISE NOTICE 'Renamed filters.created_at to created_ts';
        END IF;
    END IF;

    -- 10. inbound_megolm_sessions 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'inbound_megolm_sessions' AND column_name = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'inbound_megolm_sessions' AND column_name = 'created_ts') THEN
            ALTER TABLE inbound_megolm_sessions RENAME COLUMN created_at TO created_ts;
            RAISE NOTICE 'Renamed inbound_megolm_sessions.created_at to created_ts';
        END IF;
    END IF;

    -- 11. megolm_sessions 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'megolm_sessions' AND column_name = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'megolm_sessions' AND column_name = 'created_ts') THEN
            ALTER TABLE megolm_sessions RENAME COLUMN created_at TO created_ts;
            RAISE NOTICE 'Renamed megolm_sessions.created_at to created_ts';
        END IF;
    END IF;

    -- 12. key_backups 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'key_backups' AND column_name = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'key_backups' AND column_name = 'created_ts') THEN
            ALTER TABLE key_backups RENAME COLUMN created_at TO created_ts;
            RAISE NOTICE 'Renamed key_backups.created_at to created_ts';
        END IF;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'key_backups' AND column_name = 'updated_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'key_backups' AND column_name = 'updated_ts') THEN
            ALTER TABLE key_backups RENAME COLUMN updated_at TO updated_ts;
            RAISE NOTICE 'Renamed key_backups.updated_at to updated_ts';
        END IF;
    END IF;

    -- 13. openid_tokens 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'openid_tokens' AND column_name = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'openid_tokens' AND column_name = 'created_ts') THEN
            ALTER TABLE openid_tokens RENAME COLUMN created_at TO created_ts;
            RAISE NOTICE 'Renamed openid_tokens.created_at to created_ts';
        END IF;
    END IF;

    -- 14. push_config 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'push_config' AND column_name = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'push_config' AND column_name = 'created_ts') THEN
            ALTER TABLE push_config RENAME COLUMN created_at TO created_ts;
            RAISE NOTICE 'Renamed push_config.created_at to created_ts';
        END IF;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'push_config' AND column_name = 'updated_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'push_config' AND column_name = 'updated_ts') THEN
            ALTER TABLE push_config RENAME COLUMN updated_at TO updated_ts;
            RAISE NOTICE 'Renamed push_config.updated_at to updated_ts';
        END IF;
    END IF;

    -- 15. push_device 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'push_device' AND column_name = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'push_device' AND column_name = 'created_ts') THEN
            ALTER TABLE push_device RENAME COLUMN created_at TO created_ts;
            RAISE NOTICE 'Renamed push_device.created_at to created_ts';
        END IF;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'push_device' AND column_name = 'updated_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'push_device' AND column_name = 'updated_ts') THEN
            ALTER TABLE push_device RENAME COLUMN updated_at TO updated_ts;
            RAISE NOTICE 'Renamed push_device.updated_at to updated_ts';
        END IF;
    END IF;

    -- 16. push_rule 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'push_rule' AND column_name = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'push_rule' AND column_name = 'created_ts') THEN
            ALTER TABLE push_rule RENAME COLUMN created_at TO created_ts;
            RAISE NOTICE 'Renamed push_rule.created_at to created_ts';
        END IF;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'push_rule' AND column_name = 'updated_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'push_rule' AND column_name = 'updated_ts') THEN
            ALTER TABLE push_rule RENAME COLUMN updated_at TO updated_ts;
            RAISE NOTICE 'Renamed push_rule.updated_at to updated_ts';
        END IF;
    END IF;

    -- 17. push_stats 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'push_stats' AND column_name = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'push_stats' AND column_name = 'created_ts') THEN
            ALTER TABLE push_stats RENAME COLUMN created_at TO created_ts;
            RAISE NOTICE 'Renamed push_stats.created_at to created_ts';
        END IF;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'push_stats' AND column_name = 'updated_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'push_stats' AND column_name = 'updated_ts') THEN
            ALTER TABLE push_stats RENAME COLUMN updated_at TO updated_ts;
            RAISE NOTICE 'Renamed push_stats.updated_at to updated_ts';
        END IF;
    END IF;

    -- 18. push_notification_queue 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'push_notification_queue' AND column_name = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'push_notification_queue' AND column_name = 'created_ts') THEN
            ALTER TABLE push_notification_queue RENAME COLUMN created_at TO created_ts;
            RAISE NOTICE 'Renamed push_notification_queue.created_at to created_ts';
        END IF;
    END IF;

    -- 19. backup_keys 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'backup_keys' AND column_name = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'backup_keys' AND column_name = 'created_ts') THEN
            ALTER TABLE backup_keys RENAME COLUMN created_at TO created_ts;
            RAISE NOTICE 'Renamed backup_keys.created_at to created_ts';
        END IF;
    END IF;

    -- 20. federation_blacklist_config 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'federation_blacklist_config' AND column_name = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'federation_blacklist_config' AND column_name = 'created_ts') THEN
            ALTER TABLE federation_blacklist_config RENAME COLUMN created_at TO created_ts;
            RAISE NOTICE 'Renamed federation_blacklist_config.created_at to created_ts';
        END IF;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'federation_blacklist_config' AND column_name = 'updated_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'federation_blacklist_config' AND column_name = 'updated_ts') THEN
            ALTER TABLE federation_blacklist_config RENAME COLUMN updated_at TO updated_ts;
            RAISE NOTICE 'Renamed federation_blacklist_config.updated_at to updated_ts';
        END IF;
    END IF;

    -- 21. federation_blacklist_log 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'federation_blacklist_log' AND column_name = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'federation_blacklist_log' AND column_name = 'created_ts') THEN
            ALTER TABLE federation_blacklist_log RENAME COLUMN created_at TO created_ts;
            RAISE NOTICE 'Renamed federation_blacklist_log.created_at to created_ts';
        END IF;
    END IF;

    -- 22. media_quota_alerts 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'media_quota_alerts' AND column_name = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'media_quota_alerts' AND column_name = 'created_ts') THEN
            ALTER TABLE media_quota_alerts RENAME COLUMN created_at TO created_ts;
            RAISE NOTICE 'Renamed media_quota_alerts.created_at to created_ts';
        END IF;
    END IF;

    -- 23. module_execution_logs 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'module_execution_logs' AND column_name = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'module_execution_logs' AND column_name = 'created_ts') THEN
            ALTER TABLE module_execution_logs RENAME COLUMN created_at TO created_ts;
            RAISE NOTICE 'Renamed module_execution_logs.created_at to created_ts';
        END IF;
    END IF;

    -- 24. modules 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'modules' AND column_name = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'modules' AND column_name = 'created_ts') THEN
            ALTER TABLE modules RENAME COLUMN created_at TO created_ts;
            RAISE NOTICE 'Renamed modules.created_at to created_ts';
        END IF;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'modules' AND column_name = 'updated_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'modules' AND column_name = 'updated_ts') THEN
            ALTER TABLE modules RENAME COLUMN updated_at TO updated_ts;
            RAISE NOTICE 'Renamed modules.updated_at to updated_ts';
        END IF;
    END IF;

    -- 25. registration_captcha 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'registration_captcha' AND column_name = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'registration_captcha' AND column_name = 'created_ts') THEN
            ALTER TABLE registration_captcha RENAME COLUMN created_at TO created_ts;
            RAISE NOTICE 'Renamed registration_captcha.created_at to created_ts';
        END IF;
    END IF;

    -- 26. retention_policies 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'retention_policies' AND column_name = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'retention_policies' AND column_name = 'created_ts') THEN
            ALTER TABLE retention_policies RENAME COLUMN created_at TO created_ts;
            RAISE NOTICE 'Renamed retention_policies.created_at to created_ts';
        END IF;
    END IF;

    RAISE NOTICE 'Timestamp column naming standardization (Phase 2) completed';
END $$;
