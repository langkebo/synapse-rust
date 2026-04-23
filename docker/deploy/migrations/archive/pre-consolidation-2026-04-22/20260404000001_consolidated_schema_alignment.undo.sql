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
