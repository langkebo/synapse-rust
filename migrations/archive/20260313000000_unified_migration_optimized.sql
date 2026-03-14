-- ============================================================================
-- synapse-rust 迁移文件优化合并
-- 创建日期: 2026-03-13
-- 说明: 合并重复迁移文件，优化数据库结构
-- 
-- 合并内容:
--   - 20260312000001_add_missing_p1_tables.sql
--   - 20260312000001_presence_subscriptions.sql
--   - 20260312000002_add_missing_p2_tables.sql
--   - 20260312000002_call_sessions.sql
--   - 20260312000003_add_missing_p3_tables.sql
--   - 20260312000004_add_missing_indexes.sql
--   - 20260312000004_fix_timestamp_field_names.sql
--   - 20260312000005_qr_login.sql
--   - 20260312000006_invite_blocklist.sql
--   - 20260312000007_sticky_event.sql
--   - 20260313000008_field_name_fix.sql
-- ============================================================================

SET TIME ZONE 'UTC';

-- ============================================================================
-- 第一部分: 核心功能表 (P1 高优先级)
-- ============================================================================

-- Presence 订阅表
CREATE TABLE IF NOT EXISTS presence_subscriptions (
    id BIGSERIAL PRIMARY KEY,
    subscriber_id VARCHAR(255) NOT NULL,
    target_id VARCHAR(255) NOT NULL,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    UNIQUE(subscriber_id, target_id)
);

CREATE INDEX IF NOT EXISTS idx_presence_subscriptions_subscriber ON presence_subscriptions(subscriber_id);
CREATE INDEX IF NOT EXISTS idx_presence_subscriptions_target ON presence_subscriptions(target_id);

-- VOIP 呼叫会话表
CREATE TABLE IF NOT EXISTS call_sessions (
    id BIGSERIAL PRIMARY KEY,
    call_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    caller_id VARCHAR(255) NOT NULL,
    callee_id VARCHAR(255),
    state VARCHAR(50) NOT NULL DEFAULT 'ringing',
    offer_sdp TEXT,
    answer_sdp TEXT,
    lifetime BIGINT DEFAULT 60000,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    updated_ts BIGINT,
    ended_ts BIGINT,
    UNIQUE(call_id, room_id)
);

CREATE INDEX IF NOT EXISTS idx_call_sessions_room ON call_sessions(room_id);
CREATE INDEX IF NOT EXISTS idx_call_sessions_caller ON call_sessions(caller_id);
CREATE INDEX IF NOT EXISTS idx_call_sessions_callee ON call_sessions(callee_id);
CREATE INDEX IF NOT EXISTS idx_call_sessions_state ON call_sessions(state);

-- 呼叫候选人表
CREATE TABLE IF NOT EXISTS call_candidates (
    id BIGSERIAL PRIMARY KEY,
    call_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    sender_id VARCHAR(255) NOT NULL,
    candidate JSONB NOT NULL,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT
);

CREATE INDEX IF NOT EXISTS idx_call_candidates_call ON call_candidates(call_id, room_id);

-- QR 登录事务表
CREATE TABLE IF NOT EXISTS qr_login_transactions (
    id BIGSERIAL PRIMARY KEY,
    transaction_id TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL,
    device_id TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    updated_ts BIGINT,
    expires_at BIGINT NOT NULL,
    completed_at BIGINT,
    access_token TEXT
);

CREATE INDEX IF NOT EXISTS idx_qr_login_transactions_user ON qr_login_transactions(user_id);
CREATE INDEX IF NOT EXISTS idx_qr_login_transactions_status ON qr_login_transactions(status);

-- 邀请黑名单表
CREATE TABLE IF NOT EXISTS room_invite_blocklist (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    UNIQUE(room_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_room_invite_blocklist_room ON room_invite_blocklist(room_id);

-- 邀请白名单表
CREATE TABLE IF NOT EXISTS room_invite_allowlist (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    UNIQUE(room_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_room_invite_allowlist_room ON room_invite_allowlist(room_id);

-- 粘性事件表
CREATE TABLE IF NOT EXISTS room_sticky_events (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    sticky BOOLEAN NOT NULL DEFAULT true,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    updated_ts BIGINT,
    UNIQUE(room_id, user_id, event_type)
);

CREATE INDEX IF NOT EXISTS idx_room_sticky_events_room ON room_sticky_events(room_id);
CREATE INDEX IF NOT EXISTS idx_room_sticky_events_user ON room_sticky_events(user_id);

-- ============================================================================
-- 第二部分: 扩展功能表 (P2 中优先级)
-- ============================================================================

-- 后台更新历史表
CREATE TABLE IF NOT EXISTS background_update_history (
    id BIGSERIAL PRIMARY KEY,
    update_name TEXT NOT NULL,
    total_items BIGINT DEFAULT 0,
    processed_items BIGINT DEFAULT 0,
    total_duration_ms BIGINT DEFAULT 0,
    completed_ts BIGINT,
    status TEXT DEFAULT 'completed',
    error_message TEXT
);

CREATE INDEX IF NOT EXISTS idx_background_update_history_name ON background_update_history(update_name);
CREATE INDEX IF NOT EXISTS idx_background_update_history_completed ON background_update_history(completed_ts);

-- 后台更新锁表
CREATE TABLE IF NOT EXISTS background_update_locks (
    id BIGSERIAL PRIMARY KEY,
    lock_name TEXT NOT NULL UNIQUE,
    owner TEXT NOT NULL,
    acquired_ts BIGINT NOT NULL,
    expires_at BIGINT
);

CREATE INDEX IF NOT EXISTS idx_background_update_locks_expires ON background_update_locks(expires_ts) WHERE expires_ts IS NOT NULL;

-- 后台更新统计表
CREATE TABLE IF NOT EXISTS background_update_stats (
    id BIGSERIAL PRIMARY KEY,
    update_name TEXT NOT NULL UNIQUE,
    total_runs BIGINT DEFAULT 0,
    total_items_processed BIGINT DEFAULT 0,
    avg_duration_ms BIGINT,
    last_run_ts BIGINT
);

-- 联邦黑名单配置表
CREATE TABLE IF NOT EXISTS federation_blacklist_config (
    id BIGSERIAL PRIMARY KEY,
    config_key TEXT NOT NULL UNIQUE,
    config_value TEXT NOT NULL,
    description TEXT,
    updated_ts BIGINT
);

-- 联邦黑名单规则表
CREATE TABLE IF NOT EXISTS federation_blacklist_rule (
    id BIGSERIAL PRIMARY KEY,
    rule_pattern TEXT NOT NULL,
    rule_type TEXT DEFAULT 'domain',
    action TEXT DEFAULT 'block',
    reason TEXT,
    is_enabled BOOLEAN DEFAULT TRUE,
    created_ts BIGINT NOT NULL,
    expires_at BIGINT
);

CREATE INDEX IF NOT EXISTS idx_federation_blacklist_rule_enabled ON federation_blacklist_rule(is_enabled) WHERE is_enabled = TRUE;

-- 联邦黑名单日志表
CREATE TABLE IF NOT EXISTS federation_blacklist_log (
    id BIGSERIAL PRIMARY KEY,
    server_name TEXT NOT NULL,
    action TEXT NOT NULL,
    reason TEXT,
    performed_by TEXT,
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_federation_blacklist_log_server ON federation_blacklist_log(server_name);
CREATE INDEX IF NOT EXISTS idx_federation_blacklist_log_created ON federation_blacklist_log(created_ts);

-- 已删除事件索引表
CREATE TABLE IF NOT EXISTS deleted_events_index (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    event_id TEXT NOT NULL UNIQUE,
    origin_server_ts BIGINT NOT NULL,
    deleted_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_deleted_events_index_room ON deleted_events_index(room_id);
CREATE INDEX IF NOT EXISTS idx_deleted_events_index_ts ON deleted_events_index(deleted_ts);

-- 保留清理日志表
CREATE TABLE IF NOT EXISTS retention_cleanup_logs (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT,
    batch_id BIGINT,
    deleted_count BIGINT DEFAULT 0,
    duration_ms BIGINT,
    status TEXT DEFAULT 'completed',
    error_message TEXT,
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_retention_cleanup_logs_room ON retention_cleanup_logs(room_id);
CREATE INDEX IF NOT EXISTS idx_retention_cleanup_logs_created ON retention_cleanup_logs(created_ts);

-- 保留清理队列表
CREATE TABLE IF NOT EXISTS retention_cleanup_queue (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    batch_size BIGINT DEFAULT 1000,
    priority INTEGER DEFAULT 0,
    scheduled_ts BIGINT,
    enqueued_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_retention_cleanup_queue_scheduled ON retention_cleanup_queue(scheduled_ts) WHERE scheduled_ts IS NOT NULL;

-- 通知模板表
CREATE TABLE IF NOT EXISTS notification_templates (
    id BIGSERIAL PRIMARY KEY,
    template_name TEXT NOT NULL UNIQUE,
    template_type TEXT NOT NULL,
    subject TEXT,
    body TEXT NOT NULL,
    variables JSONB DEFAULT '{}',
    is_default BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT
);

CREATE INDEX IF NOT EXISTS idx_notification_templates_default ON notification_templates(is_default) WHERE is_default = TRUE;

-- 通知投递日志表
CREATE TABLE IF NOT EXISTS notification_delivery_log (
    id BIGSERIAL PRIMARY KEY,
    notification_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    channel TEXT NOT NULL,
    status TEXT DEFAULT 'pending',
    sent_ts BIGINT,
    error_message TEXT,
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_notification_delivery_log_user ON notification_delivery_log(user_id);
CREATE INDEX IF NOT EXISTS idx_notification_delivery_log_status ON notification_delivery_log(status);

-- 定时通知表
CREATE TABLE IF NOT EXISTS scheduled_notifications (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    room_id TEXT,
    event_id TEXT,
    scheduled_ts BIGINT NOT NULL,
    template_name TEXT NOT NULL,
    variables JSONB DEFAULT '{}',
    is_sent BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_scheduled_notifications_user ON scheduled_notifications(user_id);
CREATE INDEX IF NOT EXISTS idx_scheduled_notifications_scheduled ON scheduled_notifications(scheduled_ts) WHERE is_sent = FALSE;

-- 用户通知状态表
CREATE TABLE IF NOT EXISTS user_notification_status (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT,
    notification_id TEXT NOT NULL,
    is_read BOOLEAN DEFAULT FALSE,
    is_dismissed BOOLEAN DEFAULT FALSE,
    read_ts BIGINT,
    created_ts BIGINT NOT NULL,
    UNIQUE(user_id, notification_id)
);

CREATE INDEX IF NOT EXISTS idx_user_notification_status_user ON user_notification_status(user_id);

-- 推送设备表
CREATE TABLE IF NOT EXISTS push_device (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    push_provider TEXT NOT NULL,
    push_token TEXT NOT NULL,
    app_id TEXT,
    is_active BOOLEAN DEFAULT TRUE,
    last_seen_ts BIGINT,
    created_ts BIGINT NOT NULL,
    UNIQUE(user_id, device_id)
);

CREATE INDEX IF NOT EXISTS idx_push_device_user ON push_device(user_id);
CREATE INDEX IF NOT EXISTS idx_push_device_active ON push_device(is_active) WHERE is_active = TRUE;

-- 注册令牌批次表
CREATE TABLE IF NOT EXISTS registration_token_batches (
    id BIGSERIAL PRIMARY KEY,
    batch_name TEXT NOT NULL,
    token_prefix TEXT NOT NULL,
    tokens_generated BIGINT DEFAULT 0,
    tokens_used BIGINT DEFAULT 0,
    is_active BOOLEAN DEFAULT TRUE,
    created_ts BIGINT NOT NULL,
    expires_at BIGINT
);

CREATE INDEX IF NOT EXISTS idx_registration_token_batches_active ON registration_token_batches(is_active) WHERE is_active = TRUE;

-- Rendezvous 消息表
CREATE TABLE IF NOT EXISTS rendezvous_messages (
    id BIGSERIAL PRIMARY KEY,
    session_id TEXT NOT NULL,
    message_id TEXT NOT NULL,
    sender TEXT NOT NULL,
    content TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    UNIQUE(session_id, message_id)
);

CREATE INDEX IF NOT EXISTS idx_rendezvous_messages_session ON rendezvous_messages(session_id);

-- ============================================================================
-- 第三部分: 可选功能表 (P3 低优先级)
-- ============================================================================

-- Beacon 信息表
CREATE TABLE IF NOT EXISTS beacon_info (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    description TEXT,
    timestamp BIGINT NOT NULL,
    live BOOLEAN DEFAULT TRUE,
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_beacon_info_room ON beacon_info(room_id);
CREATE INDEX IF NOT EXISTS idx_beacon_info_user ON beacon_info(user_id);
CREATE INDEX IF NOT EXISTS idx_beacon_info_live ON beacon_info(live) WHERE live = TRUE;

-- Beacon 位置表
CREATE TABLE IF NOT EXISTS beacon_locations (
    id BIGSERIAL PRIMARY KEY,
    beacon_id BIGINT NOT NULL REFERENCES beacon_info(id) ON DELETE CASCADE,
    timestamp BIGINT NOT NULL,
    latitude REAL NOT NULL,
    longitude REAL NOT NULL,
    accuracy REAL,
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_beacon_locations_beacon ON beacon_locations(beacon_id);
CREATE INDEX IF NOT EXISTS idx_beacon_locations_timestamp ON beacon_locations(timestamp);

-- 脱水设备表
CREATE TABLE IF NOT EXISTS dehydrated_devices (
    id BIGSERIAL PRIMARY KEY,
    device_id TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL,
    device_data JSONB NOT NULL,
    expires_at BIGINT,
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_dehydrated_devices_user ON dehydrated_devices(user_id);
CREATE INDEX IF NOT EXISTS idx_dehydrated_devices_expires ON dehydrated_devices(expires_ts) WHERE expires_ts IS NOT NULL;

-- 邮件验证表
CREATE TABLE IF NOT EXISTS email_verification (
    id BIGSERIAL PRIMARY KEY,
    email TEXT NOT NULL,
    token TEXT NOT NULL,
    code TEXT,
    status TEXT DEFAULT 'pending',
    attempts INTEGER DEFAULT 0,
    verified_at BIGINT,
    expires_at BIGINT NOT NULL,
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_email_verification_email ON email_verification(email);
CREATE INDEX IF NOT EXISTS idx_email_verification_token ON email_verification(token);

-- 联邦统计表
CREATE TABLE IF NOT EXISTS federation_stats (
    id BIGSERIAL PRIMARY KEY,
    server_name TEXT NOT NULL,
    stat_type TEXT NOT NULL,
    stat_value BIGINT NOT NULL,
    recorded_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_federation_stats_server ON federation_stats(server_name);
CREATE INDEX IF NOT EXISTS idx_federation_stats_type ON federation_stats(stat_type);

-- 性能监控表
CREATE TABLE IF NOT EXISTS performance_metrics (
    id BIGSERIAL PRIMARY KEY,
    metric_name TEXT NOT NULL,
    metric_value BIGINT NOT NULL,
    tags JSONB DEFAULT '{}',
    recorded_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_performance_metrics_name ON performance_metrics(metric_name);
CREATE INDEX IF NOT EXISTS idx_performance_metrics_ts ON performance_metrics(recorded_ts);

-- 审计日志表
CREATE TABLE IF NOT EXISTS audit_log (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT,
    action TEXT NOT NULL,
    resource_type TEXT,
    resource_id TEXT,
    ip_address TEXT,
    user_agent TEXT,
    status TEXT DEFAULT 'success',
    error_message TEXT,
    recorded_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_audit_log_user ON audit_log(user_id);
CREATE INDEX IF NOT EXISTS idx_audit_log_action ON audit_log(action);
CREATE INDEX IF NOT EXISTS idx_audit_log_resource ON audit_log(resource_type, resource_id);
CREATE INDEX IF NOT EXISTS idx_audit_log_ts ON audit_log(recorded_ts);

-- ============================================================================
-- 第四部分: 索引优化
-- ============================================================================

-- Presence 订阅复合索引
CREATE INDEX IF NOT EXISTS idx_presence_subscriptions_user_target 
    ON presence_subscriptions(subscriber_id, target_id);

-- ============================================================================
-- 第五部分: 字段命名规范化
-- ============================================================================

-- 重命名 created_at -> created_ts
DO $$
DECLARE
    t RECORD;
BEGIN
    FOR t IN 
        SELECT table_name 
        FROM information_schema.columns 
        WHERE column_name = 'created_at' 
        AND table_schema = 'public'
    LOOP
        EXECUTE format('ALTER TABLE %I RENAME COLUMN created_at TO created_ts', t.table_name);
    END LOOP;
END $$;

-- 重命名 updated_at -> updated_ts
DO $$
DECLARE
    t RECORD;
BEGIN
    FOR t IN 
        SELECT table_name 
        FROM information_schema.columns 
        WHERE column_name = 'updated_at' 
        AND table_schema = 'public'
    LOOP
        EXECUTE format('ALTER TABLE %I RENAME COLUMN updated_at TO updated_ts', t.table_name);
    END LOOP;
END $$;

-- 重命名 expires_ts -> expires_at (可选时间戳)
DO $$
DECLARE
    t RECORD;
BEGIN
    FOR t IN 
        SELECT table_name 
        FROM information_schema.columns 
        WHERE column_name = 'expires_ts' 
        AND table_schema = 'public'
    LOOP
        EXECUTE format('ALTER TABLE %I RENAME COLUMN expires_ts TO expires_at', t.table_name);
    END LOOP;
END $$;

-- 重命名 revoked_ts -> revoked_at (可选时间戳)
DO $$
DECLARE
    t RECORD;
BEGIN
    FOR t IN 
        SELECT table_name 
        FROM information_schema.columns 
        WHERE column_name = 'revoked_ts' 
        AND table_schema = 'public'
    LOOP
        EXECUTE format('ALTER TABLE %I RENAME COLUMN revoked_ts TO revoked_at', t.table_name);
    END LOOP;
END $$;

-- 重命名 validated_ts -> validated_at (可选时间戳)
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'threepids' AND column_name = 'validated_ts'
    ) THEN
        ALTER TABLE threepids RENAME COLUMN validated_ts TO validated_at;
    END IF;
    
    IF EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'user_threepids' AND column_name = 'validated_ts'
    ) THEN
        ALTER TABLE user_threepids RENAME COLUMN validated_ts TO validated_at;
    END IF;
END $$;

-- 重命名 verification_expires_ts -> verification_expires_at
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'user_threepids' AND column_name = 'verification_expires_ts'
    ) THEN
        ALTER TABLE user_threepids RENAME COLUMN verification_expires_ts TO verification_expires_at;
    END IF;
END $$;

-- ============================================================================
-- 完成提示
-- ============================================================================

DO $$
BEGIN
    RAISE NOTICE '==========================================';
    RAISE NOTICE '迁移文件优化合并完成';
    RAISE NOTICE '合并时间: %', NOW();
    RAISE NOTICE '包含表数量: 30+';
    RAISE NOTICE '==========================================';
END $$;
