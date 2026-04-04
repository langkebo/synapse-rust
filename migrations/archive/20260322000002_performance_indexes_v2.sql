-- ============================================================================
-- synapse-rust 性能优化索引 v2
-- 创建日期: 2026-03-22
--
-- 说明: 基于实际数据库结构，仅添加不存在的索引
-- ============================================================================

SET TIME ZONE 'UTC';

-- ============================================================================
-- 1. events 表 - 已有索引检查
-- ============================================================================

-- 已有: idx_events_room_time, idx_events_sender_time, idx_events_type_room

-- 添加缺失的复合索引
CREATE INDEX IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_events_type_sender 
ON events(event_type, sender);

-- ============================================================================
-- 2. user_threepids 表索引优化
-- ============================================================================

-- 检查 user_threepids 表结构
-- 添加第三方 ID 验证状态查询索引
CREATE INDEX IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_threepids_medium_address 
ON user_threepids(medium, address);

-- ============================================================================
-- 3. access_tokens 表索引优化
-- ============================================================================

-- 优化: 令牌有效性检查
CREATE INDEX IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_tokens_user_revoked 
ON access_tokens(user_id, is_revoked) 
WHERE is_revoked = FALSE;

-- ============================================================================
-- 4. devices 表 - 已有索引检查
-- ============================================================================

-- 已有: idx_devices_user_last_seen

-- ============================================================================
-- 5. presence 表 - 已有索引检查  
-- ============================================================================

-- 已有: idx_presence_user_status

-- ============================================================================
-- 验证索引创建
-- ============================================================================

SELECT 
    indexrelname as index_name,
    idx_scan,
    idx_tup_read,
    idx_tup_fetch
FROM pg_stat_user_indexes
WHERE schemaname = 'public'
ORDER BY idx_scan DESC
LIMIT 20;
