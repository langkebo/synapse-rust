-- =============================================================================
-- Synapse-Rust 数据库回滚脚本
-- 版本: 20260219000000
-- 创建日期: 2026-02-19
-- 描述: 回滚完整数据库迁移脚本
--
-- 警告: 此脚本将删除所有由迁移脚本创建的表和数据
-- 执行方式:
--   docker exec -i synapse-postgres psql -U synapse -d synapse_test < 20260219000000_complete_schema_rollback.sql
-- =============================================================================

BEGIN;

-- 删除版本记录
DELETE FROM schema_migrations WHERE version = '20260219000000';

-- =============================================================================
-- 回滚: 第二十五部分 - 默认数据
-- =============================================================================

DELETE FROM federation_blacklist_rule WHERE rule_name IN ('block_malicious_servers', 'block_spam_servers', 'quarantine_new_servers');
DELETE FROM push_rules WHERE user_id = '.default';
DELETE FROM push_rule WHERE user_id = '.default';
DELETE FROM push_config WHERE config_key IN ('fcm.enabled', 'apns.enabled', 'webpush.enabled', 'push.rate_limit_per_minute', 'push.batch_size');
DELETE FROM captcha_config WHERE config_key LIKE '%.code_length' OR config_key LIKE '%.code_expiry_minutes' OR config_key LIKE '%.max_attempts' OR config_key = 'global.block_duration_minutes';
DELETE FROM captcha_template WHERE template_name IN ('default_email', 'default_sms');

-- =============================================================================
-- 回滚: 第二十四部分 - 邮件验证表
-- =============================================================================

DROP TABLE IF EXISTS email_verification_tokens CASCADE;

-- =============================================================================
-- 回滚: 第二十三部分 - 监控和性能表
-- =============================================================================

DROP TABLE IF EXISTS security_events CASCADE;
DROP TABLE IF EXISTS ip_blocks CASCADE;
DROP TABLE IF EXISTS ip_reputation CASCADE;

-- =============================================================================
-- 回滚: 第二十二部分 - 工作节点表
-- =============================================================================

DROP TABLE IF EXISTS worker_health_checks CASCADE;
DROP TABLE IF EXISTS worker_connections CASCADE;
DROP TABLE IF EXISTS workers CASCADE;

-- =============================================================================
-- 回滚: 第二十一部分 - 应用服务表
-- =============================================================================

DROP TABLE IF EXISTS application_services CASCADE;

-- =============================================================================
-- 回滚: 第二十部分 - 服务器通知表
-- =============================================================================

DROP TABLE IF EXISTS scheduled_notifications CASCADE;
DROP TABLE IF EXISTS server_notifications CASCADE;

-- =============================================================================
-- 回滚: 第十九部分 - 媒体配额表
-- =============================================================================

DROP TABLE IF EXISTS media_quota_alerts CASCADE;
DROP TABLE IF EXISTS server_media_quota CASCADE;
DROP TABLE IF EXISTS user_media_quota CASCADE;

-- =============================================================================
-- 回滚: 第十八部分 - 数据保留表
-- =============================================================================

DROP TABLE IF EXISTS retention_cleanup_logs CASCADE;
DROP TABLE IF EXISTS retention_policies CASCADE;

-- =============================================================================
-- 回滚: 第十七部分 - 线程表
-- =============================================================================

DROP TABLE IF EXISTS thread_subscriptions CASCADE;
DROP TABLE IF EXISTS thread_replies CASCADE;
DROP TABLE IF EXISTS thread_roots CASCADE;

-- =============================================================================
-- 回滚: 第十六部分 - 空间表
-- =============================================================================

DROP TABLE IF EXISTS space_children CASCADE;
DROP TABLE IF EXISTS spaces CASCADE;

-- =============================================================================
-- 回滚: 第十五部分 - 刷新令牌表
-- =============================================================================

DROP TABLE IF EXISTS refresh_token_usage CASCADE;
DROP TABLE IF EXISTS refresh_token_rotations CASCADE;
DROP TABLE IF EXISTS refresh_token_families CASCADE;
DROP TABLE IF EXISTS refresh_tokens CASCADE;

-- =============================================================================
-- 回滚: 第十四部分 - 后台更新表
-- =============================================================================

DROP TABLE IF EXISTS background_updates CASCADE;

-- =============================================================================
-- 回滚: 第十三部分 - 事件报告表
-- =============================================================================

DROP TABLE IF EXISTS event_report_stats CASCADE;
DROP TABLE IF EXISTS event_report_history CASCADE;
DROP TABLE IF EXISTS event_reports CASCADE;

-- =============================================================================
-- 回滚: 第十二部分 - 注册令牌表
-- =============================================================================

DROP TABLE IF EXISTS registration_token_usage CASCADE;
DROP TABLE IF EXISTS registration_token_batches CASCADE;
DROP TABLE IF EXISTS registration_tokens CASCADE;

-- =============================================================================
-- 回滚: 第十一部分 - 模块管理表
-- =============================================================================

DROP TABLE IF EXISTS module_execution_logs CASCADE;
DROP TABLE IF EXISTS modules CASCADE;

-- =============================================================================
-- 回滚: 第十部分 - 联邦黑名单表
-- =============================================================================

DROP TABLE IF EXISTS federation_blacklist_config CASCADE;
DROP TABLE IF EXISTS federation_blacklist_log CASCADE;
DROP TABLE IF EXISTS federation_blacklist_rule CASCADE;
DROP TABLE IF EXISTS federation_blacklist CASCADE;

-- =============================================================================
-- 回滚: 第九部分 - CAS 认证表
-- =============================================================================

DROP TABLE IF EXISTS cas_user_attributes CASCADE;
DROP TABLE IF EXISTS cas_services CASCADE;
DROP TABLE IF EXISTS cas_tickets CASCADE;

-- =============================================================================
-- 回滚: 第八部分 - SAML 认证表
-- =============================================================================

DROP TABLE IF EXISTS saml_identity_providers CASCADE;
DROP TABLE IF EXISTS saml_sessions CASCADE;
DROP TABLE IF EXISTS saml_user_mapping CASCADE;

-- =============================================================================
-- 回滚: 第七部分 - 验证码表
-- =============================================================================

DROP TABLE IF EXISTS captcha_config CASCADE;
DROP TABLE IF EXISTS captcha_template CASCADE;
DROP TABLE IF EXISTS captcha_rate_limit CASCADE;
DROP TABLE IF EXISTS captcha_send_log CASCADE;
DROP TABLE IF EXISTS registration_captcha CASCADE;

-- =============================================================================
-- 回滚: 第六部分 - 推送通知表
-- =============================================================================

DROP TABLE IF EXISTS pushers CASCADE;
DROP TABLE IF EXISTS push_rules CASCADE;
DROP TABLE IF EXISTS push_stats CASCADE;
DROP TABLE IF EXISTS push_config CASCADE;
DROP TABLE IF EXISTS push_notification_log CASCADE;
DROP TABLE IF EXISTS push_notification_queue CASCADE;
DROP TABLE IF EXISTS push_rule CASCADE;
DROP TABLE IF EXISTS push_device CASCADE;

-- =============================================================================
-- 回滚: 第五部分 - 媒体和语音消息表
-- =============================================================================

DROP TABLE IF EXISTS voice_messages CASCADE;
DROP TABLE IF EXISTS media_repository CASCADE;

-- =============================================================================
-- 回滚: 第四部分 - E2EE 加密密钥表
-- =============================================================================

DROP TABLE IF EXISTS device_signatures CASCADE;
DROP TABLE IF EXISTS cross_signing_keys CASCADE;
DROP TABLE IF EXISTS device_keys CASCADE;

-- =============================================================================
-- 回滚: 第三部分 - 事件表
-- =============================================================================

DROP TABLE IF EXISTS events CASCADE;

-- =============================================================================
-- 回滚: 第二部分 - 房间和成员表
-- =============================================================================

DROP TABLE IF EXISTS room_members CASCADE;
DROP TABLE IF EXISTS rooms CASCADE;

-- =============================================================================
-- 回滚: 第一部分 - 核心用户和认证表
-- =============================================================================

DROP TABLE IF EXISTS access_tokens CASCADE;
DROP TABLE IF EXISTS devices CASCADE;
DROP TABLE IF EXISTS users CASCADE;

-- =============================================================================
-- 回滚: 版本记录表
-- =============================================================================

DROP TABLE IF EXISTS schema_migrations CASCADE;

-- =============================================================================
-- 验证回滚
-- =============================================================================

DO $$
DECLARE
    table_count INTEGER;
BEGIN
    SELECT COUNT(*) INTO table_count
    FROM information_schema.tables 
    WHERE table_schema = 'public';
    
    RAISE NOTICE '==========================================';
    RAISE NOTICE 'Rollback completed!';
    RAISE NOTICE 'Remaining tables in public schema: %', table_count;
    RAISE NOTICE '==========================================';
END $$;

COMMIT;

-- =============================================================================
-- 回滚完成
-- =============================================================================
