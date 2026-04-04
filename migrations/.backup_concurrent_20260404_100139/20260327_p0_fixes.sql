-- P0 Fixes Migration Script
-- Date: 2026-03-27
-- Issues Fixed:
--   1. olm_accounts table - ensure proper schema with Rust model compatibility
--   2. olm_sessions table - ensure proper schema with Rust model compatibility

BEGIN;

-- ============================================================
-- Olm Accounts Table
-- Purpose: Stores Olm account information for E2EE
-- ============================================================
CREATE TABLE IF NOT EXISTS olm_accounts (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    identity_key TEXT NOT NULL,
    serialized_account TEXT NOT NULL,
    is_one_time_keys_published BOOLEAN NOT NULL DEFAULT FALSE,
    is_fallback_key_published BOOLEAN NOT NULL DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT uq_olm_accounts_user_device UNIQUE (user_id, device_id)
);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_olm_accounts_user_device ON olm_accounts(user_id, device_id);

-- ============================================================
-- Olm Sessions Table
-- Purpose: Stores Olm session information for E2EE
-- ============================================================
CREATE TABLE IF NOT EXISTS olm_sessions (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    sender_key TEXT NOT NULL,
    receiver_key TEXT NOT NULL,
    serialized_state TEXT NOT NULL,
    message_index INTEGER NOT NULL DEFAULT 0,
    created_ts BIGINT NOT NULL,
    last_used_ts BIGINT NOT NULL,
    expires_at BIGINT,
    CONSTRAINT uq_olm_sessions_session UNIQUE (session_id)
);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_olm_sessions_user_device ON olm_sessions(user_id, device_id);
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_olm_sessions_sender_key ON olm_sessions(sender_key);
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_olm_sessions_expires_at ON olm_sessions(expires_at) WHERE expires_at IS NOT NULL;

-- ============================================================
-- Track Migration in schema_migrations table
-- ============================================================
INSERT INTO schema_migrations (version, description, applied_ts)
VALUES ('20260327_p0_fixes', 'P0 fixes: olm_accounts and olm_sessions tables', EXTRACT(EPOCH FROM NOW())::BIGINT * 1000)
ON CONFLICT (version) DO NOTHING;

COMMIT;