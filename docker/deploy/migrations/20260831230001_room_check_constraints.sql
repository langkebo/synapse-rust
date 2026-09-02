-- P2-2: rooms 和 room_summaries 表的 CHECK 约束（捕获非法数据写入）
--
-- 注意：所有 ck_rooms_* 与 ck_room_summaries_* 约束在 00000000_unified_schema_v11.sql
-- 的 baseline 中已声明（line 236-248 / 398-406）。本迁移作为幂等的
-- "确保存在" 守卫，对新部署（v10 跳级或重置重建后）以及已升级但约束缺失的
-- 边缘场景仍然安全。
--
-- 测量基础（migrations/INDEXES.md + 测试 schema 扫描）：
-- - rooms.join_rules: 现存数据仅 'invite' (175行)
-- - rooms.history_visibility: 仅 'shared'(125), 'joined'(50)
-- - rooms.visibility: 仅 'private' (175)
-- - rooms.room_version: '1'(34), '6'(47), '10'(94) — Matrix 规范允许 1-11
-- - room_summaries.join_rules/history_visibility/guest_access: 同上枚举
-- - 所有 member_count/unread_* 等数值字段: 测试数据全部 >=0
--
-- 零停机策略：
-- 1. ALTER TABLE ... ADD CONSTRAINT ... CHECK (...) NOT VALID  —— 不扫描现有行，仅对未来 INSERT/UPDATE 生效
-- 2. ALTER TABLE VALIDATE CONSTRAINT ...                       —— 仅 ShareUpdateExclusiveLock，并发读写不阻塞
--
-- 命名规范：ck_<table>_<col>_<rule>
--
-- 回滚：见 .undo.sql（直接 DROP CONSTRAINT IF EXISTS）
--

-- ============================================================
-- 1. rooms 表枚举约束（幂等：仅当约束不存在时添加）
-- ============================================================

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'ck_rooms_join_rules_valid' AND conrelid = 'rooms'::regclass
    ) THEN
        ALTER TABLE rooms
            ADD CONSTRAINT ck_rooms_join_rules_valid
            CHECK (join_rules IS NULL OR join_rules IN ('invite', 'public', 'knock', 'restricted'))
            NOT VALID;
        ALTER TABLE rooms VALIDATE CONSTRAINT ck_rooms_join_rules_valid;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'ck_rooms_history_visibility_valid' AND conrelid = 'rooms'::regclass
    ) THEN
        ALTER TABLE rooms
            ADD CONSTRAINT ck_rooms_history_visibility_valid
            CHECK (history_visibility IS NULL OR history_visibility IN ('shared', 'invited', 'joined', 'world_readable'))
            NOT VALID;
        ALTER TABLE rooms VALIDATE CONSTRAINT ck_rooms_history_visibility_valid;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'ck_rooms_visibility_valid' AND conrelid = 'rooms'::regclass
    ) THEN
        ALTER TABLE rooms
            ADD CONSTRAINT ck_rooms_visibility_valid
            CHECK (visibility IS NULL OR visibility IN ('public', 'private'))
            NOT VALID;
        ALTER TABLE rooms VALIDATE CONSTRAINT ck_rooms_visibility_valid;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'ck_rooms_room_version_valid' AND conrelid = 'rooms'::regclass
    ) THEN
        ALTER TABLE rooms
            ADD CONSTRAINT ck_rooms_room_version_valid
            CHECK (
                room_version IS NULL OR room_version = ANY (ARRAY[
                    '1','2','3','4','5','6','7','8','9','10','11'
                ])
            )
            NOT VALID;
        ALTER TABLE rooms VALIDATE CONSTRAINT ck_rooms_room_version_valid;
    END IF;
END $$;

-- ============================================================
-- 2. room_summaries 表枚举约束（幂等）
-- ============================================================

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'ck_room_summaries_join_rules_valid' AND conrelid = 'room_summaries'::regclass
    ) THEN
        ALTER TABLE room_summaries
            ADD CONSTRAINT ck_room_summaries_join_rules_valid
            CHECK (join_rules IN ('invite', 'public', 'knock', 'restricted'))
            NOT VALID;
        ALTER TABLE room_summaries VALIDATE CONSTRAINT ck_room_summaries_join_rules_valid;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'ck_room_summaries_history_visibility_valid' AND conrelid = 'room_summaries'::regclass
    ) THEN
        ALTER TABLE room_summaries
            ADD CONSTRAINT ck_room_summaries_history_visibility_valid
            CHECK (history_visibility IN ('shared', 'invited', 'joined', 'world_readable'))
            NOT VALID;
        ALTER TABLE room_summaries VALIDATE CONSTRAINT ck_room_summaries_history_visibility_valid;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'ck_room_summaries_guest_access_valid' AND conrelid = 'room_summaries'::regclass
    ) THEN
        ALTER TABLE room_summaries
            ADD CONSTRAINT ck_room_summaries_guest_access_valid
            CHECK (guest_access IN ('can_join', 'forbidden'))
            NOT VALID;
        ALTER TABLE room_summaries VALIDATE CONSTRAINT ck_room_summaries_guest_access_valid;
    END IF;
END $$;

-- ============================================================
-- 3. room_summaries 数值字段非负约束（幂等）
-- ============================================================

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'ck_room_summaries_member_count_nonneg' AND conrelid = 'room_summaries'::regclass
    ) THEN
        ALTER TABLE room_summaries
            ADD CONSTRAINT ck_room_summaries_member_count_nonneg
            CHECK (member_count >= 0 AND joined_member_count >= 0 AND invited_member_count >= 0)
            NOT VALID;
        ALTER TABLE room_summaries VALIDATE CONSTRAINT ck_room_summaries_member_count_nonneg;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'ck_room_summaries_unread_nonneg' AND conrelid = 'room_summaries'::regclass
    ) THEN
        ALTER TABLE room_summaries
            ADD CONSTRAINT ck_room_summaries_unread_nonneg
            CHECK (unread_notifications >= 0 AND unread_highlight >= 0)
            NOT VALID;
        ALTER TABLE room_summaries VALIDATE CONSTRAINT ck_room_summaries_unread_nonneg;
    END IF;
END $$;

-- ============================================================
-- 4. 时间戳字段非负约束（幂等）
-- ============================================================

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'ck_rooms_timestamps_nonneg' AND conrelid = 'rooms'::regclass
    ) THEN
        ALTER TABLE rooms
            ADD CONSTRAINT ck_rooms_timestamps_nonneg
            CHECK (created_ts >= 0 AND (last_activity_ts IS NULL OR last_activity_ts >= 0))
            NOT VALID;
        ALTER TABLE rooms VALIDATE CONSTRAINT ck_rooms_timestamps_nonneg;
    END IF;
END $$;
