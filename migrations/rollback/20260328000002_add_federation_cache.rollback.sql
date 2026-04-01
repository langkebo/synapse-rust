-- ============================================================================
-- 回滚脚本: 20260328000002_add_federation_cache
-- 回滚日期: 2026-03-30
-- ============================================================================

SET TIME ZONE 'UTC';

-- 删除索引
DROP INDEX IF EXISTS idx_federation_cache_key;
DROP INDEX IF EXISTS idx_federation_cache_expiry;

-- 删除表
DROP TABLE IF EXISTS federation_cache;
