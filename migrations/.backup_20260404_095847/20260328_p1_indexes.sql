-- ============================================================================
-- P1 性能索引优化
-- 创建日期: 2026-03-28
--
-- 说明: 基于 P1 性能分析添加缺失的索引
-- 幂等性: 使用 CREATE INDEX CONCURRENTLY IF NOT EXISTS，可重复执行
-- ============================================================================

SET TIME ZONE 'UTC';

-- ============================================================================
-- 1. search_index 表索引优化
-- ============================================================================

-- 问题: 搜索结果按时间排序时缺少索引
-- 优化: 支持搜索结果按创建时间排序
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_search_index_created_ts
ON search_index(created_ts DESC);

-- 复合索引: room_id + created_ts 用于房间内搜索结果排序
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_search_index_room_created
ON search_index(room_id, created_ts DESC);

-- ============================================================================
-- 2. sliding_sync_rooms 表索引优化
-- ============================================================================

-- 问题: 按 room_id 单独查询时效率低
-- 优化: 支持按 room_id 高效查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_sliding_sync_rooms_room
ON sliding_sync_rooms(room_id);

-- 复合索引: room_id + bump_stamp 用于房间内按活跃度排序
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_sliding_sync_rooms_room_bump
ON sliding_sync_rooms(room_id, bump_stamp DESC);

-- ============================================================================
-- 3. room_memberships 表 - 补充索引
-- ============================================================================

-- 复合索引: room_id + membership + user_id 用于联合查询优化
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_room_memberships_room_membership_user
ON room_memberships(room_id, membership, user_id);

-- ============================================================================
-- 4. events 表 - 补充索引
-- ============================================================================

-- 覆盖索引: 减少回表查询，提升事件详情查询性能
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_events_room_time_covering
ON events(room_id, origin_server_ts DESC)
INCLUDE (event_id, event_type, sender, state_key);

-- ============================================================================
-- 5. notifications 表 - 补充索引
-- ============================================================================

-- 复合索引: 用户 + 房间 + 时间 用于通知列表查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_notifications_user_room_ts
ON notifications(user_id, room_id, ts DESC);

-- ============================================================================
-- 6. event_receipts 表 - 补充索引
-- ============================================================================

-- 复合索引: 房间 + 类型 + 时间 用于回执列表查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_event_receipts_room_type
ON event_receipts(room_id, receipt_type, ts DESC);

-- ============================================================================
-- 验证索引创建
-- ============================================================================

SELECT
    indexrelname as index_name,
    relname as table_name,
    idx_scan,
    idx_tup_read,
    idx_tup_fetch,
    pg_size_pretty(pg_relation_size(indexrelid)) as index_size
FROM pg_stat_user_indexes
WHERE schemaname = 'public'
AND indexrelname LIKE 'idx_%'
ORDER BY idx_scan DESC
LIMIT 30;

-- ============================================================================
-- 记录迁移
-- ============================================================================

INSERT INTO schema_migrations (version, description, applied_ts)
VALUES ('20260328_p1_indexes', 'P1 performance indexes: search_index, sliding_sync_rooms, room_memberships, events, notifications, event_receipts', EXTRACT(EPOCH FROM NOW())::BIGINT * 1000)
ON CONFLICT (version) DO NOTHING;
