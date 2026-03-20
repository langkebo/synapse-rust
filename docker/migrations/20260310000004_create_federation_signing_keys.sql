-- 创建联邦签名密钥表
-- 用于存储联邦服务器的签名密钥

CREATE TABLE IF NOT EXISTS federation_signing_keys (
    id SERIAL,
    server_name TEXT NOT NULL,
    key_id TEXT NOT NULL,
    secret_key TEXT NOT NULL,
    public_key TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    expires_at BIGINT,
    key_json TEXT,
    ts_added_ms BIGINT,
    ts_valid_until_ms BIGINT,
    PRIMARY KEY (server_name, key_id)
);

-- 创建索引
CREATE INDEX IF NOT EXISTS idx_federation_signing_keys_server ON federation_signing_keys(server_name);
CREATE INDEX IF NOT EXISTS idx_federation_signing_keys_key_id ON federation_signing_keys(key_id);
CREATE INDEX IF NOT EXISTS idx_federation_signing_keys_expires ON federation_signing_keys(expires_at);

-- 插入默认签名密钥
INSERT INTO federation_signing_keys (server_name, key_id, secret_key, public_key, created_ts, expires_at, key_json, ts_added_ms, ts_valid_until_ms)
VALUES (
    'cjystx.top',
    'ed25519:test4BGgmI',
    'vOF91ju/RyF0Tx0KVPHNwViHVviGIsF5GtHaAwKkzR8',
    'test_public_key_placeholder',
    EXTRACT(EPOCH FROM NOW())::BIGINT * 1000,
    NULL,
    '{"server_name":"cjystx.top","old_verify_keys":{},"valid_until_ts":' || (EXTRACT(EPOCH FROM NOW())::BIGINT * 1000 + 86400000) || ',"verify_keys":{"ed25519:test4BGgmI":{"key":"test_public_key_placeholder"}}}',
    EXTRACT(EPOCH FROM NOW())::BIGINT * 1000,
    EXTRACT(EPOCH FROM NOW())::BIGINT * 1000 + 86400000
) ON CONFLICT (server_name, key_id) DO NOTHING;
