-- =============================================================================
-- Synapse-Rust 数据库迁移脚本
-- 版本: 20260228000000
-- 描述: 添加外键约束以确保数据完整性
-- 任务: Task 9 - 补充外键约束
-- =============================================================================

SET client_encoding = 'UTF8';
SET standard_conforming_strings = on;

-- =============================================================================
-- 第一部分: 清理孤儿数据
-- 在添加外键约束之前，需要先清理不满足外键条件的数据
-- =============================================================================

-- 创建系统用户 .default 用于推送规则默认配置
INSERT INTO users (user_id, username, creation_ts)
VALUES ('.default', '.default_system_user', 0)
ON CONFLICT (user_id) DO NOTHING;

-- 清理 room_members 孤儿数据
DELETE FROM room_members WHERE room_id IS NOT NULL AND room_id != '' 
    AND NOT EXISTS (SELECT 1 FROM rooms WHERE rooms.room_id = room_members.room_id);
DELETE FROM room_members WHERE user_id IS NOT NULL AND user_id != '' 
    AND NOT EXISTS (SELECT 1 FROM users WHERE users.user_id = room_members.user_id);
DELETE FROM room_members WHERE inviter_id IS NOT NULL AND inviter_id != '' 
    AND NOT EXISTS (SELECT 1 FROM users WHERE users.user_id = room_members.inviter_id);

-- 清理 events 孤儿数据
DELETE FROM events WHERE room_id IS NOT NULL AND room_id != '' 
    AND NOT EXISTS (SELECT 1 FROM rooms WHERE rooms.room_id = events.room_id);
DELETE FROM events WHERE sender IS NOT NULL AND sender != '' 
    AND NOT EXISTS (SELECT 1 FROM users WHERE users.user_id = events.sender);

-- 清理 device_keys 孤儿数据
DELETE FROM device_keys WHERE user_id IS NOT NULL AND user_id != '' 
    AND NOT EXISTS (SELECT 1 FROM users WHERE users.user_id = device_keys.user_id);

-- 清理 cross_signing_keys 孤儿数据
DELETE FROM cross_signing_keys WHERE user_id IS NOT NULL AND user_id != '' 
    AND NOT EXISTS (SELECT 1 FROM users WHERE users.user_id = cross_signing_keys.user_id);

-- 清理 device_signatures 孤儿数据
DELETE FROM device_signatures WHERE user_id IS NOT NULL AND user_id != '' 
    AND NOT EXISTS (SELECT 1 FROM users WHERE users.user_id = device_signatures.user_id);

-- 清理 megolm_sessions 孤儿数据 (表可能不存在)
DELETE FROM megolm_sessions WHERE room_id IS NOT NULL AND room_id != '' 
    AND NOT EXISTS (SELECT 1 FROM rooms WHERE rooms.room_id = megolm_sessions.room_id);

-- 清理 inbound_megolm_sessions 孤儿数据 (表可能不存在)
DELETE FROM inbound_megolm_sessions WHERE room_id IS NOT NULL AND room_id != '' 
    AND NOT EXISTS (SELECT 1 FROM rooms WHERE rooms.room_id = inbound_megolm_sessions.room_id);

-- 清理 key_backups 孤儿数据 (表可能不存在)
DELETE FROM key_backups WHERE user_id IS NOT NULL AND user_id != '' 
    AND NOT EXISTS (SELECT 1 FROM users WHERE users.user_id = key_backups.user_id);

-- 清理 backup_keys 孤儿数据 (表可能不存在)
DELETE FROM backup_keys WHERE user_id IS NOT NULL AND user_id != '' 
    AND NOT EXISTS (SELECT 1 FROM users WHERE users.user_id = backup_keys.user_id);
DELETE FROM backup_keys WHERE backup_id IS NOT NULL AND backup_id != '' 
    AND NOT EXISTS (SELECT 1 FROM key_backups WHERE key_backups.backup_id = backup_keys.backup_id);

-- 清理 voice_messages 孤儿数据
DELETE FROM voice_messages WHERE room_id IS NOT NULL AND room_id != '' 
    AND NOT EXISTS (SELECT 1 FROM rooms WHERE rooms.room_id = voice_messages.room_id);
DELETE FROM voice_messages WHERE user_id IS NOT NULL AND user_id != '' 
    AND NOT EXISTS (SELECT 1 FROM users WHERE users.user_id = voice_messages.user_id);

-- 清理 push_device 孤儿数据
DELETE FROM push_device WHERE user_id IS NOT NULL AND user_id != '' 
    AND NOT EXISTS (SELECT 1 FROM users WHERE users.user_id = push_device.user_id);

-- 清理 push_rule 孤儿数据 (排除 .default 系统用户)
DELETE FROM push_rule WHERE user_id IS NOT NULL AND user_id != '' AND user_id != '.default'
    AND NOT EXISTS (SELECT 1 FROM users WHERE users.user_id = push_rule.user_id);

-- 清理 push_notification_queue 孤儿数据
DELETE FROM push_notification_queue WHERE user_id IS NOT NULL AND user_id != '' 
    AND NOT EXISTS (SELECT 1 FROM users WHERE users.user_id = push_notification_queue.user_id);

-- 清理 push_notification_log 孤儿数据
DELETE FROM push_notification_log WHERE user_id IS NOT NULL AND user_id != '' 
    AND NOT EXISTS (SELECT 1 FROM users WHERE users.user_id = push_notification_log.user_id);

-- 清理 pushers 孤儿数据
DELETE FROM pushers WHERE user_id IS NOT NULL AND user_id != '' 
    AND NOT EXISTS (SELECT 1 FROM users WHERE users.user_id = pushers.user_id);

-- 清理 push_rules 孤儿数据 (排除 .default 系统用户)
DELETE FROM push_rules WHERE user_id IS NOT NULL AND user_id != '' AND user_id != '.default'
    AND NOT EXISTS (SELECT 1 FROM users WHERE users.user_id = push_rules.user_id);

-- 清理 push_stats 孤儿数据
DELETE FROM push_stats WHERE user_id IS NOT NULL AND user_id != '' 
    AND NOT EXISTS (SELECT 1 FROM users WHERE users.user_id = push_stats.user_id);

-- 清理 saml_user_mapping 孤儿数据
DELETE FROM saml_user_mapping WHERE user_id IS NOT NULL AND user_id != '' 
    AND NOT EXISTS (SELECT 1 FROM users WHERE users.user_id = saml_user_mapping.user_id);

-- 清理 saml_sessions 孤儿数据
DELETE FROM saml_sessions WHERE user_id IS NOT NULL AND user_id != '' 
    AND NOT EXISTS (SELECT 1 FROM users WHERE users.user_id = saml_sessions.user_id);

-- 清理 cas_tickets 孤儿数据
DELETE FROM cas_tickets WHERE user_id IS NOT NULL AND user_id != '' 
    AND NOT EXISTS (SELECT 1 FROM users WHERE users.user_id = cas_tickets.user_id);

-- 清理 cas_user_attributes 孤儿数据
DELETE FROM cas_user_attributes WHERE user_id IS NOT NULL AND user_id != '' 
    AND NOT EXISTS (SELECT 1 FROM users WHERE users.user_id = cas_user_attributes.user_id);

-- 清理 spaces 孤儿数据
DELETE FROM spaces WHERE room_id IS NOT NULL AND room_id != '' 
    AND NOT EXISTS (SELECT 1 FROM rooms WHERE rooms.room_id = spaces.room_id);
DELETE FROM spaces WHERE creator IS NOT NULL AND creator != '' 
    AND NOT EXISTS (SELECT 1 FROM users WHERE users.user_id = spaces.creator);

-- 清理 space_children 孤儿数据
DELETE FROM space_children WHERE space_id IS NOT NULL AND space_id != '' 
    AND NOT EXISTS (SELECT 1 FROM spaces WHERE spaces.space_id = space_children.space_id);
DELETE FROM space_children WHERE room_id IS NOT NULL AND room_id != '' 
    AND NOT EXISTS (SELECT 1 FROM rooms WHERE rooms.room_id = space_children.room_id);
DELETE FROM space_children WHERE added_by IS NOT NULL AND added_by != '' 
    AND NOT EXISTS (SELECT 1 FROM users WHERE users.user_id = space_children.added_by);

-- 清理 thread_roots 孤儿数据
DELETE FROM thread_roots WHERE room_id IS NOT NULL AND room_id != '' 
    AND NOT EXISTS (SELECT 1 FROM rooms WHERE rooms.room_id = thread_roots.room_id);
DELETE FROM thread_roots WHERE creator IS NOT NULL AND creator != '' 
    AND NOT EXISTS (SELECT 1 FROM users WHERE users.user_id = thread_roots.creator);

-- 清理 thread_replies 孤儿数据
DELETE FROM thread_replies WHERE room_id IS NOT NULL AND room_id != '' 
    AND NOT EXISTS (SELECT 1 FROM rooms WHERE rooms.room_id = thread_replies.room_id);
DELETE FROM thread_replies WHERE sender IS NOT NULL AND sender != '' 
    AND NOT EXISTS (SELECT 1 FROM users WHERE users.user_id = thread_replies.sender);

-- 清理 thread_subscriptions 孤儿数据
DELETE FROM thread_subscriptions WHERE user_id IS NOT NULL AND user_id != '' 
    AND NOT EXISTS (SELECT 1 FROM users WHERE users.user_id = thread_subscriptions.user_id);

-- 清理 retention_policies 孤儿数据
DELETE FROM retention_policies WHERE room_id IS NOT NULL AND room_id != '' 
    AND NOT EXISTS (SELECT 1 FROM rooms WHERE rooms.room_id = retention_policies.room_id);

-- 清理 retention_cleanup_logs 孤儿数据
DELETE FROM retention_cleanup_logs WHERE room_id IS NOT NULL AND room_id != '' 
    AND NOT EXISTS (SELECT 1 FROM rooms WHERE rooms.room_id = retention_cleanup_logs.room_id);

-- 清理 media_quota_alerts 孤儿数据
DELETE FROM media_quota_alerts WHERE user_id IS NOT NULL AND user_id != '' 
    AND NOT EXISTS (SELECT 1 FROM users WHERE users.user_id = media_quota_alerts.user_id);

-- 清理 scheduled_notifications 孤儿数据
DELETE FROM scheduled_notifications WHERE notification_id IS NOT NULL 
    AND NOT EXISTS (SELECT 1 FROM server_notifications WHERE server_notifications.id = scheduled_notifications.notification_id);

-- 清理 worker_connections 孤儿数据
DELETE FROM worker_connections WHERE source_worker_id IS NOT NULL AND source_worker_id != '' 
    AND NOT EXISTS (SELECT 1 FROM workers WHERE workers.worker_id = worker_connections.source_worker_id);
DELETE FROM worker_connections WHERE target_worker_id IS NOT NULL AND target_worker_id != '' 
    AND NOT EXISTS (SELECT 1 FROM workers WHERE workers.worker_id = worker_connections.target_worker_id);

-- 清理 worker_health_checks 孤儿数据
DELETE FROM worker_health_checks WHERE worker_id IS NOT NULL AND worker_id != '' 
    AND NOT EXISTS (SELECT 1 FROM workers WHERE workers.worker_id = worker_health_checks.worker_id);

-- 清理 email_verification_tokens 孤儿数据
DELETE FROM email_verification_tokens WHERE user_id IS NOT NULL AND user_id != '' 
    AND NOT EXISTS (SELECT 1 FROM users WHERE users.user_id = email_verification_tokens.user_id);

-- 清理 e2ee_secret_storage_keys 孤儿数据
DELETE FROM e2ee_secret_storage_keys WHERE user_id IS NOT NULL AND user_id != '' 
    AND NOT EXISTS (SELECT 1 FROM users WHERE users.user_id = e2ee_secret_storage_keys.user_id);

-- 清理 e2ee_stored_secrets 孤儿数据
DELETE FROM e2ee_stored_secrets WHERE user_id IS NOT NULL AND user_id != '' 
    AND NOT EXISTS (SELECT 1 FROM users WHERE users.user_id = e2ee_stored_secrets.user_id);

-- 清理 security_events 孤儿数据 (保留 user_id 为 NULL 的记录，因为安全事件可能不关联用户)
DELETE FROM security_events WHERE user_id IS NOT NULL AND user_id != '' 
    AND NOT EXISTS (SELECT 1 FROM users WHERE users.user_id = security_events.user_id);

-- 清理 media_repository 孤儿数据
DELETE FROM media_repository WHERE user_id IS NOT NULL AND user_id != '' 
    AND NOT EXISTS (SELECT 1 FROM users WHERE users.user_id = media_repository.user_id);

-- =============================================================================
-- 第二部分: 添加外键约束 - 核心表
-- =============================================================================

-- room_members 表外键
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_room_members_room_id'
    ) THEN
        ALTER TABLE room_members 
            ADD CONSTRAINT fk_room_members_room_id 
            FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_room_members_user_id'
    ) THEN
        ALTER TABLE room_members 
            ADD CONSTRAINT fk_room_members_user_id 
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_room_members_inviter_id'
    ) THEN
        ALTER TABLE room_members 
            ADD CONSTRAINT fk_room_members_inviter_id 
            FOREIGN KEY (inviter_id) REFERENCES users(user_id) ON DELETE SET NULL;
    END IF;
END $$;

-- events 表外键
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_events_room_id'
    ) THEN
        ALTER TABLE events 
            ADD CONSTRAINT fk_events_room_id 
            FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_events_sender'
    ) THEN
        ALTER TABLE events 
            ADD CONSTRAINT fk_events_sender 
            FOREIGN KEY (sender) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;
END $$;

-- =============================================================================
-- 第三部分: 添加外键约束 - E2EE 相关表
-- =============================================================================

-- device_keys 表外键
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_device_keys_user_id'
    ) THEN
        ALTER TABLE device_keys 
            ADD CONSTRAINT fk_device_keys_user_id 
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;
END $$;

-- cross_signing_keys 表外键
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_cross_signing_keys_user_id'
    ) THEN
        IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'cross_signing_keys') THEN
            ALTER TABLE cross_signing_keys 
                ADD CONSTRAINT fk_cross_signing_keys_user_id 
                FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
        END IF;
    END IF;
END $$;

-- device_signatures 表外键
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_device_signatures_user_id'
    ) THEN
        IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'device_signatures') THEN
            ALTER TABLE device_signatures 
                ADD CONSTRAINT fk_device_signatures_user_id 
                FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
        END IF;
    END IF;
END $$;

-- megolm_sessions 表外键 (表可能不存在)
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'megolm_sessions') THEN
        IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_megolm_sessions_room_id') THEN
            ALTER TABLE megolm_sessions 
                ADD CONSTRAINT fk_megolm_sessions_room_id 
                FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
        END IF;
    END IF;
END $$;

-- inbound_megolm_sessions 表外键 (表可能不存在)
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'inbound_megolm_sessions') THEN
        IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_inbound_megolm_sessions_room_id') THEN
            ALTER TABLE inbound_megolm_sessions 
                ADD CONSTRAINT fk_inbound_megolm_sessions_room_id 
                FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
        END IF;
    END IF;
END $$;

-- key_backups 表外键 (表可能不存在)
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'key_backups') THEN
        IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_key_backups_user_id') THEN
            ALTER TABLE key_backups 
                ADD CONSTRAINT fk_key_backups_user_id 
                FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
        END IF;
    END IF;
END $$;

-- backup_keys 表外键 (表可能不存在)
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'backup_keys') THEN
        IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_backup_keys_user_id') THEN
            ALTER TABLE backup_keys 
                ADD CONSTRAINT fk_backup_keys_user_id 
                FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
        END IF;
        
        IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'backup_keys' AND column_name = 'backup_id') THEN
            IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fk_backup_keys_backup_id') THEN
                ALTER TABLE backup_keys 
                    ADD CONSTRAINT fk_backup_keys_backup_id 
                    FOREIGN KEY (backup_id) REFERENCES key_backups(backup_id) ON DELETE CASCADE;
            END IF;
        END IF;
    END IF;
END $$;

-- e2ee_secret_storage_keys 表外键 (表可能不存在)
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'e2ee_secret_storage_keys') THEN
        ALTER TABLE e2ee_secret_storage_keys 
            ADD CONSTRAINT fk_e2ee_secret_storage_keys_user_id 
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;
END $$;

-- e2ee_stored_secrets 表外键 (表可能不存在)
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'e2ee_stored_secrets') THEN
        ALTER TABLE e2ee_stored_secrets 
            ADD CONSTRAINT fk_e2ee_stored_secrets_user_id 
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
        
        IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'e2ee_secret_storage_keys') THEN
            ALTER TABLE e2ee_stored_secrets 
                ADD CONSTRAINT fk_e2ee_stored_secrets_key_id 
                FOREIGN KEY (key_id, user_id) REFERENCES e2ee_secret_storage_keys(key_id, user_id) ON DELETE CASCADE;
        END IF;
    END IF;
END $$;

-- =============================================================================
-- 第四部分: 添加外键约束 - 推送通知表
-- =============================================================================

-- push_device 表外键
ALTER TABLE push_device 
    ADD CONSTRAINT fk_push_device_user_id 
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;

-- push_rule 表外键 (排除 .default 系统用户)
ALTER TABLE push_rule 
    ADD CONSTRAINT fk_push_rule_user_id 
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;

-- push_notification_queue 表外键
ALTER TABLE push_notification_queue 
    ADD CONSTRAINT fk_push_notification_queue_user_id 
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;

-- push_notification_log 表外键
ALTER TABLE push_notification_log 
    ADD CONSTRAINT fk_push_notification_log_user_id 
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;

-- pushers 表外键
ALTER TABLE pushers 
    ADD CONSTRAINT fk_pushers_user_id 
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;

-- push_rules 表外键
ALTER TABLE push_rules 
    ADD CONSTRAINT fk_push_rules_user_id 
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;

-- push_stats 表外键
ALTER TABLE push_stats 
    ADD CONSTRAINT fk_push_stats_user_id 
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;

-- =============================================================================
-- 第五部分: 添加外键约束 - 认证表 (SAML/CAS)
-- =============================================================================

-- saml_user_mapping 表外键
ALTER TABLE saml_user_mapping 
    ADD CONSTRAINT fk_saml_user_mapping_user_id 
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;

-- saml_sessions 表外键
ALTER TABLE saml_sessions 
    ADD CONSTRAINT fk_saml_sessions_user_id 
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;

-- cas_tickets 表外键
ALTER TABLE cas_tickets 
    ADD CONSTRAINT fk_cas_tickets_user_id 
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;

-- cas_user_attributes 表外键
ALTER TABLE cas_user_attributes 
    ADD CONSTRAINT fk_cas_user_attributes_user_id 
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;

-- =============================================================================
-- 第六部分: 添加外键约束 - 空间和线程表
-- =============================================================================

-- spaces 表外键
ALTER TABLE spaces 
    ADD CONSTRAINT fk_spaces_room_id 
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;

ALTER TABLE spaces 
    ADD CONSTRAINT fk_spaces_creator 
    FOREIGN KEY (creator) REFERENCES users(user_id) ON DELETE CASCADE;

ALTER TABLE spaces 
    ADD CONSTRAINT fk_spaces_parent_space_id 
    FOREIGN KEY (parent_space_id) REFERENCES spaces(space_id) ON DELETE SET NULL;

-- space_children 表外键
ALTER TABLE space_children 
    ADD CONSTRAINT fk_space_children_space_id 
    FOREIGN KEY (space_id) REFERENCES spaces(space_id) ON DELETE CASCADE;

ALTER TABLE space_children 
    ADD CONSTRAINT fk_space_children_room_id 
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;

ALTER TABLE space_children 
    ADD CONSTRAINT fk_space_children_added_by 
    FOREIGN KEY (added_by) REFERENCES users(user_id) ON DELETE CASCADE;

-- thread_roots 表外键
ALTER TABLE thread_roots 
    ADD CONSTRAINT fk_thread_roots_room_id 
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;

ALTER TABLE thread_roots 
    ADD CONSTRAINT fk_thread_roots_creator 
    FOREIGN KEY (creator) REFERENCES users(user_id) ON DELETE CASCADE;

-- thread_replies 表外键
ALTER TABLE thread_replies 
    ADD CONSTRAINT fk_thread_replies_room_id 
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;

ALTER TABLE thread_replies 
    ADD CONSTRAINT fk_thread_replies_sender 
    FOREIGN KEY (sender) REFERENCES users(user_id) ON DELETE CASCADE;

-- thread_subscriptions 表外键
ALTER TABLE thread_subscriptions 
    ADD CONSTRAINT fk_thread_subscriptions_user_id 
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;

-- =============================================================================
-- 第七部分: 添加外键约束 - 数据保留表
-- =============================================================================

-- retention_policies 表外键
ALTER TABLE retention_policies 
    ADD CONSTRAINT fk_retention_policies_room_id 
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;

-- retention_cleanup_logs 表外键
ALTER TABLE retention_cleanup_logs 
    ADD CONSTRAINT fk_retention_cleanup_logs_room_id 
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE SET NULL;

-- =============================================================================
-- 第八部分: 添加外键约束 - 媒体和配额表
-- =============================================================================

-- voice_messages 表外键
ALTER TABLE voice_messages 
    ADD CONSTRAINT fk_voice_messages_room_id 
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;

ALTER TABLE voice_messages 
    ADD CONSTRAINT fk_voice_messages_user_id 
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;

-- media_quota_alerts 表外键
ALTER TABLE media_quota_alerts 
    ADD CONSTRAINT fk_media_quota_alerts_user_id 
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;

-- media_repository 表外键
ALTER TABLE media_repository 
    ADD CONSTRAINT fk_media_repository_user_id 
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE SET NULL;

-- =============================================================================
-- 第九部分: 添加外键约束 - 通知和 Worker 表
-- =============================================================================

-- scheduled_notifications 表外键
ALTER TABLE scheduled_notifications 
    ADD CONSTRAINT fk_scheduled_notifications_notification_id 
    FOREIGN KEY (notification_id) REFERENCES server_notifications(id) ON DELETE CASCADE;

-- worker_connections 表外键
ALTER TABLE worker_connections 
    ADD CONSTRAINT fk_worker_connections_source_worker_id 
    FOREIGN KEY (source_worker_id) REFERENCES workers(worker_id) ON DELETE CASCADE;

ALTER TABLE worker_connections 
    ADD CONSTRAINT fk_worker_connections_target_worker_id 
    FOREIGN KEY (target_worker_id) REFERENCES workers(worker_id) ON DELETE CASCADE;

-- worker_health_checks 表外键
ALTER TABLE worker_health_checks 
    ADD CONSTRAINT fk_worker_health_checks_worker_id 
    FOREIGN KEY (worker_id) REFERENCES workers(worker_id) ON DELETE CASCADE;

-- =============================================================================
-- 第十部分: 添加外键约束 - 安全和验证表
-- =============================================================================

-- email_verification_tokens 表外键
ALTER TABLE email_verification_tokens 
    ADD CONSTRAINT fk_email_verification_tokens_user_id 
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;

-- security_events 表外键 (允许 NULL，因为安全事件可能不关联用户)
ALTER TABLE security_events 
    ADD CONSTRAINT fk_security_events_user_id 
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE SET NULL;

-- =============================================================================
-- 第十一部分: 记录迁移版本
-- =============================================================================

INSERT INTO schema_migrations (version, description, success)
VALUES ('20260228000000', 'Add foreign key constraints for data integrity', TRUE)
ON CONFLICT (version) DO UPDATE SET success = TRUE, executed_at = NOW();

-- =============================================================================
-- 第十二部分: 验证外键约束
-- =============================================================================

DO $$
DECLARE
    fk_count INTEGER;
BEGIN
    SELECT COUNT(*) INTO fk_count
    FROM information_schema.table_constraints 
    WHERE constraint_type = 'FOREIGN KEY' 
    AND table_schema = 'public';
    
    RAISE NOTICE '==========================================';
    RAISE NOTICE 'Foreign key constraints migration completed!';
    RAISE NOTICE 'Total foreign key constraints: %', fk_count;
    RAISE NOTICE '==========================================';
END $$;
