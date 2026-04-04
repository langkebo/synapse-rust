-- 创建 space_statistics 表
-- 用于存储 Space 统计信息

CREATE TABLE IF NOT EXISTS space_statistics (
    space_id TEXT PRIMARY KEY,
    name TEXT,
    is_public BOOLEAN,
    child_room_count BIGINT DEFAULT 0,
    member_count BIGINT DEFAULT 0,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT
);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_space_statistics_member_count ON space_statistics(member_count DESC);
