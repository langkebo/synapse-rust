-- =============================================================================
-- Synapse-Rust 数据库迁移脚本
-- 版本: 20260225000002
-- 描述: 创建 SSSS (Secret Storage) 表
-- =============================================================================

SET client_encoding = 'UTF8';
SET standard_conforming_strings = on;

-- =============================================================================
-- SSSS 密钥存储表
-- =============================================================================

CREATE TABLE IF NOT EXISTS e2ee_secret_storage_keys (
    id BIGSERIAL PRIMARY KEY,
    key_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    algorithm VARCHAR(100) NOT NULL,
    encrypted_key TEXT NOT NULL,
    public_key VARCHAR(255),
    signatures JSONB DEFAULT '{}'::JSONB,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    updated_ts BIGINT,
    UNIQUE(key_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_e2ee_secret_storage_keys_user_id 
    ON e2ee_secret_storage_keys(user_id);

-- =============================================================================
-- SSSS 密钥存储表
-- =============================================================================

CREATE TABLE IF NOT EXISTS e2ee_stored_secrets (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    secret_name VARCHAR(255) NOT NULL,
    encrypted_secret TEXT NOT NULL,
    key_id VARCHAR(255) NOT NULL,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    updated_ts BIGINT,
    UNIQUE(user_id, secret_name)
);

CREATE INDEX IF NOT EXISTS idx_e2ee_stored_secrets_user_id 
    ON e2ee_stored_secrets(user_id);

-- =============================================================================
-- 记录迁移版本
-- =============================================================================

INSERT INTO schema_migrations (version, description, success)
VALUES ('20260225000002', 'Create SSSS tables', TRUE)
ON CONFLICT (version) DO UPDATE SET success = TRUE, executed_at = NOW();
