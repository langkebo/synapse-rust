-- ============================================================================
-- 回滚脚本: 20260328_p1_indexes
-- 回滚日期: 2026-03-30
-- ============================================================================

SET TIME ZONE 'UTC';

-- ============================================================================
-- 回滚索引
-- ============================================================================

DROP INDEX IF EXISTS idx_search_index_created_ts;
DROP INDEX IF EXISTS idx_search_index_room_created;
DROP INDEX IF EXISTS idx_sliding_sync_rooms_room;
DROP INDEX IF EXISTS idx_sliding_sync_rooms_room_bump;
DROP INDEX IF EXISTS idx_room_memberships_room_membership_user;
DROP INDEX IF EXISTS idx_events_room_time_covering;
DROP INDEX IF EXISTS idx_notifications_user_room_ts;
DROP INDEX IF EXISTS idx_event_receipts_room_type;

-- ============================================================================
-- 记录回滚
-- ============================================================================

DELETE FROM schema_migrations WHERE version = '20260328_p1_indexes';
