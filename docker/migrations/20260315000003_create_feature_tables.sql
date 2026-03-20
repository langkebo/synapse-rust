-- 功能模块所需表（非重复部分）
-- 创建日期: 2026-03-15
-- 更新日期: 2026-03-14 (移除与 unified_migration_optimized.sql 重复的表)

-- 注意：以下表已在 20260313000000_unified_migration_optimized.sql 中定义，不再重复创建：
-- - call_sessions
-- - call_candidates
-- - beacon_info
-- - beacon_locations
-- - dehydrated_devices
-- - presence_subscriptions
-- - email_verification (类似 email_verification_tokens)

-- 1. MatrixRTC 会话表
CREATE TABLE IF NOT EXISTS matrixrtc_sessions (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    creator_user_id TEXT NOT NULL,
    session_data JSONB,
    is_active BOOLEAN DEFAULT TRUE,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    ended_at BIGINT
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_matrixrtc_sessions_room_session ON matrixrtc_sessions(room_id, session_id);

-- 2. MatrixRTC 成员表
CREATE TABLE IF NOT EXISTS matrixrtc_memberships (
    id BIGSERIAL PRIMARY KEY,
    session_id BIGINT NOT NULL REFERENCES matrixrtc_sessions(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    membership_data JSONB,
    is_active BOOLEAN DEFAULT TRUE,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    left_at BIGINT
);

CREATE INDEX IF NOT EXISTS idx_matrixrtc_memberships_session ON matrixrtc_memberships(session_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_matrixrtc_memberships_user_device ON matrixrtc_memberships(session_id, user_id, device_id);

-- 3. MatrixRTC 加密密钥表
CREATE TABLE IF NOT EXISTS matrixrtc_encryption_keys (
    id BIGSERIAL PRIMARY KEY,
    session_id BIGINT NOT NULL REFERENCES matrixrtc_sessions(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    key_data JSONB NOT NULL,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT
);

CREATE INDEX IF NOT EXISTS idx_matrixrtc_encryption_keys_session ON matrixrtc_encryption_keys(session_id);

-- 4. 邮箱验证令牌表
CREATE TABLE IF NOT EXISTS email_verification_tokens (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    email TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    expires_at BIGINT NOT NULL,
    is_used BOOLEAN DEFAULT FALSE
);

CREATE INDEX IF NOT EXISTS idx_email_verification_tokens_user ON email_verification_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_email_verification_tokens_email ON email_verification_tokens(email);

-- 5. 延迟事件表
CREATE TABLE IF NOT EXISTS delayed_events (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    sender TEXT NOT NULL,
    content JSONB NOT NULL,
    delay_ms BIGINT NOT NULL,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    scheduled_ts BIGINT NOT NULL,
    is_sent BOOLEAN DEFAULT FALSE,
    sent_at BIGINT
);

CREATE INDEX IF NOT EXISTS idx_delayed_events_room ON delayed_events(room_id);
CREATE INDEX IF NOT EXISTS idx_delayed_events_scheduled ON delayed_events(scheduled_ts) WHERE is_sent = FALSE;

-- 6. 数据库元数据表
CREATE TABLE IF NOT EXISTS db_metadata (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT
);

-- 7. 服务器媒体配额表
CREATE TABLE IF NOT EXISTS server_media_quota (
    id INTEGER PRIMARY KEY DEFAULT 1,
    total_size_limit BIGINT,
    total_count_limit BIGINT,
    used_size BIGINT DEFAULT 0,
    used_count BIGINT DEFAULT 0,
    updated_ts BIGINT
);

-- 8. 媒体使用日志表
CREATE TABLE IF NOT EXISTS media_usage_log (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    media_id TEXT NOT NULL,
    action TEXT NOT NULL,
    size BIGINT,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT
);

CREATE INDEX IF NOT EXISTS idx_media_usage_log_user ON media_usage_log(user_id);
CREATE INDEX IF NOT EXISTS idx_media_usage_log_created ON media_usage_log(created_ts);

-- 9. 媒体配额警告表
CREATE TABLE IF NOT EXISTS media_quota_alerts (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT,
    alert_type TEXT NOT NULL,
    threshold_value BIGINT NOT NULL,
    current_value BIGINT NOT NULL,
    is_resolved BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    resolved_at BIGINT
);

CREATE INDEX IF NOT EXISTS idx_media_quota_alerts_user ON media_quota_alerts(user_id);
