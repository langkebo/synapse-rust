-- membership 状态扩展：允许 'forget' 表示 room forget（MSC4267 语义）
--
-- 背景：storage `RoomMemberStorage::forget_member` 会 UPDATE room_memberships
-- SET membership = 'forget'（见 synapse-storage/src/membership/mod.rs），
-- 但 schema 的 CHECK 约束只接受 ('invite','join','knock','leave','ban')，
-- 5 个 Matrix spec 标准的 membership 值。forget 是 synapse-rust 内部用来
-- 表达"用户已 forget 房间"的隐式状态，不写入 timeline、不参与 m.room.member 事件，
-- 只在 room_memberships 行里区分 leave vs forget。
--
-- 兼容性：原约束已通过迁移 20260904010000 加上。forget 是新增允许值，
-- 现有数据 (membership in (invite, join, knock, leave, ban)) 不受影响。

ALTER TABLE room_memberships DROP CONSTRAINT IF EXISTS ck_room_memberships_valid;
ALTER TABLE room_memberships
    ADD CONSTRAINT ck_room_memberships_valid
    CHECK (membership IN ('invite', 'join', 'knock', 'leave', 'ban', 'forget'));
