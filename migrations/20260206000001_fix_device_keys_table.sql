-- 修复 device_keys 表结构以匹配代码期望
-- 执行时间: 2026-02-06

-- 删除旧表
DROP TABLE IF EXISTS device_keys CASCADE;

-- 创建新的 device_keys 表
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

-- 创建索引
CREATE INDEX idx_device_keys_user ON device_keys(user_id);
CREATE INDEX idx_device_keys_device ON device_keys(device_id);
CREATE INDEX idx_device_keys_key_id ON device_keys(key_id);
CREATE INDEX idx_device_keys_verified ON device_keys(verified) WHERE verified = TRUE;
CREATE INDEX idx_device_keys_ts ON device_keys(ts_last_accessed);
