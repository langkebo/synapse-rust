-- ============================================================================
-- Migration: Add missing foreign key constraints
-- Version: 20260308000002
-- Description: 添加缺失的外键约束，确保数据完整性
-- ============================================================================

-- ============================================================================
-- 用户相关表外键约束
-- ============================================================================

-- user_threepids -> users
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_user_threepids_user_id' 
        AND table_name = 'user_threepids'
    ) THEN
        ALTER TABLE user_threepids
        ADD CONSTRAINT fk_user_threepids_user_id
        FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;
END $$;

-- devices -> users
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_devices_user_id' 
        AND table_name = 'devices'
    ) THEN
        ALTER TABLE devices
        ADD CONSTRAINT fk_devices_user_id
        FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;
END $$;

-- access_tokens -> users
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_access_tokens_user_id' 
        AND table_name = 'access_tokens'
    ) THEN
        ALTER TABLE access_tokens
        ADD CONSTRAINT fk_access_tokens_user_id
        FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;
END $$;

-- access_tokens -> devices
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_access_tokens_device_id' 
        AND table_name = 'access_tokens'
    ) THEN
        ALTER TABLE access_tokens
        ADD CONSTRAINT fk_access_tokens_device_id
        FOREIGN KEY (device_id) REFERENCES devices(device_id) ON DELETE SET NULL;
    END IF;
END $$;

-- refresh_tokens -> users
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_refresh_tokens_user_id' 
        AND table_name = 'refresh_tokens'
    ) THEN
        ALTER TABLE refresh_tokens
        ADD CONSTRAINT fk_refresh_tokens_user_id
        FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;
END $$;

-- ============================================================================
-- 房间相关表外键约束
-- ============================================================================

-- room_memberships -> rooms
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_room_memberships_room_id' 
        AND table_name = 'room_memberships'
    ) THEN
        ALTER TABLE room_memberships
        ADD CONSTRAINT fk_room_memberships_room_id
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;
END $$;

-- room_memberships -> users
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_room_memberships_user_id' 
        AND table_name = 'room_memberships'
    ) THEN
        ALTER TABLE room_memberships
        ADD CONSTRAINT fk_room_memberships_user_id
        FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;
END $$;

-- events -> rooms
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_events_room_id' 
        AND table_name = 'events'
    ) THEN
        ALTER TABLE events
        ADD CONSTRAINT fk_events_room_id
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;
END $$;

-- room_summaries -> rooms
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_room_summaries_room_id' 
        AND table_name = 'room_summaries'
    ) THEN
        ALTER TABLE room_summaries
        ADD CONSTRAINT fk_room_summaries_room_id
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;
END $$;

-- room_directory -> rooms
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_room_directory_room_id' 
        AND table_name = 'room_directory'
    ) THEN
        ALTER TABLE room_directory
        ADD CONSTRAINT fk_room_directory_room_id
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;
END $$;

-- room_aliases -> rooms
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_room_aliases_room_id' 
        AND table_name = 'room_aliases'
    ) THEN
        ALTER TABLE room_aliases
        ADD CONSTRAINT fk_room_aliases_room_id
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;
END $$;

-- thread_statistics -> rooms
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_thread_statistics_room_id' 
        AND table_name = 'thread_statistics'
    ) THEN
        ALTER TABLE thread_statistics
        ADD CONSTRAINT fk_thread_statistics_room_id
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;
END $$;

-- ============================================================================
-- 加密相关表外键约束
-- ============================================================================

-- device_keys -> devices
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_device_keys_device_id' 
        AND table_name = 'device_keys'
    ) THEN
        ALTER TABLE device_keys
        ADD CONSTRAINT fk_device_keys_device_id
        FOREIGN KEY (device_id) REFERENCES devices(device_id) ON DELETE CASCADE;
    END IF;
END $$;

-- cross_signing_keys -> users
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_cross_signing_keys_user_id' 
        AND table_name = 'cross_signing_keys'
    ) THEN
        ALTER TABLE cross_signing_keys
        ADD CONSTRAINT fk_cross_signing_keys_user_id
        FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;
END $$;

-- megolm_sessions -> rooms
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_megolm_sessions_room_id' 
        AND table_name = 'megolm_sessions'
    ) THEN
        ALTER TABLE megolm_sessions
        ADD CONSTRAINT fk_megolm_sessions_room_id
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;
END $$;

-- event_signatures -> events
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_event_signatures_event_id' 
        AND table_name = 'event_signatures'
    ) THEN
        ALTER TABLE event_signatures
        ADD CONSTRAINT fk_event_signatures_event_id
        FOREIGN KEY (event_id) REFERENCES events(event_id) ON DELETE CASCADE;
    END IF;
END $$;

-- device_signatures -> devices
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_device_signatures_device_id' 
        AND table_name = 'device_signatures'
    ) THEN
        ALTER TABLE device_signatures
        ADD CONSTRAINT fk_device_signatures_device_id
        FOREIGN KEY (device_id) REFERENCES devices(device_id) ON DELETE CASCADE;
    END IF;
END $$;

-- key_backups -> users
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_key_backups_user_id' 
        AND table_name = 'key_backups'
    ) THEN
        ALTER TABLE key_backups
        ADD CONSTRAINT fk_key_backups_user_id
        FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;
END $$;

-- backup_keys -> key_backups
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_backup_keys_backup_id' 
        AND table_name = 'backup_keys'
    ) THEN
        ALTER TABLE backup_keys
        ADD CONSTRAINT fk_backup_keys_backup_id
        FOREIGN KEY (backup_id) REFERENCES key_backups(id) ON DELETE CASCADE;
    END IF;
END $$;

-- ============================================================================
-- 新功能表外键约束
-- ============================================================================

-- beacon_info -> rooms
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'beacon_info') THEN
        IF NOT EXISTS (
            SELECT 1 FROM information_schema.table_constraints 
            WHERE constraint_name = 'fk_beacon_info_room_id' 
            AND table_name = 'beacon_info'
        ) THEN
            ALTER TABLE beacon_info
            ADD CONSTRAINT fk_beacon_info_room_id
            FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
        END IF;
    END IF;
END $$;

-- beacon_info -> users (sender)
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'beacon_info') THEN
        IF NOT EXISTS (
            SELECT 1 FROM information_schema.table_constraints 
            WHERE constraint_name = 'fk_beacon_info_sender' 
            AND table_name = 'beacon_info'
        ) THEN
            ALTER TABLE beacon_info
            ADD CONSTRAINT fk_beacon_info_sender
            FOREIGN KEY (sender) REFERENCES users(user_id) ON DELETE CASCADE;
        END IF;
    END IF;
END $$;

-- beacon_locations -> beacon_info
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'beacon_locations') THEN
        IF NOT EXISTS (
            SELECT 1 FROM information_schema.table_constraints 
            WHERE constraint_name = 'fk_beacon_locations_beacon_info_id' 
            AND table_name = 'beacon_locations'
        ) THEN
            ALTER TABLE beacon_locations
            ADD CONSTRAINT fk_beacon_locations_beacon_info_id
            FOREIGN KEY (beacon_info_id) REFERENCES beacon_info(event_id) ON DELETE CASCADE;
        END IF;
    END IF;
END $$;

-- matrixrtc_sessions -> rooms
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'matrixrtc_sessions') THEN
        IF NOT EXISTS (
            SELECT 1 FROM information_schema.table_constraints 
            WHERE constraint_name = 'fk_matrixrtc_sessions_room_id' 
            AND table_name = 'matrixrtc_sessions'
        ) THEN
            ALTER TABLE matrixrtc_sessions
            ADD CONSTRAINT fk_matrixrtc_sessions_room_id
            FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
        END IF;
    END IF;
END $$;

-- dehydrated_devices -> users
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'dehydrated_devices') THEN
        IF NOT EXISTS (
            SELECT 1 FROM information_schema.table_constraints 
            WHERE constraint_name = 'fk_dehydrated_devices_user_id' 
            AND table_name = 'dehydrated_devices'
        ) THEN
            ALTER TABLE dehydrated_devices
            ADD CONSTRAINT fk_dehydrated_devices_user_id
            FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
        END IF;
    END IF;
END $$;

-- moderation_rules -> users (created_by)
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'moderation_rules') THEN
        IF NOT EXISTS (
            SELECT 1 FROM information_schema.table_constraints 
            WHERE constraint_name = 'fk_moderation_rules_created_by' 
            AND table_name = 'moderation_rules'
        ) THEN
            ALTER TABLE moderation_rules
            ADD CONSTRAINT fk_moderation_rules_created_by
            FOREIGN KEY (created_by) REFERENCES users(user_id) ON DELETE CASCADE;
        END IF;
    END IF;
END $$;

-- ============================================================================
-- 推送通知相关表外键约束
-- ============================================================================

-- push_devices -> users
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_push_devices_user_id' 
        AND table_name = 'push_devices'
    ) THEN
        ALTER TABLE push_devices
        ADD CONSTRAINT fk_push_devices_user_id
        FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;
END $$;

-- push_rules -> users
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_push_rules_user_id' 
        AND table_name = 'push_rules'
    ) THEN
        ALTER TABLE push_rules
        ADD CONSTRAINT fk_push_rules_user_id
        FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;
END $$;

-- ============================================================================
-- 账户数据相关表外键约束
-- ============================================================================

-- filters -> users
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_filters_user_id' 
        AND table_name = 'filters'
    ) THEN
        ALTER TABLE filters
        ADD CONSTRAINT fk_filters_user_id
        FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;
END $$;

-- openid_tokens -> users
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_openid_tokens_user_id' 
        AND table_name = 'openid_tokens'
    ) THEN
        ALTER TABLE openid_tokens
        ADD CONSTRAINT fk_openid_tokens_user_id
        FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;
END $$;

-- account_data -> users
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_account_data_user_id' 
        AND table_name = 'account_data'
    ) THEN
        ALTER TABLE account_data
        ADD CONSTRAINT fk_account_data_user_id
        FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;
END $$;

-- ============================================================================
-- 记录迁移完成
-- ============================================================================

INSERT INTO schema_migrations (version, name, applied_ts, description)
VALUES ('20260308000002', 'add_missing_foreign_key_constraints', 
        EXTRACT(EPOCH FROM NOW()) * 1000, 
        '添加缺失的外键约束，确保数据完整性')
ON CONFLICT (version) DO NOTHING;
