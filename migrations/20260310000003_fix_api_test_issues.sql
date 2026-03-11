-- ============================================================================
-- synapse-rust API测试问题修复迁移
-- 创建日期: 2026-03-10
-- 说明: 修复API测试发现的数据库字段和类型问题
-- ============================================================================

-- ============================================================================
-- 问题1: member_count 类型不匹配 (INTEGER -> BIGINT)
-- ============================================================================

-- 修改 rooms 表的 member_count 列类型
ALTER TABLE rooms ALTER COLUMN member_count TYPE BIGINT;

-- ============================================================================
-- 问题2: user_threepids 表 validated_at 字段
-- ============================================================================

-- 检查并添加 validated_at 列（如果不存在）
DO $$ 
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'user_threepids' AND column_name = 'validated_at'
    ) THEN
        ALTER TABLE user_threepids ADD COLUMN validated_at BIGINT;
    END IF;
END $$;

-- ============================================================================
-- 问题3: device_keys 表 ts_updated_ms 字段
-- ============================================================================

-- 检查并添加 ts_updated_ms 列（如果不存在）
DO $$ 
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'device_keys' AND column_name = 'ts_updated_ms'
    ) THEN
        ALTER TABLE device_keys ADD COLUMN ts_updated_ms BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT;
    END IF;
END $$;

-- 检查并添加 ts_added_ms 列（如果不存在）
DO $$ 
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'device_keys' AND column_name = 'ts_added_ms'
    ) THEN
        ALTER TABLE device_keys ADD COLUMN ts_added_ms BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT;
    END IF;
END $$;

-- ============================================================================
-- 问题4: key_backups 表 backup_id 字段
-- ============================================================================

-- 检查并添加 backup_id 列（如果不存在）
DO $$ 
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'key_backups' AND column_name = 'backup_id'
    ) THEN
        ALTER TABLE key_backups ADD COLUMN backup_id VARCHAR(255);
        -- 创建唯一索引
        CREATE UNIQUE INDEX IF NOT EXISTS uq_key_backups_backup_id ON key_backups(backup_id) WHERE backup_id IS NOT NULL;
    END IF;
END $$;

-- 检查并添加其他 key_backups 缺失字段
DO $$ 
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'key_backups' AND column_name = 'auth_key'
    ) THEN
        ALTER TABLE key_backups ADD COLUMN auth_key TEXT;
    END IF;
END $$;

DO $$ 
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'key_backups' AND column_name = 'mgmt_key'
    ) THEN
        ALTER TABLE key_backups ADD COLUMN mgmt_key TEXT;
    END IF;
END $$;

DO $$ 
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'key_backups' AND column_name = 'backup_data'
    ) THEN
        ALTER TABLE key_backups ADD COLUMN backup_data JSONB;
    END IF;
END $$;

DO $$ 
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'key_backups' AND column_name = 'etag'
    ) THEN
        ALTER TABLE key_backups ADD COLUMN etag TEXT;
    END IF;
END $$;

-- ============================================================================
-- 问题5: 创建 search_index 表（如果不存在）
-- ============================================================================

CREATE TABLE IF NOT EXISTS search_index (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    sender TEXT NOT NULL,
    event_type TEXT NOT NULL,
    content TEXT,
    content_vector tsvector,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    CONSTRAINT uq_search_index_event UNIQUE (event_id)
);

CREATE INDEX IF NOT EXISTS idx_search_index_room ON search_index(room_id);
CREATE INDEX IF NOT EXISTS idx_search_index_sender ON search_index(sender);
CREATE INDEX IF NOT EXISTS idx_search_index_type ON search_index(event_type);
CREATE INDEX IF NOT EXISTS idx_search_index_content ON search_index USING GIN(content_vector);

-- ============================================================================
-- 问题6: 创建 spaces 表（如果不存在）
-- ============================================================================

CREATE TABLE IF NOT EXISTS spaces (
    id BIGSERIAL PRIMARY KEY,
    space_id TEXT NOT NULL UNIQUE,
    name TEXT,
    topic TEXT,
    avatar_url TEXT,
    creator TEXT NOT NULL,
    is_public BOOLEAN DEFAULT FALSE,
    is_federated BOOLEAN DEFAULT TRUE,
    room_version TEXT DEFAULT '6',
    join_rules TEXT DEFAULT 'invite',
    history_visibility TEXT DEFAULT 'shared',
    member_count BIGINT DEFAULT 0,
    children_count BIGINT DEFAULT 0,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    CONSTRAINT fk_spaces_creator FOREIGN KEY (creator) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_spaces_creator ON spaces(creator);
CREATE INDEX IF NOT EXISTS idx_spaces_is_public ON spaces(is_public) WHERE is_public = TRUE;
CREATE INDEX IF NOT EXISTS idx_spaces_name ON spaces(name);

-- ============================================================================
-- 问题7: 创建 backup_keys 表（如果不存在）
-- ============================================================================

CREATE TABLE IF NOT EXISTS backup_keys (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    backup_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    first_message_index BIGINT DEFAULT 0,
    forwarded_count BIGINT DEFAULT 0,
    is_verified BOOLEAN DEFAULT FALSE,
    backup_data JSONB NOT NULL,
    created_ts BIGINT NOT NULL,
    CONSTRAINT uq_backup_keys_unique UNIQUE (user_id, backup_id, room_id, session_id, first_message_index)
);

CREATE INDEX IF NOT EXISTS idx_backup_keys_user ON backup_keys(user_id);
CREATE INDEX IF NOT EXISTS idx_backup_keys_backup ON backup_keys(backup_id);
CREATE INDEX IF NOT EXISTS idx_backup_keys_room ON backup_keys(room_id);

-- ============================================================================
-- 问题8: 创建 space_summaries 表（如果不存在）
-- ============================================================================

CREATE TABLE IF NOT EXISTS space_summaries (
    id BIGSERIAL PRIMARY KEY,
    space_id TEXT NOT NULL UNIQUE,
    summary JSONB NOT NULL,
    children_count BIGINT DEFAULT 0,
    member_count BIGINT DEFAULT 0,
    updated_ts BIGINT NOT NULL,
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_space_summaries_space ON space_summaries(space_id);

-- ============================================================================
-- 问题9: 创建 space_statistics 表（如果不存在）
-- ============================================================================

CREATE TABLE IF NOT EXISTS space_statistics (
    id BIGSERIAL PRIMARY KEY,
    space_id TEXT NOT NULL UNIQUE,
    total_rooms BIGINT DEFAULT 0,
    total_members BIGINT DEFAULT 0,
    active_rooms BIGINT DEFAULT 0,
    active_members BIGINT DEFAULT 0,
    member_count BIGINT DEFAULT 0,
    children_count BIGINT DEFAULT 0,
    updated_ts BIGINT NOT NULL,
    created_ts BIGINT NOT NULL,
    CONSTRAINT fk_space_statistics_space FOREIGN KEY (space_id) REFERENCES spaces(space_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_space_statistics_space ON space_statistics(space_id);

-- ============================================================================
-- 完成提示
-- ============================================================================

-- 验证修复
DO $$
DECLARE
    v_count INTEGER;
BEGIN
    -- 验证 rooms.member_count 类型
    SELECT COUNT(*) INTO v_count FROM information_schema.columns 
    WHERE table_name = 'rooms' AND column_name = 'member_count' AND data_type = 'bigint';
    IF v_count = 0 THEN
        RAISE NOTICE 'WARNING: rooms.member_count is not BIGINT type';
    ELSE
        RAISE NOTICE 'OK: rooms.member_count is BIGINT type';
    END IF;
    
    -- 验证 user_threepids.validated_at
    SELECT COUNT(*) INTO v_count FROM information_schema.columns 
    WHERE table_name = 'user_threepids' AND column_name = 'validated_at';
    IF v_count = 0 THEN
        RAISE NOTICE 'WARNING: user_threepids.validated_at does not exist';
    ELSE
        RAISE NOTICE 'OK: user_threepids.validated_at exists';
    END IF;
    
    -- 验证 device_keys.ts_updated_ms
    SELECT COUNT(*) INTO v_count FROM information_schema.columns 
    WHERE table_name = 'device_keys' AND column_name = 'ts_updated_ms';
    IF v_count = 0 THEN
        RAISE NOTICE 'WARNING: device_keys.ts_updated_ms does not exist';
    ELSE
        RAISE NOTICE 'OK: device_keys.ts_updated_ms exists';
    END IF;
    
    -- 验证 key_backups.backup_id
    SELECT COUNT(*) INTO v_count FROM information_schema.columns 
    WHERE table_name = 'key_backups' AND column_name = 'backup_id';
    IF v_count = 0 THEN
        RAISE NOTICE 'WARNING: key_backups.backup_id does not exist';
    ELSE
        RAISE NOTICE 'OK: key_backups.backup_id exists';
    END IF;
    
    -- 验证 spaces 表
    SELECT COUNT(*) INTO v_count FROM information_schema.tables WHERE table_name = 'spaces';
    IF v_count = 0 THEN
        RAISE NOTICE 'WARNING: spaces table does not exist';
    ELSE
        RAISE NOTICE 'OK: spaces table exists';
    END IF;
    
    -- 验证 search_index 表
    SELECT COUNT(*) INTO v_count FROM information_schema.tables WHERE table_name = 'search_index';
    IF v_count = 0 THEN
        RAISE NOTICE 'WARNING: search_index table does not exist';
    ELSE
        RAISE NOTICE 'OK: search_index table exists';
    END IF;
    
    RAISE NOTICE 'Migration completed successfully!';
END $$;
