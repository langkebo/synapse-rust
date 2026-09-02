-- P2-2: 为 rooms 和 room_summaries 表加 CHECK 约束（捕获非法数据写入）
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
-- 2. ALTER TABLE ... VALIDATE CONSTRAINT ...                  —— 仅 ShareUpdateExclusiveLock，并发读写不阻塞
--
-- 命名规范：ck_<table>_<col>_<rule>
--
-- 回滚：见 .undo.sql（直接 DROP CONSTRAINT）
--

-- ============================================================
-- 1. rooms 表枚举约束
-- ============================================================

-- join_rules: Matrix 规范允许值（invite/public/knock/restricted）
ALTER TABLE rooms
    ADD CONSTRAINT ck_rooms_join_rules_valid
    CHECK (join_rules IS NULL OR join_rules IN ('invite', 'public', 'knock', 'restricted'))
    NOT VALID;

ALTER TABLE rooms VALIDATE CONSTRAINT ck_rooms_join_rules_valid;

-- history_visibility: shared/invited/joined/world_readable
ALTER TABLE rooms
    ADD CONSTRAINT ck_rooms_history_visibility_valid
    CHECK (history_visibility IS NULL OR history_visibility IN ('shared', 'invited', 'joined', 'world_readable'))
    NOT VALID;

ALTER TABLE rooms VALIDATE CONSTRAINT ck_rooms_history_visibility_valid;

-- visibility: public/private（Matrix 房间目录枚举）
ALTER TABLE rooms
    ADD CONSTRAINT ck_rooms_visibility_valid
    CHECK (visibility IS NULL OR visibility IN ('public', 'private'))
    NOT VALID;

ALTER TABLE rooms VALIDATE CONSTRAINT ck_rooms_visibility_valid;

-- room_version: Matrix 规范当前允许 1-11
ALTER TABLE rooms
    ADD CONSTRAINT ck_rooms_room_version_valid
    CHECK (
        room_version IS NULL OR room_version = ANY (ARRAY[
            '1','2','3','4','5','6','7','8','9','10','11'
        ])
    )
    NOT VALID;

ALTER TABLE rooms VALIDATE CONSTRAINT ck_rooms_room_version_valid;

-- ============================================================
-- 2. room_summaries 表枚举约束
-- ============================================================

ALTER TABLE room_summaries
    ADD CONSTRAINT ck_room_summaries_join_rules_valid
    CHECK (join_rules IN ('invite', 'public', 'knock', 'restricted'))
    NOT VALID;

ALTER TABLE room_summaries VALIDATE CONSTRAINT ck_room_summaries_join_rules_valid;

ALTER TABLE room_summaries
    ADD CONSTRAINT ck_room_summaries_history_visibility_valid
    CHECK (history_visibility IN ('shared', 'invited', 'joined', 'world_readable'))
    NOT VALID;

ALTER TABLE room_summaries VALIDATE CONSTRAINT ck_room_summaries_history_visibility_valid;

ALTER TABLE room_summaries
    ADD CONSTRAINT ck_room_summaries_guest_access_valid
    CHECK (guest_access IN ('can_join', 'forbidden'))
    NOT VALID;

ALTER TABLE room_summaries VALIDATE CONSTRAINT ck_room_summaries_guest_access_valid;

-- ============================================================
-- 3. room_summaries 数值字段非负约束（防止减计数溢出）
-- ============================================================

ALTER TABLE room_summaries
    ADD CONSTRAINT ck_room_summaries_member_count_nonneg
    CHECK (member_count >= 0 AND joined_member_count >= 0 AND invited_member_count >= 0)
    NOT VALID;

ALTER TABLE room_summaries VALIDATE CONSTRAINT ck_room_summaries_member_count_nonneg;

ALTER TABLE room_summaries
    ADD CONSTRAINT ck_room_summaries_unread_nonneg
    CHECK (unread_notifications >= 0 AND unread_highlight >= 0)
    NOT VALID;

ALTER TABLE room_summaries VALIDATE CONSTRAINT ck_room_summaries_unread_nonneg;

-- ============================================================
-- 4. 时间戳字段非负约束（msec-since-epoch，必须 >=0）
-- ============================================================

ALTER TABLE rooms
    ADD CONSTRAINT ck_rooms_timestamps_nonneg
    CHECK (created_ts >= 0 AND (last_activity_ts IS NULL OR last_activity_ts >= 0))
    NOT VALID;

ALTER TABLE rooms VALIDATE CONSTRAINT ck_rooms_timestamps_nonneg;