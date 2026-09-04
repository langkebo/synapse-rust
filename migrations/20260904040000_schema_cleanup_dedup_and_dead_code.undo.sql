-- =====================================================================
-- Undo Migration: 20260904040000_schema_cleanup_dedup_and_dead_code.undo.sql
-- 回滚: 重新添加 events.reference_image 列
-- 警告: 此 undo 不恢复 baseline 文件中的重复 trgm 索引定义（v12 baseline 重构时清理）
-- =====================================================================

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'events' AND column_name = 'reference_image'
    ) THEN
        ALTER TABLE events ADD COLUMN reference_image TEXT;
    END IF;
END $$;