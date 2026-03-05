-- Migration: Add retention and space related tables
-- Version: 20260302000002
-- Description: 添加消息保留策略和Space相关的数据库表

-- ============================================================================
-- 1. 消息保留策略相关表
-- ============================================================================

-- 房间保留策略表
CREATE TABLE IF NOT EXISTS room_retention_policies (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL UNIQUE,
    max_lifetime BIGINT,
    min_lifetime BIGINT NOT NULL DEFAULT 0,
    expire_on_clients BOOLEAN DEFAULT FALSE,
    is_server_default BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_room_retention_policies_room ON room_retention_policies(room_id);
CREATE INDEX IF NOT EXISTS idx_room_retention_policies_max_lifetime ON room_retention_policies(max_lifetime) WHERE max_lifetime IS NOT NULL;

-- 服务器保留策略表
CREATE TABLE IF NOT EXISTS server_retention_policy (
    id BIGSERIAL PRIMARY KEY,
    max_lifetime BIGINT,
    min_lifetime BIGINT NOT NULL DEFAULT 0,
    expire_on_clients BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL
);

-- 保留清理队列表
CREATE TABLE IF NOT EXISTS retention_cleanup_queue (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    event_id TEXT,
    event_type TEXT,
    origin_server_ts BIGINT NOT NULL,
    scheduled_ts BIGINT NOT NULL,
    status TEXT DEFAULT 'pending',
    created_ts BIGINT NOT NULL,
    processed_ts BIGINT,
    error_message TEXT,
    retry_count INTEGER DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_retention_cleanup_queue_status ON retention_cleanup_queue(status);
CREATE INDEX IF NOT EXISTS idx_retention_cleanup_queue_room ON retention_cleanup_queue(room_id);
CREATE INDEX IF NOT EXISTS idx_retention_cleanup_queue_scheduled ON retention_cleanup_queue(scheduled_ts) WHERE status = 'pending';

-- 保留清理日志表
CREATE TABLE IF NOT EXISTS retention_cleanup_logs (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    events_deleted BIGINT DEFAULT 0,
    state_events_deleted BIGINT DEFAULT 0,
    media_deleted BIGINT DEFAULT 0,
    bytes_freed BIGINT DEFAULT 0,
    started_ts BIGINT NOT NULL,
    completed_ts BIGINT,
    status TEXT DEFAULT 'pending',
    error_message TEXT
);

CREATE INDEX IF NOT EXISTS idx_retention_cleanup_logs_room ON retention_cleanup_logs(room_id);
CREATE INDEX IF NOT EXISTS idx_retention_cleanup_logs_status ON retention_cleanup_logs(status);

-- 已删除事件索引表
CREATE TABLE IF NOT EXISTS deleted_events_index (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    deletion_ts BIGINT NOT NULL,
    reason TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_deleted_events_room ON deleted_events_index(room_id);
CREATE INDEX IF NOT EXISTS idx_deleted_events_event ON deleted_events_index(event_id);
CREATE INDEX IF NOT EXISTS idx_deleted_events_deletion_ts ON deleted_events_index(deletion_ts);

-- 保留统计表
CREATE TABLE IF NOT EXISTS retention_stats (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL UNIQUE,
    total_events BIGINT DEFAULT 0,
    events_in_retention BIGINT DEFAULT 0,
    events_expired BIGINT DEFAULT 0,
    last_cleanup_ts BIGINT,
    next_cleanup_ts BIGINT
);

CREATE INDEX IF NOT EXISTS idx_retention_stats_room ON retention_stats(room_id);
CREATE INDEX IF NOT EXISTS idx_retention_stats_next_cleanup ON retention_stats(next_cleanup_ts) WHERE next_cleanup_ts IS NOT NULL;

-- ============================================================================
-- 2. Space 相关表增强
-- ============================================================================

-- Spaces 主表
CREATE TABLE IF NOT EXISTS spaces (
    id BIGSERIAL PRIMARY KEY,
    space_id TEXT NOT NULL UNIQUE,
    room_id TEXT NOT NULL,
    name TEXT,
    topic TEXT,
    avatar_url TEXT,
    creator TEXT NOT NULL,
    join_rule TEXT DEFAULT 'invite',
    visibility TEXT DEFAULT 'private',
    is_public BOOLEAN DEFAULT FALSE,
    room_type TEXT DEFAULT 'm.space',
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    parent_space_id TEXT
);

CREATE INDEX IF NOT EXISTS idx_spaces_room ON spaces(room_id);
CREATE INDEX IF NOT EXISTS idx_spaces_creator ON spaces(creator);
CREATE INDEX IF NOT EXISTS idx_spaces_public ON spaces(is_public) WHERE is_public = TRUE;
CREATE INDEX IF NOT EXISTS idx_spaces_parent ON spaces(parent_space_id) WHERE parent_space_id IS NOT NULL;

-- 为现有 space_children 表添加缺失的列（如果需要）
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'space_children' AND column_name = 'suggested'
    ) THEN
        ALTER TABLE space_children ADD COLUMN suggested BOOLEAN DEFAULT FALSE;
    END IF;
END $$;

-- ============================================================================
-- 3. 后台更新统计表
-- ============================================================================

CREATE TABLE IF NOT EXISTS background_update_stats (
    id BIGSERIAL PRIMARY KEY,
    job_name TEXT NOT NULL UNIQUE,
    total_updates INTEGER DEFAULT 0,
    completed_updates INTEGER DEFAULT 0,
    failed_updates INTEGER DEFAULT 0,
    last_run_ts BIGINT,
    next_run_ts BIGINT,
    average_duration_ms BIGINT DEFAULT 0,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_background_update_stats_job ON background_update_stats(job_name);
CREATE INDEX IF NOT EXISTS idx_background_update_stats_next_run ON background_update_stats(next_run_ts) WHERE next_run_ts IS NOT NULL;

-- ============================================================================
-- 4. 模块回调表增强
-- ============================================================================

-- 确保 password_auth_providers 表有 is_enabled 列
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'password_auth_providers' AND column_name = 'is_enabled'
    ) THEN
        ALTER TABLE password_auth_providers ADD COLUMN is_enabled BOOLEAN DEFAULT TRUE;
    END IF;
END $$;

-- 模块执行日志表
CREATE TABLE IF NOT EXISTS module_execution_logs (
    id BIGSERIAL PRIMARY KEY,
    module_name TEXT NOT NULL,
    event_type TEXT NOT NULL,
    event_id TEXT,
    room_id TEXT,
    user_id TEXT,
    result TEXT NOT NULL,
    error_message TEXT,
    duration_ms BIGINT,
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_module_execution_logs_module ON module_execution_logs(module_name);
CREATE INDEX IF NOT EXISTS idx_module_execution_logs_created ON module_execution_logs(created_ts);

-- ============================================================================
-- 5. 初始化服务器保留策略（如果不存在）
-- ============================================================================

INSERT INTO server_retention_policy (max_lifetime, min_lifetime, expire_on_clients, created_ts, updated_ts)
SELECT NULL, 0, FALSE, EXTRACT(EPOCH FROM NOW())::BIGINT, EXTRACT(EPOCH FROM NOW())::BIGINT
WHERE NOT EXISTS (SELECT 1 FROM server_retention_policy LIMIT 1);
