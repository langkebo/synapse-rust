-- ============================================================================
-- 添加缺失的 space_members 表
-- 创建日期: 2026-03-11
-- 说明: 修复 Spaces API 错误 - 添加 space_members 表
-- ============================================================================

-- Space 成员表
-- 存储 Space 的成员关系
CREATE TABLE IF NOT EXISTS space_members (
    id BIGSERIAL,
    space_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    membership TEXT NOT NULL DEFAULT 'join',
    joined_ts BIGINT,
    invited_ts BIGINT,
    left_ts BIGINT,
    inviter TEXT,
    is_suggested BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    CONSTRAINT pk_space_members PRIMARY KEY (id),
    CONSTRAINT uq_space_members_space_user UNIQUE (space_id, user_id),
    CONSTRAINT fk_space_members_space FOREIGN KEY (space_id) REFERENCES spaces(space_id) ON DELETE CASCADE,
    CONSTRAINT fk_space_members_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_space_members_space ON space_members(space_id);
CREATE INDEX IF NOT EXISTS idx_space_members_user ON space_members(user_id);
CREATE INDEX IF NOT EXISTS idx_space_members_membership ON space_members(membership);

-- 添加 spaces 表缺失的字段
ALTER TABLE spaces ADD COLUMN IF NOT EXISTS room_id TEXT;
ALTER TABLE spaces ADD COLUMN IF NOT EXISTS join_rule TEXT DEFAULT 'invite';
ALTER TABLE spaces ADD COLUMN IF NOT EXISTS visibility TEXT DEFAULT 'private';
ALTER TABLE spaces ADD COLUMN IF NOT EXISTS updated_ts BIGINT;
ALTER TABLE spaces ADD COLUMN IF NOT EXISTS parent_space_id TEXT;
ALTER TABLE spaces ADD COLUMN IF NOT EXISTS room_type TEXT;

-- 插入迁移记录
INSERT INTO schema_migrations (version, name, applied_ts, description)
VALUES (
    'v6.0.2', 
    'add_space_members_table', 
    EXTRACT(EPOCH FROM NOW()) * 1000, 
    'Add space_members table and missing columns to spaces table'
) ON CONFLICT (version) DO NOTHING;
