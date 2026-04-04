-- ============================================================================
-- synapse-rust 性能优化索引
-- 创建日期: 2026-03-22
-- 更新日期: 2026-03-24
--
-- 说明: 提升查询性能，添加高频查询所需的复合索引
-- 幂等性: 完全幂等，可重复执行
-- ============================================================================

SET TIME ZONE 'UTC';

-- ============================================================================
-- 辅助函数
-- ============================================================================
CREATE OR REPLACE FUNCTION index_exists_if_not_canceled(index_name TEXT)
RETURNS BOOLEAN AS $$
BEGIN
    RETURN EXISTS (
        SELECT 1 FROM pg_indexes
        WHERE schemaname = 'public' AND indexname = index_exists_if_not_canceled.index_name
    );
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION column_exists_check(table_name TEXT, column_name TEXT)
RETURNS BOOLEAN AS $$
BEGIN
    RETURN EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public'
        AND table_name = column_exists_check.table_name
        AND column_name = column_exists_check.column_name
    );
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- 1. events 表复合索引优化
-- ============================================================================

-- 优化: 范围查询 (room_id + origin_server_ts DESC)
-- 用于获取房间历史消息
CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_events_room_time
ON events(room_id, origin_server_ts DESC);

-- 优化: sender + origin_server_ts 查询
-- 用于获取用户发送的消息
CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_events_sender_time
ON events(sender, origin_server_ts DESC);

-- 优化: event_type + room_id 查询
-- 用于统计房间内特定类型事件
CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_events_type_room
ON events(event_type, room_id);

-- 优化: event_type + sender 查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_events_type_sender
ON events(event_type, sender);

-- ============================================================================
-- 2. room_memberships 表索引优化
-- 注意: room_memberships 表没有 is_direct 和 power_level 列
-- ============================================================================

-- 优化: 用户在房间中的成员关系查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_memberships_user_room
ON room_memberships(user_id, room_id);

-- 优化: 房间成员查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_memberships_room_membership
ON room_memberships(room_id, membership);

-- ============================================================================
-- 3. pushers 表索引优化
-- ============================================================================

-- 优化: 用户推送设备查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_pushers_user
ON pushers(user_id);

-- 优化: 推送有效性查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_pushers_enabled
ON pushers(is_enabled)
WHERE is_enabled = TRUE;

-- ============================================================================
-- 4. devices 表索引优化
-- ============================================================================

-- 优化: 用户设备列表查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_devices_user_id
ON devices(user_id);

-- 优化: 设备最后访问时间查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_devices_last_seen
ON devices(last_seen_ts DESC);

-- ============================================================================
-- 5. user_threepids 表索引优化
-- ============================================================================

-- 优化: 用户第三方 ID 查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_user_threepids_user
ON user_threepids(user_id);

-- 优化: 第三方 ID 验证状态查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_threepids_medium_address
ON user_threepids(medium, address);

-- ============================================================================
-- 6. access_tokens 表索引优化
-- ============================================================================

-- 优化: 用户令牌查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_access_tokens_user_id
ON access_tokens(user_id);

-- 优化: 令牌有效性检查
CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_access_tokens_valid
ON access_tokens(is_revoked)
WHERE is_revoked = FALSE;

-- ============================================================================
-- 7. notifications 表索引优化
-- ============================================================================

-- 优化: 用户通知查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_notifications_user_id
ON notifications(user_id);

-- 优化: 房间通知查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_notifications_room
ON notifications(room_id);

-- 优化: 通知时间戳查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_notifications_ts
ON notifications(ts DESC);

-- ============================================================================
-- 8. presence 表索引优化
-- ============================================================================

-- 优化: 用户在线状态查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_presence_user_status
ON presence(user_id, presence);

-- ============================================================================
-- 9. event_receipts 表索引优化
-- ============================================================================

-- 优化: 事件回执查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_event_receipts_event
ON event_receipts(event_id);

-- 优化: 房间回执查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_event_receipts_room
ON event_receipts(room_id);

-- ============================================================================
-- 10. read_markers 表索引优化
-- ============================================================================

-- 优化: 阅读标记查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_read_markers_room_user
ON read_markers(room_id, user_id);

-- ============================================================================
-- 11. room_state_events 表索引优化
-- ============================================================================

-- 优化: 房间状态查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_room_state_events_room
ON room_state_events(room_id);

-- ============================================================================
-- 12. room_events 表索引优化
-- ============================================================================

-- 优化: 房间事件查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_room_events_room
ON room_events(room_id);

-- 优化: 事件查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_room_events_event
ON room_events(event_id);

-- ============================================================================
-- 13. sliding_sync_rooms 表索引优化
-- ============================================================================

-- 优化: Sliding Sync 房间查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_sliding_sync_rooms_user_device
ON sliding_sync_rooms(user_id, device_id);

-- 优化: Sliding Sync 房间唯一性
CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_sliding_sync_rooms_unique
ON sliding_sync_rooms(user_id, room_id);

-- ============================================================================
-- 14. key_backups 表索引优化
-- ============================================================================

-- 优化: 密钥备份查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_key_backups_user
ON key_backups(user_id);

-- ============================================================================
-- 15. backup_keys 表索引优化
-- ============================================================================

-- 优化: 备份密钥查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_backup_keys_backup
ON backup_keys(backup_id);

-- 优化: 房间密钥备份查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_backup_keys_room
ON backup_keys(room_id);

-- ============================================================================
-- 16. e2ee_key_requests 表索引优化
-- ============================================================================

-- 优化: E2EE 密钥请求查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_e2ee_key_requests_user
ON e2ee_key_requests(user_id);

-- 优化: 待处理密钥请求查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_e2ee_key_requests_pending
ON e2ee_key_requests(is_fulfilled)
WHERE is_fulfilled = FALSE;

-- ============================================================================
-- 17. olm_sessions 表索引优化
-- ============================================================================

-- 优化: Olm 会话查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_olm_sessions_user_device
ON olm_sessions(user_id, device_id);

-- 优化: Olm 会话过期查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_olm_sessions_expires
ON olm_sessions(expires_at)
WHERE expires_at IS NOT NULL;

-- ============================================================================
-- 18. megolm_sessions 表索引优化
-- ============================================================================

-- 优化: Megolm 会话查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_megolm_sessions_room
ON megolm_sessions(room_id);

-- 优化: Megolm 会话唯一性查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_megolm_sessions_session
ON megolm_sessions(session_id);

-- ============================================================================
-- 验证索引创建
-- ============================================================================

SELECT
    schemaname,
    indexrelname as indexname,
    relname as tablename,
    idx_scan,
    idx_tup_read,
    idx_tup_fetch
FROM pg_stat_user_indexes
WHERE schemaname = 'public'
ORDER BY idx_scan DESC
LIMIT 20;

-- 检查索引大小
SELECT
    indexrelname as indexname,
    pg_size_pretty(pg_relation_size(indexrelid)) as size
FROM pg_stat_user_indexes
WHERE schemaname = 'public'
ORDER BY pg_relation_size(indexrelid) DESC
LIMIT 10;
