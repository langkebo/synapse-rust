-- 推送通知功能迁移脚本
-- 实现 FCM/APNS/WebPush 推送通知

-- 推送设备注册表
CREATE TABLE IF NOT EXISTS push_device (
    id SERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255) NOT NULL,
    push_token TEXT NOT NULL,
    push_type VARCHAR(20) NOT NULL,
    app_id VARCHAR(255),
    platform VARCHAR(50),
    platform_version VARCHAR(50),
    app_version VARCHAR(50),
    locale VARCHAR(20),
    timezone VARCHAR(50),
    enabled BOOLEAN DEFAULT true,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    last_used_at TIMESTAMP WITH TIME ZONE,
    last_error TEXT,
    error_count INTEGER DEFAULT 0,
    metadata JSONB DEFAULT '{}',
    CONSTRAINT push_device_user_device_unique UNIQUE(user_id, device_id),
    CONSTRAINT push_device_type_check CHECK (push_type IN ('fcm', 'apns', 'webpush', 'upstream'))
);

CREATE INDEX idx_push_device_user ON push_device(user_id);
CREATE INDEX idx_push_device_token ON push_device(push_token);
CREATE INDEX idx_push_device_type ON push_device(push_type);
CREATE INDEX idx_push_device_enabled ON push_device(enabled);

-- 推送规则表
CREATE TABLE IF NOT EXISTS push_rule (
    id SERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    rule_id VARCHAR(255) NOT NULL,
    scope VARCHAR(50) NOT NULL DEFAULT 'global',
    kind VARCHAR(50) NOT NULL,
    priority INTEGER DEFAULT 0,
    conditions JSONB DEFAULT '[]',
    actions JSONB DEFAULT '[]',
    enabled BOOLEAN DEFAULT true,
    is_default BOOLEAN DEFAULT false,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    CONSTRAINT push_rule_user_rule_unique UNIQUE(user_id, scope, kind, rule_id),
    CONSTRAINT push_rule_scope_check CHECK (scope IN ('global', 'device')),
    CONSTRAINT push_rule_kind_check CHECK (kind IN ('override', 'content', 'room', 'sender', 'underride'))
);

CREATE INDEX idx_push_rule_user ON push_rule(user_id);
CREATE INDEX idx_push_rule_scope ON push_rule(scope);
CREATE INDEX idx_push_rule_kind ON push_rule(kind);
CREATE INDEX idx_push_rule_enabled ON push_rule(enabled);
CREATE INDEX idx_push_rule_priority ON push_rule(priority);

-- 插入默认推送规则
INSERT INTO push_rule (user_id, rule_id, scope, kind, priority, conditions, actions, enabled, is_default)
VALUES 
    ('.default', 'm.rule.master', 'global', 'override', 0, '[]', '["dont_notify"]', true, true),
    ('.default', 'm.rule.suppress_notices', 'global', 'override', 1, '[{"kind": "event_match", "key": "content.msgtype", "pattern": "m.notice"}]', '["dont_notify"]', true, true),
    ('.default', 'm.rule.invite_for_me', 'global', 'override', 2, '[{"kind": "event_match", "key": "type", "pattern": "m.room.member"}, {"kind": "event_match", "key": "content.membership", "pattern": "invite"}, {"kind": "event_match", "key": "state_key", "pattern": "_self"}]', '["notify", {"set_tweak": "sound", "value": "default"}]', true, true),
    ('.default', 'm.rule.member_event', 'global', 'override', 3, '[{"kind": "event_match", "key": "type", "pattern": "m.room.member"}]', '["dont_notify"]', true, true),
    ('.default', 'm.rule.contains_display_name', 'global', 'content', 4, '[{"kind": "contains_display_name"}]', '["notify", {"set_tweak": "sound", "value": "default"}, {"set_tweak": "highlight", "value": true}]', true, true),
    ('.default', 'm.rule.tombstone', 'global', 'override', 5, '[{"kind": "event_match", "key": "type", "pattern": "m.room.tombstone"}, {"kind": "event_match", "key": "state_key", "pattern": ""}]', '["notify", {"set_tweak": "highlight", "value": true}]', true, true),
    ('.default', 'm.rule.room_notif', 'global', 'content', 6, '[{"kind": "event_match", "key": "content.body", "pattern": "@room"}]', '["notify", {"set_tweak": "highlight", "value": true}]', true, true),
    ('.default', 'm.rule.message', 'global', 'underride', 7, '[{"kind": "event_match", "key": "type", "pattern": "m.room.message"}]', '["notify", {"set_tweak": "sound", "value": "default"}]', true, true),
    ('.default', 'm.rule.encrypted', 'global', 'underride', 8, '[{"kind": "event_match", "key": "type", "pattern": "m.room.encrypted"}]', '["notify", {"set_tweak": "sound", "value": "default"}]', true, true)
ON CONFLICT (user_id, scope, kind, rule_id) DO NOTHING;

-- 推送通知队列表
CREATE TABLE IF NOT EXISTS push_notification_queue (
    id SERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255) NOT NULL,
    event_id VARCHAR(255),
    room_id VARCHAR(255),
    notification_type VARCHAR(50),
    content JSONB NOT NULL,
    priority INTEGER DEFAULT 5,
    status VARCHAR(20) DEFAULT 'pending',
    attempts INTEGER DEFAULT 0,
    max_attempts INTEGER DEFAULT 5,
    next_attempt_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    sent_at TIMESTAMP WITH TIME ZONE,
    error_message TEXT,
    CONSTRAINT push_notification_queue_status_check CHECK (status IN ('pending', 'processing', 'sent', 'failed', 'cancelled'))
);

CREATE INDEX idx_push_notification_queue_user ON push_notification_queue(user_id);
CREATE INDEX idx_push_notification_queue_status ON push_notification_queue(status);
CREATE INDEX idx_push_notification_queue_next_attempt ON push_notification_queue(next_attempt_at);
CREATE INDEX idx_push_notification_queue_priority ON push_notification_queue(priority);

-- 推送通知发送日志表
CREATE TABLE IF NOT EXISTS push_notification_log (
    id SERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255) NOT NULL,
    event_id VARCHAR(255),
    room_id VARCHAR(255),
    notification_type VARCHAR(50),
    push_type VARCHAR(20),
    sent_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    success BOOLEAN DEFAULT true,
    error_message TEXT,
    provider_response TEXT,
    response_time_ms INTEGER,
    metadata JSONB DEFAULT '{}',
    CONSTRAINT push_notification_log_push_type_check CHECK (push_type IN ('fcm', 'apns', 'webpush', 'upstream'))
);

CREATE INDEX idx_push_notification_log_user ON push_notification_log(user_id);
CREATE INDEX idx_push_notification_log_sent_at ON push_notification_log(sent_at);
CREATE INDEX idx_push_notification_log_success ON push_notification_log(success);

-- 推送配置表
CREATE TABLE IF NOT EXISTS push_config (
    id SERIAL PRIMARY KEY,
    config_key VARCHAR(100) NOT NULL UNIQUE,
    config_value TEXT NOT NULL,
    description TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    CONSTRAINT push_config_key_unique UNIQUE(config_key)
);

CREATE INDEX idx_push_config_key ON push_config(config_key);

-- 插入默认推送配置
INSERT INTO push_config (config_key, config_value, description)
VALUES 
    ('fcm.enabled', 'false', 'Enable FCM push notifications'),
    ('fcm.api_key', '', 'FCM API key'),
    ('fcm.project_id', '', 'FCM project ID'),
    ('fcm.max_retries', '3', 'Maximum retry attempts for FCM'),
    ('apns.enabled', 'false', 'Enable APNS push notifications'),
    ('apns.certificate_path', '', 'Path to APNS certificate'),
    ('apns.private_key_path', '', 'Path to APNS private key'),
    ('apns.topic', '', 'APNS topic (bundle ID)'),
    ('apns.environment', 'sandbox', 'APNS environment (sandbox/production)'),
    ('apns.max_retries', '3', 'Maximum retry attempts for APNS'),
    ('webpush.enabled', 'false', 'Enable WebPush notifications'),
    ('webpush.vapid_private_key', '', 'VAPID private key'),
    ('webpush.vapid_public_key', '', 'VAPID public key'),
    ('webpush.subject', '', 'WebPush subject (mailto: or https:)'),
    ('webpush.max_retries', '3', 'Maximum retry attempts for WebPush'),
    ('push.rate_limit_per_minute', '60', 'Rate limit per user per minute'),
    ('push.batch_size', '100', 'Batch size for push processing'),
    ('push.worker_count', '4', 'Number of push worker threads'),
    ('push.retry_delay_seconds', '60', 'Delay between retry attempts'),
    ('push.max_queue_size', '10000', 'Maximum queue size')
ON CONFLICT (config_key) DO NOTHING;

-- 推送统计表
CREATE TABLE IF NOT EXISTS push_stats (
    id SERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    date DATE NOT NULL,
    total_sent INTEGER DEFAULT 0,
    total_failed INTEGER DEFAULT 0,
    fcm_sent INTEGER DEFAULT 0,
    fcm_failed INTEGER DEFAULT 0,
    apns_sent INTEGER DEFAULT 0,
    apns_failed INTEGER DEFAULT 0,
    webpush_sent INTEGER DEFAULT 0,
    webpush_failed INTEGER DEFAULT 0,
    avg_response_time_ms INTEGER DEFAULT 0,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    CONSTRAINT push_stats_user_date_unique UNIQUE(user_id, date)
);

CREATE INDEX idx_push_stats_user ON push_stats(user_id);
CREATE INDEX idx_push_stats_date ON push_stats(date);

COMMENT ON TABLE push_device IS '推送设备注册表';
COMMENT ON TABLE push_rule IS '推送规则表';
COMMENT ON TABLE push_notification_queue IS '推送通知队列表';
COMMENT ON TABLE push_notification_log IS '推送通知发送日志表';
COMMENT ON TABLE push_config IS '推送配置表';
COMMENT ON TABLE push_stats IS '推送统计表';
