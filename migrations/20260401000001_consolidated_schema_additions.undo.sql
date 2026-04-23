-- Undo: Consolidated Schema Additions (reverse order)

-- ===== From: 20260404000002_consolidated_minor_features.undo.sql =====
-- ============================================================================
-- Consolidated Minor Features Rollback
-- Created: 2026-04-04
-- Description: Rolls back consolidated minor features migration
-- Replaces: 20260328000002 rollback, 20260330000010 undo, 20260330000011 undo
-- ============================================================================

SET TIME ZONE 'UTC';

DROP INDEX IF EXISTS idx_feature_flag_targets_lookup;
DROP INDEX IF EXISTS idx_feature_flags_expires_at;
DROP INDEX IF EXISTS idx_feature_flags_scope_status;
DROP TABLE IF EXISTS feature_flag_targets;
DROP TABLE IF EXISTS feature_flags;

DROP INDEX IF EXISTS idx_federation_cache_expiry;
DROP INDEX IF EXISTS idx_federation_cache_key;
DROP TABLE IF EXISTS federation_cache;

-- ===== From: 20260404000001_consolidated_schema_alignment.undo.sql =====
-- ============================================================================
-- Consolidated Schema Alignment Rollback
-- Created: 2026-04-04
-- Description: Rolls back consolidated schema alignment migration where possible
-- Replaces: 20260330000001 through 20260330000009, 20260330000013 rollback paths
-- Note: Some ALTER COLUMN changes from 20260330000013 are not automatically reversed.
-- ============================================================================

-- Part 10: 20260330000013_align_legacy_timestamp_columns
-- Timestamp nullability/default alignment is not automatically reversible.

-- Part 9: 20260330000009_align_beacon_and_call_exceptions
DO $$
BEGIN
    DROP TABLE IF EXISTS matrixrtc_encryption_keys;
    DROP TABLE IF EXISTS matrixrtc_memberships;
    DROP TABLE IF EXISTS matrixrtc_sessions;
    DROP TABLE IF EXISTS call_candidates;
    DROP TABLE IF EXISTS call_sessions;
    DROP TABLE IF EXISTS beacon_locations;
    DROP TABLE IF EXISTS beacon_info;
END $$;

-- Part 8: 20260330000008_align_background_update_exceptions
DROP INDEX IF EXISTS idx_background_update_stats_created;
DROP INDEX IF EXISTS idx_background_update_history_job_start;
DROP INDEX IF EXISTS idx_background_update_locks_expires;

DROP TABLE IF EXISTS background_update_stats;
DROP TABLE IF EXISTS background_update_history;
DROP TABLE IF EXISTS background_update_locks;

-- Part 7: 20260330000007_align_uploads_and_user_settings_exceptions
DROP INDEX IF EXISTS idx_upload_chunks_upload_order;
DROP INDEX IF EXISTS idx_upload_progress_user_created_active;
DROP INDEX IF EXISTS idx_upload_progress_expires;

DROP TABLE IF EXISTS upload_chunks;
DROP TABLE IF EXISTS upload_progress;
DROP TABLE IF EXISTS user_settings;

-- Part 6: 20260330000006_align_notifications_push_and_misc_exceptions
DROP VIEW IF EXISTS worker_type_statistics;
DROP VIEW IF EXISTS active_workers;

DROP INDEX IF EXISTS idx_worker_connections_source;
DROP INDEX IF EXISTS idx_application_service_users_as;
DROP INDEX IF EXISTS idx_secure_backup_session_keys_backup;
DROP INDEX IF EXISTS idx_secure_key_backups_user_created;
DROP INDEX IF EXISTS idx_scheduled_notifications_pending;
DROP INDEX IF EXISTS idx_notification_delivery_log_notification_delivered;
DROP INDEX IF EXISTS idx_notification_templates_enabled;
DROP INDEX IF EXISTS idx_user_notification_status_user_created;
DROP INDEX IF EXISTS idx_server_notifications_enabled_priority_created;
DROP INDEX IF EXISTS idx_widget_sessions_widget_active_last_active;
DROP INDEX IF EXISTS idx_widget_permissions_user;
DROP INDEX IF EXISTS idx_widget_permissions_widget;
DROP INDEX IF EXISTS idx_widgets_user_active_created;
DROP INDEX IF EXISTS idx_widgets_room_active_created;
DROP INDEX IF EXISTS idx_worker_task_assignments_worker_status;
DROP INDEX IF EXISTS idx_worker_task_assignments_status_priority_created;
DROP INDEX IF EXISTS idx_worker_load_stats_worker_recorded;
DROP INDEX IF EXISTS idx_moderation_logs_sender_created;
DROP INDEX IF EXISTS idx_moderation_logs_room_created;
DROP INDEX IF EXISTS idx_moderation_logs_event_created;
DROP INDEX IF EXISTS idx_moderation_rules_type_active;
DROP INDEX IF EXISTS idx_moderation_rules_active_priority;
DROP INDEX IF EXISTS idx_moderation_actions_user_created;
DROP INDEX IF EXISTS idx_verification_requests_to_user_state;
DROP INDEX IF EXISTS idx_cross_signing_trust_user_trusted;
DROP INDEX IF EXISTS idx_device_trust_status_user_level;
DROP INDEX IF EXISTS idx_deleted_events_index_room_ts;
DROP INDEX IF EXISTS idx_retention_cleanup_logs_room_started;
DROP INDEX IF EXISTS idx_retention_cleanup_queue_status_origin;
DROP INDEX IF EXISTS idx_room_children_child;
DROP INDEX IF EXISTS idx_room_children_parent_suggested;
DROP INDEX IF EXISTS idx_room_summary_update_queue_status_priority_created;
DROP INDEX IF EXISTS idx_room_summary_state_room;

DROP TABLE IF EXISTS application_service_statistics;
DROP TABLE IF EXISTS application_service_users;
DROP TABLE IF EXISTS secure_backup_session_keys;
DROP TABLE IF EXISTS secure_key_backups;
DROP TABLE IF EXISTS scheduled_notifications;
DROP TABLE IF EXISTS notification_delivery_log;
DROP TABLE IF EXISTS notification_templates;
DROP TABLE IF EXISTS user_notification_status;
DROP TABLE IF EXISTS server_notifications;
DROP TABLE IF EXISTS widget_sessions;
DROP TABLE IF EXISTS widget_permissions;
DROP TABLE IF EXISTS widgets;
DROP TABLE IF EXISTS worker_connections;
DROP TABLE IF EXISTS worker_task_assignments;
DROP TABLE IF EXISTS worker_load_stats;
DROP TABLE IF EXISTS replication_positions;
DROP TABLE IF EXISTS moderation_logs;
DROP TABLE IF EXISTS moderation_rules;
DROP TABLE IF EXISTS moderation_actions;
DROP TABLE IF EXISTS verification_qr;
DROP TABLE IF EXISTS verification_sas;
DROP TABLE IF EXISTS verification_requests;
DROP TABLE IF EXISTS cross_signing_trust;
DROP TABLE IF EXISTS device_trust_status;
DROP TABLE IF EXISTS deleted_events_index;
DROP TABLE IF EXISTS retention_stats;
DROP TABLE IF EXISTS retention_cleanup_logs;
DROP TABLE IF EXISTS retention_cleanup_queue;
DROP TABLE IF EXISTS room_children;
DROP TABLE IF EXISTS room_summary_update_queue;
DROP TABLE IF EXISTS room_summary_stats;
DROP TABLE IF EXISTS room_summary_state;

-- Part 5: 20260330000005_align_remaining_schema_exceptions
DROP INDEX IF EXISTS idx_registration_token_batches_enabled_created;
DROP INDEX IF EXISTS idx_registration_token_batches_created;
DROP INDEX IF EXISTS idx_reaction_aggregations_room_relates_origin;
DROP INDEX IF EXISTS idx_qr_login_transactions_user_created;
DROP INDEX IF EXISTS idx_qr_login_transactions_expires;
DROP INDEX IF EXISTS idx_user_notification_settings_updated;
DROP INDEX IF EXISTS idx_server_notices_sent;
DROP INDEX IF EXISTS idx_rate_limits_updated;
DROP INDEX IF EXISTS idx_push_device_user_enabled;

DROP TABLE IF EXISTS registration_token_batches;
DROP TABLE IF EXISTS reaction_aggregations;
DROP TABLE IF EXISTS qr_login_transactions;
DROP TABLE IF EXISTS server_notices;
DROP TABLE IF EXISTS user_notification_settings;
DROP TABLE IF EXISTS rate_limits;
DROP TABLE IF EXISTS push_device;

-- Part 4: 20260330000004_align_space_schema_and_add_space_events
DROP TABLE IF EXISTS space_events;
DROP TABLE IF EXISTS space_statistics;
DROP TABLE IF EXISTS space_summaries;
DROP TABLE IF EXISTS space_members;

DROP INDEX IF EXISTS idx_spaces_parent;
DROP INDEX IF EXISTS idx_space_summary_space;
DROP INDEX IF EXISTS idx_space_statistics_member_count;

-- Part 3: 20260330000003_align_retention_and_room_summary_schema
DROP TABLE IF EXISTS room_retention_policies;
DROP TABLE IF EXISTS room_summary_members;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'room_summaries' AND column_name = 'joined_member_count'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'room_summaries' AND column_name = 'joined_members'
    ) THEN
        ALTER TABLE room_summaries RENAME COLUMN joined_member_count TO joined_members;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'room_summaries' AND column_name = 'invited_member_count'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'room_summaries' AND column_name = 'invited_members'
    ) THEN
        ALTER TABLE room_summaries RENAME COLUMN invited_member_count TO invited_members;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'server_retention_policy' AND column_name = 'max_lifetime'
    ) THEN
        RAISE NOTICE 'server_retention_policy column additions are NOT REVERSIBLE automatically';
    END IF;
END $$;

-- Part 2: 20260330000002_align_thread_schema_and_relations
DROP TABLE IF EXISTS thread_relations;

DROP INDEX IF EXISTS idx_thread_roots_room_thread_unique;
DROP INDEX IF EXISTS idx_thread_roots_room_last_reply_created;
DROP INDEX IF EXISTS idx_thread_replies_room_thread_event;

-- Part 1: 20260330000001_add_thread_replies_and_receipts
DROP TABLE IF EXISTS thread_read_receipts;
DROP TABLE IF EXISTS thread_replies;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'thread_roots' AND column_name = 'root_event_id'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'thread_roots' AND column_name = 'event_id'
    ) THEN
        ALTER TABLE thread_roots RENAME COLUMN root_event_id TO event_id;
    END IF;

    IF EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'uq_thread_roots_room_root_event'
    ) AND NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'uq_thread_roots_room_event'
    ) THEN
        ALTER TABLE thread_roots
        RENAME CONSTRAINT uq_thread_roots_room_root_event TO uq_thread_roots_room_event;
    END IF;

    IF EXISTS (
        SELECT 1 FROM pg_indexes
        WHERE schemaname = 'public' AND indexname = 'idx_thread_roots_root_event'
    ) AND NOT EXISTS (
        SELECT 1 FROM pg_indexes
        WHERE schemaname = 'public' AND indexname = 'idx_thread_roots_event'
    ) THEN
        ALTER INDEX idx_thread_roots_root_event RENAME TO idx_thread_roots_event;
    END IF;
END $$;

-- ===== From: 20260403000001_add_openclaw_integration.undo.sql =====
-- Rollback: OpenClaw Integration Tables
-- Version: 1.0.0
-- Date: 2026-04-03

-- 删除触发器
DROP TRIGGER IF EXISTS update_openclaw_connections_updated_ts ON openclaw_connections;
DROP TRIGGER IF EXISTS update_ai_conversations_updated_ts ON ai_conversations;
DROP TRIGGER IF EXISTS update_ai_chat_roles_updated_ts ON ai_chat_roles;

-- 删除函数
DROP FUNCTION IF EXISTS update_updated_ts_column();

-- 删除索引
DROP INDEX IF EXISTS idx_openclaw_connections_user;
DROP INDEX IF EXISTS idx_openclaw_connections_provider;
DROP INDEX IF EXISTS idx_openclaw_connections_active;
DROP INDEX IF EXISTS idx_ai_conversations_user;
DROP INDEX IF EXISTS idx_ai_conversations_connection;
DROP INDEX IF EXISTS idx_ai_conversations_pinned;
DROP INDEX IF EXISTS idx_ai_conversations_updated;
DROP INDEX IF EXISTS idx_ai_messages_conversation;
DROP INDEX IF EXISTS idx_ai_messages_created;
DROP INDEX IF EXISTS idx_ai_messages_role;
DROP INDEX IF EXISTS idx_ai_generations_user;
DROP INDEX IF EXISTS idx_ai_generations_conversation;
DROP INDEX IF EXISTS idx_ai_generations_type;
DROP INDEX IF EXISTS idx_ai_generations_status;
DROP INDEX IF EXISTS idx_ai_chat_roles_user;
DROP INDEX IF EXISTS idx_ai_chat_roles_public;
DROP INDEX IF EXISTS idx_ai_chat_roles_category;

-- 删除表（按依赖顺序）
DROP TABLE IF EXISTS ai_chat_roles;
DROP TABLE IF EXISTS ai_generations;
DROP TABLE IF EXISTS ai_messages;
DROP TABLE IF EXISTS ai_conversations;
DROP TABLE IF EXISTS openclaw_connections;

-- ===== From: 20260331000100_add_event_relations_table.undo.sql =====
-- V260331_001__MIG-RELATIONS__add_event_relations_table.undo.sql
--
-- 回滚: 删除 event_relations 表
-- 对应迁移: V260331_001__MIG-RELATIONS__add_event_relations_table.sql

SET TIME ZONE 'UTC';

DO $$
BEGIN
    RAISE NOTICE '开始回滚 event_relations 表...';
END $$;

-- 删除索引
DROP INDEX IF EXISTS idx_event_relations_unique;
DROP INDEX IF EXISTS idx_event_relations_room_event;
DROP INDEX IF EXISTS idx_event_relations_sender;
DROP INDEX IF EXISTS idx_event_relations_origin_ts;

-- 删除表
DROP TABLE IF EXISTS event_relations;

DO $$
BEGIN
    RAISE NOTICE 'event_relations 表回滚完成';
END $$;

-- ===== From: 20260330000012_add_federation_signing_keys.undo.sql =====
DROP TABLE IF EXISTS federation_signing_keys;

-- ===== From: 20260329000100_add_missing_schema_tables.undo.sql =====
-- V260330_001__MIG-XXX__add_missing_schema_tables.undo.sql
--
-- 描述: 回滚 V260330_001__MIG-XXX__add_missing_schema_tables.sql
-- 删除所有新增的表
--
-- 注意: 此回滚会删除数据和表结构，不可逆

SET TIME ZONE 'UTC';

DO $$
BEGIN
    RAISE NOTICE '开始回滚缺失 schema 表...';
END $$;

-- ============================================================================
-- 回滚所有创建的表 (按依赖关系逆序)
-- ============================================================================

-- 删除 leak_alerts
DROP TABLE IF EXISTS leak_alerts CASCADE;

-- 删除 federation_blacklist_rule
DROP TABLE IF EXISTS federation_blacklist_rule CASCADE;

-- 删除 federation_blacklist_log
DROP TABLE IF EXISTS federation_blacklist_log CASCADE;

-- 删除 federation_blacklist_config
DROP TABLE IF EXISTS federation_blacklist_config CASCADE;

-- 删除 federation_access_stats
DROP TABLE IF EXISTS federation_access_stats CASCADE;

-- 删除 email_verification_tokens
DROP TABLE IF EXISTS email_verification_tokens CASCADE;

-- 删除 e2ee_stored_secrets
DROP TABLE IF EXISTS e2ee_stored_secrets CASCADE;

-- 删除 e2ee_secret_storage_keys
DROP TABLE IF EXISTS e2ee_secret_storage_keys CASCADE;

-- 删除 e2ee_audit_log
DROP TABLE IF EXISTS e2ee_audit_log CASCADE;

-- 删除 delayed_events
DROP TABLE IF EXISTS delayed_events CASCADE;

-- 删除 dehydrated_devices
DROP TABLE IF EXISTS dehydrated_devices CASCADE;

-- ============================================================================
-- 完成
-- ============================================================================

DO $$
BEGIN
    RAISE NOTICE '缺失 schema 表回滚完成';
END $$;

-- ===== From: 20260329000000_create_migration_audit_table.undo.sql =====
-- +----------------------------------------------------------------------------+
-- | Rollback: V260329_000__SYS_0001__create_migration_audit_table
-- | Jira: SYS-0001
-- | Author: synapse-rust team
-- | Date: 2026-03-29
-- | Description: 回滚创建 migration_audit 表
-- +----------------------------------------------------------------------------+

BEGIN;

DROP TABLE IF EXISTS migration_audit;

COMMIT;

