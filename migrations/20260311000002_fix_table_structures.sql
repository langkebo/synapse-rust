-- ============================================================================
-- 修复数据库表结构以匹配代码实现
-- 创建日期: 2026-03-11
-- 说明: 修复 workers, retention, thread 相关表结构
-- 参考: DATABASE_FIELD_STANDARDS.md
-- ============================================================================

-- ============================================================================
-- 1. 修复 workers 表结构
-- 字段规范: 时间戳使用 _ts 后缀，布尔字段使用 is_ 前缀
-- ============================================================================

DROP TABLE IF EXISTS workers CASCADE;

CREATE TABLE IF NOT EXISTS workers (
    id BIGSERIAL,
    worker_id TEXT NOT NULL,
    worker_name TEXT NOT NULL,
    worker_type TEXT NOT NULL,
    host TEXT NOT NULL DEFAULT 'localhost',
    port INTEGER NOT NULL DEFAULT 8080,
    status TEXT NOT NULL DEFAULT 'starting',
    last_heartbeat_ts BIGINT,
    started_ts BIGINT NOT NULL,
    stopped_ts BIGINT,
    config JSONB DEFAULT '{}',
    metadata JSONB DEFAULT '{}',
    version TEXT,
    is_enabled BOOLEAN DEFAULT TRUE,
    CONSTRAINT pk_workers PRIMARY KEY (id),
    CONSTRAINT uq_workers_id UNIQUE (worker_id)
);

CREATE INDEX IF NOT EXISTS idx_workers_type ON workers(worker_type);
CREATE INDEX IF NOT EXISTS idx_workers_status ON workers(status);
CREATE INDEX IF NOT EXISTS idx_workers_heartbeat ON workers(last_heartbeat_ts) WHERE last_heartbeat_ts IS NOT NULL;

-- ============================================================================
-- 2. 创建 worker_commands 表
-- ============================================================================

CREATE TABLE IF NOT EXISTS worker_commands (
    id BIGSERIAL,
    command_id TEXT NOT NULL,
    target_worker_id TEXT NOT NULL,
    source_worker_id TEXT,
    command_type TEXT NOT NULL,
    command_data JSONB DEFAULT '{}',
    priority INTEGER DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'pending',
    created_ts BIGINT NOT NULL,
    sent_ts BIGINT,
    completed_ts BIGINT,
    error_message TEXT,
    retry_count INTEGER DEFAULT 0,
    max_retries INTEGER DEFAULT 3,
    CONSTRAINT pk_worker_commands PRIMARY KEY (id),
    CONSTRAINT uq_worker_commands_id UNIQUE (command_id),
    CONSTRAINT fk_worker_commands_target FOREIGN KEY (target_worker_id) REFERENCES workers(worker_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_worker_commands_target ON worker_commands(target_worker_id);
CREATE INDEX IF NOT EXISTS idx_worker_commands_status ON worker_commands(status);

-- ============================================================================
-- 3. 创建 worker_events 表
-- ============================================================================

CREATE TABLE IF NOT EXISTS worker_events (
    id BIGSERIAL,
    event_id TEXT NOT NULL,
    stream_id BIGINT NOT NULL,
    event_type TEXT NOT NULL,
    room_id TEXT,
    sender TEXT,
    event_data JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    processed_by JSONB DEFAULT '[]',
    CONSTRAINT pk_worker_events PRIMARY KEY (id),
    CONSTRAINT uq_worker_events_id UNIQUE (event_id)
);

CREATE INDEX IF NOT EXISTS idx_worker_events_stream ON worker_events(stream_id);
CREATE INDEX IF NOT EXISTS idx_worker_events_type ON worker_events(event_type);

-- ============================================================================
-- 4. 创建 retention 相关表
-- 字段规范: 时间戳使用 _ts 后缀
-- ============================================================================

-- 房间保留策略表
CREATE TABLE IF NOT EXISTS room_retention_policies (
    id BIGSERIAL,
    room_id TEXT NOT NULL,
    max_lifetime BIGINT,
    min_lifetime BIGINT NOT NULL DEFAULT 0,
    expire_on_clients BOOLEAN DEFAULT FALSE,
    is_server_default BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT pk_room_retention_policies PRIMARY KEY (id),
    CONSTRAINT uq_room_retention_policies_room UNIQUE (room_id)
);

CREATE INDEX IF NOT EXISTS idx_room_retention_policies_room ON room_retention_policies(room_id);

-- 服务器保留策略表
CREATE TABLE IF NOT EXISTS server_retention_policy (
    id BIGSERIAL,
    max_lifetime BIGINT,
    min_lifetime BIGINT NOT NULL DEFAULT 0,
    expire_on_clients BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT pk_server_retention_policy PRIMARY KEY (id)
);

-- 插入默认服务器策略
INSERT INTO server_retention_policy (max_lifetime, min_lifetime, expire_on_clients, created_ts, updated_ts)
SELECT NULL, 0, FALSE, EXTRACT(EPOCH FROM NOW()) * 1000, EXTRACT(EPOCH FROM NOW()) * 1000
WHERE NOT EXISTS (SELECT 1 FROM server_retention_policy);

-- 保留清理队列表
CREATE TABLE IF NOT EXISTS retention_cleanup_queue (
    id BIGSERIAL,
    room_id TEXT NOT NULL,
    event_id TEXT,
    event_type TEXT,
    origin_server_ts BIGINT NOT NULL,
    scheduled_ts BIGINT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    created_ts BIGINT NOT NULL,
    processed_ts BIGINT,
    error_message TEXT,
    retry_count INTEGER DEFAULT 0,
    CONSTRAINT pk_retention_cleanup_queue PRIMARY KEY (id)
);

CREATE INDEX IF NOT EXISTS idx_retention_cleanup_status ON retention_cleanup_queue(status);
CREATE INDEX IF NOT EXISTS idx_retention_cleanup_scheduled ON retention_cleanup_queue(scheduled_ts);

-- 保留清理日志表
CREATE TABLE IF NOT EXISTS retention_cleanup_logs (
    id BIGSERIAL,
    room_id TEXT NOT NULL,
    events_deleted BIGINT DEFAULT 0,
    state_events_deleted BIGINT DEFAULT 0,
    media_deleted BIGINT DEFAULT 0,
    bytes_freed BIGINT DEFAULT 0,
    started_ts BIGINT NOT NULL,
    completed_ts BIGINT,
    status TEXT NOT NULL DEFAULT 'running',
    error_message TEXT,
    CONSTRAINT pk_retention_cleanup_logs PRIMARY KEY (id)
);

CREATE INDEX IF NOT EXISTS idx_retention_cleanup_logs_room ON retention_cleanup_logs(room_id);

-- 保留统计表
CREATE TABLE IF NOT EXISTS retention_stats (
    id BIGSERIAL,
    room_id TEXT NOT NULL,
    total_events BIGINT DEFAULT 0,
    events_in_retention BIGINT DEFAULT 0,
    events_expired BIGINT DEFAULT 0,
    last_cleanup_ts BIGINT,
    next_cleanup_ts BIGINT,
    CONSTRAINT pk_retention_stats PRIMARY KEY (id),
    CONSTRAINT uq_retention_stats_room UNIQUE (room_id)
);

-- ============================================================================
-- 5. 修复 thread_roots 表结构
-- 字段规范: 时间戳使用 _ts 后缀，布尔字段使用 is_ 前缀
-- ============================================================================

-- 删除旧表并重新创建（如果存在）
DROP TABLE IF EXISTS thread_roots CASCADE;

CREATE TABLE IF NOT EXISTS thread_roots (
    id BIGSERIAL,
    room_id TEXT NOT NULL,
    root_event_id TEXT NOT NULL,
    sender TEXT NOT NULL,
    thread_id TEXT,
    reply_count BIGINT DEFAULT 0,
    last_reply_event_id TEXT,
    last_reply_sender TEXT,
    last_reply_ts BIGINT,
    participants JSONB DEFAULT '[]',
    is_fetched BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    CONSTRAINT pk_thread_roots PRIMARY KEY (id),
    CONSTRAINT uq_thread_roots_event UNIQUE (room_id, root_event_id)
);

CREATE INDEX IF NOT EXISTS idx_thread_roots_room ON thread_roots(room_id);
CREATE INDEX IF NOT EXISTS idx_thread_roots_thread ON thread_roots(thread_id) WHERE thread_id IS NOT NULL;

-- ============================================================================
-- 6. 创建 thread_replies 表
-- ============================================================================

CREATE TABLE IF NOT EXISTS thread_replies (
    id BIGSERIAL,
    room_id TEXT NOT NULL,
    thread_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    root_event_id TEXT NOT NULL,
    sender TEXT NOT NULL,
    in_reply_to_event_id TEXT,
    content JSONB DEFAULT '{}',
    origin_server_ts BIGINT NOT NULL,
    is_edited BOOLEAN DEFAULT FALSE,
    is_redacted BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_thread_replies PRIMARY KEY (id),
    CONSTRAINT uq_thread_replies_event UNIQUE (room_id, event_id)
);

CREATE INDEX IF NOT EXISTS idx_thread_replies_thread ON thread_replies(thread_id);
CREATE INDEX IF NOT EXISTS idx_thread_replies_root ON thread_replies(root_event_id);

-- ============================================================================
-- 7. 创建 thread_subscriptions 表
-- ============================================================================

CREATE TABLE IF NOT EXISTS thread_subscriptions (
    id BIGSERIAL,
    room_id TEXT NOT NULL,
    thread_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    notification_level TEXT NOT NULL DEFAULT 'all',
    is_muted BOOLEAN DEFAULT FALSE,
    subscribed_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT pk_thread_subscriptions PRIMARY KEY (id),
    CONSTRAINT uq_thread_subscriptions UNIQUE (room_id, thread_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_thread_subscriptions_user ON thread_subscriptions(user_id);

-- ============================================================================
-- 8. 创建 thread_read_receipts 表
-- ============================================================================

CREATE TABLE IF NOT EXISTS thread_read_receipts (
    id BIGSERIAL,
    room_id TEXT NOT NULL,
    thread_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    last_read_event_id TEXT,
    last_read_ts BIGINT NOT NULL,
    unread_count INTEGER DEFAULT 0,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT pk_thread_read_receipts PRIMARY KEY (id),
    CONSTRAINT uq_thread_read_receipts UNIQUE (room_id, thread_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_thread_read_receipts_user ON thread_read_receipts(user_id);

-- ============================================================================
-- 9. 创建 thread_relations 表
-- ============================================================================

CREATE TABLE IF NOT EXISTS thread_relations (
    id BIGSERIAL,
    room_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    relates_to_event_id TEXT NOT NULL,
    relation_type TEXT NOT NULL,
    thread_id TEXT,
    is_falling_back BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_thread_relations PRIMARY KEY (id),
    CONSTRAINT uq_thread_relations_event UNIQUE (room_id, event_id)
);

CREATE INDEX IF NOT EXISTS idx_thread_relations_relates_to ON thread_relations(relates_to_event_id);

-- ============================================================================
-- 10. 创建 captcha_send_log 表
-- ============================================================================

CREATE TABLE IF NOT EXISTS captcha_send_log (
    id BIGSERIAL,
    target TEXT NOT NULL,
    captcha_type TEXT NOT NULL,
    captcha_code TEXT NOT NULL,
    sent_ts BIGINT NOT NULL,
    expires_ts BIGINT NOT NULL,
    is_used BOOLEAN DEFAULT FALSE,
    used_ts BIGINT,
    ip_address TEXT,
    user_agent TEXT,
    CONSTRAINT pk_captcha_send_log PRIMARY KEY (id)
);

CREATE INDEX IF NOT EXISTS idx_captcha_send_log_target ON captcha_send_log(target);
CREATE INDEX IF NOT EXISTS idx_captcha_send_log_expires ON captcha_send_log(expires_ts);

-- 插入迁移记录
INSERT INTO schema_migrations (version, name, applied_ts, description)
VALUES (
    'v6.0.3', 
    'fix_table_structures', 
    EXTRACT(EPOCH FROM NOW()) * 1000, 
    'Fix workers, retention, thread, captcha table structures according to DATABASE_FIELD_STANDARDS.md'
) ON CONFLICT (version) DO NOTHING;
