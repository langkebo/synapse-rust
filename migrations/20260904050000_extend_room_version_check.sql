-- =====================================================================
-- Migration: 20260904050000_extend_room_version_check.sql
-- 目的: 扩展 rooms.room_version CHECK 约束，支持未来 v12+ room versions
-- 审计依据: .scratch/db-schema-audit-2026-09-04.md §4.1
--
-- 原约束 (v11 baseline 第 242-246 行):
--   CONSTRAINT ck_rooms_room_version_valid
--       CHECK (
--           room_version IS NULL OR room_version = ANY (ARRAY[
--               '1','2','3','4','5','6','7','8','9','10','11'
--           ])
--       )
--
-- 风险: 硬编码 1-11 白名单会拒绝 MSC4186 (Stable room IDs, v12, 2024 已标准化)
-- 及其他未来 v13+ versions。Matrix Spec evolution 不会停在 v11。
--
-- 改写策略: 用正则允许任意 `<digits>.<digits>` (e.g. '12', '12.0', '13.5')
--   但禁止空字符串/非数字字符，避免脏数据。
--   兼容历史合法值 '1'-'11'（自动匹配正则）。
--
-- ALTER CHECK 约束: PG 不支持直接 ALTER CHECK 内容，必须 DROP + ADD。
--   但 DROP CONSTRAINT 在大表上需 AccessExclusiveLock；生产环境安排在维护窗口。
-- =====================================================================

DO $$
BEGIN
    -- 仅在旧约束存在时执行（v11+ baseline 装过的实例都有）
    IF EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'ck_rooms_room_version_valid'
    ) THEN
        ALTER TABLE rooms DROP CONSTRAINT ck_rooms_room_version_valid;
    END IF;

    -- 用新约束替换（已不存在则建）—— PG 支持 `ADD CONSTRAINT` 内联 IF NOT EXISTS 检查
    -- （PG 原生不支持 ADD CONSTRAINT IF NOT EXISTS，需 DO block 包一下）
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'ck_rooms_room_version_valid_v2'
    ) THEN
        ALTER TABLE rooms ADD CONSTRAINT ck_rooms_room_version_valid_v2
            CHECK (
                room_version IS NULL OR room_version ~ '^[0-9]+(\.[0-9]+)*$'
            );
    END IF;
END $$;