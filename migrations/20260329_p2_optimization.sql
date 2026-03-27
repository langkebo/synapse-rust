-- ============================================================================
-- P2 性能索引优化
-- 创建日期: 2026-03-29
--
-- 说明: 基于 P2 性能分析添加补充索引
-- 幂等性: 使用 CREATE INDEX IF NOT EXISTS，可重复执行
-- ============================================================================

SET TIME ZONE 'UTC';

-- ============================================================================
-- 1. room_memberships 表 - 补充复合索引
-- ============================================================================

-- 问题: 房间成员列表查询常包含 membership 类型过滤
-- 优化: 支持 (room_id, membership, user_id) 联合查询
CREATE INDEX IF NOT EXISTS idx_room_memberships_room_membership_user
ON room_memberships(room_id, membership, user_id);

-- ============================================================================
-- 2. notifications 表 - 补充索引
-- ============================================================================

-- 问题: 通知列表按用户和房间查询
-- 优化: 支持 (user_id, room_id, ts) 联合查询
CREATE INDEX IF NOT EXISTS idx_notifications_user_room_ts
ON notifications(user_id, room_id, ts DESC);

-- ============================================================================
-- 3. event_receipts 表 - 补充索引
-- ============================================================================

-- 问题: 回执查询按房间和类型过滤
-- 优化: 支持 (room_id, receipt_type, origin_server_ts) 联合查询
CREATE INDEX IF NOT EXISTS idx_event_receipts_room_type
ON event_receipts(room_id, receipt_type, origin_server_ts DESC);

-- ============================================================================
-- 4. user_threepids 表 - 补充索引
-- ============================================================================

-- 问题: 3PID 关联查询需要通过 medium 和 address 查找 user_id
-- 优化: 支持通过 3PID 类型和地址快速查找用户
CREATE INDEX IF NOT EXISTS idx_user_threepids_medium_address
ON user_threepids(medium, address);

-- ============================================================================
-- 5. event_relations 表 - 补充索引
-- ============================================================================

-- 问题: 事件关系查询常通过 thread_id 查找
-- 优化: 支持通过 thread_id 高效查询关联事件
CREATE INDEX IF NOT EXISTS idx_event_relations_thread
ON event_relations(relation_thread_id);

-- ============================================================================
-- 6. pusher_threepids 表 - 补充索引
-- ============================================================================

-- 问题: 推送目标查询需要通过 user_id 查找
-- 优化: 支持通过 user_id 高效查询推送配置
CREATE INDEX IF NOT EXISTS idx_pusher_threepids_user
ON pusher_threepids(user_id);

-- ============================================================================
-- 7. device_keys 表 - 补充索引 (cross-signing)
-- ============================================================================

-- 问题: 跨设备密钥查询常通过 user_id 和 key_type 查找
-- 优化: 支持通过 (user_id, key_type) 高效查询
CREATE INDEX IF NOT EXISTS idx_device_keys_user_key_type
ON device_keys(user_id, key_type);

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
AND indexrelname IN (
    'idx_room_memberships_room_membership_user',
    'idx_notifications_user_room_ts',
    'idx_event_receipts_room_type',
    'idx_user_threepids_medium_address',
    'idx_event_relations_thread',
    'idx_pusher_threepids_user',
    'idx_device_keys_user_key_type'
)
ORDER BY indexrelname;

-- ============================================================================
-- 记录迁移
-- ============================================================================

INSERT INTO schema_migrations (version, description, applied_ts)
VALUES ('20260329_p2_optimization', 'P2 performance indexes: room_memberships, notifications, event_receipts, user_threepids, event_relations, pusher_threepids, device_keys', EXTRACT(EPOCH FROM NOW())::BIGINT * 1000)
ON CONFLICT (version) DO NOTHING;
