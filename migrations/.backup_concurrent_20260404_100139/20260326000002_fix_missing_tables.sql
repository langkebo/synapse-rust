-- ============================================================================
-- synapse-rust 数据库修复迁移
-- 创建日期: 2026-03-26
-- 描述: 修复数据库架构问题，包括缺失表和列
-- ============================================================================

SET TIME ZONE 'UTC';

DO $$
BEGIN
    RAISE NOTICE '开始执行数据库修复迁移...';
END $$;

-- ============================================================================
-- 1. 创建 blocked_rooms 表 (如不存在)
-- ============================================================================
CREATE TABLE IF NOT EXISTS blocked_rooms (
    room_id TEXT PRIMARY KEY,
    blocked_at BIGINT NOT NULL,
    blocked_by TEXT NOT NULL,
    reason TEXT
);

-- ============================================================================
-- 2. 删除 shadow_bans 表（冗余，使用 users.is_shadow_banned）
-- ============================================================================
DROP TABLE IF EXISTS shadow_bans;

-- ============================================================================
-- 3. 修复 presence 表索引列名 (status -> presence)
-- ============================================================================
DROP INDEX IF EXISTS idx_presence_user_status_error;
DROP INDEX IF EXISTS idx_presence_user_status;
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_presence_user_status ON presence(user_id, presence);

-- ============================================================================
-- 3. 确保 room_directory 表有 added_ts 列
-- ============================================================================
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public'
        AND table_name = 'room_directory'
        AND column_name = 'added_ts'
    ) THEN
        ALTER TABLE room_directory ADD COLUMN added_ts BIGINT NOT NULL DEFAULT 0;
        RAISE NOTICE '列 room_directory.added_ts 已添加';
    END IF;
END $$;

-- ============================================================================
-- 4. 确保 db_metadata 表有 created_ts 和 updated_ts 列
-- ============================================================================
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public'
        AND table_name = 'db_metadata'
        AND column_name = 'created_ts'
    ) THEN
        ALTER TABLE db_metadata ADD COLUMN created_ts BIGINT NOT NULL DEFAULT 0;
        RAISE NOTICE '列 db_metadata.created_ts 已添加';
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public'
        AND table_name = 'db_metadata'
        AND column_name = 'updated_ts'
    ) THEN
        ALTER TABLE db_metadata ADD COLUMN updated_ts BIGINT NOT NULL DEFAULT 0;
        RAISE NOTICE '列 db_metadata.updated_ts 已添加';
    END IF;
END $$;

-- ============================================================================
-- 5. 添加 event_relations 表 (如不存在)
-- ============================================================================
CREATE TABLE IF NOT EXISTS event_relations (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    relates_to_event_id TEXT NOT NULL,
    relation_type TEXT NOT NULL CHECK (relation_type IN ('m.annotation', 'm.reference', 'm.replace', 'm.thread')),
    sender TEXT NOT NULL,
    origin_server_ts BIGINT NOT NULL,
    content JSONB DEFAULT '{}',
    is_redacted BOOLEAN NOT NULL DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    UNIQUE(event_id, relation_type, sender)
);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_event_relations_room_event ON event_relations(room_id, relates_to_event_id, relation_type);
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_event_relations_sender ON event_relations(sender, relation_type);
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_event_relations_origin_ts ON event_relations(room_id, origin_server_ts DESC);

-- ============================================================================
-- 6. 添加 key_rotation_history 表 (如不存在)
-- ============================================================================
CREATE TABLE IF NOT EXISTS key_rotation_history (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    key_id TEXT NOT NULL,
    rotated_ts BIGINT NOT NULL,
    is_revoked BOOLEAN NOT NULL DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    UNIQUE(user_id, device_id, key_id)
);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_key_rotation_user_device ON key_rotation_history(user_id, device_id);
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_key_rotation_rotated_ts ON key_rotation_history(rotated_ts DESC);

-- ============================================================================
-- 7. 创建 federation_signing_keys 表 (密钥轮转)
-- ============================================================================
CREATE TABLE IF NOT EXISTS federation_signing_keys (
    server_name TEXT NOT NULL,
    key_id TEXT NOT NULL,
    secret_key TEXT NOT NULL,
    public_key TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    expires_at BIGINT NOT NULL,
    key_json TEXT,
    ts_added_ms BIGINT NOT NULL,
    ts_valid_until_ms BIGINT NOT NULL,
    PRIMARY KEY (server_name, key_id)
);

-- ============================================================================
-- 完成
-- ============================================================================
DO $$
BEGIN
    RAISE NOTICE '数据库修复迁移完成!';
END $$;