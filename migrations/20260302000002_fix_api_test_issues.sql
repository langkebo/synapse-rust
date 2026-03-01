-- ============================================================================
-- Migration: 20260302000002_fix_api_test_issues.sql
-- Created: 2026-03-02
-- Purpose: Fix database schema issues discovered during API testing
-- Issues: https://github.com/timescale/pg-aiguide/ best practices applied
-- ============================================================================

-- ============================================================================
-- 1. Application Service Tables - Fix missing as_id column
-- ============================================================================

-- Fix application_service_state table
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'application_service_state' AND column_name = 'as_id'
    ) THEN
        ALTER TABLE application_service_state ADD COLUMN as_id TEXT;
    END IF;
END $$;

-- Update existing records with empty string if as_id is NULL
UPDATE application_service_state SET as_id = '' WHERE as_id IS NULL;

-- Set NOT NULL constraint
ALTER TABLE application_service_state ALTER COLUMN as_id SET NOT NULL;

-- Create index for as_id
CREATE INDEX IF NOT EXISTS idx_app_service_state_as_id ON application_service_state(as_id);

-- Fix application_service_users table
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'application_service_users' AND column_name = 'as_id'
    ) THEN
        ALTER TABLE application_service_users ADD COLUMN as_id TEXT;
    END IF;
END $$;

UPDATE application_service_users SET as_id = '' WHERE as_id IS NULL;
ALTER TABLE application_service_users ALTER COLUMN as_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_app_service_users_as_id ON application_service_users(as_id);

-- ============================================================================
-- 2. Room Summary Tables - Fix missing columns
-- ============================================================================

-- Fix room_summaries table
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'room_summaries' AND column_name = 'last_event_ts'
    ) THEN
        ALTER TABLE room_summaries ADD COLUMN last_event_ts BIGINT;
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_room_summaries_last_event_ts 
    ON room_summaries(last_event_ts DESC NULLS LAST);

-- Fix room_summary_members table
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'room_summary_members' AND column_name = 'is_hero'
    ) THEN
        ALTER TABLE room_summary_members ADD COLUMN is_hero BOOLEAN DEFAULT FALSE;
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_room_summary_members_hero 
    ON room_summary_members(room_id, is_hero) WHERE is_hero = TRUE;

-- Fix room_summary_state table
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'room_summary_state' AND column_name = 'event_type'
    ) THEN
        ALTER TABLE room_summary_state ADD COLUMN event_type TEXT;
    END IF;
END $$;

-- Fix room_summary_stats table
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'room_summary_stats' AND column_name = 'total_state_events'
    ) THEN
        ALTER TABLE room_summary_stats ADD COLUMN total_state_events BIGINT DEFAULT 0;
    END IF;
END $$;

-- ============================================================================
-- 3. Retention Policy Tables - Fix missing columns
-- ============================================================================

-- Fix room_retention_policies table
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'room_retention_policies' AND column_name = 'id'
    ) THEN
        ALTER TABLE room_retention_policies ADD COLUMN id BIGSERIAL PRIMARY KEY;
    END IF;
END $$;

-- Fix retention_cleanup_logs table
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'retention_cleanup_logs' AND column_name = 'events_deleted'
    ) THEN
        ALTER TABLE retention_cleanup_logs ADD COLUMN events_deleted BIGINT DEFAULT 0;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'retention_cleanup_logs' AND column_name = 'state_events_deleted'
    ) THEN
        ALTER TABLE retention_cleanup_logs ADD COLUMN state_events_deleted BIGINT DEFAULT 0;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'retention_cleanup_logs' AND column_name = 'media_deleted'
    ) THEN
        ALTER TABLE retention_cleanup_logs ADD COLUMN media_deleted BIGINT DEFAULT 0;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'retention_cleanup_logs' AND column_name = 'bytes_freed'
    ) THEN
        ALTER TABLE retention_cleanup_logs ADD COLUMN bytes_freed BIGINT DEFAULT 0;
    END IF;
END $$;

-- Fix deleted_events_index table
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'deleted_events_index' AND column_name = 'deletion_ts'
    ) THEN
        ALTER TABLE deleted_events_index ADD COLUMN deletion_ts BIGINT;
    END IF;
END $$;

-- ============================================================================
-- 4. Worker Architecture Tables - Fix missing columns
-- ============================================================================

-- Fix worker_commands table
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'worker_commands' AND column_name = 'target_worker_id'
    ) THEN
        ALTER TABLE worker_commands ADD COLUMN target_worker_id TEXT;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'worker_commands' AND column_name = 'command_id'
    ) THEN
        ALTER TABLE worker_commands ADD COLUMN command_id TEXT;
    END IF;
END $$;

-- Fix worker_tasks table
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'worker_tasks' AND column_name = 'priority'
    ) THEN
        ALTER TABLE worker_tasks ADD COLUMN priority INTEGER DEFAULT 0;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'worker_tasks' AND column_name = 'assigned_worker_id'
    ) THEN
        ALTER TABLE worker_tasks ADD COLUMN assigned_worker_id TEXT;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'worker_tasks' AND column_name = 'result'
    ) THEN
        ALTER TABLE worker_tasks ADD COLUMN result JSONB;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'worker_tasks' AND column_name = 'error_message'
    ) THEN
        ALTER TABLE worker_tasks ADD COLUMN error_message TEXT;
    END IF;
END $$;

-- Fix worker_events table
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'worker_events' AND column_name = 'stream_id'
    ) THEN
        ALTER TABLE worker_events ADD COLUMN stream_id BIGINT;
    END IF;
END $$;

-- Create indexes for worker tables
CREATE INDEX IF NOT EXISTS idx_worker_commands_target ON worker_commands(target_worker_id);
CREATE INDEX IF NOT EXISTS idx_worker_tasks_priority ON worker_tasks(priority DESC);
CREATE INDEX IF NOT EXISTS idx_worker_tasks_assigned ON worker_tasks(assigned_worker_id);

-- ============================================================================
-- 5. CAS Authentication Tables - Fix missing columns and tables
-- ============================================================================

-- Fix cas_tickets table
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'cas_tickets' AND column_name = 'ticket_id'
    ) THEN
        ALTER TABLE cas_tickets ADD COLUMN ticket_id TEXT UNIQUE;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'cas_tickets' AND column_name = 'created_at'
    ) THEN
        ALTER TABLE cas_tickets ADD COLUMN created_at BIGINT;
    END IF;
END $$;

-- Fix cas_services table
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'cas_services' AND column_name = 'created_at'
    ) THEN
        ALTER TABLE cas_services ADD COLUMN created_at BIGINT;
    END IF;
END $$;

-- Create cas_proxy_tickets table if not exists
CREATE TABLE IF NOT EXISTS cas_proxy_tickets (
    id BIGSERIAL PRIMARY KEY,
    ticket_id TEXT NOT NULL UNIQUE,
    proxy_ticket TEXT NOT NULL,
    target_service TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    expires_ts BIGINT
);

CREATE INDEX IF NOT EXISTS idx_cas_proxy_tickets_ticket ON cas_proxy_tickets(ticket_id);
CREATE INDEX IF NOT EXISTS idx_cas_proxy_tickets_service ON cas_proxy_tickets(target_service);

-- ============================================================================
-- 6. Federation Blacklist Table - Fix missing column
-- ============================================================================

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'federation_blacklist' AND column_name = 'block_type'
    ) THEN
        ALTER TABLE federation_blacklist ADD COLUMN block_type TEXT DEFAULT 'server';
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_federation_blacklist_type ON federation_blacklist(block_type);

-- ============================================================================
-- 7. Token Blacklist Table - Fix token_type NOT NULL constraint
-- ============================================================================

-- Ensure token_blacklist table has token_type column with default
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'token_blacklist' AND column_name = 'token_type'
    ) THEN
        ALTER TABLE token_blacklist ADD COLUMN token_type TEXT DEFAULT 'access';
    END IF;
END $$;

-- Update existing records
UPDATE token_blacklist SET token_type = 'access' WHERE token_type IS NULL;

-- Set NOT NULL constraint
ALTER TABLE token_blacklist ALTER COLUMN token_type SET NOT NULL;

-- ============================================================================
-- 8. Create server_retention_policy table if not exists
-- ============================================================================

CREATE TABLE IF NOT EXISTS server_retention_policy (
    id BIGSERIAL PRIMARY KEY,
    max_lifetime BIGINT,
    min_lifetime BIGINT DEFAULT 0,
    expire_on_clients BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL
);

-- Insert default server retention policy if not exists
INSERT INTO server_retention_policy (min_lifetime, expire_on_clients, created_ts, updated_ts)
SELECT 0, FALSE, EXTRACT(EPOCH FROM NOW())::BIGINT * 1000, EXTRACT(EPOCH FROM NOW())::BIGINT * 1000
WHERE NOT EXISTS (SELECT 1 FROM server_retention_policy LIMIT 1);

-- ============================================================================
-- 9. Migration Record
-- ============================================================================

INSERT INTO migrations (name, applied_at) 
VALUES ('20260302000002_fix_api_test_issues', NOW())
ON CONFLICT (name) DO NOTHING;

-- ============================================================================
-- 10. Verification
-- ============================================================================

DO $$
DECLARE
    fixed_columns INTEGER := 0;
BEGIN
    -- Count fixed columns
    SELECT COUNT(*) INTO fixed_columns
    FROM information_schema.columns 
    WHERE table_schema = 'public' 
    AND (
        (table_name = 'application_service_state' AND column_name = 'as_id') OR
        (table_name = 'application_service_users' AND column_name = 'as_id') OR
        (table_name = 'room_summaries' AND column_name = 'last_event_ts') OR
        (table_name = 'room_summary_members' AND column_name = 'is_hero') OR
        (table_name = 'worker_tasks' AND column_name = 'priority') OR
        (table_name = 'federation_blacklist' AND column_name = 'block_type') OR
        (table_name = 'token_blacklist' AND column_name = 'token_type')
    );
    
    RAISE NOTICE 'Migration completed: % columns verified/fixed', fixed_columns;
END $$;
