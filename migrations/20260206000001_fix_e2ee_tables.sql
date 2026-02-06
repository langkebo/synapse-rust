-- 修复 E2EE 相关表结构以匹配代码期望
-- 执行时间: 2026-02-06

-- 1. 修复 device_keys 表
DROP TABLE IF EXISTS device_keys CASCADE;

CREATE TABLE device_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255) NOT NULL,
    display_name VARCHAR(255),
    algorithm VARCHAR(100),
    key_id VARCHAR(255) NOT NULL,
    public_key TEXT NOT NULL,
    signatures JSONB,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    ts_updated_ms BIGINT NOT NULL,
    key_json JSONB NOT NULL DEFAULT '{}',
    ts_added_ms BIGINT NOT NULL,
    ts_last_accessed BIGINT NOT NULL,
    verified BOOLEAN DEFAULT FALSE,
    blocked BOOLEAN DEFAULT FALSE,
    UNIQUE (user_id, device_id, key_id),
    FOREIGN KEY (user_id, device_id) REFERENCES devices(device_id, user_id) ON DELETE CASCADE
);

CREATE INDEX idx_device_keys_user ON device_keys(user_id);
CREATE INDEX idx_device_keys_device ON device_keys(device_id);
CREATE INDEX idx_device_keys_key_id ON device_keys(key_id);
CREATE INDEX idx_device_keys_verified ON device_keys(verified) WHERE verified = TRUE;
CREATE INDEX idx_device_keys_ts ON device_keys(ts_last_accessed);

-- 2. 修复 to_device_messages 表
DROP TABLE IF EXISTS to_device_messages CASCADE;

CREATE TABLE to_device_messages (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255) NOT NULL,
    message_type VARCHAR(255) NOT NULL,
    content JSONB NOT NULL,
    created_ts BIGINT NOT NULL,
    processed_ts BIGINT,
    FOREIGN KEY (user_id, device_id) REFERENCES devices(device_id, user_id) ON DELETE CASCADE
);

CREATE INDEX idx_to_device_user ON to_device_messages(user_id);
CREATE INDEX idx_to_device_device ON to_device_messages(device_id);
CREATE INDEX idx_to_device_created ON to_device_messages(created_ts DESC);

-- 3. 确保设备表存在
CREATE TABLE IF NOT EXISTS devices (
    device_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    display_name VARCHAR(255),
    last_seen_ip VARCHAR(45),
    last_seen_ts BIGINT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    PRIMARY KEY (user_id, device_id)
);

CREATE INDEX IF NOT EXISTS idx_devices_user ON devices(user_id);
CREATE INDEX IF NOT EXISTS idx_devices_last_seen ON devices(last_seen_ts DESC);
