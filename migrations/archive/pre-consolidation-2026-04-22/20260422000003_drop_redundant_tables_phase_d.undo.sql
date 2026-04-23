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
