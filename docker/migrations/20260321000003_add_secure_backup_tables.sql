-- E2EE Phase 3: Secure Key Backup Tables
-- Migration: 20260321000003_add_secure_backup_tables.sql
-- Description: Add secure key backup tables with passphrase encryption
-- Database: PostgreSQL

-- =====================================================
-- Table: secure_key_backups
-- Purpose: Store secure backup metadata with passphrase-derived keys
-- =====================================================
CREATE TABLE IF NOT EXISTS secure_key_backups (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    backup_id VARCHAR(255) NOT NULL,
    version VARCHAR(50) NOT NULL,
    algorithm VARCHAR(50) NOT NULL DEFAULT 'm.megolm_backup.v1.secure',
    auth_data JSONB NOT NULL,
    key_count BIGINT DEFAULT 0,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (user_id, backup_id)
);

CREATE INDEX IF NOT EXISTS idx_secure_backup_user ON secure_key_backups(user_id);
CREATE INDEX IF NOT EXISTS idx_secure_backup_id ON secure_key_backups(backup_id);

-- =====================================================
-- Table: secure_backup_session_keys
-- Purpose: Store encrypted session keys for secure backup
-- =====================================================
CREATE TABLE IF NOT EXISTS secure_backup_session_keys (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    backup_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    session_id VARCHAR(255) NOT NULL,
    encrypted_key TEXT NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (user_id, backup_id, room_id, session_id)
);

CREATE INDEX IF NOT EXISTS idx_secure_backup_keys_user ON secure_backup_session_keys(user_id, backup_id);
CREATE INDEX IF NOT EXISTS idx_secure_backup_keys_session ON secure_backup_session_keys(session_id);

-- Insert migration record
INSERT INTO schema_migrations (version, description, applied_at)
VALUES ('20260321000003', 'Add secure key backup tables for E2EE Phase 3', CURRENT_TIMESTAMP)
ON CONFLICT (version) DO UPDATE SET applied_at = CURRENT_TIMESTAMP;
