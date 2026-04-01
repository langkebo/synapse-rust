-- ============================================================================
-- 回滚脚本: 20260329_p2_optimization
-- 回滚日期: 2026-03-30
-- ============================================================================

SET TIME ZONE 'UTC';

-- ============================================================================
-- 注意: 部分索引 (idx_room_memberships_room_membership_user,
-- idx_notifications_user_room_ts, idx_event_receipts_room_type)
-- 在 20260328_p1_indexes.sql 中也已创建，回滚时需保留
-- ============================================================================

-- 删除 P2 特有的索引
DROP INDEX IF EXISTS idx_user_threepids_medium_address;
DROP INDEX IF EXISTS idx_event_relations_thread;
DROP INDEX IF EXISTS idx_pusher_threepids_user;
DROP INDEX IF EXISTS idx_device_keys_user_key_type;

-- 注意: 以下索引在 20260328_p1_indexes.sql 中已存在，不回滚
-- DROP INDEX IF EXISTS idx_room_memberships_room_membership_user;
-- DROP INDEX IF EXISTS idx_notifications_user_room_ts;
-- DROP INDEX IF EXISTS idx_event_receipts_room_type;

-- ============================================================================
-- 记录回滚
-- ============================================================================

DELETE FROM schema_migrations WHERE version = '20260329_p2_optimization';
