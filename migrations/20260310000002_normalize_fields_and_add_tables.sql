-- ============================================================================
-- 数据库字段规范化迁移
-- 创建日期: 2026-03-10
-- 说明: 
--   1. 统一 ID 类型（VARCHAR → TEXT）
--   2. 修复布尔字段命名（添加 is_ 前缀）
--   3. 添加缺失的应用服务表（P1）
--   4. 添加缺失的 Worker 表（P2）
-- ============================================================================

-- ============================================================================
-- 第一部分：修复 ID 类型不一致（VARCHAR → TEXT）
-- ============================================================================

-- notifications 表
ALTER TABLE notifications ALTER COLUMN notification_type TYPE TEXT;
ALTER TABLE notifications ALTER COLUMN profile_tag TYPE TEXT;

-- media_metadata 表
ALTER TABLE media_metadata ALTER COLUMN mime_type TYPE TEXT;

-- private_sessions 表
ALTER TABLE private_sessions ALTER COLUMN id TYPE TEXT;
ALTER TABLE private_sessions ALTER COLUMN user_id_1 TYPE TEXT;
ALTER TABLE private_sessions ALTER COLUMN user_id_2 TYPE TEXT;
ALTER TABLE private_sessions ALTER COLUMN session_type TYPE TEXT;
ALTER TABLE private_sessions ALTER COLUMN encryption_key TYPE TEXT;

-- private_messages 表
ALTER TABLE private_messages ALTER COLUMN session_id TYPE TEXT;
ALTER TABLE private_messages ALTER COLUMN sender_id TYPE TEXT;
ALTER TABLE private_messages ALTER COLUMN message_type TYPE TEXT;

-- search_index 表
ALTER TABLE search_index ALTER COLUMN event_id TYPE TEXT;
ALTER TABLE search_index ALTER COLUMN room_id TYPE TEXT;
ALTER TABLE search_index ALTER COLUMN user_id TYPE TEXT;
ALTER TABLE search_index ALTER COLUMN event_type TYPE TEXT;

-- user_privacy_settings 表
ALTER TABLE user_privacy_settings ALTER COLUMN user_id TYPE TEXT;

-- threepids 表
ALTER TABLE threepids ALTER COLUMN user_id TYPE TEXT;
ALTER TABLE threepids ALTER COLUMN medium TYPE TEXT;
ALTER TABLE threepids ALTER COLUMN address TYPE TEXT;

-- room_tags 表
ALTER TABLE room_tags ALTER COLUMN user_id TYPE TEXT;
ALTER TABLE room_tags ALTER COLUMN room_id TYPE TEXT;
ALTER TABLE room_tags ALTER COLUMN tag TYPE TEXT;

-- room_events 表
ALTER TABLE room_events ALTER COLUMN event_id TYPE TEXT;
ALTER TABLE room_events ALTER COLUMN room_id TYPE TEXT;
ALTER TABLE room_events ALTER COLUMN sender TYPE TEXT;
ALTER TABLE room_events ALTER COLUMN event_type TYPE TEXT;
ALTER TABLE room_events ALTER COLUMN state_key TYPE TEXT;
ALTER TABLE room_events ALTER COLUMN prev_event_id TYPE TEXT;

-- reports 表
ALTER TABLE reports ALTER COLUMN room_id TYPE TEXT;
ALTER TABLE reports ALTER COLUMN event_id TYPE TEXT;
ALTER TABLE reports ALTER COLUMN reporter_user_id TYPE TEXT;

-- to_device_messages 表
ALTER TABLE to_device_messages ALTER COLUMN sender_user_id TYPE TEXT;
ALTER TABLE to_device_messages ALTER COLUMN sender_device_id TYPE TEXT;
ALTER TABLE to_device_messages ALTER COLUMN recipient_user_id TYPE TEXT;
ALTER TABLE to_device_messages ALTER COLUMN recipient_device_id TYPE TEXT;
ALTER TABLE to_device_messages ALTER COLUMN event_type TYPE TEXT;
ALTER TABLE to_device_messages ALTER COLUMN message_id TYPE TEXT;

-- device_lists_changes 表
ALTER TABLE device_lists_changes ALTER COLUMN user_id TYPE TEXT;
ALTER TABLE device_lists_changes ALTER COLUMN device_id TYPE TEXT;
ALTER TABLE device_lists_changes ALTER COLUMN change_type TYPE TEXT;

-- room_ephemeral 表
ALTER TABLE room_ephemeral ALTER COLUMN room_id TYPE TEXT;
ALTER TABLE room_ephemeral ALTER COLUMN event_type TYPE TEXT;
ALTER TABLE room_ephemeral ALTER COLUMN user_id TYPE TEXT;

-- device_lists_stream 表
ALTER TABLE device_lists_stream ALTER COLUMN user_id TYPE TEXT;
ALTER TABLE device_lists_stream ALTER COLUMN device_id TYPE TEXT;

-- user_filters 表
ALTER TABLE user_filters ALTER COLUMN user_id TYPE TEXT;
ALTER TABLE user_filters ALTER COLUMN filter_id TYPE TEXT;

-- modules 表
ALTER TABLE modules ALTER COLUMN name TYPE TEXT;

-- ============================================================================
-- 第二部分：修复布尔字段命名（添加 is_ 前缀）
-- ============================================================================

-- account_validity 表
ALTER TABLE account_validity RENAME COLUMN allow_renewal TO is_renewal;

-- captcha_send_log 表
ALTER TABLE captcha_send_log RENAME COLUMN success TO is_success;

-- device_keys 表
ALTER TABLE device_keys RENAME COLUMN blocked TO is_blocked;
ALTER TABLE device_keys RENAME COLUMN verified TO is_verified;

-- events 表
ALTER TABLE events RENAME COLUMN redacted TO is_redacted;

-- notifications 表
ALTER TABLE notifications RENAME COLUMN read TO is_read;

-- private_messages 表
ALTER TABLE private_messages RENAME COLUMN read_by_receiver TO is_read_by_receiver;

-- room_directory 表
ALTER TABLE room_directory RENAME COLUMN searchable TO is_searchable;

-- room_summaries 表
ALTER TABLE room_summaries RENAME COLUMN federation_allowed TO is_federation_allowed;
ALTER TABLE room_summaries RENAME COLUMN guest_can_join TO is_guest_can_join;
ALTER TABLE room_summaries RENAME COLUMN world_readable TO is_world_readable;

-- space_children 表
ALTER TABLE space_children RENAME COLUMN suggested TO is_suggested;

-- to_device_messages 表
ALTER TABLE to_device_messages RENAME COLUMN delivered TO is_delivered;

-- typing 表
ALTER TABLE typing RENAME COLUMN typing TO is_typing;

-- ============================================================================
-- 第三部分：添加缺失的应用服务表（P1 优先级）
-- ============================================================================

-- 应用服务事件队列表
CREATE TABLE IF NOT EXISTS application_service_events (
    id BIGSERIAL PRIMARY KEY,
    as_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    content JSONB NOT NULL,
    is_processed BOOLEAN DEFAULT FALSE,
    processed_ts BIGINT,
    created_ts BIGINT NOT NULL,
    CONSTRAINT uq_application_service_events_id UNIQUE (event_id, as_id)
);

CREATE INDEX IF NOT EXISTS idx_application_service_events_as_id ON application_service_events(as_id);
CREATE INDEX IF NOT EXISTS idx_application_service_events_pending ON application_service_events(is_processed) WHERE is_processed = FALSE;

-- 应用服务状态表
CREATE TABLE IF NOT EXISTS application_service_state (
    id BIGSERIAL PRIMARY KEY,
    as_id TEXT NOT NULL,
    state_key TEXT NOT NULL,
    state_value TEXT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT uq_application_service_state_as_key UNIQUE (as_id, state_key)
);

CREATE INDEX IF NOT EXISTS idx_application_service_state_as_id ON application_service_state(as_id);

-- 应用服务事务表
CREATE TABLE IF NOT EXISTS application_service_transactions (
    id BIGSERIAL PRIMARY KEY,
    as_id TEXT NOT NULL,
    txn_id TEXT NOT NULL,
    events JSONB NOT NULL,
    is_sent BOOLEAN DEFAULT FALSE,
    sent_ts BIGINT,
    created_ts BIGINT NOT NULL,
    CONSTRAINT uq_application_service_transactions UNIQUE (as_id, txn_id)
);

CREATE INDEX IF NOT EXISTS idx_application_service_transactions_as_id ON application_service_transactions(as_id);
CREATE INDEX IF NOT EXISTS idx_application_service_transactions_pending ON application_service_transactions(is_sent) WHERE is_sent = FALSE;

-- 应用服务命名空间表
CREATE TABLE IF NOT EXISTS application_service_namespaces (
    id BIGSERIAL PRIMARY KEY,
    as_id TEXT NOT NULL,
    namespace_type TEXT NOT NULL,
    namespace_pattern TEXT NOT NULL,
    is_exclusive BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    CONSTRAINT uq_application_service_namespaces UNIQUE (as_id, namespace_type, namespace_pattern)
);

CREATE INDEX IF NOT EXISTS idx_application_service_namespaces_as_id ON application_service_namespaces(as_id);

-- 应用服务用户表
CREATE TABLE IF NOT EXISTS application_service_users (
    id BIGSERIAL PRIMARY KEY,
    as_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    is_exclusive BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    CONSTRAINT uq_application_service_users UNIQUE (as_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_application_service_users_as_id ON application_service_users(as_id);
CREATE INDEX IF NOT EXISTS idx_application_service_users_user ON application_service_users(user_id);

-- ============================================================================
-- 第四部分：添加缺失的 Worker 表（P2 优先级）
-- ============================================================================

-- Worker 命令队列表
CREATE TABLE IF NOT EXISTS worker_commands (
    id BIGSERIAL PRIMARY KEY,
    worker_id TEXT NOT NULL,
    command_type TEXT NOT NULL,
    command_data JSONB NOT NULL,
    is_processed BOOLEAN DEFAULT FALSE,
    processed_ts BIGINT,
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_worker_commands_worker ON worker_commands(worker_id);
CREATE INDEX IF NOT EXISTS idx_worker_commands_pending ON worker_commands(is_processed) WHERE is_processed = FALSE;

-- Worker 事件分发表
CREATE TABLE IF NOT EXISTS worker_events (
    id BIGSERIAL PRIMARY KEY,
    worker_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    room_id TEXT,
    is_sent BOOLEAN DEFAULT FALSE,
    sent_ts BIGINT,
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_worker_events_worker ON worker_events(worker_id);
CREATE INDEX IF NOT EXISTS idx_worker_events_pending ON worker_events(is_sent) WHERE is_sent = FALSE;

-- Worker 连接统计表
CREATE TABLE IF NOT EXISTS worker_connection_stats (
    id BIGSERIAL PRIMARY KEY,
    worker_id TEXT NOT NULL UNIQUE,
    worker_type TEXT NOT NULL,
    last_heartbeat_ts BIGINT NOT NULL,
    total_commands_processed BIGINT DEFAULT 0,
    total_events_sent BIGINT DEFAULT 0,
    is_connected BOOLEAN DEFAULT TRUE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_worker_connection_stats_type ON worker_connection_stats(worker_type);
CREATE INDEX IF NOT EXISTS idx_worker_connection_stats_connected ON worker_connection_stats(is_connected) WHERE is_connected = TRUE;

-- ============================================================================
-- 第五部分：添加外键约束
-- ============================================================================

-- 应用服务表外键
ALTER TABLE application_service_events ADD CONSTRAINT fk_as_events_as_id 
    FOREIGN KEY (as_id) REFERENCES application_services(id) ON DELETE CASCADE;

ALTER TABLE application_service_state ADD CONSTRAINT fk_as_state_as_id 
    FOREIGN KEY (as_id) REFERENCES application_services(id) ON DELETE CASCADE;

ALTER TABLE application_service_transactions ADD CONSTRAINT fk_as_txns_as_id 
    FOREIGN KEY (as_id) REFERENCES application_services(id) ON DELETE CASCADE;

ALTER TABLE application_service_namespaces ADD CONSTRAINT fk_as_namespaces_as_id 
    FOREIGN KEY (as_id) REFERENCES application_services(id) ON DELETE CASCADE;

ALTER TABLE application_service_users ADD CONSTRAINT fk_as_users_as_id 
    FOREIGN KEY (as_id) REFERENCES application_services(id) ON DELETE CASCADE;

-- ============================================================================
-- 完成提示
-- ============================================================================

DO $$
BEGIN
    RAISE NOTICE '==========================================';
    RAISE NOTICE '数据库字段规范化迁移完成';
    RAISE NOTICE '创建时间: %', NOW();
    RAISE NOTICE '变更内容:';
    RAISE NOTICE '  - 统一 ID 类型（VARCHAR → TEXT）';
    RAISE NOTICE '  - 修复布尔字段命名（添加 is_ 前缀）';
    RAISE NOTICE '  - 添加应用服务表（5个）';
    RAISE NOTICE '  - 添加 Worker 表（3个）';
    RAISE NOTICE '==========================================';
END $$;
