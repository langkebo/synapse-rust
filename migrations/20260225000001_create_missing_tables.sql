-- ============================================================================
-- Missing Tables and Columns Migration Script
-- ============================================================================
-- Version: 20260225000001
-- Created: 2026-02-25
-- ============================================================================

BEGIN;

-- ============================================================================
-- Room Summaries Tables - Complete Schema
-- ============================================================================
CREATE TABLE IF NOT EXISTS room_summaries (
    id SERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL UNIQUE,
    creator VARCHAR(255),
    name VARCHAR(255),
    topic TEXT,
    avatar_url VARCHAR(512),
    canonical_alias VARCHAR(255),
    room_type VARCHAR(50),
    is_public BOOLEAN DEFAULT false,
    is_direct BOOLEAN DEFAULT false,
    is_space BOOLEAN DEFAULT false,
    is_encrypted BOOLEAN DEFAULT false,
    encryption_algorithm VARCHAR(50),
    join_rules VARCHAR(50),
    world_readable BOOLEAN DEFAULT false,
    guest_can_join BOOLEAN DEFAULT false,
    guest_access VARCHAR(50),
    history_visibility VARCHAR(50),
    federation_allowed BOOLEAN DEFAULT true,
    member_count INTEGER DEFAULT 0,
    joined_member_count INTEGER DEFAULT 0,
    invited_member_count INTEGER DEFAULT 0,
    joined_local_members INTEGER DEFAULT 0,
    invited_local_members INTEGER DEFAULT 0,
    active_member_count INTEGER DEFAULT 0,
    user_online_count INTEGER DEFAULT 0,
    user_active_count INTEGER DEFAULT 0,
    hero_users JSONB,
    stripped_state JSONB,
    pinned_events JSONB,
    room_id_version VARCHAR(50) DEFAULT '1',
    reference_timestamp BIGINT,
    is_partial_state_room BOOLEAN DEFAULT false,
    last_event_id VARCHAR(255),
    last_event_timestamp BIGINT,
    total_events BIGINT DEFAULT 0,
    state_events INTEGER DEFAULT 0,
    membership_events INTEGER DEFAULT 0,
    message_events INTEGER DEFAULT 0,
    created_ts BIGINT,
    updated_ts BIGINT
);

CREATE TABLE IF NOT EXISTS room_summary_members (
    id SERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    display_name VARCHAR(255),
    avatar_url VARCHAR(512),
    membership VARCHAR(50),
    updated_ts BIGINT,
    UNIQUE(room_id, user_id)
);

CREATE TABLE IF NOT EXISTS room_summary_state (
    id SERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL UNIQUE,
    state_json JSONB,
    updated_ts BIGINT
);

CREATE TABLE IF NOT EXISTS room_summary_stats (
    id SERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL UNIQUE,
    total_messages BIGINT DEFAULT 0,
    total_events BIGINT DEFAULT 0,
    size_bytes BIGINT DEFAULT 0,
    last_activity_ts BIGINT,
    updated_ts BIGINT
);

-- ============================================================================
-- Worker Tables (for multi-worker support)
-- ============================================================================
CREATE TABLE IF NOT EXISTS active_workers (
    worker_id VARCHAR(255) PRIMARY KEY,
    worker_type VARCHAR(50) NOT NULL,
    instance_name VARCHAR(255),
    status VARCHAR(50) DEFAULT 'running',
    started_ts BIGINT NOT NULL,
    last_heartbeat BIGINT,
    pid INTEGER,
    cpu_usage FLOAT,
    memory_usage BIGINT,
    updated_ts BIGINT
);

CREATE TABLE IF NOT EXISTS worker_commands (
    id SERIAL PRIMARY KEY,
    worker_id VARCHAR(255) NOT NULL,
    command VARCHAR(255) NOT NULL,
    params JSONB,
    result JSONB,
    status VARCHAR(50) DEFAULT 'pending',
    created_ts BIGINT NOT NULL,
    completed_ts BIGINT,
    FOREIGN KEY (worker_id) REFERENCES active_workers(worker_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS worker_task_assignments (
    id SERIAL PRIMARY KEY,
    task_id VARCHAR(255) NOT NULL UNIQUE,
    worker_id VARCHAR(255),
    task_type VARCHAR(50),
    status VARCHAR(50) DEFAULT 'pending',
    task_data JSONB,
    assigned_ts BIGINT,
    started_ts BIGINT,
    completed_ts BIGINT,
    FOREIGN KEY (worker_id) REFERENCES active_workers(worker_id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS worker_events (
    id SERIAL PRIMARY KEY,
    worker_id VARCHAR(255) NOT NULL,
    event_type VARCHAR(100) NOT NULL,
    event_data JSONB,
    created_ts BIGINT NOT NULL,
    FOREIGN KEY (worker_id) REFERENCES active_workers(worker_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS worker_statistics (
    id SERIAL PRIMARY KEY,
    worker_id VARCHAR(255) NOT NULL,
    stat_type VARCHAR(100) NOT NULL,
    stat_value JSONB NOT NULL,
    recorded_ts BIGINT NOT NULL,
    FOREIGN KEY (worker_id) REFERENCES active_workers(worker_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS worker_type_statistics (
    id SERIAL PRIMARY KEY,
    worker_type VARCHAR(50) NOT NULL,
    instance_count INTEGER DEFAULT 0,
    total_tasks INTEGER DEFAULT 0,
    active_tasks INTEGER DEFAULT 0,
    avg_response_time FLOAT,
    recorded_ts BIGINT NOT NULL,
    UNIQUE(worker_type)
);

-- ============================================================================
-- Retention Tables
-- ============================================================================
CREATE TABLE IF NOT EXISTS room_retention_policies (
    room_id VARCHAR(255) PRIMARY KEY,
    min_lifetime BIGINT,
    max_lifetime BIGINT,
    expires_at TIMESTAMP WITH TIME ZONE,
    created_ts BIGINT,
    updated_ts BIGINT
);

CREATE TABLE IF NOT EXISTS retention_stats (
    room_id VARCHAR(255) PRIMARY KEY,
    events_deleted BIGINT DEFAULT 0,
    bytes_deleted BIGINT DEFAULT 0,
    last_cleanup_ts BIGINT,
    updated_ts BIGINT
);

CREATE TABLE IF NOT EXISTS deleted_events_index (
    id SERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    event_id VARCHAR(255) NOT NULL,
    original_ts BIGINT,
    deleted_ts BIGINT NOT NULL,
    reason TEXT
);

CREATE INDEX IF NOT EXISTS idx_deleted_events_room ON deleted_events_index(room_id);
CREATE INDEX IF NOT EXISTS idx_deleted_events_ts ON deleted_events_index(deleted_ts);

CREATE TABLE IF NOT EXISTS retention_cleanup_queue (
    id SERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    min_ts BIGINT,
    max_ts BIGINT,
    status VARCHAR(50) DEFAULT 'pending',
    scheduled_ts BIGINT,
    started_ts BIGINT,
    completed_ts BIGINT,
    events_processed INTEGER DEFAULT 0,
    UNIQUE(room_id)
);

CREATE TABLE IF NOT EXISTS server_retention_policy (
    id SERIAL PRIMARY KEY,
    min_lifetime BIGINT,
    max_lifetime BIGINT,
    allow_default BOOLEAN DEFAULT true,
    is_default BOOLEAN DEFAULT false,
    updated_ts BIGINT
);

-- ============================================================================
-- Application Service Tables
-- ============================================================================
CREATE TABLE IF NOT EXISTS application_service_state (
    id SERIAL PRIMARY KEY,
    appservice_id VARCHAR(255) NOT NULL,
    state VARCHAR(50) NOT NULL,
    last_check_ts BIGINT,
    error_message TEXT,
    UNIQUE(appservice_id)
);

CREATE TABLE IF NOT EXISTS application_service_users (
    id SERIAL PRIMARY KEY,
    appservice_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    created_ts BIGINT,
    UNIQUE(appservice_id, user_id)
);

CREATE TABLE IF NOT EXISTS application_service_user_namespaces (
    id SERIAL PRIMARY KEY,
    appservice_id VARCHAR(255) NOT NULL,
    namespace_type VARCHAR(50) NOT NULL,
    pattern VARCHAR(255) NOT NULL,
    created_ts BIGINT,
    UNIQUE(appservice_id, namespace_type, pattern)
);

CREATE TABLE IF NOT EXISTS application_service_events (
    id SERIAL PRIMARY KEY,
    appservice_id VARCHAR(255) NOT NULL,
    event_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255),
    event_type VARCHAR(100),
    processed BOOLEAN DEFAULT false,
    created_ts BIGINT,
    processed_ts BIGINT,
    UNIQUE(appservice_id, event_id)
);

CREATE TABLE IF NOT EXISTS application_service_statistics (
    id SERIAL PRIMARY KEY,
    appservice_id VARCHAR(255) NOT NULL,
    rooms_count INTEGER DEFAULT 0,
    users_count INTEGER DEFAULT 0,
    messages_count BIGINT DEFAULT 0,
    recorded_ts BIGINT NOT NULL,
    UNIQUE(appservice_id, recorded_ts)
);

-- ============================================================================
-- Space Tables
-- ============================================================================
CREATE TABLE IF NOT EXISTS space_statistics (
    space_id VARCHAR(255) PRIMARY KEY,
    child_count INTEGER DEFAULT 0,
    member_count INTEGER DEFAULT 0,
    updated_ts BIGINT
);

-- ============================================================================
-- Module Tables
-- ============================================================================
CREATE TABLE IF NOT EXISTS spam_check_results (
    id SERIAL PRIMARY KEY,
    event_id VARCHAR(255) NOT NULL UNIQUE,
    sender VARCHAR(255) NOT NULL,
    room_id VARCHAR(255),
    result VARCHAR(50) NOT NULL,
    reason TEXT,
    checked_ts BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS third_party_rule_results (
    id SERIAL PRIMARY KEY,
    event_id VARCHAR(255) NOT NULL,
    rule_type VARCHAR(50) NOT NULL,
    rule_action VARCHAR(50) NOT NULL,
    result VARCHAR(50) NOT NULL,
    reason TEXT,
    evaluated_ts BIGINT NOT NULL,
    UNIQUE(event_id, rule_type)
);

-- ============================================================================
-- Account Validity Tables
-- ============================================================================
CREATE TABLE IF NOT EXISTS account_validity (
    user_id VARCHAR(255) PRIMARY KEY,
    expiration_ts BIGINT,
    renewed_ts BIGINT,
    allow_renewal BOOLEAN DEFAULT true
);

-- ============================================================================
-- Password Auth Providers
-- ============================================================================
CREATE TABLE IF NOT EXISTS password_auth_providers (
    id SERIAL PRIMARY KEY,
    provider_id VARCHAR(255) NOT NULL UNIQUE,
    enabled BOOLEAN DEFAULT true,
    config JSONB,
    created_ts BIGINT,
    updated_ts BIGINT
);

-- ============================================================================
-- Presence Routes
-- ============================================================================
CREATE TABLE IF NOT EXISTS presence_routes (
    id SERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL UNIQUE,
    presence_server VARCHAR(255),
    updated_ts BIGINT
);

-- ============================================================================
-- Callback Tables
-- ============================================================================
CREATE TABLE IF NOT EXISTS media_callbacks (
    id SERIAL PRIMARY KEY,
    callback_type VARCHAR(50) NOT NULL,
    media_id VARCHAR(255),
    user_id VARCHAR(255),
    status VARCHAR(50),
    result JSONB,
    created_ts BIGINT,
    completed_ts BIGINT
);

CREATE TABLE IF NOT EXISTS rate_limit_callbacks (
    id SERIAL PRIMARY KEY,
    callback_type VARCHAR(50) NOT NULL,
    user_id VARCHAR(255),
    ip_address VARCHAR(45),
    rate_limit_type VARCHAR(50),
    result JSONB,
    created_ts BIGINT
);

CREATE TABLE IF NOT EXISTS account_data_callbacks (
    id SERIAL PRIMARY KEY,
    callback_type VARCHAR(50) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    data_type VARCHAR(100),
    result JSONB,
    created_ts BIGINT
);

-- ============================================================================
-- Federation Stats
-- ============================================================================
CREATE TABLE IF NOT EXISTS federation_access_stats (
    id SERIAL PRIMARY KEY,
    server_name VARCHAR(255) NOT NULL,
    direction VARCHAR(20) NOT NULL,
    requests_count BIGINT DEFAULT 0,
    bytes_sent BIGINT DEFAULT 0,
    bytes_received BIGINT DEFAULT 0,
    error_count INTEGER DEFAULT 0,
    recorded_ts BIGINT NOT NULL,
    UNIQUE(server_name, direction, recorded_ts)
);

-- ============================================================================
-- Room Aliases
-- ============================================================================
CREATE TABLE IF NOT EXISTS room_aliases (
    room_alias VARCHAR(255) PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    server_name VARCHAR(255) NOT NULL,
    created_ts BIGINT,
    updated_ts BIGINT
);

-- ============================================================================
-- Set admin user as admin (if not already set)
-- ============================================================================
UPDATE users SET is_admin = true WHERE username = 'admin' AND is_admin = false;

COMMIT;
