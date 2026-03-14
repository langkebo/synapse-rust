-- ============================================================================
-- 迁移: 20260311000008_fix_key_backups_constraints.sql
-- 描述: 修复 key_backups 表约束和字段问题
-- 日期: 2026-03-11
-- ============================================================================

-- 1. 删除现有的单一 user_id 唯一约束（因为一个用户可以有多个备份版本）
ALTER TABLE key_backups DROP CONSTRAINT IF EXISTS uq_key_backups_user;

-- 2. 添加复合唯一约束 (user_id, backup_id)
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint 
        WHERE conname = 'uq_key_backups_user_backup_id'
    ) THEN
        ALTER TABLE key_backups 
        ADD CONSTRAINT uq_key_backups_user_backup_id UNIQUE (user_id, backup_id);
        RAISE NOTICE 'Added constraint: uq_key_backups_user_backup_id';
    END IF;
END $$;

-- 3. 添加复合唯一约束 (user_id, version)
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint 
        WHERE conname = 'uq_key_backups_user_version'
    ) THEN
        ALTER TABLE key_backups 
        ADD CONSTRAINT uq_key_backups_user_version UNIQUE (user_id, version);
        RAISE NOTICE 'Added constraint: uq_key_backups_user_version';
    END IF;
END $$;

-- 4. 确保 version 字段有默认值
ALTER TABLE key_backups ALTER COLUMN version SET DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT;

-- 5. 修改 auth_data 为可空（代码使用 backup_data）
ALTER TABLE key_backups ALTER COLUMN auth_data DROP NOT NULL;

-- 6. 确保 backup_data 有默认值
ALTER TABLE key_backups ALTER COLUMN backup_data SET DEFAULT '{}';

-- 7. 添加索引优化查询
CREATE INDEX IF NOT EXISTS idx_key_backups_user_version ON key_backups(user_id, version DESC);

-- ============================================================================
-- 验证
-- ============================================================================
DO $$
DECLARE
    constraint_count INTEGER;
BEGIN
    SELECT COUNT(*) INTO constraint_count
    FROM pg_constraint 
    WHERE conname IN ('uq_key_backups_user_backup_id', 'uq_key_backups_user_version');
    
    IF constraint_count >= 2 THEN
        RAISE NOTICE 'Key backups constraints fixed successfully';
    ELSE
        RAISE WARNING 'Key backups constraints may have issues';
    END IF;
END $$;
