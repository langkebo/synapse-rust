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
