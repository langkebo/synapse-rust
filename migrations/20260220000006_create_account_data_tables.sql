-- Migration: Create account_data and related tables
-- Version: 20260220000006
-- Description: 创建账户数据相关表

-- 创建 account_data 表
CREATE TABLE IF NOT EXISTS account_data (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    data_type VARCHAR(255) NOT NULL,
    content JSONB NOT NULL DEFAULT '{}',
    created_at BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    updated_at BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    UNIQUE(user_id, data_type)
);

CREATE INDEX IF NOT EXISTS idx_account_data_user ON account_data(user_id);
CREATE INDEX IF NOT EXISTS idx_account_data_type ON account_data(data_type);

-- 创建 room_account_data 表
CREATE TABLE IF NOT EXISTS room_account_data (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    data_type VARCHAR(255) NOT NULL,
    content JSONB NOT NULL DEFAULT '{}',
    created_at BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    updated_at BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    UNIQUE(user_id, room_id, data_type)
);

CREATE INDEX IF NOT EXISTS idx_room_account_data_user ON room_account_data(user_id);
CREATE INDEX IF NOT EXISTS idx_room_account_data_room ON room_account_data(room_id);

-- 创建 filters 表
CREATE TABLE IF NOT EXISTS filters (
    id BIGSERIAL PRIMARY KEY,
    filter_id VARCHAR(64) NOT NULL UNIQUE,
    user_id VARCHAR(255) NOT NULL,
    content JSONB NOT NULL DEFAULT '{}',
    created_at BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT
);

CREATE INDEX IF NOT EXISTS idx_filters_user ON filters(user_id);
CREATE INDEX IF NOT EXISTS idx_filters_filter_id ON filters(filter_id);

-- 创建 openid_tokens 表
CREATE TABLE IF NOT EXISTS openid_tokens (
    id BIGSERIAL PRIMARY KEY,
    token VARCHAR(255) NOT NULL UNIQUE,
    user_id VARCHAR(255) NOT NULL,
    created_at BIGINT NOT NULL,
    expires_at BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_openid_tokens_token ON openid_tokens(token);
CREATE INDEX IF NOT EXISTS idx_openid_tokens_user ON openid_tokens(user_id);

-- 创建 user_profiles 表 (用于 profile API)
CREATE TABLE IF NOT EXISTS user_profiles (
    user_id VARCHAR(255) PRIMARY KEY,
    displayname VARCHAR(255),
    avatar_url VARCHAR(1024),
    updated_at BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT
);

-- 创建 presence 表 (用于在线状态 API)
CREATE TABLE IF NOT EXISTS presence (
    user_id VARCHAR(255) PRIMARY KEY,
    presence VARCHAR(50) NOT NULL DEFAULT 'offline',
    status_msg TEXT,
    last_active_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    updated_at BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT
);

CREATE INDEX IF NOT EXISTS idx_presence_user ON presence(user_id);

-- 验证
DO $$
BEGIN
    RAISE NOTICE 'Account data and related tables created successfully';
END $$;
