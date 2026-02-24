-- =============================================================================
-- Synapse-Rust 数据库迁移脚本
-- 版本: 20260225000001
-- 描述: 创建密钥请求表
-- =============================================================================

SET client_encoding = 'UTF8';
SET standard_conforming_strings = on;

-- =============================================================================
-- 密钥请求表
-- =============================================================================

CREATE TABLE IF NOT EXISTS e2ee_key_requests (
    id BIGSERIAL PRIMARY KEY,
    request_id VARCHAR(255) NOT NULL UNIQUE,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    session_id VARCHAR(255) NOT NULL,
    algorithm VARCHAR(100) NOT NULL,
    action VARCHAR(50) NOT NULL DEFAULT 'request',
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    fulfilled BOOLEAN DEFAULT FALSE,
    fulfilled_by_device VARCHAR(255),
    fulfilled_ts BIGINT,
    UNIQUE(request_id, user_id, device_id)
);

CREATE INDEX IF NOT EXISTS idx_e2ee_key_requests_user_id 
    ON e2ee_key_requests(user_id);

CREATE INDEX IF NOT EXISTS idx_e2ee_key_requests_session_id 
    ON e2ee_key_requests(session_id);

CREATE INDEX IF NOT EXISTS idx_e2ee_key_requests_fulfilled 
    ON e2ee_key_requests(fulfilled) WHERE fulfilled = FALSE;

-- =============================================================================
-- 记录迁移版本
-- =============================================================================

INSERT INTO schema_migrations (version, description, success)
VALUES ('20260225000001', 'Create key requests table', TRUE)
ON CONFLICT (version) DO UPDATE SET success = TRUE, executed_at = NOW();
