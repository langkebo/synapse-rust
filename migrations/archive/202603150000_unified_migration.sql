-- ============================================================
-- synapse-rust 数据库统一迁移脚本
-- 
-- 版本: 202603150000
-- 日期: 2026-03-14
-- 描述: 合并所有字段规范化、外键约束和索引优化
-- 
-- 执行方式:
--   psql -U synapse -d synapse -f 202603150000_unified_migration.sql
-- ============================================================

\echo '=== 开始执行统一迁移 ==='

-- ============================================================
-- 第一部分: 字段规范化
-- ============================================================

\echo '--- 第一部分: 字段规范化 ---'

-- 检查并修复 refresh_tokens 表的字段 (如果存在)
DO $$
BEGIN
    -- 检查并添加缺失字段
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'refresh_tokens' AND column_name = 'device_id'
    ) THEN
        ALTER TABLE refresh_tokens ADD COLUMN device_id TEXT;
    END IF;
    
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'refresh_tokens' AND column_name = 'created_ts'
    ) THEN
        ALTER TABLE refresh_tokens ADD COLUMN created_ts BIGINT;
    END IF;
    
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'refresh_tokens' AND column_name = 'expires_at'
    ) THEN
        ALTER TABLE refresh_tokens ADD COLUMN expires_at BIGINT;
    END IF;
    
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'refresh_tokens' AND column_name = 'is_revoked'
    ) THEN
        ALTER TABLE refresh_tokens ADD COLUMN is_revoked BOOLEAN DEFAULT FALSE;
    END IF;
END $$;

-- ============================================================
-- 第二部分: 外键约束
-- ============================================================

\echo '--- 第二部分: 外键约束 ---'

-- devices -> users
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_devices_user' 
        AND table_name = 'devices'
    ) THEN
        ALTER TABLE devices 
        ADD CONSTRAINT fk_devices_user 
        FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;
END $$;

-- access_tokens -> users
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_access_tokens_user' 
        AND table_name = 'access_tokens'
    ) THEN
        ALTER TABLE access_tokens 
        ADD CONSTRAINT fk_access_tokens_user 
        FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;
END $$;

-- access_tokens -> devices
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_access_tokens_device' 
        AND table_name = 'access_tokens'
    ) THEN
        ALTER TABLE access_tokens 
        ADD CONSTRAINT fk_access_tokens_device 
        FOREIGN KEY (device_id) REFERENCES devices(device_id) ON DELETE SET NULL;
    END IF;
END $$;

-- refresh_tokens -> users
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_refresh_tokens_user' 
        AND table_name = 'refresh_tokens'
    ) THEN
        ALTER TABLE refresh_tokens 
        ADD CONSTRAINT fk_refresh_tokens_user 
        FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;
END $$;

-- room_memberships -> users
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_room_memberships_user' 
        AND table_name = 'room_memberships'
    ) THEN
        ALTER TABLE room_memberships 
        ADD CONSTRAINT fk_room_memberships_user 
        FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;
END $$;

-- room_memberships -> rooms
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_room_memberships_room' 
        AND table_name = 'room_memberships'
    ) THEN
        ALTER TABLE room_memberships 
        ADD CONSTRAINT fk_room_memberships_room 
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;
END $$;

-- events -> rooms
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_events_room' 
        AND table_name = 'events'
    ) THEN
        ALTER TABLE events 
        ADD CONSTRAINT fk_events_room 
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;
END $$;

-- events -> users (sender)
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_events_sender' 
        AND table_name = 'events'
    ) THEN
        ALTER TABLE events 
        ADD CONSTRAINT fk_events_sender 
        FOREIGN KEY (sender) REFERENCES users(user_id) ON DELETE SET NULL;
    END IF;
END $$;

-- room_state_events -> rooms
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_room_state_events_room' 
        AND table_name = 'room_state_events'
    ) THEN
        ALTER TABLE room_state_events 
        ADD CONSTRAINT fk_room_state_events_room 
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;
END $$;

-- room_aliases -> rooms
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_room_aliases_room' 
        AND table_name = 'room_aliases'
    ) THEN
        ALTER TABLE room_aliases 
        ADD CONSTRAINT fk_room_aliases_room 
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;
END $$;

-- room_tags -> rooms
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_room_tags_room' 
        AND table_name = 'room_tags'
    ) THEN
        ALTER TABLE room_tags 
        ADD CONSTRAINT fk_room_tags_room 
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;
END $$;

-- room_tags -> users
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_room_tags_user' 
        AND table_name = 'room_tags'
    ) THEN
        ALTER TABLE room_tags 
        ADD CONSTRAINT fk_room_tags_user 
        FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;
END $$;

-- notifications -> users
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_notifications_user' 
        AND table_name = 'notifications'
    ) THEN
        ALTER TABLE notifications 
        ADD CONSTRAINT fk_notifications_user 
        FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;
END $$;

-- notifications -> rooms
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_notifications_room' 
        AND table_name = 'notifications'
    ) THEN
        ALTER TABLE notifications 
        ADD CONSTRAINT fk_notifications_room 
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;
END $$;

-- device_keys -> users
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_device_keys_user' 
        AND table_name = 'device_keys'
    ) THEN
        ALTER TABLE device_keys 
        ADD CONSTRAINT fk_device_keys_user 
        FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;
END $$;

-- device_keys -> devices
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_device_keys_device' 
        AND table_name = 'device_keys'
    ) THEN
        ALTER TABLE device_keys 
        ADD CONSTRAINT fk_device_keys_device 
        FOREIGN KEY (device_id) REFERENCES devices(device_id) ON DELETE CASCADE;
    END IF;
END $$;

-- pushers -> users
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_pushers_user' 
        AND table_name = 'pushers'
    ) THEN
        ALTER TABLE pushers 
        ADD CONSTRAINT fk_pushers_user 
        FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;
END $$;

-- filters -> users
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_filters_user' 
        AND table_name = 'filters'
    ) THEN
        ALTER TABLE filters 
        ADD CONSTRAINT fk_filters_user 
        FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;
END $$;

-- user_threepids -> users
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_user_threepids_user' 
        AND table_name = 'user_threepids'
    ) THEN
        ALTER TABLE user_threepids 
        ADD CONSTRAINT fk_user_threepids_user 
        FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;
END $$;

-- account_data -> users
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_account_data_user' 
        AND table_name = 'account_data'
    ) THEN
        ALTER TABLE account_data 
        ADD CONSTRAINT fk_account_data_user 
        FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;
END $$;

-- room_account_data -> users
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_room_account_data_user' 
        AND table_name = 'room_account_data'
    ) THEN
        ALTER TABLE room_account_data 
        ADD CONSTRAINT fk_room_account_data_user 
        FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
    END IF;
END $$;

-- room_account_data -> rooms
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_room_account_data_room' 
        AND table_name = 'room_account_data'
    ) THEN
        ALTER TABLE room_account_data 
        ADD CONSTRAINT fk_room_account_data_room 
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;
END $$;

-- event_receipts -> rooms
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_event_receipts_room' 
        AND table_name = 'event_receipts'
    ) THEN
        ALTER TABLE event_receipts 
        ADD CONSTRAINT fk_event_receipts_room 
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;
END $$;

-- event_receipts -> events
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'fk_event_receipts_event' 
        AND table_name = 'event_receipts'
    ) THEN
        ALTER TABLE event_receipts 
        ADD CONSTRAINT fk_event_receipts_event 
        FOREIGN KEY (event_id) REFERENCES events(event_id) ON DELETE CASCADE;
    END IF;
END $$;

-- ============================================================
-- 第三部分: 索引优化
-- ============================================================

\echo '--- 第三部分: 索引优化 ---'

-- devices 表索引
CREATE INDEX IF NOT EXISTS idx_devices_user ON devices(user_id);
CREATE INDEX IF NOT EXISTS idx_devices_last_seen ON devices(last_seen_ts DESC);

-- access_tokens 表索引
CREATE INDEX IF NOT EXISTS idx_access_tokens_user ON access_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_access_tokens_user_valid ON access_tokens(user_id, is_valid);

-- refresh_tokens 表索引
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_user ON refresh_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_user_revoked ON refresh_tokens(user_id, is_revoked);

-- room_memberships 表索引
CREATE INDEX IF NOT EXISTS idx_room_memberships_user ON room_memberships(user_id);
CREATE INDEX IF NOT EXISTS idx_room_memberships_user_membership ON room_memberships(user_id, membership);
CREATE INDEX IF NOT EXISTS idx_room_memberships_room_membership ON room_memberships(room_id, membership);

-- events 表索引
CREATE INDEX IF NOT EXISTS idx_events_room_ts ON events(room_id, origin_server_ts DESC);
CREATE INDEX IF NOT EXISTS idx_events_sender ON events(sender, origin_server_ts DESC);

-- room_events 表索引
CREATE INDEX IF NOT EXISTS idx_room_events_room_ts ON room_events(room_id, origin_server_ts DESC);

-- room_state_events 表索引
CREATE INDEX IF NOT EXISTS idx_room_state_events_room ON room_state_events(room_id, type);

-- notifications 表索引
CREATE INDEX IF NOT EXISTS idx_notifications_user ON notifications(user_id);
CREATE INDEX IF NOT EXISTS idx_notifications_user_room ON notifications(user_id, room_id, stream_ordering DESC);

-- presence 表索引
CREATE INDEX IF NOT EXISTS idx_presence_user ON presence(user_id);

-- typing 表索引
CREATE INDEX IF NOT EXISTS idx_typing_room ON typing(room_id);

-- read_markers 表索引
CREATE INDEX IF NOT EXISTS idx_read_markers_user ON read_markers(user_id);

-- ============================================================
-- 第四部分: 数据清理
-- ============================================================

\echo '--- 第四部分: 数据清理 ---'

-- 清理孤立的设备记录
DELETE FROM devices 
WHERE user_id IS NOT NULL 
AND NOT EXISTS (SELECT 1 FROM users u WHERE u.user_id = devices.user_id);

-- 清理孤立的 room_memberships 记录
DELETE FROM room_memberships 
WHERE room_id IS NOT NULL 
AND NOT EXISTS (SELECT 1 FROM rooms r WHERE r.room_id = room_memberships.room_id);

DELETE FROM room_memberships 
WHERE user_id IS NOT NULL 
AND NOT EXISTS (SELECT 1 FROM users u WHERE u.user_id = room_memberships.user_id);

-- 清理孤立的 access_tokens 记录
DELETE FROM access_tokens 
WHERE user_id IS NOT NULL 
AND NOT EXISTS (SELECT 1 FROM users u WHERE u.user_id = access_tokens.user_id);

\echo '=== 统一迁移执行完成 ==='

-- 记录迁移
INSERT INTO schema_migrations (version, description, applied_ts)
VALUES (202603150000, 'Unified migration: fields, FK, indexes', extract(epoch from now())::bigint)
ON CONFLICT (version) DO UPDATE 
SET description = 'Unified migration: fields, FK, indexes',
    applied_ts = extract(epoch from now())::bigint;
