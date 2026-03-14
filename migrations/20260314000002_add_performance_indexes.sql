-- ============================================================================
-- 添加性能优化索引
-- 创建日期: 2026-03-13
-- 
-- 根据优化计划 2.1.2 添加缺失的复合索引以提升查询性能
-- ============================================================================

-- events 表优化索引
-- 用于按房间和时间范围查询事件（覆盖索引）
CREATE INDEX IF NOT EXISTS idx_events_room_time_covering 
ON events(room_id, origin_server_ts DESC) 
INCLUDE (event_id, event_type, sender, content);

-- access_tokens 表优化索引
-- 用于按用户和有效性查询令牌
CREATE INDEX IF NOT EXISTS idx_access_tokens_user_valid 
ON access_tokens(user_id, is_revoked);

-- 用于快速验证令牌有效性（部分索引）
CREATE INDEX IF NOT EXISTS idx_access_tokens_token_valid 
ON access_tokens(token, is_revoked) 
WHERE is_revoked = FALSE;

-- room_memberships 表优化索引
-- 用于按房间和加入时间查询成员
CREATE INDEX IF NOT EXISTS idx_room_memberships_room_membership_time 
ON room_memberships(room_id, membership, joined_ts DESC);

-- refresh_tokens 表优化索引
-- 用于按用户和有效性查询刷新令牌
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_user_valid 
ON refresh_tokens(user_id, is_revoked);

-- users 表优化索引
-- 用于查询已停用用户
CREATE INDEX IF NOT EXISTS idx_users_deactivated 
ON users(is_deactivated) WHERE is_deactivated = TRUE;

-- 用于查询影子封禁用户
CREATE INDEX IF NOT EXISTS idx_users_shadow_banned 
ON users(is_shadow_banned) WHERE is_shadow_banned = TRUE;

-- events 表优化索引
-- 用于按发送者和时间查询事件
CREATE INDEX IF NOT EXISTS idx_events_sender_time 
ON events(sender, origin_server_ts DESC);

-- 用于按类型和时间查询事件
CREATE INDEX IF NOT EXISTS idx_events_type_time 
ON events(event_type, origin_server_ts DESC);

-- rooms 表优化索引
-- 用于查询公开房间
CREATE INDEX IF NOT EXISTS idx_rooms_public_active 
ON rooms(is_public, last_activity_ts DESC) WHERE is_public = TRUE;

-- devices 表优化索引
-- 用于按用户查询设备
CREATE INDEX IF NOT EXISTS idx_devices_user_seen 
ON devices(user_id, last_seen_ts DESC);

-- user_threepids 表优化索引
-- 用于查询未验证的第三方身份
CREATE INDEX IF NOT EXISTS idx_user_threepids_unverified 
ON user_threepids(user_id, is_verified) WHERE is_verified = FALSE;

-- presence 表优化索引
-- 用于按状态查询用户
CREATE INDEX IF NOT EXISTS idx_presence_status 
ON presence(presence, last_active_ts DESC);

-- 用于查询最近活跃的用户
CREATE INDEX IF NOT EXISTS idx_presence_active 
ON presence(last_active_ts DESC) WHERE presence = 'online';
