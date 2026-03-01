-- Fix missing columns for database migration errors
-- This script adds missing columns that cause migration failures

-- Fix token_blacklist table - add revoked_at column
DO $$ 
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'token_blacklist' 
        AND column_name = 'revoked_at'
    ) THEN
        ALTER TABLE token_blacklist ADD COLUMN revoked_at BIGINT;
    END IF;
END $$;

-- Fix users table - add name column
DO $$ 
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'users' 
        AND column_name = 'name'
    ) THEN
        ALTER TABLE users ADD COLUMN name VARCHAR(255);
    END IF;
END $$;

-- Fix federation_blacklist_log table - add created_at column
DO $$ 
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'federation_blacklist_log' 
        AND column_name = 'created_at'
    ) THEN
        ALTER TABLE federation_blacklist_log ADD COLUMN created_at BIGINT;
    END IF;
END $$;

-- Fix module_execution_logs table - add created_at column
DO $$ 
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'module_execution_logs' 
        AND column_name = 'created_at'
    ) THEN
        ALTER TABLE module_execution_logs ADD COLUMN created_at BIGINT;
    END IF;
END $$;

-- Create missing tables if they don't exist
CREATE TABLE IF NOT EXISTS token_blacklist (
    id SERIAL PRIMARY KEY,
    token_jti VARCHAR(255),
    user_id VARCHAR(255),
    revoked_at BIGINT,
    reason VARCHAR(255)
);

CREATE TABLE IF NOT EXISTS federation_blacklist_log (
    id SERIAL PRIMARY KEY,
    server_name VARCHAR(255),
    action VARCHAR(50),
    reason TEXT,
    created_at BIGINT
);

CREATE TABLE IF NOT EXISTS module_execution_logs (
    id SERIAL PRIMARY KEY,
    module_name VARCHAR(255),
    execution_time_ms BIGINT,
    success BOOLEAN,
    error_message TEXT,
    created_at BIGINT
);

-- Recreate indexes that failed
CREATE INDEX IF NOT EXISTS idx_token_blacklist_revoked_at ON token_blacklist(revoked_at);
CREATE INDEX IF NOT EXISTS idx_users_name ON users(name);
CREATE INDEX IF NOT EXISTS idx_federation_blacklist_log_created ON federation_blacklist_log(created_at);
CREATE INDEX IF NOT EXISTS idx_module_execution_logs_created ON module_execution_logs(created_at);
