-- P2-2 撤销脚本：移除所有 CHECK 约束
--
-- 策略：直接 DROP CONSTRAINT（CHECK 约束本身不持锁，安全）
-- 回滚后 schema 与 v10 baseline 一致（无 CHECK 约束）

ALTER TABLE rooms DROP CONSTRAINT IF EXISTS ck_rooms_join_rules_valid;
ALTER TABLE rooms DROP CONSTRAINT IF EXISTS ck_rooms_history_visibility_valid;
ALTER TABLE rooms DROP CONSTRAINT IF EXISTS ck_rooms_visibility_valid;
ALTER TABLE rooms DROP CONSTRAINT IF EXISTS ck_rooms_room_version_valid;
ALTER TABLE rooms DROP CONSTRAINT IF EXISTS ck_rooms_timestamps_nonneg;

ALTER TABLE room_summaries DROP CONSTRAINT IF EXISTS ck_room_summaries_join_rules_valid;
ALTER TABLE room_summaries DROP CONSTRAINT IF EXISTS ck_room_summaries_history_visibility_valid;
ALTER TABLE room_summaries DROP CONSTRAINT IF EXISTS ck_room_summaries_guest_access_valid;
ALTER TABLE room_summaries DROP CONSTRAINT IF EXISTS ck_room_summaries_member_count_nonneg;
ALTER TABLE room_summaries DROP CONSTRAINT IF EXISTS ck_room_summaries_unread_nonneg;