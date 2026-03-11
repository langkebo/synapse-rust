-- ============================================================================
-- 添加缺失的 E2EE 表
-- 创建日期: 2026-03-10
-- 说明: 添加 olm_accounts, olm_sessions, e2ee_key_requests 表到统一 Schema
-- ============================================================================

-- ============================================================================
-- Olm 账户表
-- 存储 Olm 加密账户信息
-- ============================================================================
CREATE TABLE IF NOT EXISTS olm_accounts (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    identity_key TEXT NOT NULL,
    serialized_account TEXT NOT NULL,
    is_one_time_keys_published BOOLEAN DEFAULT FALSE,
    is_fallback_key_published BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT uq_olm_accounts_user_device UNIQUE (user_id, device_id)
);

CREATE INDEX IF NOT EXISTS idx_olm_accounts_user ON olm_accounts(user_id);
CREATE INDEX IF NOT EXISTS idx_olm_accounts_device ON olm_accounts(device_id);

-- ============================================================================
-- Olm 会话表
-- 存储 Olm 加密会话信息
-- ============================================================================
CREATE TABLE IF NOT EXISTS olm_sessions (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    sender_key TEXT NOT NULL,
    receiver_key TEXT NOT NULL,
    serialized_state TEXT NOT NULL,
    message_index INTEGER DEFAULT 0,
    created_ts BIGINT NOT NULL,
    last_used_ts BIGINT NOT NULL,
    expires_ts BIGINT,
    CONSTRAINT uq_olm_sessions_session UNIQUE (session_id)
);

CREATE INDEX IF NOT EXISTS idx_olm_sessions_user_device ON olm_sessions(user_id, device_id);
CREATE INDEX IF NOT EXISTS idx_olm_sessions_sender_key ON olm_sessions(sender_key);
CREATE INDEX IF NOT EXISTS idx_olm_sessions_expires ON olm_sessions(expires_ts) WHERE expires_ts IS NOT NULL;

-- ============================================================================
-- E2EE 密钥请求表
-- 存储密钥请求记录
-- ============================================================================
CREATE TABLE IF NOT EXISTS e2ee_key_requests (
    id BIGSERIAL PRIMARY KEY,
    request_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    algorithm TEXT NOT NULL,
    action TEXT NOT NULL,
    is_fulfilled BOOLEAN DEFAULT FALSE,
    fulfilled_by_device TEXT,
    fulfilled_ts BIGINT,
    created_ts BIGINT NOT NULL,
    CONSTRAINT uq_e2ee_key_requests_request UNIQUE (request_id)
);

CREATE INDEX IF NOT EXISTS idx_e2ee_key_requests_user ON e2ee_key_requests(user_id);
CREATE INDEX IF NOT EXISTS idx_e2ee_key_requests_session ON e2ee_key_requests(session_id);
CREATE INDEX IF NOT EXISTS idx_e2ee_key_requests_pending ON e2ee_key_requests(is_fulfilled) WHERE is_fulfilled = FALSE;

-- ============================================================================
-- 添加外键约束
-- ============================================================================
ALTER TABLE olm_accounts ADD CONSTRAINT fk_olm_accounts_user 
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;

ALTER TABLE olm_sessions ADD CONSTRAINT fk_olm_sessions_user 
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;

ALTER TABLE e2ee_key_requests ADD CONSTRAINT fk_e2ee_key_requests_user 
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
