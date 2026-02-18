-- =============================================================================
-- Synapse-Rust 空间功能 (MSC1772) 数据库迁移脚本
-- 版本: 1.0
-- 创建日期: 2026-02-13
-- PostgreSQL版本: 15.x 兼容
-- 描述: 实现 Matrix Spaces 功能，支持房间层级组织
-- =============================================================================

-- 空间表: 存储空间基本信息
-- 空间本质上是一个特殊类型的房间，具有 m.space 创建事件类型
CREATE TABLE IF NOT EXISTS spaces (
    space_id VARCHAR(255) NOT NULL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL UNIQUE,
    name VARCHAR(255),
    topic VARCHAR(512),
    avatar_url VARCHAR(512),
    creator VARCHAR(255) NOT NULL,
    join_rule VARCHAR(50) DEFAULT 'invite',
    visibility VARCHAR(50) DEFAULT 'private',
    creation_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    is_public BOOLEAN DEFAULT FALSE,
    parent_space_id VARCHAR(255),
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    FOREIGN KEY (creator) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (parent_space_id) REFERENCES spaces(space_id) ON DELETE SET NULL
);

-- 空间索引
CREATE INDEX IF NOT EXISTS idx_spaces_room ON spaces(room_id);
CREATE INDEX IF NOT EXISTS idx_spaces_creator ON spaces(creator);
CREATE INDEX IF NOT EXISTS idx_spaces_public ON spaces(is_public) WHERE is_public = TRUE;
CREATE INDEX IF NOT EXISTS idx_spaces_parent ON spaces(parent_space_id);
CREATE INDEX IF NOT EXISTS idx_spaces_creation ON spaces(creation_ts DESC);

-- 空间子房间表: 存储空间与子房间的关系
CREATE TABLE IF NOT EXISTS space_children (
    id BIGSERIAL PRIMARY KEY,
    space_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    via_servers TEXT[],
    "order" VARCHAR(255),
    suggested BOOLEAN DEFAULT FALSE,
    added_by VARCHAR(255) NOT NULL,
    added_ts BIGINT NOT NULL,
    removed_ts BIGINT,
    UNIQUE(space_id, room_id),
    FOREIGN KEY (space_id) REFERENCES spaces(space_id) ON DELETE CASCADE,
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    FOREIGN KEY (added_by) REFERENCES users(user_id) ON DELETE CASCADE
);

-- 空间子房间索引
CREATE INDEX IF NOT EXISTS idx_space_children_space ON space_children(space_id);
CREATE INDEX IF NOT EXISTS idx_space_children_room ON space_children(room_id);
CREATE INDEX IF NOT EXISTS idx_space_children_suggested ON space_children(space_id, suggested) WHERE suggested = TRUE;
CREATE INDEX IF NOT EXISTS idx_space_children_order ON space_children(space_id, "order");

-- 空间成员表: 存储空间成员关系
CREATE TABLE IF NOT EXISTS space_members (
    space_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    membership VARCHAR(50) NOT NULL DEFAULT 'join',
    joined_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    left_ts BIGINT,
    inviter VARCHAR(255),
    PRIMARY KEY (space_id, user_id),
    FOREIGN KEY (space_id) REFERENCES spaces(space_id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (inviter) REFERENCES users(user_id) ON DELETE SET NULL
);

-- 空间成员索引
CREATE INDEX IF NOT EXISTS idx_space_members_space ON space_members(space_id);
CREATE INDEX IF NOT EXISTS idx_space_members_user ON space_members(user_id);
CREATE INDEX IF NOT EXISTS idx_space_members_membership ON space_members(space_id, membership);

-- 空间摘要缓存表: 用于快速获取空间层级信息
CREATE TABLE IF NOT EXISTS space_summaries (
    space_id VARCHAR(255) NOT NULL PRIMARY KEY,
    summary JSONB NOT NULL DEFAULT '{}'::jsonb,
    children_count BIGINT DEFAULT 0,
    member_count BIGINT DEFAULT 0,
    updated_ts BIGINT NOT NULL,
    FOREIGN KEY (space_id) REFERENCES spaces(space_id) ON DELETE CASCADE
);

-- 空间摘要索引
CREATE INDEX IF NOT EXISTS idx_space_summaries_updated ON space_summaries(updated_ts DESC);

-- 空间事件表: 记录空间相关事件
CREATE TABLE IF NOT EXISTS space_events (
    event_id VARCHAR(255) NOT NULL PRIMARY KEY,
    space_id VARCHAR(255) NOT NULL,
    event_type VARCHAR(255) NOT NULL,
    sender VARCHAR(255) NOT NULL,
    content JSONB NOT NULL DEFAULT '{}'::jsonb,
    state_key VARCHAR(255),
    origin_server_ts BIGINT NOT NULL,
    processed_ts BIGINT,
    FOREIGN KEY (space_id) REFERENCES spaces(space_id) ON DELETE CASCADE,
    FOREIGN KEY (sender) REFERENCES users(user_id) ON DELETE CASCADE
);

-- 空间事件索引
CREATE INDEX IF NOT EXISTS idx_space_events_space ON space_events(space_id);
CREATE INDEX IF NOT EXISTS idx_space_events_type ON space_events(event_type);
CREATE INDEX IF NOT EXISTS idx_space_events_sender ON space_events(sender);
CREATE INDEX IF NOT EXISTS idx_space_events_ts ON space_events(origin_server_ts DESC);

-- 添加注释
COMMENT ON TABLE spaces IS 'Matrix Spaces (MSC1772) - 空间是房间的集合，用于组织层级结构';
COMMENT ON TABLE space_children IS '空间子房间关系表，记录空间包含的房间';
COMMENT ON TABLE space_members IS '空间成员表，记录用户与空间的关系';
COMMENT ON TABLE space_summaries IS '空间摘要缓存表，用于快速获取空间信息';
COMMENT ON TABLE space_events IS '空间事件表，记录空间相关的状态事件';

-- 创建更新时间戳触发器函数
CREATE OR REPLACE FUNCTION update_space_timestamp()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_ts = EXTRACT(EPOCH FROM NOW()) * 1000;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- 为 spaces 表添加触发器
DROP TRIGGER IF EXISTS update_spaces_timestamp ON spaces;
CREATE TRIGGER update_spaces_timestamp
    BEFORE UPDATE ON spaces
    FOR EACH ROW
    EXECUTE FUNCTION update_space_timestamp();

-- 创建空间统计视图
CREATE OR REPLACE VIEW space_statistics AS
SELECT 
    s.space_id,
    s.name,
    s.is_public,
    COUNT(DISTINCT sc.room_id) AS child_room_count,
    COUNT(DISTINCT sm.user_id) AS member_count,
    s.creation_ts,
    s.updated_ts
FROM spaces s
LEFT JOIN space_children sc ON s.space_id = sc.space_id AND sc.removed_ts IS NULL
LEFT JOIN space_members sm ON s.space_id = sm.space_id AND sm.membership = 'join'
GROUP BY s.space_id, s.name, s.is_public, s.creation_ts, s.updated_ts;

COMMENT ON VIEW space_statistics IS '空间统计视图，提供空间的基本统计信息';
