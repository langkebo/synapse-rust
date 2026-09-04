-- =====================================================================
-- Migration: 20260904040000_schema_cleanup_dedup_and_dead_code.sql
-- 目的:
--   1. 删除 events.reference_image 死字段：经查 INSERT 语句
--      (create.rs:13/70/174/293) 从未将 reference_image 列入 INSERT 列清单，
--      SELECT 路径虽包含此列但取值永远为 NULL。属于零值冗余字段。
--   2. v11 baseline 第 3542/3543 与 4035/4036 重复定义
--      idx_rooms_name_trgm 与 idx_rooms_canonical_alias_trgm，但
--      PostgreSQL 同名索引不会真正共存（第二个 IF NOT EXISTS 被跳过），
--      因此数据库中只会有一个同名索引，无需 DROP。审计报告 §4.2 中
--      "两个索引定义重复"的描述属于 baseline 文件级冗余，已记录待 v12
--      baseline 重构时清理。
-- 审计依据: .scratch/db-schema-audit-2026-09-04.md §4.2 / §4.3
-- =====================================================================

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'events' AND column_name = 'reference_image'
    ) THEN
        ALTER TABLE events DROP COLUMN reference_image;
    END IF;
END $$;