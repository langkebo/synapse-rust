-- ============================================================================
-- P2 性能索引优化（活跃迁移）
-- 创建日期: 2026-03-29
-- 状态: 活跃
--
-- 说明: 基于 P2 性能分析添加 5 个补充索引，进一步提升查询性能
-- 幂等性: 使用 CREATE INDEX CONCURRENTLY IF NOT EXISTS，可重复执行
-- 应用场景: 升级路径必需，空库初始化可选
-- 依赖: 需在 20260328_p1_indexes.sql 之后执行
-- ============================================================================

SET TIME ZONE 'UTC';

-- ============================================================================
-- 4. user_threepids 表 - 补充索引
-- ============================================================================

-- 问题: 3PID 关联查询需要通过 medium 和 address 查找 user_id
-- 优化: 支持通过 3PID 类型和地址快速查找用户
SELECT 'CREATE INDEX IF NOT EXISTS idx_user_threepids_medium_address ON user_threepids(medium, address)' AS sql
WHERE EXISTS (
    SELECT 1
    FROM information_schema.columns
    WHERE table_schema = 'public'
      AND table_name = 'user_threepids'
      AND column_name IN ('medium', 'address')
    GROUP BY table_name
    HAVING COUNT(*) = 2
)
\gexec

-- ============================================================================
-- 5. event_relations 表 - 补充索引
-- ============================================================================

-- 问题: 事件关系查询常通过 thread_id 查找
-- 优化: 支持通过 thread_id 高效查询关联事件
SELECT 'CREATE INDEX IF NOT EXISTS idx_event_relations_thread ON event_relations(relation_thread_id)' AS sql
WHERE EXISTS (
    SELECT 1
    FROM information_schema.columns
    WHERE table_schema = 'public'
      AND table_name = 'event_relations'
      AND column_name = 'relation_thread_id'
)
\gexec

-- ============================================================================
-- 6. pusher_threepids 表 - 补充索引
-- ============================================================================

-- 问题: 推送目标查询需要通过 user_id 查找
-- 优化: 支持通过 user_id 高效查询推送配置
SELECT 'CREATE INDEX IF NOT EXISTS idx_pusher_threepids_user ON pusher_threepids(user_id)' AS sql
WHERE EXISTS (
    SELECT 1
    FROM information_schema.columns
    WHERE table_schema = 'public'
      AND table_name = 'pusher_threepids'
      AND column_name = 'user_id'
)
\gexec

-- ============================================================================
-- 7. device_keys 表 - 补充索引 (cross-signing)
-- ============================================================================

-- 问题: 跨设备密钥查询常通过 user_id 和 key_type 查找
-- 优化: 支持通过 (user_id, key_type) 高效查询
SELECT 'CREATE INDEX IF NOT EXISTS idx_device_keys_user_key_type ON device_keys(user_id, key_type)' AS sql
WHERE EXISTS (
    SELECT 1
    FROM information_schema.columns
    WHERE table_schema = 'public'
      AND table_name = 'device_keys'
      AND column_name IN ('user_id', 'key_type')
    GROUP BY table_name
    HAVING COUNT(*) = 2
)
\gexec

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
    'idx_user_threepids_medium_address',
    'idx_event_relations_thread',
    'idx_pusher_threepids_user',
    'idx_device_keys_user_key_type'
)
ORDER BY indexrelname;

-- ============================================================================
-- 记录迁移
-- ============================================================================

INSERT INTO schema_migrations (version, name, success, description, applied_ts)
VALUES ('20260329_p2_optimization', 'p2_optimization', TRUE, 'P2 performance indexes: user_threepids, event_relations, pusher_threepids, device_keys', EXTRACT(EPOCH FROM NOW())::BIGINT * 1000)
ON CONFLICT (version) DO NOTHING;
