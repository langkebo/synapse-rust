-- =============================================================================
-- Synapse-Rust 统一数据库回滚脚本
-- 版本: 1.0.0
-- 创建日期: 2026-02-20
-- 描述: 回滚统一迁移脚本创建的所有表结构
-- 
-- 执行方式:
--   docker exec -i synapse-postgres psql -U synapse -d synapse_test < 00000000_unified_rollback.sql
-- =============================================================================

BEGIN;

-- =============================================================================
-- 删除所有表 (按依赖顺序反向删除)
-- =============================================================================

-- 第二十三部分: 邮件验证表
DROP TABLE IF EXISTS email_verification_tokens CASCADE;

-- 第二十二部分: 安全和监控表
DROP TABLE IF EXISTS security_events CASCADE;
DROP TABLE IF EXISTS ip_blocks CASCADE;
DROP TABLE IF EXISTS ip_reputation CASCADE;

-- 第二十一部分: Worker 表
DROP TABLE IF EXISTS worker_health_checks CASCADE;
DROP TABLE IF EXISTS worker_connections CASCADE;
DROP TABLE IF EXISTS workers CASCADE;

-- 第二十部分: 应用服务表
DROP TABLE IF EXISTS application_services CASCADE;

-- 第十九部分: 服务器通知表
DROP TABLE IF EXISTS scheduled_notifications CASCADE;
DROP TABLE IF EXISTS server_notifications CASCADE;

-- 第十八部分: 媒体配额表
DROP TABLE IF EXISTS media_quota_alerts CASCADE;
DROP TABLE IF EXISTS server_media_quota CASCADE;
DROP TABLE IF EXISTS user_media_quota CASCADE;

-- 第十七部分: 数据保留表
DROP TABLE IF EXISTS retention_cleanup_logs CASCADE;
DROP TABLE IF EXISTS retention_policies CASCADE;

-- 第十六部分: 线程表
DROP TABLE IF EXISTS thread_subscriptions CASCADE;
DROP TABLE IF EXISTS thread_replies CASCADE;
DROP TABLE IF EXISTS thread_roots CASCADE;

-- 第十五部分: 空间表
DROP TABLE IF EXISTS space_children CASCADE;
DROP TABLE IF EXISTS spaces CASCADE;

-- 第十四部分: 后台更新表
DROP TABLE IF EXISTS background_updates CASCADE;

-- 第十三部分: 事件报告表
DROP TABLE IF EXISTS event_report_stats CASCADE;
DROP TABLE IF EXISTS report_rate_limits CASCADE;
DROP TABLE IF EXISTS event_report_history CASCADE;
DROP TABLE IF EXISTS event_reports CASCADE;

-- 第十二部分: 注册令牌表
DROP TABLE IF EXISTS registration_token_batches CASCADE;
DROP TABLE IF EXISTS registration_token_usage CASCADE;
DROP TABLE IF EXISTS registration_tokens CASCADE;

-- 第十一部分: 模块表
DROP TABLE IF EXISTS module_execution_logs CASCADE;
DROP TABLE IF EXISTS modules CASCADE;

-- 第十部分: CAS 认证表
DROP TABLE IF EXISTS cas_user_attributes CASCADE;
DROP TABLE IF EXISTS cas_services CASCADE;
DROP TABLE IF EXISTS cas_tickets CASCADE;

-- 第九部分: SAML 认证表
DROP TABLE IF EXISTS saml_identity_providers CASCADE;
DROP TABLE IF EXISTS saml_sessions CASCADE;
DROP TABLE IF EXISTS saml_user_mapping CASCADE;

-- 第八部分: 验证码表
DROP TABLE IF EXISTS captcha_config CASCADE;
DROP TABLE IF EXISTS captcha_template CASCADE;
DROP TABLE IF EXISTS captcha_rate_limit CASCADE;
DROP TABLE IF EXISTS captcha_send_log CASCADE;
DROP TABLE IF EXISTS registration_captcha CASCADE;

-- 第七部分: 推送通知表
DROP TABLE IF EXISTS push_rules CASCADE;
DROP TABLE IF EXISTS pushers CASCADE;
DROP TABLE IF EXISTS push_stats CASCADE;
DROP TABLE IF EXISTS push_config CASCADE;
DROP TABLE IF EXISTS push_notification_log CASCADE;
DROP TABLE IF EXISTS push_notification_queue CASCADE;
DROP TABLE IF EXISTS push_rule CASCADE;
DROP TABLE IF EXISTS push_device CASCADE;

-- 第六部分: 联邦表
DROP TABLE IF EXISTS federation_blacklist_config CASCADE;
DROP TABLE IF EXISTS federation_blacklist_log CASCADE;
DROP TABLE IF EXISTS federation_blacklist_rule CASCADE;
DROP TABLE IF EXISTS federation_blacklist CASCADE;
DROP TABLE IF EXISTS federation_signing_keys CASCADE;

-- 第五部分: 媒体和语音消息表
DROP TABLE IF EXISTS voice_messages CASCADE;
DROP TABLE IF EXISTS media_repository CASCADE;

-- 第四部分: E2EE 加密密钥表
DROP TABLE IF EXISTS device_signatures CASCADE;
DROP TABLE IF EXISTS cross_signing_keys CASCADE;
DROP TABLE IF EXISTS device_keys CASCADE;

-- 第三部分: 事件表
DROP TABLE IF EXISTS events CASCADE;

-- 第二部分: 房间和成员表
DROP TABLE IF EXISTS blocked_rooms CASCADE;
DROP TABLE IF EXISTS room_invites CASCADE;
DROP TABLE IF EXISTS room_members CASCADE;
DROP TABLE IF EXISTS rooms CASCADE;

-- 第一部分: 核心用户和认证表
DROP TABLE IF EXISTS refresh_token_usage CASCADE;
DROP TABLE IF EXISTS refresh_token_rotations CASCADE;
DROP TABLE IF EXISTS refresh_token_families CASCADE;
DROP TABLE IF EXISTS token_blacklist CASCADE;
DROP TABLE IF EXISTS refresh_tokens CASCADE;
DROP TABLE IF EXISTS access_tokens CASCADE;
DROP TABLE IF EXISTS devices CASCADE;
DROP TABLE IF EXISTS users CASCADE;

-- 版本记录表
DROP TABLE IF EXISTS db_metadata CASCADE;
DROP TABLE IF EXISTS schema_migrations CASCADE;

-- =============================================================================
-- 删除扩展 (可选)
-- =============================================================================

-- DROP EXTENSION IF EXISTS "uuid-ossp";
-- DROP EXTENSION IF EXISTS pgcrypto;

COMMIT;

-- =============================================================================
-- 回滚完成
-- =============================================================================
