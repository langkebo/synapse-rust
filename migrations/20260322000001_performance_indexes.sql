-- ============================================================================
-- synapse-rust 性能优化索引
-- 创建日期: 2026-03-22
--
-- 说明: 提升查询性能，添加高频查询所需的复合索引
-- ============================================================================

SET TIME ZONE 'UTC';

-- ============================================================================
-- 1. events 表复合索引优化
-- ============================================================================

-- 优化: 范围查询 (room_id + origin_server_ts DESC)
-- 用于获取房间历史消息
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_events_room_time 
ON events(room_id, origin_server_ts DESC);

-- 优化: sender + origin_server_ts 查询
-- 用于获取用户发送的消息
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_events_sender_time 
ON events(sender, origin_server_ts DESC);

-- 优化: event_type + room_id 查询
-- 用于统计房间内特定类型事件
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_events_type_room 
ON events(event_type, room_id);

-- ============================================================================
-- 2. room_memberships 表索引优化
-- ============================================================================

-- 优化: 直接消息 (is_direct) 查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_memberships_user_direct 
ON room_memberships(user_id, is_direct) 
WHERE is_direct = TRUE;

-- 优化: 用户在房间中的角色查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_memberships_user_power 
ON room_memberships(user_id, room_id, power_level);

-- ============================================================================
-- 3. pushers 表索引优化
-- ============================================================================

-- 优化: 用户推送设备查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_pushers_user_kind 
ON pushers(user_id, kind);

-- 优化: 推送有效性查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_pushers_active 
ON pushers(user_id, device_id, is_active) 
WHERE is_active = TRUE;

-- ============================================================================
-- 4. device 表索引优化
-- ============================================================================

-- 优化: 用户设备列表查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_devices_user_last_seen 
ON devices(user_id, last_seen_ts DESC);

-- ============================================================================
-- 5. room_state 表索引优化
-- ============================================================================

-- 优化: 房间状态查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_room_state_room_type 
ON room_state(room_id, event_type, state_key);

-- ============================================================================
-- 6. user_threepids 表索引优化
-- ============================================================================

-- 优化: 第三方 ID 验证状态查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_threepids_validated 
ON user_threepids(medium, address) 
WHERE is_verified = TRUE;

-- ============================================================================
-- 7. access_tokens 表索引优化
-- ============================================================================

-- 优化: 令牌有效性检查
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_tokens_user_valid 
ON access_tokens(user_id, is_revoked) 
WHERE is_revoked = FALSE;

-- ============================================================================
-- 8. notifications 表索引优化
-- ============================================================================

-- 优化: 用户通知查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_notifications_user_room 
ON notifications(user_id, room_id, stream_ordering DESC);

-- 优化: 未读通知查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_notifications_unread 
ON notifications(user_id, is_read, stream_ordering DESC) 
WHERE is_read = FALSE;

-- ============================================================================
-- 9. presence 表索引优化
-- ============================================================================

-- 优化: 用户在线状态查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_presence_user_status 
ON presence(user_id, status);

-- ============================================================================
-- 10. read_receipts 表索引优化
-- ============================================================================

-- 优化: 房间阅读回执查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_receipts_room_user 
ON room_read_receipts(room_id, user_id, event_id);

-- ============================================================================
-- 验证索引创建
-- ============================================================================

SELECT 
    schemaname,
    tablename,
    indexname,
    idx_scan,
    idx_tup_read,
    idx_tup_fetch
FROM pg_stat_user_indexes
WHERE schemaname = 'public'
ORDER BY idx_scan DESC
LIMIT 20;

-- 检查索引大小
SELECT 
    indexname,
    pg_size_pretty(pg_relation_size(indexname::regclass)) as size
FROM pg_stat_user_indexes
WHERE schemaname = 'public'
ORDER BY pg_relation_size(indexname::regclass) DESC
LIMIT 10;
