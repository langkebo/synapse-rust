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
