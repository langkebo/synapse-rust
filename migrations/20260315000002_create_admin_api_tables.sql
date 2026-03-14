-- Admin API 所需表
-- 创建日期: 2026-03-15
-- 更新日期: 2026-03-14 (字段命名规范化)

-- 1. 影子封禁表
CREATE TABLE IF NOT EXISTS shadow_bans (
    user_id TEXT PRIMARY KEY,
    banned_at BIGINT NOT NULL
);

-- 2. 速率限制表
CREATE TABLE IF NOT EXISTS rate_limits (
    user_id TEXT PRIMARY KEY,
    messages_per_second DOUBLE PRECISION DEFAULT 5.0,
    burst_count INTEGER DEFAULT 10,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT
);

-- 3. 服务器通知表
CREATE TABLE IF NOT EXISTS server_notices (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    content JSONB,
    sent_ts BIGINT NOT NULL,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT
);

CREATE INDEX IF NOT EXISTS idx_server_notices_user ON server_notices(user_id);

-- 4. 用户通知设置表
CREATE TABLE IF NOT EXISTS user_notification_settings (
    user_id TEXT PRIMARY KEY,
    is_enabled BOOLEAN DEFAULT TRUE,
    updated_ts BIGINT
);

-- 5. 封禁房间表
CREATE TABLE IF NOT EXISTS blocked_rooms (
    room_id TEXT PRIMARY KEY,
    blocked_at BIGINT NOT NULL,
    blocked_by TEXT NOT NULL,
    reason TEXT
);

-- 6. 联邦目标表
CREATE TABLE IF NOT EXISTS federation_destinations (
    destination TEXT PRIMARY KEY,
    retry_last_ts BIGINT,
    retry_interval BIGINT,
    failure_ts BIGINT,
    last_successful_ts BIGINT,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT
);

-- 7. 联邦房间表
CREATE TABLE IF NOT EXISTS federation_rooms (
    destination TEXT NOT NULL,
    room_id TEXT NOT NULL,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    PRIMARY KEY (destination, room_id)
);

CREATE INDEX IF NOT EXISTS idx_federation_rooms_room ON federation_rooms(room_id);

-- 8. 联邦缓存表
CREATE TABLE IF NOT EXISTS federation_cache (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    expires_at BIGINT,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT
);

CREATE INDEX IF NOT EXISTS idx_federation_cache_expiry ON federation_cache(expires_at) WHERE expires_at IS NOT NULL;

-- 9. 联邦黑名单表
CREATE TABLE IF NOT EXISTS federation_blacklist (
    server_name TEXT PRIMARY KEY,
    added_at BIGINT NOT NULL,
    reason TEXT,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT
);

-- 10. 服务器保留策略表
CREATE TABLE IF NOT EXISTS server_retention_policy (
    id INTEGER PRIMARY KEY DEFAULT 1,
    max_lifetime BIGINT,
    min_lifetime BIGINT,
    updated_ts BIGINT
);

-- 11. 房间保留策略表
CREATE TABLE IF NOT EXISTS room_retention_policy (
    room_id TEXT PRIMARY KEY,
    max_lifetime BIGINT,
    min_lifetime BIGINT,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    updated_ts BIGINT
);

-- 12. 媒体配额表
CREATE TABLE IF NOT EXISTS user_media_quota (
    user_id TEXT PRIMARY KEY,
    media_size_limit BIGINT,
    media_count_limit INTEGER,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    updated_ts BIGINT
);
