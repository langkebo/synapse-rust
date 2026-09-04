-- =====================================================================
-- Undo migration: 20260904050000_extend_room_version_check.undo.sql
-- 目的: 回滚 rooms.room_version CHECK 约束恢复为 v11 baseline 的硬编码版本
--
-- WARNING: 回滚前确保所有 rooms.room_version 值均在 {'1'..'11'} 范围内，
--          否则重新 ADD 旧约束会失败。
-- =====================================================================

DO $$
BEGIN
    -- 删除新约束
    IF EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'ck_rooms_room_version_valid_v2'
    ) THEN
        ALTER TABLE rooms DROP CONSTRAINT ck_rooms_room_version_valid_v2;
    END IF;

    -- 恢复原 v11 baseline 约束
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'ck_rooms_room_version_valid'
    ) THEN
        ALTER TABLE rooms ADD CONSTRAINT ck_rooms_room_version_valid
            CHECK (
                room_version IS NULL OR room_version = ANY (ARRAY[
                    '1','2','3','4','5','6','7','8','9','10','11'
                ])
            );
    END IF;
END $$;