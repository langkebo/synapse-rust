-- ============================================================================
-- P2 扩展功能迁移脚本
-- 执行日期: 2026-03-27
-- ============================================================================

BEGIN;

-- 1. 创建 space_members 表
CREATE TABLE IF NOT EXISTS space_members (
    id BIGSERIAL PRIMARY KEY,
    space_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    membership TEXT NOT NULL DEFAULT 'join',
    joined_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    left_ts BIGINT,
    inviter TEXT,
    CONSTRAINT uq_space_members_space_user UNIQUE (space_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_space_members_space ON space_members(space_id);
CREATE INDEX IF NOT EXISTS idx_space_members_user ON space_members(user_id);
CREATE INDEX IF NOT EXISTS idx_space_members_membership ON space_members(membership);

-- 2. 创建 space_summaries 表
CREATE TABLE IF NOT EXISTS space_summaries (
    id BIGSERIAL PRIMARY KEY,
    space_id TEXT NOT NULL UNIQUE,
    summary JSONB DEFAULT '{}',
    children_count BIGINT DEFAULT 0,
    member_count BIGINT DEFAULT 0,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT fk_space_summary_space FOREIGN KEY (space_id) REFERENCES spaces(space_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_space_summary_space ON space_summaries(space_id);

-- 3. 添加 key_backups 列
ALTER TABLE key_backups ADD COLUMN IF NOT EXISTS backup_data JSONB DEFAULT '{}';
ALTER TABLE key_backups ADD COLUMN IF NOT EXISTS etag TEXT DEFAULT '';

-- 4. 创建 media 表
CREATE TABLE IF NOT EXISTS media (
    media_id TEXT NOT NULL PRIMARY KEY,
    media_type TEXT,
    upload_name TEXT,
    created_ts BIGINT NOT NULL,
    last_access_ts BIGINT,
    media_length BIGINT DEFAULT 0,
    user_id TEXT NOT NULL,
    quarantined BOOLEAN DEFAULT FALSE,
    safe_from_deletion BOOLEAN DEFAULT FALSE
);

CREATE INDEX IF NOT EXISTS idx_media_user ON media(user_id);
CREATE INDEX IF NOT EXISTS idx_media_created ON media(created_ts DESC);

-- 5. 添加 registration_tokens 列
ALTER TABLE registration_tokens ADD COLUMN IF NOT EXISTS uses_allowed BIGINT;
ALTER TABLE registration_tokens ADD COLUMN IF NOT EXISTS used_count BIGINT DEFAULT 0;
ALTER TABLE registration_tokens ADD COLUMN IF NOT EXISTS pending BIGINT DEFAULT 0;
ALTER TABLE registration_tokens ADD COLUMN IF NOT EXISTS completed BIGINT DEFAULT 0;
ALTER TABLE registration_tokens ADD COLUMN IF NOT EXISTS expiry_time BIGINT DEFAULT 0;

-- 6. 修复 spaces 表 - 添加缺失的列
ALTER TABLE spaces ADD COLUMN IF NOT EXISTS room_id TEXT;
ALTER TABLE spaces ADD COLUMN IF NOT EXISTS join_rule TEXT DEFAULT 'invite';
ALTER TABLE spaces ADD COLUMN IF NOT EXISTS visibility TEXT DEFAULT 'private';
ALTER TABLE spaces ADD COLUMN IF NOT EXISTS parent_space_id TEXT;
ALTER TABLE spaces ADD COLUMN IF NOT EXISTS creation_ts BIGINT DEFAULT 0;

-- 7. 修复 federation_blacklist 表 - 添加 added_at 列
ALTER TABLE federation_blacklist ADD COLUMN IF NOT EXISTS added_at BIGINT DEFAULT 0;

-- 8. 修复 user_threepids 表 - 添加 added_at 列
ALTER TABLE user_threepids ADD COLUMN IF NOT EXISTS added_at BIGINT DEFAULT 0;

-- 9. 创建 sliding_sync 相关表
BEGIN;

CREATE TABLE IF NOT EXISTS sliding_sync_lists (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    conn_id TEXT,
    list_key TEXT NOT NULL,
    sort JSONB DEFAULT '[]',
    filters JSONB DEFAULT '{}',
    room_subscription JSONB DEFAULT '{}',
    ranges JSONB DEFAULT '[]',
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_sliding_sync_lists_unique ON sliding_sync_lists(user_id, device_id, COALESCE(conn_id, ''), list_key);
CREATE INDEX IF NOT EXISTS idx_sliding_sync_lists_user_device ON sliding_sync_lists(user_id, device_id);

CREATE TABLE IF NOT EXISTS sliding_sync_tokens (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    conn_id TEXT,
    token TEXT NOT NULL,
    pos BIGINT NOT NULL,
    created_ts BIGINT NOT NULL,
    expires_at BIGINT
);

CREATE INDEX IF NOT EXISTS idx_sliding_sync_tokens_user ON sliding_sync_tokens(user_id, device_id);

CREATE TABLE IF NOT EXISTS sliding_sync_rooms (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    conn_id TEXT,
    list_key TEXT,
    bump_stamp BIGINT NOT NULL,
    highlight_count INTEGER DEFAULT 0,
    notification_count INTEGER DEFAULT 0,
    is_dm BOOLEAN DEFAULT FALSE,
    is_encrypted BOOLEAN DEFAULT FALSE,
    is_tombstoned BOOLEAN DEFAULT FALSE,
    invited BOOLEAN DEFAULT FALSE,
    name TEXT,
    avatar TEXT,
    timestamp BIGINT NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_sliding_sync_rooms_unique ON sliding_sync_rooms(user_id, device_id, room_id, COALESCE(conn_id, ''));
CREATE INDEX IF NOT EXISTS idx_sliding_sync_rooms_user_device ON sliding_sync_rooms(user_id, device_id);
CREATE INDEX IF NOT EXISTS idx_sliding_sync_rooms_bump_stamp ON sliding_sync_rooms(bump_stamp DESC);

COMMIT;

-- 记录迁移
INSERT INTO schema_migrations (version, description, applied_ts)
VALUES ('20260327_p2_fixes', 'P2 fixes: space_members, space_summaries, media, key_backups backup_data/etag, registration_tokens uses_allowed', EXTRACT(EPOCH FROM NOW())::BIGINT * 1000)
ON CONFLICT (version) DO NOTHING;

COMMIT;