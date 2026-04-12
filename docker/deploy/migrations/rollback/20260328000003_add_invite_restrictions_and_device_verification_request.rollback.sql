-- ============================================================================
-- 回滚脚本: 20260328000003_add_invite_restrictions_and_device_verification_request
-- 回滚日期: 2026-03-30
-- ============================================================================

SET TIME ZONE 'UTC';

-- 删除索引
DROP INDEX IF EXISTS idx_device_verification_request_user_device_pending;
DROP INDEX IF EXISTS idx_device_verification_request_expires_pending;
DROP INDEX IF EXISTS idx_room_invite_allowlist_user;
DROP INDEX IF EXISTS idx_room_invite_allowlist_room;
DROP INDEX IF EXISTS idx_room_invite_blocklist_user;
DROP INDEX IF EXISTS idx_room_invite_blocklist_room;

-- 删除表
DROP TABLE IF EXISTS device_verification_request;
DROP TABLE IF EXISTS room_invite_allowlist;
DROP TABLE IF EXISTS room_invite_blocklist;
