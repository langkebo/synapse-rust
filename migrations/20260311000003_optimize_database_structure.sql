-- ============================================================================
-- synapse-rust 数据库优化迁移脚本
-- 创建日期: 2026-03-11
-- 说明: 修复表结构与代码不匹配的问题，遵循 DATABASE_FIELD_STANDARDS.md 规范
-- 版本: v6.0.4
-- ============================================================================

-- ============================================================================
-- 1. background_updates 表优化
-- 字段规范: 时间戳使用 _ts 后缀
-- ============================================================================

ALTER TABLE background_updates ADD COLUMN IF NOT EXISTS job_name TEXT;
ALTER TABLE background_updates ADD COLUMN IF NOT EXISTS job_type TEXT;
ALTER TABLE background_updates ADD COLUMN IF NOT EXISTS description TEXT;
ALTER TABLE background_updates ADD COLUMN IF NOT EXISTS table_name TEXT;
ALTER TABLE background_updates ADD COLUMN IF NOT EXISTS column_name TEXT;
ALTER TABLE background_updates ADD COLUMN IF NOT EXISTS status TEXT DEFAULT 'pending';
ALTER TABLE background_updates ADD COLUMN IF NOT EXISTS total_items INTEGER DEFAULT 0;
ALTER TABLE background_updates ADD COLUMN IF NOT EXISTS processed_items INTEGER DEFAULT 0;
ALTER TABLE background_updates ADD COLUMN IF NOT EXISTS created_ts BIGINT;
ALTER TABLE background_updates ADD COLUMN IF NOT EXISTS last_updated_ts BIGINT;
ALTER TABLE background_updates ADD COLUMN IF NOT EXISTS retry_count INTEGER DEFAULT 0;
ALTER TABLE background_updates ADD COLUMN IF NOT EXISTS max_retries INTEGER DEFAULT 3;
ALTER TABLE background_updates ADD COLUMN IF NOT EXISTS batch_size INTEGER DEFAULT 100;
ALTER TABLE background_updates ADD COLUMN IF NOT EXISTS sleep_ms INTEGER DEFAULT 100;
ALTER TABLE background_updates ADD COLUMN IF NOT EXISTS depends_on JSONB DEFAULT '[]';
ALTER TABLE background_updates ADD COLUMN IF NOT EXISTS metadata JSONB DEFAULT '{}';

-- 重命名时间戳字段以符合规范
ALTER TABLE background_updates RENAME COLUMN started_at TO started_ts;
ALTER TABLE background_updates RENAME COLUMN completed_at TO completed_ts;

-- 更新现有记录的 created_ts
UPDATE background_updates SET created_ts = EXTRACT(EPOCH FROM NOW()) * 1000 WHERE created_ts IS NULL;

CREATE INDEX IF NOT EXISTS idx_background_updates_status ON background_updates(status);
CREATE INDEX IF NOT EXISTS idx_background_updates_running ON background_updates(is_running) WHERE is_running = TRUE;

-- ============================================================================
-- 2. background_update_stats 表
-- ============================================================================

CREATE TABLE IF NOT EXISTS background_update_stats (
    id BIGSERIAL,
    job_name TEXT NOT NULL,
    total_updates INTEGER DEFAULT 0,
    completed_updates INTEGER DEFAULT 0,
    failed_updates INTEGER DEFAULT 0,
    last_run_ts BIGINT,
    next_run_ts BIGINT,
    average_duration_ms BIGINT DEFAULT 0,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT pk_background_update_stats PRIMARY KEY (id),
    CONSTRAINT uq_background_update_stats_job UNIQUE (job_name)
);

-- ============================================================================
-- 3. workers 表优化
-- 字段规范: 时间戳使用 _ts 后缀，布尔字段使用 is_ 前缀
-- ============================================================================

-- 确保 workers 表存在
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

ALTER TABLE workers ADD COLUMN IF NOT EXISTS is_enabled BOOLEAN DEFAULT TRUE;

CREATE INDEX IF NOT EXISTS idx_workers_type ON workers(worker_type);
CREATE INDEX IF NOT EXISTS idx_workers_status ON workers(status);
CREATE INDEX IF NOT EXISTS idx_workers_heartbeat ON workers(last_heartbeat_ts) WHERE last_heartbeat_ts IS NOT NULL;

-- ============================================================================
-- 4. worker_commands 表
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
    CONSTRAINT uq_worker_commands_id UNIQUE (command_id)
);

CREATE INDEX IF NOT EXISTS idx_worker_commands_target ON worker_commands(target_worker_id);
CREATE INDEX IF NOT EXISTS idx_worker_commands_status ON worker_commands(status);

-- ============================================================================
-- 5. worker_events 表
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
-- 6. worker_statistics 表
-- ============================================================================

CREATE TABLE IF NOT EXISTS worker_statistics (
    id BIGSERIAL,
    worker_id TEXT NOT NULL,
    total_messages_sent BIGINT DEFAULT 0,
    total_messages_received BIGINT DEFAULT 0,
    total_errors BIGINT DEFAULT 0,
    last_message_ts BIGINT,
    last_error_ts BIGINT,
    avg_processing_time_ms BIGINT,
    uptime_seconds BIGINT DEFAULT 0,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT pk_worker_statistics PRIMARY KEY (id)
);

-- ============================================================================
-- 7. active_workers 视图
-- ============================================================================

CREATE OR REPLACE VIEW active_workers AS
SELECT id, worker_id, worker_name, worker_type, host, port, status, 
       last_heartbeat_ts, started_ts, stopped_ts, config, metadata, version, is_enabled
FROM workers
WHERE status = 'running' OR status = 'starting';

-- ============================================================================
-- 8. retention 相关表
-- 字段规范: 时间戳使用 _ts 后缀
-- ============================================================================

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

CREATE TABLE IF NOT EXISTS server_retention_policy (
    id BIGSERIAL,
    max_lifetime BIGINT,
    min_lifetime BIGINT NOT NULL DEFAULT 0,
    expire_on_clients BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT pk_server_retention_policy PRIMARY KEY (id)
);

INSERT INTO server_retention_policy (max_lifetime, min_lifetime, expire_on_clients, created_ts, updated_ts)
SELECT NULL, 0, FALSE, EXTRACT(EPOCH FROM NOW()) * 1000, EXTRACT(EPOCH FROM NOW()) * 1000
WHERE NOT EXISTS (SELECT 1 FROM server_retention_policy);

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
-- 9. thread 相关表
-- 字段规范: 时间戳使用 _ts 后缀，布尔字段使用 is_ 前缀
-- ============================================================================

-- 确保 thread_roots 表结构正确
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

CREATE TABLE IF NOT EXISTS thread_summaries (
    id BIGSERIAL,
    room_id TEXT NOT NULL,
    thread_id TEXT NOT NULL,
    root_event_id TEXT NOT NULL,
    root_sender TEXT NOT NULL,
    root_content JSONB DEFAULT '{}',
    root_origin_server_ts BIGINT,
    latest_event_id TEXT,
    latest_sender TEXT,
    latest_content JSONB,
    latest_origin_server_ts BIGINT,
    reply_count INTEGER DEFAULT 0,
    participants JSONB DEFAULT '[]',
    is_frozen BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT pk_thread_summaries PRIMARY KEY (id),
    CONSTRAINT uq_thread_summaries UNIQUE (room_id, thread_id)
);

CREATE INDEX IF NOT EXISTS idx_thread_summaries_room ON thread_summaries(room_id);

CREATE TABLE IF NOT EXISTS thread_statistics (
    id BIGSERIAL,
    room_id TEXT NOT NULL,
    thread_id TEXT NOT NULL,
    total_replies INTEGER DEFAULT 0,
    total_participants INTEGER DEFAULT 0,
    total_edits INTEGER DEFAULT 0,
    total_redactions INTEGER DEFAULT 0,
    first_reply_ts BIGINT,
    last_reply_ts BIGINT,
    avg_reply_time_ms BIGINT,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT pk_thread_statistics PRIMARY KEY (id)
);

-- ============================================================================
-- 10. captcha_send_log 表
-- 字段规范: 时间戳使用 _ts 后缀，布尔字段使用 is_ 前缀
-- ============================================================================

CREATE TABLE IF NOT EXISTS captcha_send_log (
    id BIGSERIAL,
    target TEXT NOT NULL,
    captcha_type TEXT NOT NULL,
    captcha_code TEXT NOT NULL,
    sent_ts BIGINT NOT NULL,
    expires_ts BIGINT,
    is_used BOOLEAN DEFAULT FALSE,
    used_ts BIGINT,
    ip_address TEXT,
    user_agent TEXT,
    captcha_id TEXT,
    is_success BOOLEAN,
    error_message TEXT,
    provider TEXT,
    provider_response TEXT,
    CONSTRAINT pk_captcha_send_log PRIMARY KEY (id)
);

CREATE INDEX IF NOT EXISTS idx_captcha_send_log_target ON captcha_send_log(target);
CREATE INDEX IF NOT EXISTS idx_captcha_send_log_expires ON captcha_send_log(expires_ts) WHERE expires_ts IS NOT NULL;

-- ============================================================================
-- 11. captcha_template 表
-- ============================================================================

CREATE TABLE IF NOT EXISTS captcha_template (
    id BIGSERIAL,
    template_name TEXT NOT NULL,
    captcha_type TEXT NOT NULL,
    subject TEXT,
    content TEXT NOT NULL,
    variables JSONB DEFAULT '{}',
    is_default BOOLEAN DEFAULT FALSE,
    is_enabled BOOLEAN DEFAULT TRUE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT pk_captcha_template PRIMARY KEY (id),
    CONSTRAINT uq_captcha_template_name UNIQUE (template_name)
);

INSERT INTO captcha_template (template_name, captcha_type, subject, content, variables, is_default, is_enabled, created_ts, updated_ts)
SELECT 'default_email', 'email', '您的验证码', '您的验证码是：{code}，有效期{expire_minutes}分钟。', '{"code": "", "expire_minutes": 5}', TRUE, TRUE, EXTRACT(EPOCH FROM NOW()) * 1000, EXTRACT(EPOCH FROM NOW()) * 1000
WHERE NOT EXISTS (SELECT 1 FROM captcha_template WHERE template_name = 'default_email');

INSERT INTO captcha_template (template_name, captcha_type, subject, content, variables, is_default, is_enabled, created_ts, updated_ts)
SELECT 'default_sms', 'sms', NULL, '您的验证码是：{code}，有效期{expire_minutes}分钟。', '{"code": "", "expire_minutes": 5}', TRUE, TRUE, EXTRACT(EPOCH FROM NOW()) * 1000, EXTRACT(EPOCH FROM NOW()) * 1000
WHERE NOT EXISTS (SELECT 1 FROM captcha_template WHERE template_name = 'default_sms');

-- ============================================================================
-- 12. media_quota 相关表
-- 字段规范: 时间戳使用 _ts 后缀
-- ============================================================================

CREATE TABLE IF NOT EXISTS media_quota_configs (
    id BIGSERIAL,
    name TEXT NOT NULL,
    description TEXT,
    max_storage_bytes BIGINT,
    max_file_size_bytes BIGINT,
    max_files_count INTEGER,
    allowed_mime_types JSONB,
    blocked_mime_types JSONB,
    is_default BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT pk_media_quota_configs PRIMARY KEY (id)
);

CREATE TABLE IF NOT EXISTS user_media_quotas (
    id BIGSERIAL,
    user_id TEXT NOT NULL,
    quota_config_id BIGINT,
    custom_max_storage_bytes BIGINT,
    custom_max_file_size_bytes BIGINT,
    custom_max_files_count INTEGER,
    current_storage_bytes BIGINT DEFAULT 0,
    current_files_count INTEGER DEFAULT 0,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT pk_user_media_quotas PRIMARY KEY (id),
    CONSTRAINT uq_user_media_quotas_user UNIQUE (user_id)
);

CREATE TABLE IF NOT EXISTS server_media_quota (
    id BIGSERIAL,
    max_storage_bytes BIGINT,
    max_file_size_bytes BIGINT,
    max_files_count INTEGER,
    current_storage_bytes BIGINT DEFAULT 0,
    current_files_count INTEGER DEFAULT 0,
    alert_threshold_percent INTEGER DEFAULT 80,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT pk_server_media_quota PRIMARY KEY (id)
);

INSERT INTO server_media_quota (max_storage_bytes, max_file_size_bytes, max_files_count, created_ts, updated_ts)
SELECT NULL, NULL, NULL, EXTRACT(EPOCH FROM NOW()) * 1000, EXTRACT(EPOCH FROM NOW()) * 1000
WHERE NOT EXISTS (SELECT 1 FROM server_media_quota);

-- ============================================================================
-- 13. event_report_stats 表优化
-- 字段规范: 时间戳使用 _ts 后缀
-- ============================================================================

ALTER TABLE event_report_stats ADD COLUMN IF NOT EXISTS date DATE;
ALTER TABLE event_report_stats ADD COLUMN IF NOT EXISTS avg_resolution_time_hours FLOAT;

UPDATE event_report_stats SET date = stat_date WHERE date IS NULL;

-- ============================================================================
-- 14. 插入迁移记录
-- ============================================================================

INSERT INTO schema_migrations (version, name, applied_ts, description)
VALUES (
    'v6.0.4', 
    'optimize_database_structure', 
    EXTRACT(EPOCH FROM NOW()) * 1000, 
    'Optimize database structure: background_updates, workers, retention, threads, captcha, media_quota, event_reports - following DATABASE_FIELD_STANDARDS.md'
) ON CONFLICT (version) DO NOTHING;
