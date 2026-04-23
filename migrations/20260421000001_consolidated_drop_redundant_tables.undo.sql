-- Undo: Consolidated Drop Redundant Tables (recreate all)

-- ===== From: 20260422000003_drop_redundant_tables_phase_d.undo.sql =====
-- Undo: Recreate dropped redundant tables (Phase D - retention)

CREATE TABLE IF NOT EXISTS retention_cleanup_queue (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    event_id VARCHAR(255),
    event_type VARCHAR(255),
    origin_server_ts BIGINT NOT NULL,
    scheduled_ts BIGINT NOT NULL,
    status VARCHAR(50) DEFAULT 'pending',
    created_ts BIGINT NOT NULL,
    processed_ts BIGINT,
    error_message TEXT,
    retry_count INT DEFAULT 0
);

CREATE TABLE IF NOT EXISTS retention_cleanup_logs (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    events_deleted BIGINT DEFAULT 0,
    state_events_deleted BIGINT DEFAULT 0,
    media_deleted BIGINT DEFAULT 0,
    bytes_freed BIGINT DEFAULT 0,
    started_ts BIGINT NOT NULL,
    completed_ts BIGINT,
    status VARCHAR(50) DEFAULT 'running',
    error_message TEXT
);

-- ===== From: 20260422000002_drop_redundant_tables_phase_c.undo.sql =====
-- Undo: Recreate dropped redundant tables (Phase C)
-- These tables were over-engineered; if restored, they will be empty.

CREATE TABLE IF NOT EXISTS worker_load_stats (
    id BIGSERIAL PRIMARY KEY,
    worker_id VARCHAR(255) NOT NULL,
    cpu_usage REAL,
    memory_usage BIGINT,
    active_connections INT,
    requests_per_second REAL,
    average_latency_ms REAL,
    queue_depth INT,
    recorded_ts BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS worker_connections (
    id BIGSERIAL PRIMARY KEY,
    source_worker_id VARCHAR(255) NOT NULL,
    target_worker_id VARCHAR(255) NOT NULL,
    connection_type VARCHAR(100) NOT NULL,
    status VARCHAR(50) DEFAULT 'connected',
    established_ts BIGINT,
    last_activity_ts BIGINT,
    bytes_sent BIGINT DEFAULT 0,
    bytes_received BIGINT DEFAULT 0,
    messages_sent BIGINT DEFAULT 0,
    messages_received BIGINT DEFAULT 0,
    UNIQUE(source_worker_id, target_worker_id, connection_type)
);

CREATE TABLE IF NOT EXISTS retention_stats (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL UNIQUE,
    total_events BIGINT DEFAULT 0,
    events_in_retention BIGINT DEFAULT 0,
    events_expired BIGINT DEFAULT 0,
    last_cleanup_ts BIGINT,
    next_cleanup_ts BIGINT
);

CREATE TABLE IF NOT EXISTS deleted_events_index (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    event_id VARCHAR(255) NOT NULL,
    deletion_ts BIGINT NOT NULL,
    reason TEXT
);

CREATE TABLE IF NOT EXISTS event_report_history (
    id BIGSERIAL PRIMARY KEY,
    report_id BIGINT NOT NULL,
    action VARCHAR(100) NOT NULL,
    actor_user_id VARCHAR(255),
    actor_role VARCHAR(100),
    old_status VARCHAR(50),
    new_status VARCHAR(50),
    reason TEXT,
    created_ts BIGINT NOT NULL,
    metadata JSONB
);

CREATE TABLE IF NOT EXISTS event_report_stats (
    id BIGSERIAL PRIMARY KEY,
    stat_date DATE NOT NULL,
    total_reports BIGINT DEFAULT 0,
    open_reports BIGINT DEFAULT 0,
    resolved_reports BIGINT DEFAULT 0,
    dismissed_reports BIGINT DEFAULT 0,
    avg_resolution_time_ms BIGINT,
    created_ts BIGINT,
    updated_ts BIGINT
);

CREATE TABLE IF NOT EXISTS spam_check_results (
    id BIGSERIAL PRIMARY KEY,
    event_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    sender VARCHAR(255) NOT NULL,
    event_type VARCHAR(255) NOT NULL,
    content JSONB,
    result VARCHAR(50) NOT NULL,
    score INT DEFAULT 0,
    reason TEXT,
    checker_module VARCHAR(255) NOT NULL,
    checked_ts BIGINT NOT NULL,
    action_taken VARCHAR(100),
    UNIQUE(event_id, checker_module)
);

CREATE TABLE IF NOT EXISTS third_party_rule_results (
    id BIGSERIAL PRIMARY KEY,
    event_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    sender VARCHAR(255) NOT NULL,
    event_type VARCHAR(255) NOT NULL,
    rule_name VARCHAR(255) NOT NULL,
    allowed BOOLEAN NOT NULL,
    reason TEXT,
    modified_content JSONB,
    checked_ts BIGINT NOT NULL,
    UNIQUE(event_id, rule_name)
);

CREATE TABLE IF NOT EXISTS rate_limit_callbacks (
    id BIGSERIAL PRIMARY KEY,
    callback_name VARCHAR(255),
    callback_type VARCHAR(100),
    user_id VARCHAR(255),
    ip_address VARCHAR(100),
    rate_limit_type VARCHAR(100),
    result VARCHAR(100),
    config JSONB,
    is_enabled BOOLEAN DEFAULT TRUE,
    priority INT DEFAULT 100,
    created_ts BIGINT DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    updated_ts BIGINT
);

-- ===== From: 20260422000001_drop_redundant_tables_phase_b.undo.sql =====
-- Undo: Recreate dropped redundant tables (Phase B)

CREATE TABLE IF NOT EXISTS password_policy (
    name VARCHAR(100) PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS key_rotation_history (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255) NOT NULL,
    key_id VARCHAR(255),
    rotated_ts BIGINT,
    revoked BOOLEAN DEFAULT FALSE,
    created_ts BIGINT DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT
);

CREATE TABLE IF NOT EXISTS presence_routes (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255),
    presence_server VARCHAR(500),
    route_name VARCHAR(255),
    route_type VARCHAR(100),
    config JSONB,
    is_enabled BOOLEAN DEFAULT TRUE,
    priority INT DEFAULT 100,
    updated_ts BIGINT,
    created_ts BIGINT DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT
);

CREATE TABLE IF NOT EXISTS password_auth_providers (
    id BIGSERIAL PRIMARY KEY,
    provider_name VARCHAR(255) NOT NULL,
    provider_type VARCHAR(100) NOT NULL,
    config JSONB,
    is_enabled BOOLEAN DEFAULT TRUE,
    priority INT DEFAULT 100,
    created_ts BIGINT DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    updated_ts BIGINT
);

-- ===== From: 20260421000001_drop_unused_tables.undo.sql =====
-- Undo: recreate dropped tables (for rollback only)

CREATE TABLE IF NOT EXISTS private_sessions (
    id VARCHAR(255) NOT NULL,
    user_id_1 VARCHAR(255) NOT NULL,
    user_id_2 VARCHAR(255) NOT NULL,
    session_type VARCHAR(50) DEFAULT 'direct',
    encryption_key VARCHAR(255),
    created_ts BIGINT NOT NULL,
    last_activity_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    unread_count INTEGER DEFAULT 0,
    encrypted_content TEXT,
    CONSTRAINT pk_private_sessions PRIMARY KEY (id),
    CONSTRAINT uq_private_sessions_users UNIQUE (user_id_1, user_id_2)
);

CREATE TABLE IF NOT EXISTS private_messages (
    id BIGSERIAL,
    session_id VARCHAR(255) NOT NULL,
    sender_id VARCHAR(255) NOT NULL,
    content TEXT NOT NULL,
    encrypted_content TEXT,
    created_ts BIGINT NOT NULL,
    message_type VARCHAR(50) DEFAULT 'm.text',
    is_read BOOLEAN DEFAULT FALSE,
    read_by_receiver BOOLEAN DEFAULT FALSE,
    read_ts BIGINT,
    edit_history JSONB,
    is_deleted BOOLEAN DEFAULT FALSE,
    deleted_at BIGINT,
    is_edited BOOLEAN DEFAULT FALSE,
    unread_count INTEGER DEFAULT 0,
    CONSTRAINT pk_private_messages PRIMARY KEY (id)
);

CREATE TABLE IF NOT EXISTS room_children (
    id BIGSERIAL PRIMARY KEY,
    parent_room_id TEXT NOT NULL,
    child_room_id TEXT NOT NULL,
    state_key TEXT,
    content JSONB NOT NULL DEFAULT '{}',
    suggested BOOLEAN NOT NULL DEFAULT FALSE,
    created_ts BIGINT NOT NULL DEFAULT 0,
    updated_ts BIGINT
);

CREATE TABLE IF NOT EXISTS ip_reputation (
    id BIGSERIAL,
    ip_address TEXT NOT NULL,
    score INTEGER DEFAULT 0,
    last_seen_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    details JSONB,
    CONSTRAINT pk_ip_reputation PRIMARY KEY (id),
    CONSTRAINT uq_ip_reputation_ip UNIQUE (ip_address)
);

