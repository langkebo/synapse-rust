-- 创建 space_members 表
-- 用于管理 Space 的成员关系

CREATE TABLE IF NOT EXISTS space_members (
    space_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    membership TEXT NOT NULL DEFAULT 'join',
    joined_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    left_ts BIGINT,
    inviter TEXT,
    CONSTRAINT pk_space_members PRIMARY KEY (space_id, user_id),
    CONSTRAINT fk_space_members_space FOREIGN KEY (space_id) REFERENCES spaces(space_id) ON DELETE CASCADE,
    CONSTRAINT fk_space_members_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_space_members_user ON space_members(user_id);
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_space_members_membership ON space_members(membership);
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_space_members_joined_ts ON space_members(joined_ts);
