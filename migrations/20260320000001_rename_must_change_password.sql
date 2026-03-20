-- Migration: Rename must_change_password to is_password_change_required
-- Date: 2026-03-20
-- Description: 重命名 must_change_password 字段为 is_password_change_required 以符合布尔字段命名规范
-- Issue: db-comprehensive-audit-v1 P2 优化

BEGIN;

-- 重命名 must_change_password 为 is_password_change_required
ALTER TABLE users RENAME COLUMN must_change_password TO is_password_change_required;

-- 更新相关索引
DROP INDEX IF EXISTS idx_users_must_change_password;
CREATE INDEX IF NOT EXISTS idx_users_is_password_change_required
    ON users(is_password_change_required) WHERE is_password_change_required = TRUE;

-- 验证列已重命名
SELECT column_name FROM information_schema.columns
WHERE table_name = 'users' AND column_name = 'is_password_change_required';

COMMIT;

-- 回滚方案 (如需回滚):
-- BEGIN;
-- ALTER TABLE users RENAME COLUMN is_password_change_required TO must_change_password;
-- DROP INDEX IF EXISTS idx_users_is_password_change_required;
-- CREATE INDEX IF NOT EXISTS idx_users_must_change_password
--     ON users(must_change_password) WHERE must_change_password = TRUE;
-- COMMIT;