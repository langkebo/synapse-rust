-- Migration: Add missing foreign key constraints
-- Date: 2026-03-14
-- Description: Add foreign key constraints to tables that are missing them

-- Add foreign key constraints for admin_api_tables
DO $$
BEGIN
    -- shadow_bans -> users
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_shadow_bans_user' AND table_name = 'shadow_bans'
    ) THEN
        ALTER TABLE shadow_bans ADD CONSTRAINT fk_shadow_bans_user 
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;

    -- rate_limits -> users
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_rate_limits_user' AND table_name = 'rate_limits'
    ) THEN
        ALTER TABLE rate_limits ADD CONSTRAINT fk_rate_limits_user 
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;

    -- server_notices -> users
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_server_notices_user' AND table_name = 'server_notices'
    ) THEN
        ALTER TABLE server_notices ADD CONSTRAINT fk_server_notices_user 
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;

    -- user_notification_settings -> users
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_user_notification_settings_user' AND table_name = 'user_notification_settings'
    ) THEN
        ALTER TABLE user_notification_settings ADD CONSTRAINT fk_user_notification_settings_user 
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;

    -- blocked_rooms -> rooms
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_blocked_rooms_room' AND table_name = 'blocked_rooms'
    ) THEN
        ALTER TABLE blocked_rooms ADD CONSTRAINT fk_blocked_rooms_room 
            FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    -- room_retention_policy -> rooms
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_room_retention_policy_room' AND table_name = 'room_retention_policy'
    ) THEN
        ALTER TABLE room_retention_policy ADD CONSTRAINT fk_room_retention_policy_room 
            FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    -- user_media_quota -> users
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_user_media_quota_user' AND table_name = 'user_media_quota'
    ) THEN
        ALTER TABLE user_media_quota ADD CONSTRAINT fk_user_media_quota_user 
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;
END $$;

-- Add foreign key constraints for feature_tables
DO $$
BEGIN
    -- call_sessions -> users
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_call_sessions_user' AND table_name = 'call_sessions'
    ) THEN
        ALTER TABLE call_sessions ADD CONSTRAINT fk_call_sessions_user 
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;

    -- call_sessions -> rooms
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_call_sessions_room' AND table_name = 'call_sessions'
    ) THEN
        ALTER TABLE call_sessions ADD CONSTRAINT fk_call_sessions_room 
            FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    -- call_candidates -> call_sessions (via call_id)
    -- Note: call_id is not a foreign key to call_sessions.id, it's a logical reference

    -- beacon_info -> users
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_beacon_info_user' AND table_name = 'beacon_info'
    ) THEN
        ALTER TABLE beacon_info ADD CONSTRAINT fk_beacon_info_user 
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;

    -- beacon_info -> rooms
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_beacon_info_room' AND table_name = 'beacon_info'
    ) THEN
        ALTER TABLE beacon_info ADD CONSTRAINT fk_beacon_info_room 
            FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    -- dehydrated_devices -> users
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_dehydrated_devices_user' AND table_name = 'dehydrated_devices'
    ) THEN
        ALTER TABLE dehydrated_devices ADD CONSTRAINT fk_dehydrated_devices_user 
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;

    -- matrixrtc_sessions -> users
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_matrixrtc_sessions_creator' AND table_name = 'matrixrtc_sessions'
    ) THEN
        ALTER TABLE matrixrtc_sessions ADD CONSTRAINT fk_matrixrtc_sessions_creator 
            FOREIGN KEY (creator_user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;

    -- matrixrtc_sessions -> rooms
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_matrixrtc_sessions_room' AND table_name = 'matrixrtc_sessions'
    ) THEN
        ALTER TABLE matrixrtc_sessions ADD CONSTRAINT fk_matrixrtc_sessions_room 
            FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    -- email_verification_tokens -> users
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_email_verification_tokens_user' AND table_name = 'email_verification_tokens'
    ) THEN
        ALTER TABLE email_verification_tokens ADD CONSTRAINT fk_email_verification_tokens_user 
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;

    -- delayed_events -> rooms
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_delayed_events_room' AND table_name = 'delayed_events'
    ) THEN
        ALTER TABLE delayed_events ADD CONSTRAINT fk_delayed_events_room 
            FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    -- media_usage_log -> users
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_media_usage_log_user' AND table_name = 'media_usage_log'
    ) THEN
        ALTER TABLE media_usage_log ADD CONSTRAINT fk_media_usage_log_user 
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;

    -- media_quota_alerts -> users
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_media_quota_alerts_user' AND table_name = 'media_quota_alerts'
    ) THEN
        ALTER TABLE media_quota_alerts ADD CONSTRAINT fk_media_quota_alerts_user 
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE SET NULL;
    END IF;

    -- presence_subscriptions -> users (subscriber)
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_presence_subscriptions_user' AND table_name = 'presence_subscriptions'
    ) THEN
        ALTER TABLE presence_subscriptions ADD CONSTRAINT fk_presence_subscriptions_user 
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;

    -- presence_subscriptions -> users (observed)
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_presence_subscriptions_observed' AND table_name = 'presence_subscriptions'
    ) THEN
        ALTER TABLE presence_subscriptions ADD CONSTRAINT fk_presence_subscriptions_observed 
            FOREIGN KEY (observed_user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;
END $$;

-- Add foreign key constraints for unified_migration_optimized tables
DO $$
BEGIN
    -- qr_login_transactions -> users
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_qr_login_transactions_user' AND table_name = 'qr_login_transactions'
    ) THEN
        ALTER TABLE qr_login_transactions ADD CONSTRAINT fk_qr_login_transactions_user 
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;

    -- room_invite_blocklist -> rooms
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_room_invite_blocklist_room' AND table_name = 'room_invite_blocklist'
    ) THEN
        ALTER TABLE room_invite_blocklist ADD CONSTRAINT fk_room_invite_blocklist_room 
            FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    -- room_invite_blocklist -> users
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_room_invite_blocklist_user' AND table_name = 'room_invite_blocklist'
    ) THEN
        ALTER TABLE room_invite_blocklist ADD CONSTRAINT fk_room_invite_blocklist_user 
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;

    -- room_invite_allowlist -> rooms
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_room_invite_allowlist_room' AND table_name = 'room_invite_allowlist'
    ) THEN
        ALTER TABLE room_invite_allowlist ADD CONSTRAINT fk_room_invite_allowlist_room 
            FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    -- room_invite_allowlist -> users
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_room_invite_allowlist_user' AND table_name = 'room_invite_allowlist'
    ) THEN
        ALTER TABLE room_invite_allowlist ADD CONSTRAINT fk_room_invite_allowlist_user 
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;

    -- room_sticky_events -> rooms
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_room_sticky_events_room' AND table_name = 'room_sticky_events'
    ) THEN
        ALTER TABLE room_sticky_events ADD CONSTRAINT fk_room_sticky_events_room 
            FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    -- room_sticky_events -> users
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_room_sticky_events_user' AND table_name = 'room_sticky_events'
    ) THEN
        ALTER TABLE room_sticky_events ADD CONSTRAINT fk_room_sticky_events_user 
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;

    -- deleted_events_index -> rooms
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_deleted_events_index_room' AND table_name = 'deleted_events_index'
    ) THEN
        ALTER TABLE deleted_events_index ADD CONSTRAINT fk_deleted_events_index_room 
            FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    -- retention_cleanup_logs -> rooms
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_retention_cleanup_logs_room' AND table_name = 'retention_cleanup_logs'
    ) THEN
        ALTER TABLE retention_cleanup_logs ADD CONSTRAINT fk_retention_cleanup_logs_room 
            FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE SET NULL;
    END IF;

    -- retention_cleanup_queue -> rooms
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_retention_cleanup_queue_room' AND table_name = 'retention_cleanup_queue'
    ) THEN
        ALTER TABLE retention_cleanup_queue ADD CONSTRAINT fk_retention_cleanup_queue_room 
            FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;

    -- notification_delivery_log -> users
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_notification_delivery_log_user' AND table_name = 'notification_delivery_log'
    ) THEN
        ALTER TABLE notification_delivery_log ADD CONSTRAINT fk_notification_delivery_log_user 
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;

    -- scheduled_notifications -> users
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_scheduled_notifications_user' AND table_name = 'scheduled_notifications'
    ) THEN
        ALTER TABLE scheduled_notifications ADD CONSTRAINT fk_scheduled_notifications_user 
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;

    -- scheduled_notifications -> rooms
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_scheduled_notifications_room' AND table_name = 'scheduled_notifications'
    ) THEN
        ALTER TABLE scheduled_notifications ADD CONSTRAINT fk_scheduled_notifications_room 
            FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE SET NULL;
    END IF;

    -- user_notification_status -> users
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_user_notification_status_user' AND table_name = 'user_notification_status'
    ) THEN
        ALTER TABLE user_notification_status ADD CONSTRAINT fk_user_notification_status_user 
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;

    -- push_device -> users
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_push_device_user' AND table_name = 'push_device'
    ) THEN
        ALTER TABLE push_device ADD CONSTRAINT fk_push_device_user 
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;

    -- audit_log -> users
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_audit_log_user' AND table_name = 'audit_log'
    ) THEN
        ALTER TABLE audit_log ADD CONSTRAINT fk_audit_log_user 
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE SET NULL;
    END IF;
END $$;

DO $$
BEGIN
    RAISE NOTICE 'Foreign key constraints migration completed successfully';
END $$;
