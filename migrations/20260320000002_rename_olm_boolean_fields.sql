-- Migration: Rename olm_accounts boolean fields for naming consistency
-- Date: 2026-03-20
-- Description: 重命名 olm_accounts 表中的布尔字段以符合命名规范
-- Issue: db-comprehensive-audit-v1 P2 优化

BEGIN;

-- 重命名 is_one_time_keys_published 为 has_published_one_time_keys
ALTER TABLE olm_accounts RENAME COLUMN is_one_time_keys_published TO has_published_one_time_keys;

-- 重命名 is_fallback_key_published 为 has_published_fallback_key
ALTER TABLE olm_accounts RENAME COLUMN is_fallback_key_published TO has_published_fallback_key;

-- 验证列已重命名
SELECT column_name FROM information_schema.columns
WHERE table_name = 'olm_accounts' AND column_name LIKE 'has_published%';

COMMIT;

-- 回滚方案 (如需回滚):
-- BEGIN;
-- ALTER TABLE olm_accounts RENAME COLUMN has_published_one_time_keys TO is_one_time_keys_published;
-- ALTER TABLE olm_accounts RENAME COLUMN has_published_fallback_key TO is_fallback_key_published;
-- COMMIT;