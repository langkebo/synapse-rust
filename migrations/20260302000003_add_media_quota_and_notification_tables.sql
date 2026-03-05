-- Migration: Add media quota and server notification tables
-- Version: 20260302000003
-- Description: 添加媒体配额和服务器通知相关的数据库表
-- Author: System
-- Date: 2026-03-02
-- Prerequisites: 20260302000002_add_retention_and_space_tables.sql

-- ============================================================================
-- 1. 媒体配额相关表
-- ============================================================================

-- 配额配置表
CREATE TABLE IF NOT EXISTS media_quota_config (
    id SERIAL PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    description TEXT,
    max_storage_bytes BIGINT NOT NULL DEFAULT 1073741824,
    max_file_size_bytes BIGINT NOT NULL DEFAULT 104857600,
    max_files_count INTEGER NOT NULL DEFAULT 1000,
    allowed_mime_types JSONB DEFAULT '[]'::jsonb,
    blocked_mime_types JSONB DEFAULT '[]'::jsonb,
    is_default BOOLEAN DEFAULT FALSE,
    is_enabled BOOLEAN DEFAULT TRUE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL
);

COMMENT ON TABLE media_quota_config IS '媒体配额配置表';
COMMENT ON COLUMN media_quota_config.max_storage_bytes IS '最大存储字节数，默认1GB';
COMMENT ON COLUMN media_quota_config.max_file_size_bytes IS '单文件最大字节数，默认100MB';

CREATE INDEX IF NOT EXISTS idx_media_quota_config_default ON media_quota_config(is_default) WHERE is_default = TRUE;
CREATE INDEX IF NOT EXISTS idx_media_quota_config_enabled ON media_quota_config(is_enabled) WHERE is_enabled = TRUE;

-- 用户媒体配额表
CREATE TABLE IF NOT EXISTS user_media_quota (
    id SERIAL PRIMARY KEY,
    user_id TEXT NOT NULL UNIQUE,
    quota_config_id INTEGER REFERENCES media_quota_config(id) ON DELETE SET NULL,
    custom_max_storage_bytes BIGINT,
    custom_max_file_size_bytes BIGINT,
    custom_max_files_count INTEGER,
    current_storage_bytes BIGINT DEFAULT 0,
    current_files_count INTEGER DEFAULT 0,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL
);

COMMENT ON TABLE user_media_quota IS '用户媒体配额表';

CREATE INDEX IF NOT EXISTS idx_user_media_quota_user ON user_media_quota(user_id);
CREATE INDEX IF NOT EXISTS idx_user_media_quota_config ON user_media_quota(quota_config_id);

-- 媒体使用日志表
CREATE TABLE IF NOT EXISTS media_usage_log (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    media_id TEXT NOT NULL,
    file_name TEXT,
    file_size_bytes BIGINT NOT NULL,
    mime_type TEXT,
    operation TEXT NOT NULL CHECK (operation IN ('upload', 'delete', 'copy', 'thumbnail')),
    timestamp BIGINT NOT NULL,
    room_id TEXT,
    details JSONB DEFAULT '{}'::jsonb
);

COMMENT ON TABLE media_usage_log IS '媒体使用日志表';

CREATE INDEX IF NOT EXISTS idx_media_usage_log_user ON media_usage_log(user_id);
CREATE INDEX IF NOT EXISTS idx_media_usage_log_timestamp ON media_usage_log(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_media_usage_log_operation ON media_usage_log(operation);
CREATE INDEX IF NOT EXISTS idx_media_usage_log_media ON media_usage_log(media_id);

-- 配额告警表
CREATE TABLE IF NOT EXISTS media_quota_alerts (
    id SERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    alert_type TEXT NOT NULL CHECK (alert_type IN ('storage_warning', 'storage_exceeded', 'file_size_exceeded', 'file_count_warning')),
    threshold_percent INTEGER NOT NULL,
    current_usage_bytes BIGINT NOT NULL,
    quota_limit_bytes BIGINT NOT NULL,
    message TEXT,
    is_read BOOLEAN DEFAULT FALSE,
    read_ts BIGINT,
    created_ts BIGINT NOT NULL
);

COMMENT ON TABLE media_quota_alerts IS '媒体配额告警表';

CREATE INDEX IF NOT EXISTS idx_media_quota_alerts_user ON media_quota_alerts(user_id);
CREATE INDEX IF NOT EXISTS idx_media_quota_alerts_unread ON media_quota_alerts(user_id, is_read) WHERE is_read = FALSE;
CREATE INDEX IF NOT EXISTS idx_media_quota_alerts_type ON media_quota_alerts(alert_type);
CREATE INDEX IF NOT EXISTS idx_media_quota_alerts_created ON media_quota_alerts(created_ts DESC);

-- 服务器媒体配额表
CREATE TABLE IF NOT EXISTS server_media_quota (
    id INTEGER PRIMARY KEY DEFAULT 1,
    max_storage_bytes BIGINT NOT NULL DEFAULT 1099511627776,
    max_file_size_bytes BIGINT NOT NULL DEFAULT 1073741824,
    max_files_count INTEGER NOT NULL DEFAULT 100000,
    current_storage_bytes BIGINT DEFAULT 0,
    current_files_count INTEGER DEFAULT 0,
    alert_threshold_percent INTEGER DEFAULT 90,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT server_media_quota_single_row CHECK (id = 1)
);

COMMENT ON TABLE server_media_quota IS '服务器媒体配额表（单行配置）';
COMMENT ON COLUMN server_media_quota.max_storage_bytes IS '服务器最大存储，默认1TB';

-- 初始化服务器配额
INSERT INTO server_media_quota (id, updated_ts)
VALUES (1, EXTRACT(EPOCH FROM NOW())::BIGINT * 1000)
ON CONFLICT (id) DO NOTHING;

-- ============================================================================
-- 2. 服务器通知相关表
-- ============================================================================

-- 服务器通知表
CREATE TABLE IF NOT EXISTS server_notifications (
    id SERIAL PRIMARY KEY,
    title TEXT NOT NULL,
    content TEXT NOT NULL,
    notification_type TEXT NOT NULL DEFAULT 'info' CHECK (notification_type IN ('info', 'warning', 'alert', 'maintenance', 'security')),
    priority INTEGER DEFAULT 0,
    target_audience TEXT NOT NULL DEFAULT 'all' CHECK (target_audience IN ('all', 'admins', 'users', 'specific')),
    target_user_ids JSONB DEFAULT '[]'::jsonb,
    target_room_ids JSONB DEFAULT '[]'::jsonb,
    starts_at BIGINT,
    expires_at BIGINT,
    is_enabled BOOLEAN DEFAULT TRUE,
    is_dismissable BOOLEAN DEFAULT TRUE,
    action_url TEXT,
    action_text TEXT,
    icon_url TEXT,
    created_by TEXT,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL
);

COMMENT ON TABLE server_notifications IS '服务器通知表';

CREATE INDEX IF NOT EXISTS idx_server_notifications_enabled ON server_notifications(is_enabled) WHERE is_enabled = TRUE;
CREATE INDEX IF NOT EXISTS idx_server_notifications_time ON server_notifications(starts_at, expires_at);
CREATE INDEX IF NOT EXISTS idx_server_notifications_type ON server_notifications(notification_type);
CREATE INDEX IF NOT EXISTS idx_server_notifications_audience ON server_notifications(target_audience);
CREATE INDEX IF NOT EXISTS idx_server_notifications_created ON server_notifications(created_ts DESC);

-- 用户通知状态表
CREATE TABLE IF NOT EXISTS user_notification_status (
    id SERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    notification_id INTEGER NOT NULL REFERENCES server_notifications(id) ON DELETE CASCADE,
    is_read BOOLEAN DEFAULT FALSE,
    is_dismissed BOOLEAN DEFAULT FALSE,
    read_ts BIGINT,
    dismissed_ts BIGINT,
    created_ts BIGINT NOT NULL,
    UNIQUE(user_id, notification_id)
);

COMMENT ON TABLE user_notification_status IS '用户通知状态表';

CREATE INDEX IF NOT EXISTS idx_user_notification_status_user ON user_notification_status(user_id);
CREATE INDEX IF NOT EXISTS idx_user_notification_status_notification ON user_notification_status(notification_id);
CREATE INDEX IF NOT EXISTS idx_user_notification_status_unread ON user_notification_status(user_id, is_read) WHERE is_read = FALSE;

-- 通知模板表
CREATE TABLE IF NOT EXISTS notification_templates (
    id SERIAL PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    title_template TEXT NOT NULL,
    content_template TEXT NOT NULL,
    notification_type TEXT NOT NULL DEFAULT 'info',
    variables JSONB DEFAULT '[]'::jsonb,
    is_enabled BOOLEAN DEFAULT TRUE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL
);

COMMENT ON TABLE notification_templates IS '通知模板表';

CREATE INDEX IF NOT EXISTS idx_notification_templates_name ON notification_templates(name);
CREATE INDEX IF NOT EXISTS idx_notification_templates_enabled ON notification_templates(is_enabled) WHERE is_enabled = TRUE;

-- 通知投递日志表
CREATE TABLE IF NOT EXISTS notification_delivery_log (
    id BIGSERIAL PRIMARY KEY,
    notification_id INTEGER NOT NULL REFERENCES server_notifications(id) ON DELETE CASCADE,
    user_id TEXT,
    delivery_method TEXT NOT NULL CHECK (delivery_method IN ('push', 'email', 'sms', 'in_app')),
    status TEXT NOT NULL CHECK (status IN ('pending', 'sent', 'delivered', 'failed', 'bounced')),
    error_message TEXT,
    delivered_ts BIGINT NOT NULL,
    retry_count INTEGER DEFAULT 0
);

COMMENT ON TABLE notification_delivery_log IS '通知投递日志表';

CREATE INDEX IF NOT EXISTS idx_notification_delivery_log_notification ON notification_delivery_log(notification_id);
CREATE INDEX IF NOT EXISTS idx_notification_delivery_log_user ON notification_delivery_log(user_id);
CREATE INDEX IF NOT EXISTS idx_notification_delivery_log_status ON notification_delivery_log(status);
CREATE INDEX IF NOT EXISTS idx_notification_delivery_log_delivered ON notification_delivery_log(delivered_ts DESC);

-- 定时通知表
CREATE TABLE IF NOT EXISTS scheduled_notifications (
    id SERIAL PRIMARY KEY,
    notification_id INTEGER NOT NULL REFERENCES server_notifications(id) ON DELETE CASCADE,
    scheduled_for BIGINT NOT NULL,
    is_sent BOOLEAN DEFAULT FALSE,
    sent_ts BIGINT,
    created_ts BIGINT NOT NULL
);

COMMENT ON TABLE scheduled_notifications IS '定时通知表';

CREATE INDEX IF NOT EXISTS idx_scheduled_notifications_pending ON scheduled_notifications(scheduled_for, is_sent) WHERE is_sent = FALSE;
CREATE INDEX IF NOT EXISTS idx_scheduled_notifications_notification ON scheduled_notifications(notification_id);

-- ============================================================================
-- 3. 默认配额配置
-- ============================================================================

-- 插入默认配额配置
INSERT INTO media_quota_config (name, description, max_storage_bytes, max_file_size_bytes, max_files_count, is_default, is_enabled, created_ts, updated_ts)
VALUES 
    ('default', '默认用户配额', 1073741824, 104857600, 1000, TRUE, TRUE, EXTRACT(EPOCH FROM NOW())::BIGINT * 1000, EXTRACT(EPOCH FROM NOW())::BIGINT * 1000),
    ('premium', '高级用户配额', 10737418240, 1073741824, 10000, FALSE, TRUE, EXTRACT(EPOCH FROM NOW())::BIGINT * 1000, EXTRACT(EPOCH FROM NOW())::BIGINT * 1000),
    ('admin', '管理员配额', 107374182400, 10737418240, 100000, FALSE, TRUE, EXTRACT(EPOCH FROM NOW())::BIGINT * 1000, EXTRACT(EPOCH FROM NOW())::BIGINT * 1000)
ON CONFLICT (name) DO NOTHING;

-- ============================================================================
-- 4. 默认通知模板
-- ============================================================================

INSERT INTO notification_templates (name, title_template, content_template, notification_type, variables, is_enabled, created_ts, updated_ts)
VALUES 
    ('welcome', '欢迎加入 {{server_name}}', '您好 {{username}}，欢迎加入 {{server_name}}！请阅读我们的社区准则。', 'info', '["server_name", "username"]'::jsonb, TRUE, EXTRACT(EPOCH FROM NOW())::BIGINT * 1000, EXTRACT(EPOCH FROM NOW())::BIGINT * 1000),
    ('maintenance', '系统维护通知', '系统将于 {{start_time}} 进行维护，预计持续 {{duration}} 分钟。', 'maintenance', '["start_time", "duration"]'::jsonb, TRUE, EXTRACT(EPOCH FROM NOW())::BIGINT * 1000, EXTRACT(EPOCH FROM NOW())::BIGINT * 1000),
    ('security_alert', '安全提醒', '检测到您的账户存在异常登录，请及时检查。', 'security', '[]'::jsonb, TRUE, EXTRACT(EPOCH FROM NOW())::BIGINT * 1000, EXTRACT(EPOCH FROM NOW())::BIGINT * 1000)
ON CONFLICT (name) DO NOTHING;

-- ============================================================================
-- 5. 记录迁移完成
-- ============================================================================

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'schema_migrations' AND column_name = 'name'
    ) AND EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'schema_migrations' AND column_name = 'applied_ts'
    ) THEN
        INSERT INTO schema_migrations (version, name, applied_ts, checksum)
        VALUES (
            '20260302000003',
            'add_media_quota_and_notification_tables',
            EXTRACT(EPOCH FROM NOW())::BIGINT * 1000,
            'media_quota_notifications_v1'
        )
        ON CONFLICT (version) DO UPDATE SET
            applied_ts = EXCLUDED.applied_ts;
    ELSIF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'schema_migrations' AND column_name = 'executed_at'
    ) THEN
        INSERT INTO schema_migrations (version, checksum, executed_at, success, error_message, description)
        VALUES (
            '20260302000003',
            md5('20260302000003_add_media_quota_and_notification_tables.sql'),
            NOW(),
            TRUE,
            NULL,
            'add_media_quota_and_notification_tables'
        )
        ON CONFLICT (version) DO UPDATE SET
            executed_at = EXCLUDED.executed_at,
            success = TRUE,
            error_message = NULL;
    ELSE
        INSERT INTO schema_migrations (version)
        VALUES ('20260302000003')
        ON CONFLICT (version) DO NOTHING;
    END IF;
END $$;
