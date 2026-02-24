-- =============================================================================
-- Synapse-Rust 数据库迁移回滚脚本
-- 版本: 20260228000000
-- 描述: 回滚外键约束
-- =============================================================================

SET client_encoding = 'UTF8';
SET standard_conforming_strings = on;

-- =============================================================================
-- 删除所有外键约束
-- =============================================================================

-- 核心表外键
ALTER TABLE room_members DROP CONSTRAINT IF EXISTS fk_room_members_room_id;
ALTER TABLE room_members DROP CONSTRAINT IF EXISTS fk_room_members_user_id;
ALTER TABLE room_members DROP CONSTRAINT IF EXISTS fk_room_members_inviter_id;
ALTER TABLE events DROP CONSTRAINT IF EXISTS fk_events_room_id;
ALTER TABLE events DROP CONSTRAINT IF EXISTS fk_events_sender;

-- E2EE 相关表外键
ALTER TABLE device_keys DROP CONSTRAINT IF EXISTS fk_device_keys_user_id;
ALTER TABLE cross_signing_keys DROP CONSTRAINT IF EXISTS fk_cross_signing_keys_user_id;
ALTER TABLE device_signatures DROP CONSTRAINT IF EXISTS fk_device_signatures_user_id;
ALTER TABLE megolm_sessions DROP CONSTRAINT IF EXISTS fk_megolm_sessions_room_id;
ALTER TABLE inbound_megolm_sessions DROP CONSTRAINT IF EXISTS fk_inbound_megolm_sessions_room_id;
ALTER TABLE key_backups DROP CONSTRAINT IF EXISTS fk_key_backups_user_id;
ALTER TABLE backup_keys DROP CONSTRAINT IF EXISTS fk_backup_keys_user_id;
ALTER TABLE backup_keys DROP CONSTRAINT IF EXISTS fk_backup_keys_backup_id;
ALTER TABLE e2ee_secret_storage_keys DROP CONSTRAINT IF EXISTS fk_e2ee_secret_storage_keys_user_id;
ALTER TABLE e2ee_stored_secrets DROP CONSTRAINT IF EXISTS fk_e2ee_stored_secrets_user_id;
ALTER TABLE e2ee_stored_secrets DROP CONSTRAINT IF EXISTS fk_e2ee_stored_secrets_key_id;

-- 推送通知表外键
ALTER TABLE push_device DROP CONSTRAINT IF EXISTS fk_push_device_user_id;
ALTER TABLE push_rule DROP CONSTRAINT IF EXISTS fk_push_rule_user_id;
ALTER TABLE push_notification_queue DROP CONSTRAINT IF EXISTS fk_push_notification_queue_user_id;
ALTER TABLE push_notification_log DROP CONSTRAINT IF EXISTS fk_push_notification_log_user_id;
ALTER TABLE pushers DROP CONSTRAINT IF EXISTS fk_pushers_user_id;
ALTER TABLE push_rules DROP CONSTRAINT IF EXISTS fk_push_rules_user_id;
ALTER TABLE push_stats DROP CONSTRAINT IF EXISTS fk_push_stats_user_id;

-- 认证表外键 (SAML/CAS)
ALTER TABLE saml_user_mapping DROP CONSTRAINT IF EXISTS fk_saml_user_mapping_user_id;
ALTER TABLE saml_sessions DROP CONSTRAINT IF EXISTS fk_saml_sessions_user_id;
ALTER TABLE cas_tickets DROP CONSTRAINT IF EXISTS fk_cas_tickets_user_id;
ALTER TABLE cas_user_attributes DROP CONSTRAINT IF EXISTS fk_cas_user_attributes_user_id;

-- 空间和线程表外键
ALTER TABLE spaces DROP CONSTRAINT IF EXISTS fk_spaces_room_id;
ALTER TABLE spaces DROP CONSTRAINT IF EXISTS fk_spaces_creator;
ALTER TABLE spaces DROP CONSTRAINT IF EXISTS fk_spaces_parent_space_id;
ALTER TABLE space_children DROP CONSTRAINT IF EXISTS fk_space_children_space_id;
ALTER TABLE space_children DROP CONSTRAINT IF EXISTS fk_space_children_room_id;
ALTER TABLE space_children DROP CONSTRAINT IF EXISTS fk_space_children_added_by;
ALTER TABLE thread_roots DROP CONSTRAINT IF EXISTS fk_thread_roots_room_id;
ALTER TABLE thread_roots DROP CONSTRAINT IF EXISTS fk_thread_roots_creator;
ALTER TABLE thread_replies DROP CONSTRAINT IF EXISTS fk_thread_replies_room_id;
ALTER TABLE thread_replies DROP CONSTRAINT IF EXISTS fk_thread_replies_sender;
ALTER TABLE thread_subscriptions DROP CONSTRAINT IF EXISTS fk_thread_subscriptions_user_id;

-- 数据保留表外键
ALTER TABLE retention_policies DROP CONSTRAINT IF EXISTS fk_retention_policies_room_id;
ALTER TABLE retention_cleanup_logs DROP CONSTRAINT IF EXISTS fk_retention_cleanup_logs_room_id;

-- 媒体和配额表外键
ALTER TABLE voice_messages DROP CONSTRAINT IF EXISTS fk_voice_messages_room_id;
ALTER TABLE voice_messages DROP CONSTRAINT IF EXISTS fk_voice_messages_user_id;
ALTER TABLE media_quota_alerts DROP CONSTRAINT IF EXISTS fk_media_quota_alerts_user_id;
ALTER TABLE media_repository DROP CONSTRAINT IF EXISTS fk_media_repository_user_id;

-- 通知和 Worker 表外键
ALTER TABLE scheduled_notifications DROP CONSTRAINT IF EXISTS fk_scheduled_notifications_notification_id;
ALTER TABLE worker_connections DROP CONSTRAINT IF EXISTS fk_worker_connections_source_worker_id;
ALTER TABLE worker_connections DROP CONSTRAINT IF EXISTS fk_worker_connections_target_worker_id;
ALTER TABLE worker_health_checks DROP CONSTRAINT IF EXISTS fk_worker_health_checks_worker_id;

-- 安全和验证表外键
ALTER TABLE email_verification_tokens DROP CONSTRAINT IF EXISTS fk_email_verification_tokens_user_id;
ALTER TABLE security_events DROP CONSTRAINT IF EXISTS fk_security_events_user_id;

-- 删除迁移记录
DELETE FROM schema_migrations WHERE version = '20260228000000';

-- 验证
DO $$
DECLARE
    fk_count INTEGER;
BEGIN
    SELECT COUNT(*) INTO fk_count
    FROM information_schema.table_constraints 
    WHERE constraint_type = 'FOREIGN KEY' 
    AND table_schema = 'public';
    
    RAISE NOTICE '==========================================';
    RAISE NOTICE 'Foreign key constraints rollback completed!';
    RAISE NOTICE 'Remaining foreign key constraints: %', fk_count;
    RAISE NOTICE '==========================================';
END $$;
