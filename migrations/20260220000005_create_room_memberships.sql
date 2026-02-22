-- Migration: Create room_memberships table
-- Version: 20260220000005
-- Description: 创建 room_memberships 表以支持房间成员管理

-- 创建 room_memberships 表
CREATE TABLE IF NOT EXISTS room_memberships (
    room_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    sender VARCHAR(255),
    membership VARCHAR(50) NOT NULL DEFAULT 'join',
    event_id VARCHAR(255),
    event_type VARCHAR(50) DEFAULT 'm.room.member',
    display_name VARCHAR(255),
    avatar_url VARCHAR(512),
    is_banned BOOLEAN DEFAULT FALSE,
    invite_token VARCHAR(255),
    updated_ts BIGINT,
    joined_ts BIGINT,
    left_ts BIGINT,
    reason TEXT,
    banned_by VARCHAR(255),
    ban_reason TEXT,
    ban_ts BIGINT,
    join_reason TEXT,
    PRIMARY KEY (room_id, user_id)
);

-- 创建索引
CREATE INDEX IF NOT EXISTS idx_room_memberships_room ON room_memberships(room_id);
CREATE INDEX IF NOT EXISTS idx_room_memberships_user ON room_memberships(user_id);
CREATE INDEX IF NOT EXISTS idx_room_memberships_membership ON room_memberships(membership);

-- 验证
DO $$
BEGIN
    RAISE NOTICE 'room_memberships table created successfully';
END $$;
