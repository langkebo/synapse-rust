-- =============================================================================
-- Synapse-Rust 数据库迁移脚本
-- 版本: 20260220000000
-- 描述: 创建 notifications 表并修复 push_rules 表结构
-- 问题来源: api-error.md 测试记录 - 错误 #6
-- =============================================================================

-- =============================================================================
-- 第一部分: 创建 notifications 表
-- 问题: #6 - notifications 表不存在
-- =============================================================================

CREATE TABLE IF NOT EXISTS notifications (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    event_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    ts BIGINT NOT NULL,
    notification_type VARCHAR(50),
    profile_tag VARCHAR(255),
    read BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    updated_ts BIGINT
);

CREATE INDEX IF NOT EXISTS idx_notifications_user ON notifications(user_id);
CREATE INDEX IF NOT EXISTS idx_notifications_ts ON notifications(ts DESC);
CREATE INDEX IF NOT EXISTS idx_notifications_room ON notifications(room_id);
CREATE INDEX IF NOT EXISTS idx_notifications_read ON notifications(read) WHERE read = FALSE;

-- =============================================================================
-- 第二部分: 确保 push_rules 表有正确的列
-- =============================================================================

-- 添加 created_ts 列（如果不存在）
ALTER TABLE push_rules ADD COLUMN IF NOT EXISTS created_ts BIGINT DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000;

-- 添加 updated_ts 列（如果不存在）
ALTER TABLE push_rules ADD COLUMN IF NOT EXISTS updated_ts BIGINT;

-- 更新现有记录的 created_ts
UPDATE push_rules SET created_ts = EXTRACT(EPOCH FROM NOW()) * 1000 WHERE created_ts IS NULL;

-- =============================================================================
-- 第三部分: 确保 rooms 表有必要的列
-- 问题: #3 - world_readable 列不存在
-- =============================================================================

-- 添加 join_rules 列（如果不存在）
ALTER TABLE rooms ADD COLUMN IF NOT EXISTS join_rules VARCHAR(50) DEFAULT 'invite';

-- 添加 guest_access 列（如果不存在）
ALTER TABLE rooms ADD COLUMN IF NOT EXISTS guest_access VARCHAR(50) DEFAULT 'forbidden';

-- =============================================================================
-- 第四部分: 记录迁移版本
-- =============================================================================

INSERT INTO schema_migrations (version, success, executed_at)
VALUES ('20260220000000', true, NOW())
ON CONFLICT (version) DO NOTHING;
